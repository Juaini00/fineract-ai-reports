mod common;

use app_core::{auth::token::TokenService, config::AuthConfig};
use chrono::{Duration, Utc};
use common::spawn_app;
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

fn token_service() -> TokenService {
    TokenService::new(AuthConfig {
        bootstrap_admin_token: "test-bootstrap-admin-token".into(),
        bootstrap_admin_enabled: true,
        bootstrap_admin_username: "admin".into(),
        bootstrap_admin_password: "password123".into(),
        bootstrap_admin_email: "admin@example.com".into(),
        jwt_access_secret: "test-access-secret-change-me".into(),
        jwt_refresh_secret: "test-refresh-secret-change-me".into(),
        jwt_access_token_expiry_seconds: 900,
        jwt_refresh_token_expiry_seconds: 604800,
        refresh_cookie_name: "refresh_token".into(),
        refresh_cookie_secure: false,
        refresh_cookie_same_site: "strict".into(),
        refresh_cookie_path: "/".into(),
        api_key_prefix: "air_test".into(),
        api_key_default_expiration_days: 0,
    })
}

async fn insert_session(app: &common::TestApp, user_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO user_sessions (id, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(user_id)
        .bind(Utc::now() + Duration::hours(1))
        .execute(&app.app_pool)
        .await
        .expect("insert session");
    id
}

async fn assert_unauthorized(app: &common::TestApp, token: &str) {
    assert_eq!(
        app.get_bearer("/auth/me", token).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bearer_authority_is_loaded_from_active_user_and_owned_active_session() {
    let app = spawn_app().await;
    let tokens = token_service();
    let admin_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'admin'")
        .fetch_one(&app.app_pool)
        .await
        .expect("admin user");

    let active_session = insert_session(&app, admin_id).await;
    let active_token = tokens
        .issue_access_token(admin_id, active_session, "admin")
        .unwrap()
        .token;
    assert_eq!(
        app.get_bearer("/auth/me", &active_token).await.status(),
        StatusCode::OK
    );

    sqlx::query("UPDATE users SET is_active = false WHERE id = $1")
        .bind(admin_id)
        .execute(&app.app_pool)
        .await
        .unwrap();
    assert_unauthorized(&app, &active_token).await;
    sqlx::query("UPDATE users SET is_active = true WHERE id = $1")
        .bind(admin_id)
        .execute(&app.app_pool)
        .await
        .unwrap();

    let missing_user_token = tokens
        .issue_access_token(Uuid::new_v4(), Uuid::new_v4(), "admin")
        .unwrap()
        .token;
    assert_unauthorized(&app, &missing_user_token).await;

    let missing_session_token = tokens
        .issue_access_token(admin_id, Uuid::new_v4(), "admin")
        .unwrap()
        .token;
    assert_unauthorized(&app, &missing_session_token).await;

    let other_user = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role) VALUES ($1, $2, 'unused', 'admin')",
    )
    .bind(other_user)
    .bind(format!("other-{other_user}"))
    .execute(&app.app_pool)
    .await
    .unwrap();
    let wrong_owner_session = insert_session(&app, other_user).await;
    let wrong_owner_token = tokens
        .issue_access_token(admin_id, wrong_owner_session, "admin")
        .unwrap()
        .token;
    assert_unauthorized(&app, &wrong_owner_token).await;

    sqlx::query("UPDATE user_sessions SET revoked_at = now() WHERE id = $1")
        .bind(active_session)
        .execute(&app.app_pool)
        .await
        .unwrap();
    assert_unauthorized(&app, &active_token).await;

    let expired_session = insert_session(&app, admin_id).await;
    sqlx::query("UPDATE user_sessions SET expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(expired_session)
        .execute(&app.app_pool)
        .await
        .unwrap();
    let expired_token = tokens
        .issue_access_token(admin_id, expired_session, "admin")
        .unwrap()
        .token;
    assert_unauthorized(&app, &expired_token).await;

    let role_session = insert_session(&app, admin_id).await;
    let forged_admin_token = tokens
        .issue_access_token(admin_id, role_session, "admin")
        .unwrap()
        .token;
    sqlx::query("ALTER TABLE users DROP CONSTRAINT chk_users_role")
        .execute(&app.app_pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET role = 'viewer' WHERE id = $1")
        .bind(admin_id)
        .execute(&app.app_pool)
        .await
        .unwrap();

    let me = app.get_bearer("/auth/me", &forged_admin_token).await;
    assert_eq!(me.status(), StatusCode::OK);
    let body: Value = me.json().await.unwrap();
    assert_eq!(body["data"]["role"], "viewer");
    let create_key = app
        .http
        .post(format!("{}/auth/api-keys", app.base_url))
        .bearer_auth(&forged_admin_token)
        .json(&json!({ "name": "must-not-authorize" }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_key.status(), StatusCode::FORBIDDEN);
}

/// SI-1: every chat route — including the workflow (`/chat/jobs/{id}/responses`)
/// and admin (`/vector-index/*`) endpoints — requires a bearer session JWT. A
/// request with no `Authorization` header must be rejected with 401 before any
/// handler logic runs. An API key alone can never authenticate a chat request.
#[tokio::test(flavor = "multi_thread")]
async fn si1_every_chat_route_requires_a_bearer_and_rejects_missing_auth() {
    let app = spawn_app().await;
    let job = Uuid::new_v4();

    // (method, path) for every chat/catalog/vector-index route.
    let get_routes = [
        "/chat/sessions",
        &format!("/chat/jobs/{job}"),
        &format!("/chat/jobs/{job}/audit"),
        &format!("/chat/jobs/{job}/stream"),
        "/catalog/capabilities",
        "/vector-index/status",
    ];
    for path in get_routes {
        let resp = app
            .http
            .get(format!("{}{path}", app.base_url))
            .send()
            .await
            .expect("send get");
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "GET {path} must require a bearer"
        );
    }

    let post_routes = [
        "/chat/sessions",
        "/chat/jobs",
        &format!("/chat/jobs/{job}/responses"),
        "/catalog/validate",
        "/vector-index/rebuild",
    ];
    for path in post_routes {
        let resp = app
            .http
            .post(format!("{}{path}", app.base_url))
            .json(&json!({}))
            .send()
            .await
            .expect("send post");
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "POST {path} must require a bearer"
        );
    }

    // An API key without a bearer must not authenticate a chat route either.
    let with_api_key = app
        .http
        .get(format!("{}/chat/sessions", app.base_url))
        .header("X-API-Key", "air_test_not-a-real-key")
        .send()
        .await
        .expect("send get with api key");
    assert_eq!(
        with_api_key.status(),
        StatusCode::UNAUTHORIZED,
        "an API key alone can never authenticate a chat request"
    );
}
