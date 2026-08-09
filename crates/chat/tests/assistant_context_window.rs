mod common;

use app_core::auth::model::PrincipalContext;
use chat::{
    assistant::{
        CLARIFICATION_VERSION_1, ClarificationKind, ClarificationPayload, ContextBuilder,
        ContextWarningCode, ContextWindow, ContextWindowPolicy, RelevantJobSummary,
        SessionMemoryRepository,
    },
    conversation::repository::{MessageRepository, SessionRepository},
};
use common::spawn_app;
use serde_json::json;
use uuid::Uuid;

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

#[tokio::test]
async fn explicit_job_pending_clarification_overrides_session_projection() {
    let app = spawn_app().await;
    let user_id = app.admin_user_id().await;
    let session = SessionRepository::new(app.app_pool.clone())
        .create(user_id, None)
        .await
        .unwrap();
    let session_memory = SessionMemoryRepository::new(app.app_pool.clone());
    session_memory
        .get_or_create(session.id, user_id)
        .await
        .unwrap();
    let session_pending = pending_clarification(Uuid::new_v4());
    session_memory
        .set_pending_clarification(session.id, Some(&session_pending))
        .await
        .unwrap();
    let job_pending = pending_clarification(Uuid::new_v4());
    let builder = ContextBuilder::new(
        MessageRepository::new(app.app_pool.clone()),
        session_memory,
        ContextWindowPolicy::new(100, 200, 10, 10),
    );
    let client = PrincipalContext {
        user_id,
        role: "admin".into(),
        capability_ids: vec![],
        office_ids: vec![],
        can_view_pii: true,
        legacy_api_key_id: None,
    };

    let context = builder
        .build_with_pending(session.id, &client, Some(job_pending.clone()), false)
        .await
        .unwrap();

    assert_eq!(context.pending_clarification, Some(job_pending));
    assert_ne!(context.pending_clarification, Some(session_pending));
}

fn pending_clarification(id: Uuid) -> ClarificationPayload {
    ClarificationPayload {
        version: CLARIFICATION_VERSION_1,
        id,
        revision: 0,
        kind: ClarificationKind::SelectOption,
        question: "Which report?".into(),
        options: vec![],
        fields: vec![],
        attempt: 1,
        source_intent: None,
        allow_free_text: true,
        is_missing_execution_parameters: false,
        workflow_id: None,
        node_id: None,
        resume_node_id: None,
        entity_kind: None,
    }
}
