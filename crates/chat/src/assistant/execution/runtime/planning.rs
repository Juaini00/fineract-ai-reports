use super::*;

/// Builds a JSON audit trace of one retrieval pass for `state_json.retrieval_trace`.
/// Best-effort/debug-only shape — not part of the graph contract, so it is built
/// inline at the call site rather than added as a `JobMemory` field.
pub fn build_retrieval_trace(
    intent: &AssistantIntent,
    plan: &crate::assistant::evidence::RetrievalPlan,
    evidence: &[crate::assistant::evidence::Evidence],
    decision: &RerankerDecision,
) -> serde_json::Value {
    let candidates: Vec<_> = evidence
        .iter()
        .take(10)
        .map(|e| {
            json!({
                "capability_id": e.capability_id,
                "title": e.title,
                "score": e.score,
                "source_type": e.source_type,
            })
        })
        .collect();

    let kind = match decision.decision {
        RerankerVerdict::Select => "select",
        RerankerVerdict::Clarify => "clarify",
        RerankerVerdict::Unsupported => "unsupported",
        RerankerVerdict::FailedOperational => "failed_operational",
    };
    let decision_json = json!({
        "kind": kind,
        "capability_id": decision.capability_id,
        "confidence": decision.confidence,
        "alternatives": decision.alternatives,
        "reason": decision.reason,
    });

    json!({
        "router_intent": {
            "intent": intent.intent,
            "domain": intent.domain,
            "request_shape": intent.request_shape,
            "confidence": intent.confidence,
        },
        "plan": {
            "query_text": plan.query_text,
            "allowed_capability_count": plan.allowed_capabilities.len(),
            "allow_all_capabilities": plan.allow_all_capabilities,
        },
        "candidates": candidates,
        "decision": decision_json,
    })
}
