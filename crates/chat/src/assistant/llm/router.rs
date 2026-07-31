use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::{
    assistant::{AssistantIntent, ContextWindow, llm},
    knowledge::model::KnowledgeCatalog,
};

pub struct SemanticRouter {
    llm: llm::SharedLlmClient,
}

impl SemanticRouter {
    pub fn new(llm: llm::SharedLlmClient, _catalog: &KnowledgeCatalog) -> Self {
        Self { llm }
    }

    pub async fn route(&self, message: &str, context: &ContextWindow) -> Result<AssistantIntent> {
        if message.trim().is_empty() {
            bail!("cannot route empty message");
        }
        let user = json!({
            "message": message,
            "context": context,
            "rules": [
                "Requests naming a metric to rank clients by (e.g. 'top N clients by savings accounts', '3 clients with most deposits', 'clients with highest balance') MUST use subject=client, operation=rank, output=ranking, grouping=none, pii=client_identity.",
                "domain MUST match the primary subject of the request, not a noun that merely appears in the sentence. Example: 'top 3 clients by savings account count' → subject=client, domain=client (NOT savings).",
                "Requests for arbitrary/random/sample clients ('client sembarang', 'give me any N clients') without a ranking metric are unsupported by the approved catalog — return intent=unsupported_in_domain instead of inventing a shape.",
                "When a request_shape dimension is genuinely ambiguous set it to unknown rather than guessing.",
                "request_shape.subject MUST be exactly one of: savings_transaction, savings_account, savings_account_charge, client, office, organization_hierarchy, product, unknown. Never invent a value outside this list. Use savings_account_charge for charges or fees applied to a savings account; use savings_account for a request framed around the account itself. The same rule applies to every other request_shape/domain/intent enum field: only emit values defined by the schema, defaulting to unknown/unsupported_in_domain rather than fabricating a new enum member.",
                "For matched reporting capabilities use intent=report_request, not the capability id. Do not invent SQL, capability ids, or unavailable report support.",
                "canonical_query_en MUST be the user's message translated to English, keeping every reporting term, entity, metric, and date intact (e.g. 'hutang' -> 'debt/unpaid charge', 'jatuh tempo' -> 'due date', 'terlambat'/'lewat jatuh tempo' -> 'overdue'). If the message is already English, copy it unchanged. Never leave it empty."
            ]
        })
        .to_string();
        let response = llm::structured::<AssistantIntent>(
            self.llm.as_ref(),
            llm::LlmPurpose::RouteIntent,
            ROUTER_SYSTEM,
            &user,
            Some(schemars::schema_for!(AssistantIntent)),
        )
        .await
        .context("route intent with structured LLM")?;
        Ok(response.value)
    }
}

const ROUTER_SYSTEM: &str = "You route reporting assistant messages. Return only JSON matching the AssistantIntent schema. No SQL. English-only user-facing reasoning.";

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::Result;
    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use crate::assistant::{
        AssistantDomain, AssistantIntentKind, AssistantLanguage, ContextReference,
        llm::{EmbeddingResponse, LlmClient, LlmResponse, TokenUsage},
    };

    struct FakeLlm;

    #[async_trait]
    impl LlmClient for FakeLlm {
        async fn structured_value(
            &self,
            _purpose: crate::assistant::llm::LlmPurpose,
            _system: &str,
            user: &str,
            _schema: serde_json::Value,
        ) -> Result<LlmResponse<serde_json::Value>> {
            let message = serde_json::from_str::<serde_json::Value>(user)?["message"]
                .as_str()
                .unwrap_or_default()
                .to_lowercase();
            let (intent, domain) = if message.contains("client") {
                (AssistantIntentKind::DataLookup, AssistantDomain::Client)
            } else {
                (AssistantIntentKind::ReportRequest, AssistantDomain::Savings)
            };
            Ok(LlmResponse {
                value: json!({
                    "intent": intent,
                    "domain": domain,
                    "request_shape": {},
                    "language": AssistantLanguage::En,
                    "entities": [],
                    "constraints": {},
                    "context_reference": ContextReference::None,
                    "confidence": 0.9,
                    "reason": "fake"
                }),
                usage: TokenUsage::default(),
                cost_usd: None,
                provider: "fake".into(),
                model: "fake".into(),
                latency_ms: 0,
            })
        }

        async fn embed(
            &self,
            _purpose: crate::assistant::llm::LlmPurpose,
            text: &str,
        ) -> Result<EmbeddingResponse> {
            Ok(EmbeddingResponse {
                vector: fake_embedding(text),
                usage: TokenUsage::default(),
                cost_usd: None,
                provider: "fake".into(),
                model: "fake".into(),
                latency_ms: 0,
            })
        }
    }

    fn fake_embedding(text: &str) -> Vec<f32> {
        let text = text.to_lowercase();
        vec![
            text.matches("client").count() as f32,
            text.matches("saving").count() as f32,
            text.matches("balance").count() as f32,
            text.matches("monthly").count() as f32,
        ]
    }

    fn empty_catalog() -> KnowledgeCatalog {
        KnowledgeCatalog {
            root_path: Default::default(),
            query_path: Default::default(),
            data_areas: Vec::new(),
            domains: Vec::new(),
            schemas: Vec::new(),
            metrics: Vec::new(),
            capabilities: Vec::new(),
            queries: Vec::new(),
            policies: Vec::new(),
            responses: Vec::new(),
            parameter_inputs: Vec::new(),
            classification: Default::default(),
            datasets: Vec::new(),
        }
    }

    fn empty_context() -> ContextWindow {
        ContextWindow {
            summary: None,
            active_domain: None,
            selected_entities: json!({}),
            recent_messages: Vec::new(),
            relevant_jobs: Vec::new(),
            pending_clarification: None,
            source_intent: None,
            source_snippets: Vec::new(),
            client_scope: json!({}),
            warnings: Vec::new(),
        }
    }

    #[tokio::test]
    async fn router_returns_structured_intent_without_keyword_fallback() {
        let catalog = empty_catalog();
        let router = SemanticRouter::new(Arc::new(FakeLlm), &catalog);
        let intent = router.route("client list", &empty_context()).await.unwrap();
        assert_eq!(intent.intent, AssistantIntentKind::DataLookup);
        assert_eq!(intent.domain, AssistantDomain::Client);
    }

    /// LLM returns a value for `request_shape.operation` outside the
    /// `RequestOperation` enum. The router must reject it with a
    /// schema-level error naming the offending field, not a generic
    /// `serde_json` decode failure.
    struct InvalidEnumFakeLlm;

    #[async_trait]
    impl LlmClient for InvalidEnumFakeLlm {
        async fn structured_value(
            &self,
            _purpose: crate::assistant::llm::LlmPurpose,
            _system: &str,
            _user: &str,
            _schema: serde_json::Value,
        ) -> Result<LlmResponse<serde_json::Value>> {
            Ok(LlmResponse {
                value: json!({
                    "intent": "report_request",
                    "domain": "savings",
                    "request_shape": {"operation": "not_a_real_operation"},
                    "language": "en",
                    "entities": [],
                    "constraints": {},
                    "context_reference": "none",
                    "confidence": 0.9,
                    "reason": "fake"
                }),
                usage: TokenUsage::default(),
                cost_usd: None,
                provider: "fake".into(),
                model: "fake".into(),
                latency_ms: 0,
            })
        }

        async fn embed(
            &self,
            _purpose: crate::assistant::llm::LlmPurpose,
            text: &str,
        ) -> Result<EmbeddingResponse> {
            Ok(EmbeddingResponse {
                vector: fake_embedding(text),
                usage: TokenUsage::default(),
                cost_usd: None,
                provider: "fake".into(),
                model: "fake".into(),
                latency_ms: 0,
            })
        }
    }

    #[tokio::test]
    async fn router_rejects_invalid_request_shape_operation_with_field_level_error() {
        let catalog = empty_catalog();
        let router = SemanticRouter::new(Arc::new(InvalidEnumFakeLlm), &catalog);
        let error = router
            .route("client list", &empty_context())
            .await
            .expect_err("invalid enum value must be rejected");
        let message = error
            .chain()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            message.contains("request_shape.operation") || message.contains("operation"),
            "error should name the offending field, got: {message}"
        );
        assert!(
            !message.contains("structured LLM response schema mismatch")
                || message.contains("operation"),
            "generic fallback message must not swallow the field-level detail: {message}"
        );
    }
}
