use chat::assistant::{ContextWarningCode, ContextWindow, RelevantJobSummary};
use serde_json::json;

#[test]
fn context_window_carries_phase6_fields() {
    let window: ContextWindow = serde_json::from_value(json!({
        "summary": "prior conversation",
        "selected_entities": {},
        "recent_messages": [],
        "relevant_jobs": [{"job_id":"j1","summary":"prior report","retrieval_plan":{"query_text":"x"},"evidence_decision":{"decision":"clarify"}}],
        "source_intent": {"domain":"savings"},
        "client_scope": {},
        "warnings": [{"code":"session_context_near_limit","message":"soft"}]
    })).unwrap();
    assert_eq!(window.source_intent.unwrap()["domain"], "savings");
    assert_eq!(window.relevant_jobs[0].summary, "prior report");
    assert_eq!(
        window.warnings[0].code,
        ContextWarningCode::SessionContextNearLimit
    );
}

#[test]
fn hard_exceeded_warning_is_visible() {
    let window = ContextWindow {
        summary: None,
        active_domain: None,
        selected_entities: json!({}),
        recent_messages: vec![],
        relevant_jobs: vec![RelevantJobSummary {
            job_id: "j".into(),
            session_id: None,
            domain: None,
            intent: None,
            created_at: None,
            summary: "x".into(),
            retrieval_plan: json!({}),
            evidence_decision: json!({}),
            evidence_refs: vec![],
        }],
        pending_clarification: None,
        source_intent: None,
        source_snippets: vec![],
        client_scope: json!({}),
        warnings: vec![chat::assistant::ContextWarning {
            code: ContextWarningCode::SessionContextExceeded,
            message: "hard".into(),
        }],
    };
    assert!(
        window
            .warnings
            .iter()
            .any(|w| w.code == ContextWarningCode::SessionContextExceeded)
    );
}
