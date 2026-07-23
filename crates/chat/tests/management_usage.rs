mod common;

use common::spawn_app;
use reqwest::header;

#[tokio::test(flavor = "multi_thread")]
async fn management_llm_usage_requires_admin_and_valid_bounded_range() {
    let app = spawn_app().await;
    let unauthorized = app
        .http
        .get(format!(
            "{}/management/llm-usage?from=2026-07-01T00:00:00Z&to=2026-07-02T00:00:00Z&group_by=day",
            app.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), 401);

    let token = app.login_admin().await;
    let invalid = app
        .http
        .get(format!(
            "{}/management/llm-usage?from=2026-01-01T00:00:00Z&to=2026-07-02T00:00:00Z&group_by=day",
            app.base_url
        ))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), 400);

    let valid = app
        .http
        .get(format!(
            "{}/management/llm-usage?from=2026-07-01T00:00:00Z&to=2026-07-02T00:00:00Z&group_by=day",
            app.base_url
        ))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(valid.status(), 200);
    let body: serde_json::Value = valid.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["groups"], serde_json::json!([]));
}
