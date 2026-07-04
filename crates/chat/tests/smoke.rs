//! Smoke test: harness boots, app serves `/health`, migrations ran.
//! If this passes, every other integration test can rely on `spawn_app`.

mod common;

use common::spawn_app;

#[tokio::test(flavor = "multi_thread")]
async fn health_returns_ok() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let resp = app.get("/health", None).await;

    // Assert
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

#[tokio::test(flavor = "multi_thread")]
async fn ready_reports_dependency_status() {
    // Scenario 01: /ready checks app_database, fineract_database, pgvector,
    // and redis. Redis is disabled in the harness → status "disabled" but
    // overall still "ready".
    let app = spawn_app().await;

    let resp = app.get("/ready", None).await;

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ready");
    assert_eq!(body["checks"]["app_database"], "ok");
    assert_eq!(body["checks"]["fineract_database"], "ok");
    assert_eq!(body["checks"]["pgvector"], "ok");
    assert_eq!(body["checks"]["redis"], "disabled");
}

#[tokio::test(flavor = "multi_thread")]
async fn migrations_created_expected_tables() {
    let app = spawn_app().await;

    for table in [
        "api_keys",
        "chat_sessions",
        "chat_messages",
        "chat_jobs",
        "chat_job_events",
        "chat_job_checkpoints",
    ] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = $1)",
        )
        .bind(table)
        .fetch_one(&app.app_pool)
        .await
        .expect("query information_schema");
        assert!(exists, "table {table} missing after migrations");
    }
}
