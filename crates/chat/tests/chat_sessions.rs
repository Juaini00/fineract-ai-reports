//! Session lifecycle: create, get, list messages, and bearer-user ownership.

mod common;

use common::spawn_app;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn creates_session_with_optional_title() {
    let app = spawn_app().await;
    let token = app.login_admin().await;

    let resp = app
        .post_json_bearer("/chat/sessions", &token, &json!({ "title": "Q1 review" }))
        .await;

    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["title"], "Q1 review");
    assert!(body["data"]["user_id"].is_string());
    assert!(body["data"]["api_key_id"].is_null());
    assert!(body["data"]["id"].as_str().is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn get_session_returns_only_owner_sessions() {
    let app = spawn_app().await;
    let owner = app.login_admin().await;
    let other = app.create_test_user_bearer().await;

    let created = app
        .post_json_bearer("/chat/sessions", &owner, &json!({ "title": null }))
        .await;
    let created: serde_json::Value = created.json().await.unwrap();
    let session_id = created["data"]["id"].as_str().unwrap();

    // Owner can read
    let mine = app
        .get_bearer(&format!("/chat/sessions/{session_id}"), &owner)
        .await;
    assert_eq!(mine.status(), 200);

    // Another client cannot
    let theirs = app
        .get_bearer(&format!("/chat/sessions/{session_id}"), &other)
        .await;
    assert!(
        matches!(theirs.status().as_u16(), 404),
        "other client should not read foreign session, got {}",
        theirs.status()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn create_session_without_bearer_is_unauthorized() {
    let app = spawn_app().await;

    let resp = app
        .post_json("/chat/sessions", None, &json!({ "title": "x" }))
        .await;

    assert_eq!(resp.status(), 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn list_sessions_returns_sessions_owned_by_authenticated_user() {
    let app = spawn_app().await;
    let token = app.login_admin().await;

    let first = app
        .post_json_bearer(
            "/chat/sessions",
            &token,
            &json!({ "title": "first session" }),
        )
        .await;
    assert_eq!(first.status(), 201);
    let second = app
        .post_json_bearer(
            "/chat/sessions",
            &token,
            &json!({ "title": "second session" }),
        )
        .await;
    assert_eq!(second.status(), 201);

    let resp = app.get_bearer("/chat/sessions", &token).await;

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["success"], true);
    let titles: Vec<_> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|session| session["title"].as_str().unwrap())
        .collect();
    assert!(titles.contains(&"first session"));
    assert!(titles.contains(&"second session"));
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_ignores_api_key_header_when_bearer_is_valid() {
    let app = spawn_app().await;
    let raw_key = app.insert_legacy_api_key_without_user().await;

    let token = app.login_admin().await;
    let resp = app
        .http
        .post(format!("{}/chat/sessions", app.base_url))
        .bearer_auth(token)
        .header("X-API-Key", raw_key)
        .json(&json!({ "title": "bearer owned" }))
        .send()
        .await;

    assert_eq!(resp.unwrap().status(), 201);
}
