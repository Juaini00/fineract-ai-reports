mod common;

use std::{fs, path::Path, sync::Arc};

use chat::assistant::{
    FakeLlmClient, LlmPurpose, LlmTraceRepository,
    llm::{
        LlmClient, structured,
        traced_client::{LlmTraceContext, TracedLlmClient},
    },
};
use common::spawn_app;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct TestShape {
    ok: bool,
}

async fn insert_job(app: &common::TestApp, api_key_id: Uuid) -> (Uuid, Uuid) {
    let session_id = Uuid::new_v4();
    let job_id = Uuid::new_v4();
    sqlx::query("INSERT INTO chat_sessions (id, api_key_id, status) VALUES ($1,$2,'active')")
        .bind(session_id)
        .bind(api_key_id)
        .execute(&app.app_pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO chat_jobs (id, session_id, api_key_id, status, current_step, message, expires_at) VALUES ($1,$2,$3,'running','route_intent','test', now() + interval '1 hour')",
    )
    .bind(job_id)
    .bind(session_id)
    .bind(api_key_id)
    .execute(&app.app_pool)
    .await
    .unwrap();
    (session_id, job_id)
}

fn traced(
    fake: Arc<FakeLlmClient>,
    repo: LlmTraceRepository,
    api_key_id: Uuid,
    session_id: Uuid,
    job_id: Uuid,
) -> TracedLlmClient {
    TracedLlmClient::new(
        fake,
        repo,
        Some(LlmTraceContext {
            job_id: Some(job_id),
            session_id: Some(session_id),
            api_key_id,
            graph_state: Some("test".into()),
        }),
    )
}

#[tokio::test]
async fn traced_client_records_structured_embed_errors_and_costs() {
    let app = spawn_app().await;
    let key = app.provision_wildcard_api_key(false).await;
    let (session_id, job_id) = insert_job(&app, key.id).await;
    let repo = LlmTraceRepository::new(app.app_pool.clone());

    let fake = Arc::new(FakeLlmClient::new("openai", "gpt-4o-mini"));
    fake.push_structured(json!({"ok": true}));
    fake.push_embedding(vec![0.1, 0.2]);
    fake.push_structured_error("malformed structured LLM JSON: nope");
    fake.push_structured_error("request timeout");
    fake.push_embedding_error("provider exploded");
    let client = traced(fake, repo.clone(), key.id, session_id, job_id);

    client
        .structured_value(LlmPurpose::RouteIntent, "system", "user", json!({}))
        .await
        .unwrap();
    client
        .embed(LlmPurpose::RouteEmbedding, "embed me")
        .await
        .unwrap();
    assert!(
        client
            .structured_value(
                LlmPurpose::ClarificationResolve,
                "system",
                "user",
                json!({})
            )
            .await
            .is_err()
    );
    assert!(
        client
            .structured_value(LlmPurpose::ResponseBuild, "system", "user", json!({}))
            .await
            .is_err()
    );
    assert!(
        client
            .embed(LlmPurpose::EvidenceRetrieval, "x")
            .await
            .is_err()
    );

    let traces = repo.list_for_job(job_id).await.unwrap();
    assert_eq!(traces.len(), 5);
    assert_eq!(
        repo.list_for_job_filtered(job_id, Some("route_intent"), Some("ok"))
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        repo.list_for_job_filtered(job_id, Some("route_embedding"), Some("ok"))
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        repo.list_for_job_filtered(job_id, Some("clarification_resolve"), Some("malformed"))
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        repo.list_for_job_filtered(job_id, Some("response_build"), Some("timeout"))
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        repo.list_for_job_filtered(job_id, Some("evidence_retrieval"), Some("error"))
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        traces
            .iter()
            .any(|trace| trace.purpose == "route_intent" && trace.cost_usd.is_some())
    );

    let missing = Arc::new(FakeLlmClient::new("missing", "missing"));
    missing.push_structured(json!({"ok": true}));
    let client = traced(missing, repo.clone(), key.id, session_id, job_id);
    client
        .structured_value(LlmPurpose::Test, "system", "user", json!({}))
        .await
        .unwrap();
    let missing_trace = repo
        .list_for_job_filtered(job_id, Some("test"), Some("ok"))
        .await
        .unwrap();
    assert!(missing_trace[0].cost_usd.is_none());

    let bad_shape = Arc::new(FakeLlmClient::new("openai", "gpt-4o-mini"));
    bad_shape.push_structured(json!({"not_ok": true}));
    let client = traced(bad_shape, repo.clone(), key.id, session_id, job_id);
    assert!(
        structured::<TestShape>(&client, LlmPurpose::Test, "system", "user")
            .await
            .is_err()
    );
    assert_eq!(
        repo.list_for_job_filtered(job_id, Some("test"), Some("malformed"))
            .await
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn provider_transport_is_quarantined_to_llm_module() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assistant");
    let mut offenders = Vec::new();
    scan(&root, &mut offenders);
    assert!(
        offenders.is_empty(),
        "direct provider calls outside llm module: {offenders:?}"
    );
}

fn scan(path: &Path, offenders: &mut Vec<String>) {
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().unwrap() != "llm" {
                scan(&path, offenders);
            }
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") || path.ends_with("llm.rs") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        if ["reqwest", "chat_completions_url", "bearer_auth"]
            .iter()
            .any(|needle| text.contains(needle))
        {
            offenders.push(path.display().to_string());
        }
    }
}
