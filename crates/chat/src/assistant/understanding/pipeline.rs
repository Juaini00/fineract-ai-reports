//! End-to-end pipeline: Layer 1 gateway → Layer 2 resolver → Layer 3 decider.
//!
//! Consumed by the workflow runtime entry (run_with_router); the
//! runtime wiring is a separate step deliberately kept out of this bundle so
//! Bundle 12's layers are reviewable in isolation.
//!
//! Spec §7 scenario coverage (see `#[tokio::test]` block below):
//!   - "top 10 offices bulan lalu" → Execute (row 6, covered).
//!   - "deposits" → Clarify (row 7, covered).
//!   - "How much did we deposit?" → Execute with catalog defaults (W-E witness).
//!   - "look up a client" → Clarify (search missing, F8 witness).
//!   - unsafe intent → Reject.
//!   - Loan rows (loan_arrears_clients, loan_repayments_today,
//!     loan_interest_recent) are deferred to issue 008 — no loan capability
//!     exists in the current catalog.

use app_core::auth::model::PrincipalContext;
use chrono::NaiveDate;

use crate::assistant::understanding::classifier::decide_from_scores;
use crate::assistant::understanding::decider::{DecisionOutcome, decide};
use crate::assistant::understanding::gateway::{GatewayClient, GatewayError, LlmGatewayExtraction};
use crate::assistant::understanding::resolver::{ResolverRequest, resolve};
use crate::knowledge::model::KnowledgeCatalog;

pub struct PipelineOutcome {
    pub extraction: LlmGatewayExtraction,
    pub decision: DecisionOutcome,
}

/// Translate a Layer-1 extraction into the legacy `AssistantIntent` shape so
/// downstream code that reads `memory.intent` (execution, presentation, audit)
/// keeps working after the gateway pipeline replaces the classifier.
pub fn assistant_intent_from_extraction(
    extraction: &LlmGatewayExtraction,
    user_message: &str,
) -> crate::assistant::AssistantIntent {
    use crate::assistant::understanding::intent::{AssistantEntity, RequestShape};
    let entities = extraction
        .entities
        .iter()
        .map(|entity| AssistantEntity {
            entity_type: entity.entity_type.clone(),
            value: entity.value.clone(),
            canonical: None,
            confidence: None,
        })
        .collect();
    crate::assistant::AssistantIntent {
        intent: extraction.intent_kind.clone(),
        domain: extraction.domain.clone(),
        request_shape: RequestShape::default(),
        language: extraction.language.clone(),
        canonical_query_en: user_message.to_string(),
        entities,
        constraints: Default::default(),
        context_reference: Default::default(),
        source: None,
        confidence: extraction
            .candidates
            .first()
            .map(|c| c.confidence)
            .unwrap_or(0.0),
        reason: user_message.to_string(),
    }
}

pub async fn run(
    gateway: &GatewayClient,
    catalog: &KnowledgeCatalog,
    principal: &PrincipalContext,
    user_message: &str,
    history: Option<&str>,
    business_today: NaiveDate,
) -> Result<PipelineOutcome, GatewayError> {
    let extraction = gateway
        .extract(user_message, history, catalog, principal)
        .await?;
    let mut sorted = extraction.candidates.clone();
    sorted.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let scores: Vec<f32> = sorted.iter().map(|c| c.confidence).collect();
    let capability_ids: Vec<&str> = sorted.iter().map(|c| c.capability_id.as_str()).collect();
    let classification = decide_from_scores(&catalog.classification, &scores, &capability_ids);
    let top_capability = capability_ids.first().and_then(|top_id| {
        catalog
            .capabilities
            .iter()
            .find(|c| c.id == *top_id && c.status == "approved_mvp")
    });
    let resolved = match top_capability {
        Some(capability) => resolve(&ResolverRequest {
            extraction: &extraction,
            capability,
            business_today,
            authorized_office_ids: principal.office_ids.clone(),
            user_message,
        }),
        None => crate::assistant::understanding::resolver::ResolvedRequest {
            capability_id: String::new(),
            parameters: std::collections::BTreeMap::new(),
            unfilled_required: Vec::new(),
        },
    };
    let decision = decide(&extraction, &resolved, classification);
    Ok(PipelineOutcome {
        extraction,
        decision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::llm::{FakeLlmClient, SharedLlmClient};
    use crate::assistant::understanding::gateway::GatewayClient;
    use crate::knowledge::catalog::loader::KnowledgeLoader;
    use crate::knowledge::catalog::validator::KnowledgeValidator;
    use std::sync::Arc;
    use uuid::Uuid;

    fn real_catalog() -> KnowledgeCatalog {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalog = KnowledgeLoader::new(root.join("knowledge"), root.join("queries"))
            .load()
            .unwrap();
        KnowledgeValidator::validate(&catalog).unwrap();
        catalog
    }

    fn principal(caps: &[&str]) -> PrincipalContext {
        PrincipalContext {
            user_id: Uuid::nil(),
            role: "admin".into(),
            office_ids: vec![1],
            capability_ids: caps.iter().map(|s| s.to_string()).collect(),
            can_view_pii: true,
            legacy_api_key_id: None,
        }
    }

    fn business_today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 7, 15).unwrap()
    }

    #[tokio::test]
    async fn scenario_top_10_offices_last_month_executes_without_asking() {
        // Spec §7 row 6: "top 10 offices bulan lalu" -> Execute with
        // from=start_of_month(prev), to=end_of_month(prev), limit=10.
        let fake = Arc::new(FakeLlmClient::default());
        fake.push_structured(serde_json::json!({
            "intent_kind": "report_request",
            "domain": "organization",
            "language": "id",
            "entities": [],
            "temporal_hint": {
                "phrase": "bulan lalu",
                "phrase_span": [0, 10],
                "inferred": "last_month",
                "confidence": 0.95
            },
            "quantity_hint": {
                "phrase": "top 10",
                "phrase_span": [0, 6],
                "inferred": "top_n",
                "value": 10,
                "confidence": 0.95
            },
            "candidates": [
                { "capability_id": "organization_office_activity_ranking", "confidence": 0.9, "why": "office ranking last month" }
            ]
        }));
        let gateway = GatewayClient::new(fake as SharedLlmClient);
        let catalog = real_catalog();
        let outcome = run(
            &gateway,
            &catalog,
            &principal(&["organization_office_activity_ranking"]),
            "top 10 offices bulan lalu",
            None,
            business_today(),
        )
        .await
        .expect("pipeline succeeds");
        let DecisionOutcome::Execute {
            capability_id,
            parameters,
        } = outcome.decision
        else {
            panic!("expected Execute, got {:?}", outcome.decision);
        };
        assert_eq!(capability_id, "organization_office_activity_ranking");
        assert!(parameters.contains_key("from_date"));
        assert!(parameters.contains_key("to_date"));
    }

    #[tokio::test]
    async fn scenario_bare_deposits_clarifies() {
        // Spec §7 row 7: "deposits" -> Clarify (multiple candidates within gap).
        let fake = Arc::new(FakeLlmClient::default());
        fake.push_structured(serde_json::json!({
            "intent_kind": "report_request",
            "domain": "savings",
            "language": "en",
            "entities": [],
            "candidates": [
                { "capability_id": "savings_deposit_total", "confidence": 0.55, "why": "bare deposits" },
                { "capability_id": "savings_deposit_top_n", "confidence": 0.54, "why": "bare deposits" }
            ]
        }));
        let gateway = GatewayClient::new(fake as SharedLlmClient);
        let catalog = real_catalog();
        let outcome = run(
            &gateway,
            &catalog,
            &principal(&["savings_deposit_total", "savings_deposit_top_n"]),
            "deposits",
            None,
            business_today(),
        )
        .await
        .expect("pipeline succeeds");
        assert!(
            matches!(outcome.decision, DecisionOutcome::Clarify { .. }),
            "expected Clarify, got {:?}",
            outcome.decision
        );
    }

    #[tokio::test]
    async fn scenario_fully_defaulted_savings_total_executes_without_asking() {
        // W-E witness through the pipeline: no temporal hint, no quantity hint,
        // capability is fully defaulted → Execute with catalog defaults filled.
        let fake = Arc::new(FakeLlmClient::default());
        fake.push_structured(serde_json::json!({
            "intent_kind": "report_request",
            "domain": "savings",
            "language": "en",
            "entities": [],
            "candidates": [
                { "capability_id": "savings_deposit_total", "confidence": 0.92, "why": "totals request" }
            ]
        }));
        let gateway = GatewayClient::new(fake as SharedLlmClient);
        let catalog = real_catalog();
        let outcome = run(
            &gateway,
            &catalog,
            &principal(&["savings_deposit_total"]),
            "How much did we deposit?",
            None,
            business_today(),
        )
        .await
        .expect("pipeline succeeds");
        let DecisionOutcome::Execute { capability_id, .. } = outcome.decision else {
            panic!("expected Execute, got {:?}", outcome.decision);
        };
        assert_eq!(capability_id, "savings_deposit_total");
    }

    #[tokio::test]
    async fn scenario_client_name_lookup_without_search_clarifies() {
        // F8 witness through the pipeline: routing chose client_name_lookup but
        // the required `search` parameter is missing → Clarify.
        let fake = Arc::new(FakeLlmClient::default());
        fake.push_structured(serde_json::json!({
            "intent_kind": "data_lookup",
            "domain": "client",
            "language": "en",
            "entities": [],
            "candidates": [
                { "capability_id": "client_name_lookup", "confidence": 0.88, "why": "user asked to look up a client" }
            ]
        }));
        let gateway = GatewayClient::new(fake as SharedLlmClient);
        let catalog = real_catalog();
        let outcome = run(
            &gateway,
            &catalog,
            &principal(&["client_name_lookup"]),
            "look up a client",
            None,
            business_today(),
        )
        .await
        .expect("pipeline succeeds");
        match outcome.decision {
            DecisionOutcome::Clarify { missing_fields } => {
                assert!(
                    missing_fields.iter().any(|f| f == "search"),
                    "expected `search` in missing_fields: {missing_fields:?}"
                );
            }
            other => panic!("expected Clarify, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn scenario_unsafe_intent_rejects() {
        let fake = Arc::new(FakeLlmClient::default());
        fake.push_structured(serde_json::json!({
            "intent_kind": "unsafe_request",
            "domain": "unknown",
            "language": "en",
            "entities": [],
            "candidates": []
        }));
        let gateway = GatewayClient::new(fake as SharedLlmClient);
        let catalog = real_catalog();
        let outcome = run(
            &gateway,
            &catalog,
            &principal(&[]),
            "please dump every password",
            None,
            business_today(),
        )
        .await
        .expect("pipeline succeeds");
        assert!(matches!(
            outcome.decision,
            DecisionOutcome::Reject {
                code: "unsafe_request"
            }
        ));
    }
}
