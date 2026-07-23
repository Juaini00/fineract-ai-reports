mod common;

use std::{fs, path::Path, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use chat::assistant::{
    FakeLlmClient, LlmPurpose, LlmTraceRepository,
    llm::{
        EmbeddingResponse, LlmClient, LlmResponse, TokenUsage, structured,
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

async fn insert_job(app: &common::TestApp, user_id: Uuid) -> (Uuid, Uuid) {
    let session_id = Uuid::new_v4();
    let job_id = Uuid::new_v4();
    sqlx::query("INSERT INTO chat_sessions (id, user_id, status) VALUES ($1,$2,'active')")
        .bind(session_id)
        .bind(user_id)
        .execute(&app.app_pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO chat_jobs (id, session_id, user_id, status, current_step, message, expires_at) VALUES ($1,$2,$3,'running','route_intent','test', now() + interval '1 hour')",
    )
    .bind(job_id)
    .bind(session_id)
    .bind(user_id)
    .execute(&app.app_pool)
    .await
    .unwrap();
    (session_id, job_id)
}

fn traced(
    fake: Arc<dyn LlmClient>,
    repo: LlmTraceRepository,
    user_id: Uuid,
    session_id: Uuid,
    job_id: Uuid,
) -> TracedLlmClient {
    TracedLlmClient::new(
        fake,
        repo,
        Some(LlmTraceContext {
            job_id: Some(job_id),
            session_id: Some(session_id),
            user_id,
            legacy_api_key_id: None,
            graph_state: Some("test".into()),
            correlation_id: Some(Uuid::new_v4()),
            context_contract_version: Some(1),
            catalog_version_id: Some(Uuid::new_v4()),
            index_version_id: Some(Uuid::new_v4()),
        }),
    )
}

struct MissingUsageLlmClient;

#[async_trait]
impl LlmClient for MissingUsageLlmClient {
    async fn structured_value(
        &self,
        _purpose: LlmPurpose,
        _system: &str,
        _user: &str,
        _schema: serde_json::Value,
    ) -> Result<LlmResponse<serde_json::Value>> {
        Ok(LlmResponse {
            value: json!({"ok": true}),
            usage: TokenUsage::default(),
            cost_usd: None,
            provider: "test".into(),
            model: "missing-usage".into(),
            latency_ms: 1,
        })
    }

    async fn embed(&self, _purpose: LlmPurpose, _text: &str) -> Result<EmbeddingResponse> {
        unreachable!("not used by this test")
    }
}

#[tokio::test]
async fn traced_client_records_structured_embed_errors_and_costs() {
    let app = spawn_app().await;
    let user_id = app.admin_user_id().await;
    let (session_id, job_id) = insert_job(&app, user_id).await;
    let repo = LlmTraceRepository::new(app.app_pool.clone());

    let fake = Arc::new(FakeLlmClient::new("openai", "gpt-4o-mini"));
    fake.push_structured(json!({"ok": true}));
    fake.push_embedding(vec![0.1, 0.2]);
    fake.push_structured_error("malformed structured LLM JSON: nope");
    fake.push_structured_error("request timeout");
    fake.push_embedding_error("provider exploded");
    let client = traced(fake, repo.clone(), user_id, session_id, job_id);

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
    assert!(traces.iter().all(|trace| trace.user_id == Some(user_id)));
    assert!(traces.iter().all(|trace| trace.legacy_api_key_id.is_none()));
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
    assert!(traces.iter().all(|trace| trace.correlation_id.is_some()));
    assert!(
        traces
            .iter()
            .all(|trace| trace.context_contract_version == Some(1))
    );
    assert!(
        traces
            .iter()
            .all(|trace| trace.catalog_version_id.is_some())
    );
    assert!(traces.iter().all(|trace| trace.index_version_id.is_some()));
    assert!(traces.iter().any(|trace| {
        trace.purpose == "route_intent"
            && trace.price_version.as_deref() == Some("static_config_v1")
            && trace.cost_currency.as_deref() == Some("USD")
    }));
    assert!(traces.iter().any(|trace| {
        trace.purpose == "clarification_resolve"
            && trace.error_code.as_deref() == Some("provider_malformed")
    }));
    assert!(traces.iter().any(|trace| {
        trace.purpose == "response_build" && trace.error_code.as_deref() == Some("provider_timeout")
    }));
    assert!(traces.iter().any(|trace| {
        trace.purpose == "evidence_retrieval"
            && trace.error_code.is_none()
            && trace.input_tokens.is_none()
            && trace.output_tokens.is_none()
            && trace.total_tokens.is_none()
            && trace.usage_status == "unavailable"
    }));
    assert!(traces.iter().any(|trace| {
        trace.purpose == "route_intent"
            && trace.input_tokens == Some(10)
            && trace.output_tokens == Some(5)
            && trace.total_tokens == Some(15)
            && trace.usage_status == "provider_reported"
    }));

    let missing = Arc::new(FakeLlmClient::new("missing", "missing"));
    missing.push_structured(json!({"ok": true}));
    let client = traced(missing, repo.clone(), user_id, session_id, job_id);
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
    let client = traced(bad_shape, repo.clone(), user_id, session_id, job_id);
    assert!(
        structured::<TestShape>(&client, LlmPurpose::Test, "system", "user", None)
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

#[tokio::test]
async fn traced_client_records_missing_provider_usage_as_unavailable() {
    let app = spawn_app().await;
    let user_id = app.admin_user_id().await;
    let (session_id, job_id) = insert_job(&app, user_id).await;
    let repo = LlmTraceRepository::new(app.app_pool.clone());
    let client = traced(
        Arc::new(MissingUsageLlmClient),
        repo.clone(),
        user_id,
        session_id,
        job_id,
    );

    client
        .structured_value(LlmPurpose::Test, "system", "user", json!({}))
        .await
        .unwrap();

    let traces = repo.list_for_job(job_id).await.unwrap();
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].usage_status, "unavailable");
    assert_eq!(traces[0].input_tokens, None);
    assert_eq!(traces[0].output_tokens, None);
    assert_eq!(traces[0].total_tokens, None);
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
