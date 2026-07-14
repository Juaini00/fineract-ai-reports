use std::sync::Arc;

use anyhow::{Context, ensure};
use app_core::config::AppConfig;
use chat::{
    assistant::{
        AssistantIntentKind, ContextWindow, LlmTraceRepository, SemanticRouter,
        llm::{
            rig_client::RigLlmClient, traced_client::LlmTraceContext,
            traced_client::TracedLlmClient,
        },
    },
    knowledge::catalog::loader::KnowledgeLoader,
};
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions, types::Json};
use uuid::Uuid;

const API_KEY_ID: Uuid = Uuid::from_u128(1);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&config.app_database_url)
        .await
        .context("connect app database")?;

    upsert_test_api_key(&pool).await?;

    let catalog = KnowledgeLoader::new(&config.catalog.path, &config.catalog.query_path).load()?;
    let llm = Arc::new(RigLlmClient::new(&config.llm, Some(&config.embedding))?);
    let traced = Arc::new(TracedLlmClient::new(
        llm,
        LlmTraceRepository::new(pool.clone()),
        Some(LlmTraceContext {
            job_id: None,
            session_id: None,
            api_key_id: API_KEY_ID,
            graph_state: None,
        }),
    ));

    let router = SemanticRouter::new(traced, &catalog);
    let intent = router
        .route("total savings deposit this month", &empty_context())
        .await?;
    ensure!(intent.confidence > 0.0, "router returned zero confidence");
    ensure!(
        intent.intent == AssistantIntentKind::ReportRequest,
        "router returned {:?}",
        intent.intent
    );

    let trace = latest_router_trace(&pool).await?;
    ensure!(trace.count > 0, "router trace was not recorded");
    println!(
        "phase4_router_trace_poc count={} provider={} model={} intent={:?}",
        trace.count, trace.provider, trace.model, intent.intent
    );

    Ok(())
}

fn empty_context() -> ContextWindow {
    ContextWindow {
        summary: None,
        active_domain: None,
        selected_entities: json!({}),
        recent_messages: Vec::new(),
        relevant_jobs: Vec::new(),
        pending_clarification: None,
        source_intent: None,
        source_snippets: Vec::new(),
        client_scope: json!({}),
        warnings: Vec::new(),
    }
}

async fn upsert_test_api_key(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO api_keys (
            id, name, owner, key_prefix, key_hash, allowed_office_ids,
            allowed_capabilities, can_view_pii, allow_all_capabilities, allow_all_offices
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,false,true,true)
        ON CONFLICT (id) DO UPDATE SET
            name = EXCLUDED.name,
            owner = EXCLUDED.owner,
            allow_all_capabilities = true,
            allow_all_offices = true,
            revoked_at = NULL
        "#,
    )
    .bind(API_KEY_ID)
    .bind("Phase 4 router trace POC")
    .bind("phase4-poc")
    .bind("air_poc")
    .bind("phase4-router-trace-poc-hash")
    .bind(Json(json!([])))
    .bind(Json(json!([])))
    .execute(pool)
    .await
    .context("upsert test api key")?;
    Ok(())
}

struct TraceSummary {
    count: i64,
    provider: String,
    model: String,
}

async fn latest_router_trace(pool: &PgPool) -> anyhow::Result<TraceSummary> {
    let row = sqlx::query_as::<_, (i64, String, String)>(
        r#"
        SELECT count(*) OVER () AS count, provider, model
        FROM assistant_llm_traces
        WHERE api_key_id = $1 AND purpose = 'router'
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(API_KEY_ID)
    .fetch_one(pool)
    .await
    .context("read router trace")?;

    Ok(TraceSummary {
        count: row.0,
        provider: row.1,
        model: row.2,
    })
}
