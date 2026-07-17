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
                "Return one AssistantIntent JSON object only with keys: intent, domain, request_shape, language, entities, constraints, context_reference, confidence, reason.",
                "request_shape must contain operation (total|summary|list|rank|trend|lookup|random_sample|unknown), subject (savings_transaction|savings_account|client|office|organization_hierarchy|product|unknown), grouping (none|month|office|product|unknown), output (scalar|summary|list|ranking|time_series|lookup|unknown), and pii (none|client_identity|conditional_client_identity|unknown).",
                "Requests naming a metric to rank clients by (e.g. 'top N clients by savings accounts', '3 clients with most deposits', 'clients with highest balance') MUST be subject=client, operation=rank, output=ranking, grouping=none, pii=client_identity.",
                "domain MUST match the primary subject of the request, not a noun that merely appears in the sentence. If subject=client the domain is client; if subject=office/organization_hierarchy the domain is organization; only pick savings when subject is savings_transaction or savings_account. Example: 'top 3 clients by savings account count' → subject=client, domain=client (NOT savings).",
                "Requests for arbitrary/random/sample clients ('client sembarang', 'give me any N clients') without a ranking metric are unsupported by the approved catalog — return intent=unsupported_in_domain instead of inventing a shape.",
                "When any dimension of request_shape is genuinely ambiguous set it to 'unknown' rather than guessing; the retriever tolerates 'unknown' but rejects wrong guesses.",
                "Use entities=[] when no explicit named entity is required; context_reference must be the string none, not null.",
                "intent must be one of: greeting, help, report_request, data_lookup, clarification_reply, follow_up, unsafe_request, out_of_domain, unsupported_in_domain.",
                "For matched reporting capabilities use intent=report_request, not the capability id.",
                "Do not invent SQL, capability ids, or unavailable report support.",
                "If JSON cannot match the schema, fail rather than fallback."
            ]
        })
        .to_string();
        let response = llm::structured::<AssistantIntent>(
            self.llm.as_ref(),
            llm::LlmPurpose::RouteIntent,
            ROUTER_SYSTEM,
            &user,
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

    #[tokio::test]
    async fn router_returns_structured_intent_without_keyword_fallback() {
        let catalog = KnowledgeCatalog {
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
            classification: Default::default(),
        };
        let router = SemanticRouter::new(Arc::new(FakeLlm), &catalog);
        let context = ContextWindow {
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
        };
        let intent = router.route("client list", &context).await.unwrap();
        assert_eq!(intent.intent, AssistantIntentKind::DataLookup);
        assert_eq!(intent.domain, AssistantDomain::Client);
    }
}
