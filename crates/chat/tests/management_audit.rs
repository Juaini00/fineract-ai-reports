mod common;

use chrono::{Duration, Utc};
use common::spawn_app;
use reqwest::header;
use serde_json::json;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread")]
async fn management_audit_requires_admin_and_requires_a_bounded_range() {
    let app = spawn_app().await;
    assert_eq!(app.get("/management/audit", None).await.status(), 401);

    let token = app.login_admin().await;
    let response = app
        .http
        .get(format!("{}/management/audit", app.base_url))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn management_audit_filters_and_pages_typed_safe_projections() {
    let app = spawn_app().await;
    let now = Utc::now();
    for (id, event_type, outcome, occurred_at) in [
        (
            Uuid::new_v4(),
            "chat.job_created",
            "success",
            now - Duration::minutes(1),
        ),
        (
            Uuid::new_v4(),
            "chat.job_created",
            "success",
            now - Duration::minutes(2),
        ),
        (
            Uuid::new_v4(),
            "chat.job_failed",
            "failed",
            now - Duration::minutes(3),
        ),
    ] {
        sqlx::query("INSERT INTO management_audit_events (id, aggregate_type, aggregate_id, event_type, outcome, summary_json, occurred_at) VALUES ($1, 'management', $2, $3, $4, $5, $6)")
            .bind(id).bind(Uuid::new_v4()).bind(event_type).bind(outcome)
            .bind(json!({"kind": "job_created"})).bind(occurred_at)
            .execute(&app.app_pool).await.unwrap();
    }

    let token = app.login_admin().await;
    let query = format!(
        "from={}&to={}&event_type=chat_job_created&limit=1",
        (now - Duration::hours(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        (now + Duration::hours(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    );
    let response = app
        .http
        .get(format!("{}/management/audit?{query}", app.base_url))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let item = &body["data"]["items"][0];
    assert_eq!(item["event_type"], "chat_job_created");
    assert_eq!(item["summary"]["kind"], "job_created");
    assert!(item.get("summary_json").is_none());
    let cursor = body["data"]["next_cursor"].as_str().unwrap();
    assert!(!cursor.contains("T"));
    let response = app
        .http
        .get(format!(
            "{}/management/audit?{query}&cursor={cursor}",
            app.base_url
        ))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap()["data"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let job_id = Uuid::new_v4();
    let response = app
        .http
        .get(format!(
            "{}/management/audit/jobs/{job_id}?from={}&to={}",
            app.base_url,
            (now - Duration::hours(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            (now + Duration::hours(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        ))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap()["data"]["items"],
        json!([])
    );
}
