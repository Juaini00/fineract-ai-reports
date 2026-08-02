//! Layer 1 gateway client. Wraps a shared LLM client, builds the prompt,
//! decodes into `LlmGatewayExtraction`, retries once on schema mismatch, and
//! sanitizes the returned struct against the caller's visible catalogue and
//! literal user text (spec §4.3, §4.4).

use anyhow::Result;
use app_core::auth::model::PrincipalContext;

use crate::assistant::llm::{self, LlmPurpose, SharedLlmClient};
use crate::assistant::understanding::gateway::{
    LlmGatewayExtraction, capability_summary,
    prompt::{CapabilitySummary, build_gateway_prompt},
};
use crate::knowledge::model::KnowledgeCatalog;

pub struct GatewayClient {
    llm: SharedLlmClient,
}

#[derive(Debug)]
pub enum GatewayError {
    SchemaInvalidAfterRetry,
    ProviderUnavailable(anyhow::Error),
    ProviderMalformed(anyhow::Error),
}

impl std::fmt::Display for GatewayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SchemaInvalidAfterRetry => {
                f.write_str("llm gateway returned a schema-invalid extraction after retry")
            }
            Self::ProviderUnavailable(_) => f.write_str("llm provider unavailable"),
            Self::ProviderMalformed(_) => f.write_str("llm provider response malformed"),
        }
    }
}

impl std::error::Error for GatewayError {}

const SYSTEM_PROMPT: &str = "You are the reporting assistant's structured extraction gateway. \
    Return a single JSON object matching the LlmGatewayExtraction schema and nothing else.";

impl GatewayClient {
    pub fn new(llm: SharedLlmClient) -> Self {
        Self { llm }
    }

    pub async fn extract(
        &self,
        user_message: &str,
        history: Option<&str>,
        catalog: &KnowledgeCatalog,
        principal: &PrincipalContext,
    ) -> Result<LlmGatewayExtraction, GatewayError> {
        let visible = visible_capabilities(catalog, principal);
        let summary: Vec<CapabilitySummary<'_>> =
            visible.iter().map(|c| capability_summary(c)).collect();
        let user = build_gateway_prompt(user_message, &summary, history);
        let call = || async {
            llm::structured::<LlmGatewayExtraction>(
                self.llm.as_ref(),
                LlmPurpose::RouteIntent,
                SYSTEM_PROMPT,
                &user,
                None,
            )
            .await
        };
        let response = match call().await {
            Ok(response) => response,
            Err(first) => match call().await {
                Ok(response) => response,
                Err(second) => {
                    tracing::warn!(
                        target: "assistant::gateway",
                        first_error = %first,
                        second_error = %second,
                        "llm gateway extraction schema-invalid after retry",
                    );
                    return Err(GatewayError::SchemaInvalidAfterRetry);
                }
            },
        };
        Ok(sanitize(response.value, user_message, catalog, principal))
    }
}

fn visible_capabilities<'a>(
    catalog: &'a KnowledgeCatalog,
    principal: &PrincipalContext,
) -> Vec<&'a crate::knowledge::model::CapabilityKnowledge> {
    catalog
        .capabilities
        .iter()
        .filter(|capability| {
            capability.status == "approved_mvp"
                && principal_allows(principal, capability.id.as_str())
        })
        .collect()
}

fn principal_allows(principal: &PrincipalContext, capability_id: &str) -> bool {
    principal
        .capability_ids
        .iter()
        .any(|id| id == capability_id)
}

/// Enforce spec §4.3: drop entities that don't appear verbatim in the user
/// message, and drop candidates whose capability id is not visible to the
/// principal. Anything filtered is logged via `tracing::warn!`.
/// ponytail: audit-outbox emission for dropped entities/candidates deferred;
/// add when a real dropped-event triage need materializes.
fn sanitize(
    mut extraction: LlmGatewayExtraction,
    user_message: &str,
    catalog: &KnowledgeCatalog,
    principal: &PrincipalContext,
) -> LlmGatewayExtraction {
    let before_entities = extraction.entities.len();
    extraction
        .entities
        .retain(|entity| user_message.contains(entity.value.as_str()));
    let dropped_entities = before_entities - extraction.entities.len();
    if dropped_entities > 0 {
        tracing::warn!(
            target: "assistant::gateway",
            dropped = dropped_entities,
            "sanitize dropped entities not present verbatim in user message",
        );
    }
    let before_candidates = extraction.candidates.len();
    extraction.candidates.retain(|candidate| {
        catalog
            .capabilities
            .iter()
            .any(|capability| capability.id == candidate.capability_id)
            && principal_allows(principal, candidate.capability_id.as_str())
    });
    let dropped_candidates = before_candidates - extraction.candidates.len();
    if dropped_candidates > 0 {
        tracing::warn!(
            target: "assistant::gateway",
            dropped = dropped_candidates,
            "sanitize dropped candidates outside visible catalogue",
        );
    }
    extraction
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::llm::FakeLlmClient;
    use std::sync::Arc;
    use uuid::Uuid;

    fn empty_catalog() -> KnowledgeCatalog {
        use crate::knowledge::model::{
            CapabilityDefaults, CapabilityGuards, CapabilityKnowledge, ClassificationPolicy,
        };
        use std::path::PathBuf;
        KnowledgeCatalog {
            root_path: PathBuf::new(),
            query_path: PathBuf::new(),
            data_areas: vec![],
            domains: vec![],
            schemas: vec![],
            metrics: vec![],
            capabilities: vec![CapabilityKnowledge {
                id: "savings_deposit_top_n".into(),
                status: "approved_mvp".into(),
                domain: "savings".into(),
                query_id: "savings.deposit_top_n".into(),
                dataset_recipe: None,
                output_mode: "top_n".into(),
                request_shape: Default::default(),
                display_name: None,
                description: None,
                data_areas: vec![],
                metrics: vec![],
                examples: vec![],
                required_parameters: vec![],
                optional_parameters: vec![],
                defaults: CapabilityDefaults {
                    default_limit: None,
                },
                guards: CapabilityGuards {
                    max_limit: None,
                    max_date_range_days: None,
                },
                parameter_policies: vec![],
            }],
            queries: vec![],
            policies: vec![],
            responses: vec![],
            parameter_inputs: vec![],
            classification: ClassificationPolicy::default(),
            datasets: vec![],
        }
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

    #[tokio::test]
    async fn extract_returns_deserialised_value_and_drops_unseen_entities() {
        let fixture = serde_json::json!({
            "intent_kind": "report_request",
            "domain": "savings",
            "language": "en",
            "entities": [
                { "type": "metric", "value": "deposit", "phrase_span": [12, 19] },
                { "type": "metric", "value": "not_in_user_text", "phrase_span": [0, 0] }
            ],
            "candidates": [
                { "capability_id": "savings_deposit_top_n", "confidence": 0.9, "why": "user said top 10 deposits" },
                { "capability_id": "invented_capability", "confidence": 0.4, "why": "hallucinated" }
            ]
        });
        let fake = Arc::new(FakeLlmClient::default());
        fake.push_structured(fixture);
        let client = GatewayClient::new(fake as SharedLlmClient);
        let catalog = empty_catalog();
        let extraction = client
            .extract(
                "Top 10 deposits this month",
                None,
                &catalog,
                &principal(&["savings_deposit_top_n"]),
            )
            .await
            .expect("extract succeeds");
        assert_eq!(extraction.entities.len(), 1);
        assert_eq!(extraction.entities[0].value, "deposit");
        assert_eq!(extraction.candidates.len(), 1);
        assert_eq!(
            extraction.candidates[0].capability_id,
            "savings_deposit_top_n"
        );
    }

    #[tokio::test]
    async fn extract_returns_schema_invalid_after_two_failures() {
        let fake = Arc::new(FakeLlmClient::default());
        // Push two invalid values so both attempts decode-fail.
        fake.push_structured(serde_json::json!({"broken": true}));
        fake.push_structured(serde_json::json!({"broken": true}));
        let client = GatewayClient::new(fake as SharedLlmClient);
        let catalog = empty_catalog();
        let err = client
            .extract("hi", None, &catalog, &principal(&[]))
            .await
            .expect_err("schema mismatch must fail after retry");
        assert!(matches!(err, GatewayError::SchemaInvalidAfterRetry));
    }
}
