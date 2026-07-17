mod common;

use chat::assistant::{
    AssistantIntentKind, AssistantLanguage, CanonicalStateRepository,
    CanonicalStateRepositoryError, ClarificationOption, ClarificationPayload, EffectiveConstraints,
    GraphState, GraphTransition, JobMemoryRepository, LlmTrace, LlmTraceRepository, OriginalIntent,
    PlannerInputSnapshot, PrincipalProjection, SessionMemoryRepository,
};
use chrono::Utc;
use common::spawn_app;
use serde_json::json;
use uuid::Uuid;

async fn insert_session_and_job(app: &common::TestApp, user_id: Uuid) -> (Uuid, Uuid) {
    let session_id = Uuid::new_v4();
    let job_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO chat_sessions (id, user_id, status)
        VALUES ($1, $2, 'active')
        "#,
    )
    .bind(session_id)
    .bind(user_id)
    .execute(&app.app_pool)
    .await
    .expect("insert chat session");
    sqlx::query(
        r#"
        INSERT INTO chat_jobs (id, session_id, user_id, status, current_step, message, expires_at)
        VALUES ($1, $2, $3, 'queued', 'queued', 'show savings', now() + interval '1 hour')
        "#,
    )
    .bind(job_id)
    .bind(session_id)
    .bind(user_id)
    .execute(&app.app_pool)
    .await
    .expect("insert chat job");
    (session_id, job_id)
}

#[tokio::test]
async fn job_memory_create_read_update_and_revision_conflict() {
    let app = spawn_app().await;
    let user_id = app.admin_user_id().await;
    let (_session_id, job_id) = insert_session_and_job(&app, user_id).await;
    let repo = JobMemoryRepository::new(app.app_pool.clone());

    let mut memory = repo
        .create(job_id, user_id, "receive_message")
        .await
        .unwrap();
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

    let read = repo.get(job_id, user_id).await.unwrap().unwrap();
    assert_eq!(
        read.selected_capability.as_deref(),
        Some("savings_deposit_total")
    );
}

#[tokio::test]
async fn session_memory_update_pending_source_and_revision_conflict() {
    let app = spawn_app().await;
    let user_id = app.admin_user_id().await;
    let (session_id, _job_id) = insert_session_and_job(&app, user_id).await;
    let repo = SessionMemoryRepository::new(app.app_pool.clone());

    let mut memory = repo.get_or_create(session_id, user_id).await.unwrap();
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
        is_missing_execution_parameters: false,
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
    let user_id = app.admin_user_id().await;
    let (session_id, job_id) = insert_session_and_job(&app, user_id).await;
    let job_repo = JobMemoryRepository::new(app.app_pool.clone());
    let trace_repo = LlmTraceRepository::new(app.app_pool.clone());
    let memory = job_repo
        .create(job_id, user_id, "route_intent")
        .await
        .unwrap();

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
            user_id,
            legacy_api_key_id: Some(api_key.id),
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
    assert_eq!(traces[0].user_id, Some(user_id));
    assert_eq!(traces[0].legacy_api_key_id, Some(api_key.id));
    assert_eq!(traces[0].cost_usd.unwrap().to_string(), "0.012300");
    assert_eq!(traces[0].status, "ok");
}

#[tokio::test]
async fn canonical_state_schema_accepts_valid_rows_and_rejects_invalid_relationships() {
    let app = spawn_app().await;
    let user_id = app.admin_user_id().await;
    let (session_id, job_id) = insert_session_and_job(&app, user_id).await;
    let (_, other_job_id) = insert_session_and_job(&app, user_id).await;
    let message_id = Uuid::new_v4();
    let other_message_id = Uuid::new_v4();

    for (id, job) in [(message_id, job_id), (other_message_id, other_job_id)] {
        sqlx::query(
            "INSERT INTO chat_messages (id, session_id, job_id, role, content) VALUES ($1, $2, $3, 'user', 'show savings')",
        )
        .bind(id)
        .bind(session_id)
        .bind(job)
        .execute(&app.app_pool)
        .await
        .unwrap();
    }

    let original_id = Uuid::new_v4();
    sqlx::query("INSERT INTO assistant_original_intents (id, job_id, schema_version, raw_message_id, document_json, extraction_provenance_json) VALUES ($1, $2, 1, $3, $4, $5)")
        .bind(original_id).bind(job_id).bind(message_id)
        .bind(json!({"action":"report"})).bind(json!([]))
        .execute(&app.app_pool).await.unwrap();

    let observation_sql = "INSERT INTO assistant_fact_observations (id, job_id, sequence, source_kind, source_id, field_path, typed_value_json, confidence, extractor_version) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'v1')";
    sqlx::query(observation_sql)
        .bind(Uuid::new_v4())
        .bind(job_id)
        .bind(1_i64)
        .bind("original_request")
        .bind("initial")
        .bind("metric")
        .bind(json!({"string":"deposit_total"}))
        .bind(0.9_f32)
        .execute(&app.app_pool)
        .await
        .unwrap();

    let effective_id = Uuid::new_v4();
    sqlx::query("INSERT INTO assistant_effective_constraints (id, job_id, revision, schema_version, values_json, provenance_json) VALUES ($1, $2, 0, 1, $3, $4)")
        .bind(effective_id).bind(job_id).bind(json!({"metric":"deposit_total"}))
        .bind(json!({"metric":"observation"})).execute(&app.app_pool).await.unwrap();
    let catalog_id = Uuid::new_v4();
    sqlx::query("INSERT INTO knowledge_catalog_versions (id, version, content_hash, status) VALUES ($1, 'test', $2, 'validated')")
        .bind(catalog_id).bind(Uuid::new_v4().to_string())
        .execute(&app.app_pool).await.unwrap();
    sqlx::query("INSERT INTO assistant_planner_input_snapshots (id, job_id, revision, original_intent_id, effective_constraints_id, capability_catalog_version, principal_projection_json, reference_instant, timezone, selected_capability_id, normalized_parameters_json) VALUES ($1, $2, 0, $3, $4, $5, $6, now(), 'UTC', 'savings_deposit_total', $7)")
        .bind(Uuid::new_v4()).bind(job_id).bind(original_id).bind(effective_id)
        .bind(catalog_id).bind(json!({"user_id":user_id})).bind(json!({}))
        .execute(&app.app_pool).await.unwrap();

    assert!(sqlx::query("INSERT INTO assistant_original_intents (id, job_id, schema_version, raw_message_id, document_json, extraction_provenance_json) VALUES ($1, $2, 1, $3, '{}', '[]')")
        .bind(Uuid::new_v4()).bind(other_job_id).bind(message_id)
        .execute(&app.app_pool).await.is_err());
    assert!(sqlx::query("INSERT INTO assistant_original_intents (id, job_id, schema_version, raw_message_id, document_json, extraction_provenance_json) VALUES ($1, $2, 0, $3, '{}', '[]')")
        .bind(Uuid::new_v4()).bind(other_job_id).bind(other_message_id)
        .execute(&app.app_pool).await.is_err());
    for (sequence, kind, source, confidence) in [
        (0_i64, "clarification", "bad-sequence", Some(0.5_f32)),
        (2_i64, "unknown", "bad-kind", Some(0.5_f32)),
        (3, "clarification", "bad-confidence", Some(1.1)),
        (1, "clarification", "duplicate-sequence", None),
        (4, "original_request", "initial", Some(0.9)),
    ] {
        assert!(
            sqlx::query(observation_sql)
                .bind(Uuid::new_v4())
                .bind(job_id)
                .bind(sequence)
                .bind(kind)
                .bind(source)
                .bind("metric")
                .bind(json!({"string":"other"}))
                .bind(confidence)
                .execute(&app.app_pool)
                .await
                .is_err()
        );
    }
    assert!(sqlx::query("INSERT INTO assistant_effective_constraints (id, job_id, revision, schema_version, values_json, provenance_json) VALUES ($1, $2, 0, 1, '{}', '{}')")
        .bind(Uuid::new_v4()).bind(job_id).execute(&app.app_pool).await.is_err());
    assert!(sqlx::query("INSERT INTO assistant_effective_constraints (id, job_id, revision, schema_version, values_json, provenance_json) VALUES ($1, $2, -1, 1, '{}', '{}')")
        .bind(Uuid::new_v4()).bind(other_job_id).execute(&app.app_pool).await.is_err());
    assert!(sqlx::query("INSERT INTO assistant_planner_input_snapshots (id, job_id, revision, original_intent_id, effective_constraints_id, capability_catalog_version, principal_projection_json, reference_instant, timezone, selected_capability_id, normalized_parameters_json) VALUES ($1, $2, 1, $3, $4, $5, '{}', now(), 'UTC', 'x', '{}')")
        .bind(Uuid::new_v4()).bind(other_job_id).bind(original_id).bind(effective_id).bind(catalog_id)
        .execute(&app.app_pool).await.is_err());
    assert!(sqlx::query("INSERT INTO assistant_planner_input_snapshots (id, job_id, revision, original_intent_id, effective_constraints_id, capability_catalog_version, principal_projection_json, reference_instant, timezone, selected_capability_id, normalized_parameters_json) VALUES ($1, $2, 0, $3, $4, $5, '{}', now(), 'UTC', 'x', '{}')")
        .bind(Uuid::new_v4()).bind(job_id).bind(original_id).bind(effective_id).bind(catalog_id)
        .execute(&app.app_pool).await.is_err());

    assert!(matches!(
        CanonicalStateRepository::new(app.app_pool.clone())
            .get_original_intent(job_id)
            .await,
        Err(CanonicalStateRepositoryError::InvalidJson(_))
    ));
}

#[tokio::test]
async fn canonical_original_intent_replay_conflict_and_concurrency() {
    let app = spawn_app().await;
    let user_id = app.admin_user_id().await;
    let (session_id, job_id) = insert_session_and_job(&app, user_id).await;
    let message_id = Uuid::new_v4();
    sqlx::query("INSERT INTO chat_messages (id, session_id, job_id, role, content) VALUES ($1,$2,$3,'user','show savings')")
        .bind(message_id).bind(session_id).bind(job_id)
        .execute(&app.app_pool).await.unwrap();
    let value = OriginalIntent {
        id: Uuid::new_v4(),
        job_id,
        schema_version: 1,
        raw_message_id: message_id,
        locale: AssistantLanguage::En,
        action: AssistantIntentKind::ReportRequest,
        entities: vec![],
        metrics: vec!["balance".into()],
        groupings: vec![],
        output: None,
        parameters: Default::default(),
        pii_request: false,
        extraction_provenance: vec![],
        created_at: Utc::now(),
    };
    let repo = CanonicalStateRepository::new(app.app_pool.clone());
    let (left, right) = tokio::join!(
        repo.insert_original_intent(&value),
        repo.insert_original_intent(&value)
    );
    assert!(left.is_ok() && right.is_ok());
    assert!(repo.insert_original_intent(&value).await.is_ok());

    let mut conflicting = value.clone();
    conflicting.metrics.push("count".into());
    assert!(matches!(
        repo.insert_original_intent(&conflicting).await,
        Err(CanonicalStateRepositoryError::Conflict("original intent"))
    ));
    assert_eq!(
        repo.get_original_intent(job_id)
            .await
            .unwrap()
            .unwrap()
            .metrics,
        vec!["balance"]
    );
}

#[tokio::test]
async fn canonical_snapshot_reload_is_immutable_missing_is_closed_and_revision_is_new() {
    let app = spawn_app().await;
    let user_id = app.admin_user_id().await;
    let (session_id, job_id) = insert_session_and_job(&app, user_id).await;
    let message_id = Uuid::new_v4();
    sqlx::query("INSERT INTO chat_messages (id, session_id, job_id, role, content) VALUES ($1,$2,$3,'user','show savings')")
        .bind(message_id).bind(session_id).bind(job_id).execute(&app.app_pool).await.unwrap();
    let repo = CanonicalStateRepository::new(app.app_pool.clone());
    let original = repo
        .insert_original_intent(&OriginalIntent {
            id: Uuid::new_v4(),
            job_id,
            schema_version: 1,
            raw_message_id: message_id,
            locale: AssistantLanguage::En,
            action: AssistantIntentKind::ReportRequest,
            entities: vec![],
            metrics: vec!["savings.deposit_total".into()],
            groupings: vec![],
            output: None,
            parameters: Default::default(),
            pii_request: false,
            extraction_provenance: vec![],
            created_at: Utc::now(),
        })
        .await
        .unwrap();
    let effective = repo
        .insert_effective_constraints(&EffectiveConstraints {
            id: Uuid::new_v4(),
            job_id,
            revision: 0,
            schema_version: 1,
            values: Default::default(),
            winning_observation_ids: Default::default(),
            created_at: Utc::now(),
        })
        .await
        .unwrap();
    let catalog_id = Uuid::new_v4();
    sqlx::query("INSERT INTO knowledge_catalog_versions (id, version, content_hash, status) VALUES ($1,'snapshot-test',$2,'validated')")
        .bind(catalog_id).bind(Uuid::new_v4().to_string()).execute(&app.app_pool).await.unwrap();
    let principal = PrincipalProjection {
        user_id,
        role: "admin".into(),
        capability_ids: vec!["savings_deposit_total".into()],
        office_ids: vec![1],
        can_view_pii: false,
        legacy_api_key_id: None,
    };
    let first = repo
        .insert_planner_snapshot(&PlannerInputSnapshot {
            id: Uuid::new_v4(),
            job_id,
            revision: 0,
            original_intent_id: original.id,
            effective_constraints_id: effective.id,
            capability_catalog_version: catalog_id,
            principal_projection: principal.clone(),
            reference_instant: Utc::now(),
            timezone: "UTC".into(),
            selected_capability_id: "savings_deposit_total".into(),
            normalized_parameters: json!({"currency_code":"USD"}),
            created_at: Utc::now(),
        })
        .await
        .unwrap();
    let mut mutable_copy = first.clone();
    mutable_copy.selected_capability_id = "changed".into();
    mutable_copy.normalized_parameters = json!({"currency_code":"EUR"});
    let reloaded = repo
        .get_planner_snapshot(first.id, job_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reloaded.selected_capability_id, "savings_deposit_total");
    assert_eq!(
        reloaded.normalized_parameters,
        json!({"currency_code":"USD"})
    );
    assert!(
        repo.get_planner_snapshot(Uuid::new_v4(), job_id)
            .await
            .unwrap()
            .is_none()
    );

    let revised_effective = repo
        .insert_effective_constraints(&EffectiveConstraints {
            id: Uuid::new_v4(),
            revision: 1,
            ..effective
        })
        .await
        .unwrap();
    let second = repo
        .insert_planner_snapshot(&PlannerInputSnapshot {
            id: Uuid::new_v4(),
            revision: 1,
            effective_constraints_id: revised_effective.id,
            normalized_parameters: json!({"currency_code":"EUR"}),
            ..first.clone()
        })
        .await
        .unwrap();
    assert_ne!(first.id, second.id);
    assert_ne!(
        first.effective_constraints_id,
        second.effective_constraints_id
    );
}
