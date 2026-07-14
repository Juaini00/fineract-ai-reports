mod common;

use chat::assistant::{
    ClarificationOption, ClarificationPayload, GraphState, GraphTransition, JobMemoryRepository,
    LlmTrace, LlmTraceRepository, SessionMemoryRepository,
};
use common::spawn_app;
use serde_json::json;
use uuid::Uuid;

async fn insert_session_and_job(app: &common::TestApp, api_key_id: Uuid) -> (Uuid, Uuid) {
    let session_id = Uuid::new_v4();
    let job_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO chat_sessions (id, api_key_id, status)
        VALUES ($1, $2, 'active')
        "#,
    )
    .bind(session_id)
    .bind(api_key_id)
    .execute(&app.app_pool)
    .await
    .expect("insert chat session");
    sqlx::query(
        r#"
        INSERT INTO chat_jobs (id, session_id, api_key_id, status, current_step, message, expires_at)
        VALUES ($1, $2, $3, 'queued', 'queued', 'show savings', now() + interval '1 hour')
        "#,
    )
    .bind(job_id)
    .bind(session_id)
    .bind(api_key_id)
    .execute(&app.app_pool)
    .await
    .expect("insert chat job");
    (session_id, job_id)
}

#[tokio::test]
async fn job_memory_create_read_update_and_revision_conflict() {
    let app = spawn_app().await;
    let api_key = app.provision_wildcard_api_key(false).await;
    let (_session_id, job_id) = insert_session_and_job(&app, api_key.id).await;
    let repo = JobMemoryRepository::new(app.app_pool.clone());

    let mut memory = repo.create(job_id, "receive_message").await.unwrap();
    memory.current_user_message_metadata = json!({ "message_id": "m1" });
    memory.source_intent = Some(json!({ "domain": "savings" }));
    memory.retrieval_plan = json!({ "strategy": "capability" });
    memory.retrieval_evidence = json!({ "hits": ["savings_deposit_total"] });
    memory.evidence_decision = json!({ "accepted": true });
    memory.selected_capability = Some("savings_deposit_total".into());
    memory.selected_tool = Some("sql".into());
    memory.tool_params = json!({ "office_ids": [1] });
    memory.policy_decision = json!({ "allowed": true });
    memory.execution_summary = json!({ "rows": 1 });
    memory.warnings = json!(["none"]);

    let saved = repo.save(&memory, 0).await.unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(saved.source_intent, Some(json!({ "domain": "savings" })));
    assert_eq!(saved.tool_params["office_ids"], json!([1]));
    assert!(repo.save(&memory, 0).await.is_err());

    let read = repo.get(job_id).await.unwrap().unwrap();
    assert_eq!(
        read.selected_capability.as_deref(),
        Some("savings_deposit_total")
    );
}

#[tokio::test]
async fn session_memory_update_pending_source_and_revision_conflict() {
    let app = spawn_app().await;
    let api_key = app.provision_wildcard_api_key(false).await;
    let (session_id, _job_id) = insert_session_and_job(&app, api_key.id).await;
    let repo = SessionMemoryRepository::new(app.app_pool.clone());

    let mut memory = repo.get_or_create(session_id).await.unwrap();
    memory.summary = Some("last answer".into());
    memory.active_domain = Some("savings".into());
    memory.pending_clarification = Some(ClarificationPayload {
        question: "Which savings report?".into(),
        options: vec![ClarificationOption {
            id: "savings_deposit_total".into(),
            label: "Savings total".into(),
            description: None,
        }],
        attempt: 1,
        source_intent: None,
        allow_free_text: true,
    });
    memory.pending_clarification_source_intent = Some(json!({ "domain": "savings" }));
    memory.entities = json!([{ "type": "office", "id": 1 }]);
    memory.relevant_jobs = json!([{ "job_id": "j1", "summary": "prior" }]);
    memory.context_warnings = json!([{ "code": "session_context_near_limit" }]);

    let saved = repo.save(&memory, 0).await.unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(saved.relevant_jobs[0]["summary"], "prior");
    assert!(repo.save(&memory, 0).await.is_err());

    let pending = saved.pending_clarification.as_ref().unwrap();
    let updated = repo
        .set_pending_clarification_with_source_intent(
            session_id,
            Some(pending),
            Some(&json!({ "kind": "follow_up" })),
        )
        .await
        .unwrap();
    assert_eq!(
        updated.pending_clarification_source_intent,
        Some(json!({ "kind": "follow_up" }))
    );
}

#[tokio::test]
async fn checkpoint_transition_and_llm_trace_readback() {
    let app = spawn_app().await;
    let api_key = app.provision_wildcard_api_key(false).await;
    let (session_id, job_id) = insert_session_and_job(&app, api_key.id).await;
    let job_repo = JobMemoryRepository::new(app.app_pool.clone());
    let trace_repo = LlmTraceRepository::new(app.app_pool.clone());
    let memory = job_repo.create(job_id, "route_intent").await.unwrap();

    job_repo
        .checkpoint_transition(
            job_id,
            &GraphTransition {
                from: GraphState::RouteIntent,
                to: Some(GraphState::PlanRetrieval),
                terminal: None,
                reason: "routed".into(),
            },
            memory.revision,
            json!({ "event": "metadata" }),
        )
        .await
        .unwrap();
    let checkpoints = job_repo.list_latest_checkpoints(job_id, 1).await.unwrap();
    assert_eq!(
        checkpoints[0].previous_state.as_deref(),
        Some("route_intent")
    );
    assert_eq!(checkpoints[0].current_state, "plan_retrieval");
    assert_eq!(checkpoints[0].terminal_state, None);
    assert_eq!(checkpoints[0].transition_reason.as_deref(), Some("routed"));
    assert_eq!(checkpoints[0].event_metadata_json["event"], "metadata");

    trace_repo
        .record(&LlmTrace {
            job_id: Some(job_id),
            session_id: Some(session_id),
            api_key_id: api_key.id,
            graph_state: Some("route_intent".into()),
            purpose: "route_intent".into(),
            provider: "test".into(),
            model: "tiny".into(),
            input_tokens: 10,
            output_tokens: 7,
            cost_usd: Some(0.0123),
            latency_ms: 42,
            status: "ok".into(),
            error_kind: None,
        })
        .await
        .unwrap();
    let traces = trace_repo.list_for_job(job_id).await.unwrap();
    assert_eq!(traces[0].total_tokens, 17);
    assert_eq!(traces[0].cost_usd.unwrap().to_string(), "0.012300");
    assert_eq!(traces[0].status, "ok");
}
