//! End-to-end pipeline: Layer 1 gateway → Layer 2 resolver → Layer 3 decider.
//!
//! Ready to be dropped into `AssistantGraphRuntime` (spec §7 Task 7.1); the
//! runtime wiring is a separate step deliberately kept out of this bundle so
//! Bundle 12's layers are reviewable in isolation.

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
