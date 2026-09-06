use sqlx::{AssertSqlSafe, PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

#[tokio::test]
async fn management_audit_events_are_immutable_and_trace_api_keys_are_set_null() {
    let admin_url = std::env::var("TEST_ADMIN_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://root:password@127.0.0.1:5432/postgres".into());
    let database_name = format!("ai_report_management_{}", Uuid::new_v4().simple());
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&admin_url)
        .await
        .expect("connect admin database");
    sqlx::query(AssertSqlSafe(format!(
        "CREATE DATABASE \"{database_name}\""
    )))
    .execute(&admin)
    .await
    .expect("create database");
    let pool = PgPool::connect(&admin_url.replace("/postgres", &format!("/{database_name}")))
        .await
        .expect("connect test database");

    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");

    let event_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO management_audit_events (id, aggregate_type, aggregate_id, event_type, outcome, summary_json, occurred_at) VALUES ($1, 'management', $2, 'chat.job_created', 'success', '{}'::jsonb, now())",
    )
    .bind(event_id)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .expect("insert audit event");
    assert!(
        sqlx::query("UPDATE management_audit_events SET event_type = 'changed' WHERE id = $1")
            .bind(event_id)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM management_audit_events WHERE id = $1")
            .bind(event_id)
            .execute(&pool)
            .await
            .is_err()
    );

    let delete_action: String = sqlx::query_scalar(
        "SELECT confdeltype::text FROM pg_constraint WHERE conname = 'assistant_llm_traces_api_key_id_fkey'",
    )
    .fetch_one(&pool)
    .await
    .expect("trace API-key foreign key");
    assert_eq!(delete_action, "n", "trace API-key deletion must set null");

    let api_key_id = Uuid::new_v4();
    sqlx::query("INSERT INTO api_keys (id, name, owner, key_prefix, key_hash) VALUES ($1, 'trace owner', 'test', 'trace', $2)")
        .bind(api_key_id)
        .bind(format!("hash-{api_key_id}"))
        .execute(&pool)
        .await
        .expect("insert API key");
    let trace_id = Uuid::new_v4();
    sqlx::query("INSERT INTO assistant_llm_traces (id, api_key_id, purpose, provider, model, input_tokens, output_tokens, total_tokens, latency_ms, status) VALUES ($1, $2, 'test', 'test', 'test', 0, 0, 0, 0, 'ok')")
        .bind(trace_id)
        .bind(api_key_id)
        .execute(&pool)
        .await
        .expect("insert API-key-owned trace");
    sqlx::query("DELETE FROM api_keys WHERE id = $1")
        .bind(api_key_id)
        .execute(&pool)
        .await
        .expect("delete API key without deleting trace");
    let retained: (Option<Uuid>, Option<Uuid>) = sqlx::query_as(
        "SELECT api_key_id, actor_api_key_id FROM assistant_llm_traces WHERE id = $1",
    )
    .bind(trace_id)
    .fetch_one(&pool)
    .await
    .expect("retained trace");
    assert_eq!(retained, (None, Some(api_key_id)));

    pool.close().await;
    sqlx::query(AssertSqlSafe(format!(
        "DROP DATABASE \"{database_name}\" WITH (FORCE)"
    )))
    .execute(&admin)
    .await
    .expect("drop database");
}
