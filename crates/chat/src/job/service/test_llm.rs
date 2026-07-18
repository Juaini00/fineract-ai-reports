use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::assistant::{
    AssistantDomain, AssistantEntity, AssistantEntityType, AssistantIntentKind, AssistantLanguage,
    ContextReference, Quantity, RequestGrouping, RequestOperation, RequestOutput, RequestPii,
    RequestShape, RequestSubject,
    llm::{EmbeddingResponse, LlmClient, LlmPurpose, LlmResponse, TokenUsage},
};

pub(super) struct TestLlmClient;

#[async_trait]
impl LlmClient for TestLlmClient {
    async fn structured_value(
        &self,
        _purpose: LlmPurpose,
        _system: &str,
        user: &str,
        _schema: serde_json::Value,
    ) -> Result<LlmResponse<serde_json::Value>> {
        // Reranker calls carry a `candidates` array; sniff and answer without
        // faking a router intent.
        if let Some(value) = serde_json::from_str::<Value>(user).ok()
            && let Some(candidates) = value.get("candidates").and_then(|c| c.as_array())
            && !candidates.is_empty()
        {
            let query = value
                .get("query")
                .and_then(|q| q.as_str())
                .unwrap_or("")
                .to_lowercase();
            return Ok(LlmResponse {
                value: test_reranker_pick(&query, candidates),
                usage: TokenUsage::default(),
                cost_usd: None,
                provider: "test".into(),
                model: "test".into(),
                latency_ms: 0,
            });
        }
        let message = serde_json::from_str::<serde_json::Value>(user)
            .ok()
            .and_then(|value| {
                value
                    .get("message")
                    .and_then(|message| message.as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| user.to_owned());
        let lower = message.to_lowercase();
        let (intent, domain) = if lower == "hi" || lower == "hello" {
            (AssistantIntentKind::Greeting, AssistantDomain::Unknown)
        } else if lower.contains("bisa apa") || lower.contains("help") {
            (AssistantIntentKind::Help, AssistantDomain::Unknown)
        } else if lower.contains("laptop") {
            (AssistantIntentKind::OutOfDomain, AssistantDomain::Unknown)
        } else if lower.contains("loan")
            || lower.contains("charges")
            || lower.contains("fees")
            || lower.contains("tax")
            || lower.contains("accounting")
            || lower.contains("journal")
            || lower.contains(" gl ")
        {
            (
                AssistantIntentKind::UnsupportedInDomain,
                AssistantDomain::Unknown,
            )
        } else if lower.contains("raw account") {
            (AssistantIntentKind::UnsafeRequest, AssistantDomain::Client)
        } else if lower.contains("office") || lower.contains("organization") {
            (
                AssistantIntentKind::ReportRequest,
                AssistantDomain::Organization,
            )
        } else if lower.contains("balance") || lower.contains("yang") {
            (
                AssistantIntentKind::ClarificationReply,
                AssistantDomain::Client,
            )
        } else if lower.contains("tony") || lower.contains("nama") {
            (AssistantIntentKind::DataLookup, AssistantDomain::Client)
        } else if lower.contains("client") {
            (AssistantIntentKind::ReportRequest, AssistantDomain::Client)
        } else {
            (AssistantIntentKind::ReportRequest, AssistantDomain::Savings)
        };
        let mut entities = Vec::new();
        if lower.contains("tony") {
            entities.push(AssistantEntity {
                entity_type: AssistantEntityType::PersonName,
                value: "Tony".into(),
                canonical: Some("Tony".into()),
                confidence: Some(1.0),
            });
        }
        if lower.contains("account count") || lower.contains("savings accounts") {
            entities.push(AssistantEntity {
                entity_type: AssistantEntityType::Metric,
                value: "savings account count".into(),
                canonical: Some("savings account count".into()),
                confidence: Some(1.0),
            });
        } else if lower.contains("balance") {
            entities.push(AssistantEntity {
                entity_type: AssistantEntityType::Metric,
                value: "savings balance".into(),
                canonical: Some("savings balance".into()),
                confidence: Some(1.0),
            });
        } else if lower.contains("deposit") {
            entities.push(AssistantEntity {
                entity_type: AssistantEntityType::Metric,
                value: "deposit".into(),
                canonical: Some("deposit".into()),
                confidence: Some(1.0),
            });
        } else if lower.contains("withdrawal") {
            entities.push(AssistantEntity {
                entity_type: AssistantEntityType::Metric,
                value: "withdrawal".into(),
                canonical: Some("withdrawal".into()),
                confidence: Some(1.0),
            });
        }
        let quantity = lower
            .split(|ch: char| !ch.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .find_map(|part| part.parse::<i64>().ok())
            .map(|value| Quantity::TopN { value });
        let request_shape = if lower.contains("random") || lower.contains("sembarang") {
            RequestShape {
                operation: RequestOperation::RandomSample,
                subject: RequestSubject::Client,
                grouping: RequestGrouping::None,
                output: RequestOutput::List,
                pii: RequestPii::ClientIdentity,
            }
        } else if lower.contains("hierarchy") {
            RequestShape {
                operation: RequestOperation::Summary,
                subject: RequestSubject::OrganizationHierarchy,
                grouping: RequestGrouping::None,
                output: RequestOutput::Summary,
                pii: RequestPii::None,
            }
        } else if lower.contains("office") || lower.contains("organization") {
            let per_month = lower.contains("monthly")
                || lower.contains("per month")
                || lower.contains("per bulan");
            let ranks = lower.contains("top")
                || lower.contains("ranking")
                || lower.contains("dormant")
                || lower.contains("busiest")
                || lower.contains("list");
            let (operation, grouping, output) = if per_month {
                (
                    RequestOperation::Trend,
                    RequestGrouping::Month,
                    RequestOutput::TimeSeries,
                )
            } else if ranks {
                (
                    RequestOperation::Rank,
                    RequestGrouping::Office,
                    RequestOutput::Ranking,
                )
            } else {
                (
                    RequestOperation::Summary,
                    RequestGrouping::None,
                    RequestOutput::Summary,
                )
            };
            RequestShape {
                operation,
                subject: RequestSubject::Office,
                grouping,
                output,
                pii: RequestPii::None,
            }
        } else if lower.contains("tony") || lower.contains("nama") {
            RequestShape {
                operation: RequestOperation::Lookup,
                subject: RequestSubject::Client,
                grouping: RequestGrouping::None,
                output: RequestOutput::Lookup,
                pii: RequestPii::ClientIdentity,
            }
        } else if lower.contains("client") && (lower.contains("top") || lower.contains("most")) {
            RequestShape {
                operation: RequestOperation::Rank,
                subject: RequestSubject::Client,
                grouping: RequestGrouping::None,
                output: RequestOutput::Ranking,
                pii: RequestPii::ClientIdentity,
            }
        } else if lower.contains("saving")
            || lower.contains("deposit")
            || lower.contains("withdrawal")
        {
            let per_month = lower.contains("monthly")
                || lower.contains("per month")
                || lower.contains("per bulan");
            let top = lower.contains("top") || lower.contains("teratas");
            let total = lower.contains("total") && !top;
            let portfolio = lower.contains("portfolio") || lower.contains("balance summary");
            let (operation, grouping, output, subject) = if portfolio {
                (
                    RequestOperation::Summary,
                    RequestGrouping::None,
                    RequestOutput::Summary,
                    RequestSubject::SavingsAccount,
                )
            } else if per_month && top {
                (
                    RequestOperation::Rank,
                    RequestGrouping::Month,
                    RequestOutput::Ranking,
                    RequestSubject::SavingsTransaction,
                )
            } else if per_month {
                (
                    RequestOperation::Trend,
                    RequestGrouping::Month,
                    RequestOutput::TimeSeries,
                    RequestSubject::SavingsTransaction,
                )
            } else if top {
                (
                    RequestOperation::Rank,
                    RequestGrouping::None,
                    RequestOutput::Ranking,
                    RequestSubject::SavingsTransaction,
                )
            } else if total {
                (
                    RequestOperation::Total,
                    RequestGrouping::None,
                    RequestOutput::Scalar,
                    RequestSubject::SavingsTransaction,
                )
            } else {
                (
                    RequestOperation::Unknown,
                    RequestGrouping::Unknown,
                    RequestOutput::Unknown,
                    RequestSubject::Unknown,
                )
            };
            RequestShape {
                operation,
                subject,
                grouping,
                output,
                pii: RequestPii::Unknown,
            }
        } else {
            RequestShape::default()
        };
        Ok(LlmResponse {
            value: json!({
                "intent": intent,
                "domain": domain,
                "request_shape": request_shape,
                "language": AssistantLanguage::En,
                "entities": entities,
                "constraints": { "quantity": quantity },
                "context_reference": ContextReference::None,
                "confidence": 0.9,
                "reason": "test semantic router"
            }),
            usage: TokenUsage::default(),
            cost_usd: None,
            provider: "test".into(),
            model: "test".into(),
            latency_ms: 0,
        })
    }

    async fn embed(&self, _purpose: LlmPurpose, text: &str) -> Result<EmbeddingResponse> {
        let text = text.to_lowercase();
        Ok(EmbeddingResponse {
            vector: vec![
                text.matches("client").count() as f32 + text.matches("tony").count() as f32,
                text.matches("saving").count() as f32,
                text.matches("balance").count() as f32,
                text.matches("deposit").count() as f32,
            ],
            usage: TokenUsage::default(),
            cost_usd: None,
            provider: "test".into(),
            model: "test".into(),
            latency_ms: 0,
        })
    }
}

/// Test-only reranker heuristic: pick the candidate whose id/title/description
/// shares the most alphanumeric tokens with the query, tie-broken by original
/// retrieval score. High-margin winner → Select at confidence 0.9. Otherwise
/// Clarify with the top-4 candidates as alternatives. Mirrors the semantic
/// picks a real LLM would make well enough for integration tests.
fn test_reranker_pick(query: &str, candidates: &[Value]) -> Value {
    // 4-char prefix substring matching: handles simple plurals/inflections
    // ("deposit" ↔ "deposits", "month" ↔ "monthly") without a stemmer.
    let query_probes: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 3)
        .map(|s| {
            let lower = s.to_lowercase();
            if lower.len() > 4 {
                lower[..4].to_string()
            } else {
                lower
            }
        })
        .collect();
    let mut ordered: Vec<(usize, usize, &Value)> = candidates
        .iter()
        .enumerate()
        .map(|(idx, c)| {
            let examples = c
                .get("examples")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| e.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let hay = format!(
                "{} {} {} {}",
                c.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                c.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                c.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                examples,
            )
            .to_lowercase();
            let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
            let hits = query_probes
                .iter()
                .filter(|probe| seen.insert(probe.as_str()) && hay.contains(probe.as_str()))
                .count();
            (hits, idx, c)
        })
        .collect();
    // Specificity mismatch penalty: candidate id claims a grouping the query
    // never asked for (a "monthly" cap when the query lacks any monthly cue).
    // Cheap tie-breaker that mimics what a real LLM would penalize.
    let query_lower = query.to_lowercase();
    let query_wants_monthly = query_lower.contains("month") || query_lower.contains("per ");
    for entry in ordered.iter_mut() {
        let id = entry
            .2
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        if !query_wants_monthly && id.contains("monthly") && entry.0 > 0 {
            entry.0 -= 1;
        }
    }
    ordered.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let top_hits = ordered.first().map(|(h, _, _)| *h).unwrap_or(0);
    let next_hits = ordered.get(1).map(|(h, _, _)| *h).unwrap_or(0);
    let winner = ordered.first().and_then(|(_, _, c)| c.get("id"));

    if top_hits >= 3 && top_hits > next_hits {
        json!({
            "decision": "select",
            "capability_id": winner,
            "confidence": 0.9,
            "alternatives": [],
            "reason": "test reranker: dominant keyword match",
        })
    } else {
        // ponytail: 6 (not 4) so canonical siblings that alphabetically
        // sort late (e.g. `savings_deposit_total` follows `_top_n`) still
        // land in test clarification options. Real reranker returns 2-4.
        let alternatives: Vec<Value> = ordered
            .iter()
            .take(6)
            .filter_map(|(_, _, c)| c.get("id").cloned())
            .collect();
        json!({
            "decision": "clarify",
            "capability_id": null,
            "confidence": 0.0,
            "alternatives": alternatives,
            "reason": "test reranker: ambiguous top-1",
        })
    }
}
