mod common;

use common::{TestApp, spawn_app_with_llm_api_key};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

#[tokio::test(flavor = "multi_thread")]
async fn provider_test_without_sentinel_fails_closed_not_respondable() {
    let app = spawn_app_with_llm_api_key("").await;
    let key = app.provision_wildcard_api_key(true).await;

    let resp = app
        .post_json(
            "/chat/jobs",
            Some(&key.raw),
            &json!({ "message": "Show me the top 10 clients by savings balance" }),
        )
        .await;
    assert_eq!(resp.status(), 201);
    let job_id = resp.json::<Value>().await.unwrap()["data"]["job_id"]
        .as_str()
        .unwrap()
        .to_string();

    let job = wait_until_not_running(&app, &key.raw, &job_id).await;
    assert_eq!(job["status"].as_str(), Some("failed"), "{job}");
    assert_ne!(job["status"].as_str(), Some("waiting_for_user_input"));
    assert!(job["error_json"].is_object(), "{job}");

    let response = app
        .post_json(
            &format!("/chat/jobs/{job_id}/responses"),
            Some(&key.raw),
            &json!({ "message": "try again" }),
        )
        .await;
    assert_ne!(response.status(), 201);
}

async fn wait_until_not_running(app: &TestApp, api_key: &str, job_id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let resp = app
            .get(&format!("/chat/jobs/{job_id}"), Some(api_key))
            .await;
        assert_eq!(resp.status(), 200);
        let job = resp.json::<Value>().await.unwrap()["data"].clone();
        if !matches!(job["status"].as_str(), Some("queued" | "running")) {
            return job;
        }
        assert!(Instant::now() < deadline, "job did not finish: {job}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
