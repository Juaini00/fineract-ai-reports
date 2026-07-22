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

#[tokio::test(flavor = "multi_thread")]
async fn rename_session_trims_title_and_rejects_invalid_titles() {
    let app = spawn_app().await;
    let token = app.login_admin().await;
    let created = app
        .post_json_bearer("/chat/sessions", &token, &json!({ "title": "old" }))
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let session_id = created["data"]["id"].as_str().unwrap();

    let renamed = app
        .patch_json_bearer(
            &format!("/chat/sessions/{session_id}"),
            &token,
            &json!({ "title": "  Quarterly review  " }),
        )
        .await;
    assert_eq!(renamed.status(), 200);
    let body: serde_json::Value = renamed.json().await.unwrap();
    assert_eq!(body["data"]["title"], "Quarterly review");

    for invalid in [
        json!({}),
        json!({ "title": null }),
        json!({ "title": "   " }),
        json!({ "title": "x".repeat(121) }),
    ] {
        let response = app
            .patch_json_bearer(&format!("/chat/sessions/{session_id}"), &token, &invalid)
            .await;
        assert_eq!(response.status(), 400, "invalid payload: {invalid}");
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["success"], false, "invalid payload: {invalid}");
        assert!(body["data"].is_null(), "invalid payload: {invalid}");
        assert!(body["error"].is_object(), "invalid payload: {invalid}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_archives_session_and_hides_it_from_client_access() {
    let app = spawn_app().await;
    let owner = app.login_admin().await;
    let other = app.create_test_user_bearer().await;
    let created = app
        .post_json_bearer("/chat/sessions", &owner, &json!({ "title": "remove me" }))
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let session_id = created["data"]["id"].as_str().unwrap();
    let path = format!("/chat/sessions/{session_id}");

    assert_eq!(app.delete_bearer(&path, &other).await.status(), 404);
    let deleted = app.delete_bearer(&path, &owner).await;
    assert_eq!(deleted.status(), 200);
    let body: serde_json::Value = deleted.json().await.unwrap();
    assert_eq!(body["data"]["session_id"], session_id);
    assert_eq!(body["data"]["deleted"], true);

    assert_eq!(app.get_bearer(&path, &owner).await.status(), 404);
    let listed: serde_json::Value = app
        .get_bearer("/chat/sessions", &owner)
        .await
        .json()
        .await
        .unwrap();
    assert!(
        listed["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|session| session["id"] != session_id)
    );
    assert_eq!(
        app.get_bearer(&format!("{path}/messages"), &owner)
            .await
            .status(),
        404
    );
    assert_eq!(app.delete_bearer(&path, &owner).await.status(), 404);
    assert_eq!(
        app.patch_json_bearer(&path, &owner, &json!({ "title": "revive" }))
            .await
            .status(),
        404
    );
    let row: (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, archived_at FROM chat_sessions WHERE id = $1")
            .bind(uuid::Uuid::parse_str(session_id).unwrap())
            .fetch_one(&app.app_pool)
            .await
            .unwrap();
    assert_eq!(row.0, "archived");
    assert!(row.1.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn archived_session_hides_job_routes_and_rejects_new_work() {
    let app = spawn_app().await;
    let token = app.login_admin().await;
    let user_id = app.admin_user_id().await;
    let session_id = uuid::Uuid::new_v4();
    let job_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO chat_sessions (id, user_id, status) VALUES ($1, $2, 'active')")
        .bind(session_id)
        .bind(user_id)
        .execute(&app.app_pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO chat_jobs (id, session_id, user_id, status, current_step, message, expires_at) \
         VALUES ($1, $2, $3, 'waiting_for_user_input', 'taking_decision', 'test', now() + interval '1 hour')",
    )
    .bind(job_id)
    .bind(session_id)
    .bind(user_id)
    .execute(&app.app_pool)
    .await
    .unwrap();

    let session_path = format!("/chat/sessions/{session_id}");
    assert_eq!(app.delete_bearer(&session_path, &token).await.status(), 200);
    assert_eq!(
        app.get_bearer(&format!("/chat/jobs/{job_id}"), &token)
            .await
            .status(),
        404
    );
    assert_eq!(
        app.get_bearer(&format!("/chat/jobs/{job_id}/audit"), &token)
            .await
            .status(),
        404
    );
    assert_eq!(
        app.get_bearer(&format!("/chat/jobs/{job_id}/stream"), &token)
            .await
            .status(),
        404
    );
    assert_eq!(
        app.post_json_bearer(
            "/chat/jobs",
            &token,
            &json!({ "session_id": session_id, "message": "new work" }),
        )
        .await
        .status(),
        404
    );
    assert_eq!(
        app.post_json_bearer(
            &format!("/chat/jobs/{job_id}/responses"),
            &token,
            &json!({ "message": "continue" }),
        )
        .await
        .status(),
        404
    );
}
