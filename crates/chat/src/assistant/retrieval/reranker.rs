//! LLM re-ranker (issue 02).
//!
//! Replaces the arithmetic `EvidenceEvaluator` with a natural-language pass:
//! feed the user query + top-K retrieval candidates to the LLM, decode a
//! structured `RerankerDecision`. It returns scored ranking evidence; the workflow
//! compiler owns the clarification gate. Malformed output is operational failure.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::evidence::Evidence;
use crate::assistant::llm::{LlmClient, LlmPurpose, SharedLlmClient, structured};

/// Cap on candidates sent to the LLM (input-token budget). 12 keeps the
/// prompt small while still covering domain-adjacent competitors that
/// legitimately tie in retrieval.
const MAX_CANDIDATES: usize = 12;

const RERANKER_SYSTEM: &str = "You are a reranker. Given a user query and a list of \
candidate reporting capabilities, pick the one that best matches the query.\n\n\
Rules:\n\
- decision=\"select\" with capability_id and confidence in [0.0, 1.0] when one \
candidate clearly matches the query intent.\n\
- decision=\"clarify\" with 2-4 alternative capability ids when several candidates \
plausibly fit and the user should choose.\n\
- Candidates that report a *different measure* of the same subject (client \
counts vs savings balances vs deposit volume vs staff headcount vs hierarchy \
depth) are not interchangeable. When the query asks for a \"summary\", \
\"overview\", \"report\", \"ringkasan\" or \"laporan\" of a subject without \
naming which measure it wants, and two or more candidates differ only in that \
measure, choose clarify. Picking the candidate whose title happens to echo the \
user's wording most literally is a guess presented as an answer, which for a \
banking report is worse than asking.\n\
- decision=\"unsupported\" when no candidate matches the query semantically. You \
are the only stage permitted to make that call — nothing upstream decides \
coverage — so make it on the candidate ids, descriptions and examples in front \
of you, never on a hunch about what the catalog contains.\n\
- Never select or clarify with a candidate that answers a *different* question \
than the one asked (a different subject, a different filter, or one that drops \
a filter the user named). Answering an adjacent question is worse than \
\"unsupported\".\n\
- `supported_intents`, `unsupported_intents` and `user_filters` are the \
authoritative statement of what a candidate does; `title` and `description` are \
prose and may be narrower than the truth. A candidate whose `user_filters` \
include the field the user named CAN filter by it, even if the title does not \
say so. Judge on the declared fields first and treat the title as a hint.\n\
- An ordering word in a title (\"recent\", \"latest\", \"top\") describes the \
sort order, not a restriction of the rows, unless `supported_intents` or \
`description` says the rows are actually restricted. Do not refuse an \
unrestricted \"all X\" request merely because the only matching candidate sorts \
its rows by recency.\n\
- Confidence must reflect actual certainty, not retrieval-score arithmetic.\n\
- Prefer specificity: \"total\" queries pick totals; \"top N\"/\"highest\"/\"largest\" \
queries pick top_n variants; \"per month\"/\"monthly\" queries pick monthly variants; \
\"random\"/\"sample\" queries pick random_sample variants when present; \
\"newest\"/\"latest\"/\"most recent\"/\"paling baru\" queries pick the recency \
variant over status variants like pending/overdue/outstanding.\n\
- Keep the reason short (one sentence).";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RerankerVerdict {
    Select,
    Clarify,
    Unsupported,
    FailedOperational,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RerankerDecision {
    pub decision: RerankerVerdict,
    #[serde(default)]
    pub capability_id: Option<String>,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub alternatives: Vec<String>,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub ranked_candidates: Vec<String>,
}

impl RerankerDecision {
    pub fn select(id: impl Into<String>, confidence: f32) -> Self {
        Self {
            decision: RerankerVerdict::Select,
            capability_id: Some(id.into()),
            confidence,
            alternatives: Vec::new(),
            reason: String::new(),
            ranked_candidates: Vec::new(),
        }
    }

    pub fn clarify(alternatives: Vec<String>) -> Self {
        Self {
            decision: RerankerVerdict::Clarify,
            capability_id: None,
            confidence: 0.0,
            alternatives,
            reason: String::new(),
            ranked_candidates: Vec::new(),
        }
    }

    pub fn unsupported(reason: impl Into<String>) -> Self {
        Self {
            decision: RerankerVerdict::Unsupported,
            capability_id: None,
            confidence: 0.0,
            alternatives: Vec::new(),
            reason: reason.into(),
            ranked_candidates: Vec::new(),
        }
    }
    pub fn failed_operational() -> Self {
        Self {
            decision: RerankerVerdict::FailedOperational,
            capability_id: None,
            confidence: 0.0,
            alternatives: Vec::new(),
            reason: "reranking failed".into(),
            ranked_candidates: Vec::new(),
        }
    }
}

pub struct LlmReranker<'a> {
    llm: Option<&'a SharedLlmClient>,
}

impl<'a> LlmReranker<'a> {
    pub fn new(llm: Option<&'a SharedLlmClient>) -> Self {
        Self { llm }
    }

    /// Reranks `candidates` for `query`. Empty candidates short-circuit to
    /// `Unsupported`. No LLM configured falls back to a score-tie heuristic.
    pub async fn rerank(&self, query: &str, candidates: &[Evidence]) -> RerankerDecision {
        if candidates.is_empty() {
            return RerankerDecision::unsupported("no candidates");
        }
        let Some(llm_arc) = self.llm else {
            return score_tie_fallback(candidates);
        };
        let llm: &dyn LlmClient = &**llm_arc;

        let user_json = json!({
            "query": query,
            "candidates": candidates
                .iter()
                .take(MAX_CANDIDATES)
                .map(|e| json!({
                    "id": e.capability_id,
                    "title": e.title,
                    "description": e.metadata.get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                    "examples": e.metadata.get("examples").cloned().unwrap_or(json!([])),
                    "output_mode": e.metadata.get("output_mode")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                    // The three fields below are the capability's actual
                    // boundary. Without them this stage judged coverage from
                    // the title's adjective, which is prose and can disagree
                    // with the approved SQL underneath it.
                    "supported_intents": e.metadata.get("supported_intents")
                        .cloned().unwrap_or(json!([])),
                    "unsupported_intents": e.metadata.get("unsupported_intents")
                        .cloned().unwrap_or(json!([])),
                    "user_filters": e.metadata.get("user_filters")
                        .cloned().unwrap_or(json!([])),
                }))
                .collect::<Vec<_>>(),
        });
        let user = serde_json::to_string(&user_json).unwrap_or_default();
        let schema = schemars::schema_for!(RerankerDecision);

        let mut decision = match structured::<RerankerDecision>(
            llm,
            LlmPurpose::EvidenceRetrieval,
            RERANKER_SYSTEM,
            &user,
            Some(schema.clone()),
        )
        .await
        {
            Ok(response) => response.value,
            Err(first) => {
                tracing::warn!(
                    target: "assistant::reranker",
                    error = %first,
                    "reranker schema-invalid; retrying once",
                );
                match structured::<RerankerDecision>(
                    llm,
                    LlmPurpose::EvidenceRetrieval,
                    RERANKER_SYSTEM,
                    &user,
                    Some(schema),
                )
                .await
                {
                    Ok(response) => response.value,
                    Err(second) => {
                        tracing::warn!(
                            target: "assistant::reranker",
                            error = %second,
                            "reranker failed after retry",
                        );
                        return RerankerDecision::failed_operational();
                    }
                }
            }
        };

        decision.ranked_candidates = candidates
            .iter()
            .take(MAX_CANDIDATES)
            .map(|candidate| candidate.capability_id.clone())
            .collect();
        decision
    }
}

/// No model is an operational failure; score gaps are evidence for the compiler, not a policy decision.
fn score_tie_fallback(_candidates: &[Evidence]) -> RerankerDecision {
    RerankerDecision::failed_operational()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::assistant::llm::FakeLlmClient;

    fn ev(id: &str, score: f32) -> Evidence {
        Evidence {
            capability_id: id.into(),
            title: id.into(),
            score,
            source_type: "capability".into(),
            metadata: json!({"description": id}),
            conflicting: false,
        }
    }

    fn shared(client: FakeLlmClient) -> SharedLlmClient {
        Arc::new(client)
    }

    #[tokio::test]
    async fn empty_candidates_returns_unsupported_without_llm_call() {
        let llm = shared(FakeLlmClient::default());
        let out = LlmReranker::new(Some(&llm)).rerank("anything", &[]).await;
        assert_eq!(out.decision, RerankerVerdict::Unsupported);
    }

    #[tokio::test]
    async fn select_with_high_confidence_passes_through() {
        let llm = FakeLlmClient::default();
        llm.push_structured(json!({
            "decision": "select",
            "capability_id": "savings_deposit_total",
            "confidence": 0.85,
            "alternatives": [],
            "reason": "matches total deposit intent",
        }));
        let llm = shared(llm);
        let out = LlmReranker::new(Some(&llm))
            .rerank(
                "total savings deposits",
                &[ev("savings_deposit_total", 0.7)],
            )
            .await;
        assert_eq!(out.decision, RerankerVerdict::Select);
        assert_eq!(out.capability_id.as_deref(), Some("savings_deposit_total"));
    }

    #[tokio::test]
    async fn low_confidence_select_remains_evidence_for_the_compiler() {
        let llm = FakeLlmClient::default();
        llm.push_structured(json!({
            "decision": "select",
            "capability_id": "savings_deposit_total",
            "confidence": 0.4,
            "alternatives": [],
            "reason": "unsure",
        }));
        let llm = shared(llm);
        let out = LlmReranker::new(Some(&llm))
            .rerank(
                "savings report",
                &[
                    ev("savings_deposit_total", 0.7),
                    ev("savings_deposit_top_n", 0.6),
                ],
            )
            .await;
        assert_eq!(out.decision, RerankerVerdict::Select);
        assert_eq!(
            out.ranked_candidates,
            vec!["savings_deposit_total", "savings_deposit_top_n"]
        );
    }

    #[tokio::test]
    async fn clarify_with_empty_alternatives_does_not_invent_choices() {
        let llm = FakeLlmClient::default();
        llm.push_structured(json!({
            "decision": "clarify",
            "capability_id": null,
            "confidence": 0.0,
            "alternatives": [],
            "reason": "ambiguous",
        }));
        let llm = shared(llm);
        let out = LlmReranker::new(Some(&llm))
            .rerank(
                "savings report",
                &[
                    ev("a", 0.6),
                    ev("b", 0.5),
                    ev("c", 0.4),
                    ev("d", 0.3),
                    ev("e", 0.2),
                ],
            )
            .await;
        assert_eq!(out.decision, RerankerVerdict::Clarify);
        assert!(out.alternatives.is_empty());
        assert_eq!(out.ranked_candidates, vec!["a", "b", "c", "d", "e"]);
    }

    #[tokio::test]
    async fn unsupported_passes_through() {
        let llm = FakeLlmClient::default();
        llm.push_structured(json!({
            "decision": "unsupported",
            "capability_id": null,
            "confidence": 0.0,
            "alternatives": [],
            "reason": "off topic",
        }));
        let llm = shared(llm);
        let out = LlmReranker::new(Some(&llm))
            .rerank("weather report", &[ev("savings_deposit_total", 0.4)])
            .await;
        assert_eq!(out.decision, RerankerVerdict::Unsupported);
    }

    #[tokio::test]
    async fn schema_invalid_response_retries_once_then_fails_operationally() {
        let llm = FakeLlmClient::default();
        // first response: bogus shape → schema mismatch
        llm.push_structured(json!({"nope": "bad"}));
        llm.push_structured(json!({"still": "bad"}));
        let llm = shared(llm);
        let out = LlmReranker::new(Some(&llm))
            .rerank("anything", &[ev("a", 0.5), ev("b", 0.4)])
            .await;
        assert_eq!(out.decision, RerankerVerdict::FailedOperational);
        assert!(out.alternatives.is_empty());
    }

    #[tokio::test]
    async fn no_llm_is_an_operational_failure_not_a_clarification() {
        let out = LlmReranker::new(None)
            .rerank("q", &[ev("a", 0.9), ev("b", 0.5)])
            .await;
        assert_eq!(out.decision, RerankerVerdict::FailedOperational);
    }
}
