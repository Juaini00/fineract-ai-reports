mod common;

use common::spawn_app;
use reqwest::header;

#[tokio::test(flavor = "multi_thread")]
async fn management_dashboard_requires_admin_bearer_authentication() {
    let app = spawn_app().await;
    let response = app
        .get(
            "/management/dashboard?from=2026-07-16T00:00:00Z&to=2026-07-23T00:00:00Z",
            None,
        )
        .await;
    assert_eq!(response.status(), 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn management_dashboard_returns_composed_snapshot() {
    let app = spawn_app().await;
    let token = app.login_admin().await;

    let response = app
        .http
        .get(format!(
            "{}/management/dashboard?from=2026-07-16T00:00:00Z&to=2026-07-23T00:00:00Z",
            app.base_url
        ))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["success"], true);
    let data = &body["data"];
    // Range echoed verbatim.
    assert_eq!(data["range"]["from"], "2026-07-16T00:00:00Z");
    assert_eq!(data["range"]["to"], "2026-07-23T00:00:00Z");
    // Composed sections are present with the documented shape.
    for key in [
        "status",
        "jobs",
        "activity_by_day",
        "llm_usage",
        "knowledge",
        "recent_audit_events",
        "attention_items",
    ] {
        assert!(data.get(key).is_some(), "missing {key}");
    }
    // Never leak SQL or secret configuration.
    let raw = body.to_string();
    assert!(!raw.contains("SELECT"), "response leaked SQL");
    assert!(!raw.contains("password"), "response leaked secret");
    // Daily activity covers every UTC day in the range (7 days).
    assert_eq!(data["activity_by_day"].as_array().unwrap().len(), 7);
    // Knowledge invariant.
    let k = &data["knowledge"];
    let sum = k["available"].as_i64().unwrap()
        + k["deferred"].as_i64().unwrap()
        + k["unavailable"].as_i64().unwrap();
    assert_eq!(sum, k["total"].as_i64().unwrap());
}

#[tokio::test(flavor = "multi_thread")]
async fn management_dashboard_rejects_inverted_range() {
    let app = spawn_app().await;
    let token = app.login_admin().await;
    let response = app
        .http
        .get(format!(
            "{}/management/dashboard?from=2026-07-23T00:00:00Z&to=2026-07-16T00:00:00Z",
            app.base_url
        ))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}
