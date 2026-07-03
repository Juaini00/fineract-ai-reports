//! Session lifecycle: create, get, list messages. Verifies API-key auth wraps
//! every chat route, and that sessions belong to the authenticated client.

mod common;

use common::spawn_app;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn creates_session_with_optional_title() {
    let app = spawn_app().await;
    let key = app.provision_api_key(&[], vec![1], false).await;

    let resp = app
        .post_json(
            "/chat/sessions",
            Some(&key.raw),
            &json!({ "title": "Q1 review" }),
        )
        .await;

    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["title"], "Q1 review");
    assert!(body["data"]["id"].as_str().is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn get_session_returns_only_owner_sessions() {
    let app = spawn_app().await;
    let owner = app.provision_api_key(&[], vec![1], false).await;
    let other = app.provision_api_key(&[], vec![1], false).await;

    let created = app
        .post_json(
            "/chat/sessions",
            Some(&owner.raw),
            &json!({ "title": null }),
        )
        .await;
    let created: serde_json::Value = created.json().await.unwrap();
    let session_id = created["data"]["id"].as_str().unwrap();

    // Owner can read
    let mine = app
        .get(&format!("/chat/sessions/{session_id}"), Some(&owner.raw))
        .await;
    assert_eq!(mine.status(), 200);

    // Another client cannot
    let theirs = app
        .get(&format!("/chat/sessions/{session_id}"), Some(&other.raw))
        .await;
    assert!(
        matches!(theirs.status().as_u16(), 403 | 404),
        "other client should not read foreign session, got {}",
        theirs.status()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn create_session_without_api_key_is_unauthorized() {
    let app = spawn_app().await;

    let resp = app
        .post_json("/chat/sessions", None, &json!({ "title": "x" }))
        .await;

    assert_eq!(resp.status(), 401);
}
