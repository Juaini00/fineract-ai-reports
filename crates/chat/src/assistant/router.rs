use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::json;

use crate::{
    assistant::{AssistantIntent, ContextWindow, llm},
    knowledge::model::KnowledgeCatalog,
};

pub struct SemanticRouter {
    llm: llm::SharedLlmClient,
    candidates: Vec<RouteCandidate>,
}

impl SemanticRouter {
    pub fn new(llm: llm::SharedLlmClient, catalog: &KnowledgeCatalog) -> Self {
        let candidates = catalog
            .capabilities
            .iter()
            .filter(|capability| matches!(capability.status.as_str(), "active" | "approved_mvp"))
            .map(|capability| RouteCandidate {
                id: capability.id.clone(),
                domain: capability.domain.clone(),
                description: capability.description.clone().unwrap_or_default(),
                examples: capability.examples.clone(),
                candidate_text: format!(
                    "{} {} {}",
                    capability.display_name.clone().unwrap_or_default(),
                    capability.description.clone().unwrap_or_default(),
                    capability.examples.join(" ")
                ),
            })
            .collect();
        Self { llm, candidates }
    }

    pub async fn route(&self, message: &str, context: &ContextWindow) -> Result<AssistantIntent> {
        if message.trim().is_empty() {
            bail!("cannot route empty message");
        }
        let message_vector = self
            .llm
            .embed(llm::LlmPurpose::RouteEmbedding, message)
            .await
            .context("embed route message")?
            .vector;
        let mut candidates = Vec::with_capacity(self.candidates.len());
        for candidate in &self.candidates {
            let candidate_vector = self
                .llm
                .embed(llm::LlmPurpose::RouteEmbedding, &candidate.candidate_text)
                .await
                .with_context(|| format!("embed route candidate {}", candidate.id))?
                .vector;
            candidates.push((cosine(&message_vector, &candidate_vector), candidate));
        }
        candidates.sort_by(|left, right| right.0.total_cmp(&left.0));
        let candidates = candidates
            .into_iter()
            .take(8)
            .map(|(score, candidate)| CandidatePrompt {
                id: &candidate.id,
                domain: &candidate.domain,
                description: &candidate.description,
                examples: &candidate.examples,
                similarity: score,
            })
            .collect::<Vec<_>>();
        let user = json!({
            "message": message,
            "context": context,
            "candidate_capabilities": candidates,
            "rules": [
                "Return one AssistantIntent JSON object only with keys: intent, domain, language, entities, constraints, context_reference, confidence, reason.",
                "Use entities=[] when no explicit named entity is required; context_reference must be the string none, not null.",
                "intent must be one of: greeting, help, report_request, data_lookup, clarification_reply, follow_up, unsafe_request, out_of_domain, unsupported_in_domain.",
                "For matched reporting capabilities use intent=report_request, not the capability id.",
                "Use candidates only as hints; do not invent SQL or execute anything.",
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

#[derive(Debug, Clone)]
struct RouteCandidate {
    id: String,
    domain: String,
    description: String,
    examples: Vec<String>,
    candidate_text: String,
}

#[derive(Serialize)]
struct CandidatePrompt<'a> {
    id: &'a str,
    domain: &'a str,
    description: &'a str,
    examples: &'a [String],
    similarity: f32,
}

fn cosine(left: &[f32], right: &[f32]) -> f32 {
    let dot = left.iter().zip(right).map(|(l, r)| l * r).sum::<f32>();
    let left_norm = left.iter().map(|v| v * v).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|v| v * v).sum::<f32>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm * right_norm)
    }
}

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
