use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::assistant::{
    ContextWindow,
    clarification::{ClarificationOutcome, ClarificationPayload, OTHER_CLARIFICATION_OPTION_ID},
    llm::{LlmClient, LlmPurpose, structured},
};

pub struct ClarificationResolver;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct LlmClarificationDecision {
    outcome: ClarificationOutcome,
}

impl ClarificationResolver {
    pub fn resolve_exact(
        reply: &str,
        payload: &ClarificationPayload,
    ) -> Option<ClarificationOutcome> {
        let reply = reply.trim();
        if reply.eq_ignore_ascii_case(OTHER_CLARIFICATION_OPTION_ID) {
            return Some(ClarificationOutcome::FreeFormOther {
                text: String::new(),
                confidence: 1.0,
            });
        }
        payload.options.iter().find_map(|option| {
            (reply.eq_ignore_ascii_case(&option.id) || reply.eq_ignore_ascii_case(&option.label))
                .then(|| ClarificationOutcome::SelectedOption {
                    option_id: option.id.clone(),
                    confidence: 1.0,
                })
        })
    }

    pub async fn resolve(
        reply: &str,
        payload: &ClarificationPayload,
        context: &ContextWindow,
        llm: &dyn LlmClient,
    ) -> Result<ClarificationOutcome> {
        if let Some(outcome) = Self::resolve_exact(reply, payload) {
            return Ok(outcome);
        }

        if let Some(outcome) = resolve_by_embedding(reply, payload, llm).await? {
            return Ok(outcome);
        }

        let decision = structured::<LlmClarificationDecision>(
            llm,
            LlmPurpose::ClarificationResolve,
            "Resolve a user's reply to a pending report clarification. Return only the JSON outcome. Prefer selected_option only when the reply clearly maps to one offered option.",
            &json!({
                "reply": reply,
                "pending_clarification": payload,
                "context": context,
            })
            .to_string(),
            None,
        )
        .await?;
        Ok(decision.value.outcome)
    }
}

async fn resolve_by_embedding(
    reply: &str,
    payload: &ClarificationPayload,
    llm: &dyn LlmClient,
) -> Result<Option<ClarificationOutcome>> {
    if payload.options.is_empty() {
        return Ok(None);
    }
    let reply_vector = llm
        .embed(LlmPurpose::ClarificationEmbedding, reply)
        .await?
        .vector;
    let mut scored = Vec::with_capacity(payload.options.len());
    for option in payload
        .options
        .iter()
        .filter(|option| option.id != OTHER_CLARIFICATION_OPTION_ID)
    {
        let text = match &option.description {
            Some(description) => format!("{}\n{}", option.label, description),
            None => option.label.clone(),
        };
        let option_vector = llm
            .embed(LlmPurpose::ClarificationEmbedding, &text)
            .await?
            .vector;
        scored.push((cosine(&reply_vector, &option_vector), option.id.clone()));
    }
    scored.sort_by(|left, right| right.0.total_cmp(&left.0));
    let Some((best_score, best_id)) = scored.first() else {
        return Ok(None);
    };
    let next_score = scored.get(1).map(|item| item.0).unwrap_or(0.0);
    if *best_score >= 0.72 && (*best_score - next_score) >= 0.08 {
        Ok(Some(ClarificationOutcome::SelectedOption {
            option_id: best_id.clone(),
            confidence: *best_score,
        }))
    } else {
        Ok(None)
    }
}

fn cosine(left: &[f32], right: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut left_norm = 0.0;
    let mut right_norm = 0.0;
    for (left, right) in left.iter().zip(right.iter()) {
        dot += left * right;
        left_norm += left * left;
        right_norm += right * right;
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm.sqrt() * right_norm.sqrt())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use async_trait::async_trait;

    use super::*;
    use crate::assistant::{
        ClarificationOption,
        llm::{EmbeddingResponse, LlmResponse},
    };

    struct FakeLlm;

    #[async_trait]
    impl LlmClient for FakeLlm {
        async fn structured_value(
            &self,
            _purpose: LlmPurpose,
            _system: &str,
            _user: &str,
            _schema: serde_json::Value,
        ) -> Result<LlmResponse<serde_json::Value>> {
            unreachable!("exact others should not call LLM")
        }

        async fn embed(&self, _purpose: LlmPurpose, _text: &str) -> Result<EmbeddingResponse> {
            unreachable!("exact others should not call embeddings")
        }
    }

    #[tokio::test]
    async fn exact_others_reply_becomes_free_form_other() {
        let outcome = ClarificationResolver::resolve(
            "others",
            &ClarificationPayload {
                question: "Which report?".into(),
                options: vec![ClarificationOption {
                    id: OTHER_CLARIFICATION_OPTION_ID.into(),
                    label: "Others".into(),
                    description: None,
                }],
                attempt: 1,
                source_intent: None,
                allow_free_text: true,
                is_missing_execution_parameters: false,
            },
            &ContextWindow {
                summary: None,
                active_domain: None,
                selected_entities: serde_json::json!([]),
                recent_messages: Vec::new(),
                relevant_jobs: Vec::new(),
                pending_clarification: None,
                source_intent: None,
                source_snippets: Vec::new(),
                client_scope: serde_json::json!({}),
                warnings: Vec::new(),
            },
            &FakeLlm,
        )
        .await
        .unwrap();

        assert_eq!(
            outcome,
            ClarificationOutcome::FreeFormOther {
                text: String::new(),
                confidence: 1.0,
            }
        );
    }

    #[tokio::test]
    async fn exact_option_id_becomes_selected_option_without_llm() {
        let outcome = ClarificationResolver::resolve(
            " client_top_n_by_savings_balance ",
            &ClarificationPayload {
                question: "Which report?".into(),
                options: vec![ClarificationOption {
                    id: "client_top_n_by_savings_balance".into(),
                    label: "Top clients by savings balance".into(),
                    description: None,
                }],
                attempt: 1,
                source_intent: None,
                allow_free_text: true,
                is_missing_execution_parameters: false,
            },
            &ContextWindow {
                summary: None,
                active_domain: None,
                selected_entities: serde_json::json!([]),
                recent_messages: Vec::new(),
                relevant_jobs: Vec::new(),
                pending_clarification: None,
                source_intent: None,
                source_snippets: Vec::new(),
                client_scope: serde_json::json!({}),
                warnings: Vec::new(),
            },
            &FakeLlm,
        )
        .await
        .unwrap();

        assert_eq!(
            outcome,
            ClarificationOutcome::SelectedOption {
                option_id: "client_top_n_by_savings_balance".into(),
                confidence: 1.0,
            }
        );
    }
}
