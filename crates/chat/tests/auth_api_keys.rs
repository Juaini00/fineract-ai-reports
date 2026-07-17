//! Verifies the bootstrap-admin → API-key flow, and that authentication
//! actually gates protected routes.

mod common;

use common::spawn_app;
use serde_json::json;
use uuid::Uuid;

const SCENARIO_CAPABILITIES: &[&str] = &[
    "savings_deposit_total",
    "savings_deposit_top_n",
    "savings_withdrawal_total",
    "savings_withdrawal_top_n",
    "savings_deposit_monthly_breakdown",
    "savings_deposit_monthly_top_n",
    "savings_withdrawal_monthly_breakdown",
    "savings_withdrawal_monthly_top_n",
    "savings_balance_summary",
    "organization_office_summary",
    "client_lifecycle_summary",
];

#[tokio::test(flavor = "multi_thread")]
async fn admin_can_create_api_key_and_bearer_can_create_chat_session() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let created = app
        .provision_api_key(SCENARIO_CAPABILITIES, vec![1, 2], true)
        .await;
    assert!(created.raw.starts_with("air_test_"));

    let session = app
        .post_json_bearer(
            "/chat/sessions",
            &app.login_admin().await,
            &json!({ "title": "API key session" }),
        )
        .await;

    // Assert
    assert_eq!(session.status(), 201);
}

#[tokio::test(flavor = "multi_thread")]
async fn create_api_key_without_admin_token_is_forbidden() {
    let app = spawn_app().await;

    // No Authorization header
    let resp = app
        .post_json(
            "/auth/api-keys",
            None,
            &json!({ "name": "x", "owner": "y" }),
        )
        .await;

    assert_eq!(resp.status(), 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn create_api_key_with_invalid_access_token_is_unauthorized() {
    let app = spawn_app().await;

    let resp = app
        .http
        .post(format!("{}/auth/api-keys", app.base_url))
        .header(reqwest::header::AUTHORIZATION, "Bearer wrong-token")
        .json(&json!({ "name": "x", "owner": "y" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn me_without_api_key_is_unauthorized() {
    let app = spawn_app().await;

    let resp = app.get("/chat/sessions", None).await;

    assert_eq!(resp.status(), 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_route_with_api_key_but_no_bearer_is_unauthorized() {
    let app = spawn_app().await;
    let created = app
        .provision_api_key(SCENARIO_CAPABILITIES, vec![1, 2], true)
        .await;

    let resp = app
        .http
        .get(format!("{}/chat/sessions", app.base_url))
        .header("X-API-Key", created.raw)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn api_key_lifecycle_endpoints_are_unchanged() {
    let app = spawn_app().await;

    let created = app
        .provision_api_key(SCENARIO_CAPABILITIES, vec![1, 2], true)
        .await;
    let (user_id, key_prefix, key_hash, revoked_at): (
        Uuid,
        String,
        String,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT user_id, key_prefix, key_hash, revoked_at FROM api_keys WHERE id = $1",
    )
    .bind(created.id)
    .fetch_one(&app.app_pool)
    .await
    .expect("created api key row");

    assert_eq!(user_id, app.admin_user_id().await);
    assert!(created.raw.starts_with("air_test_"));
    assert!(created.raw.starts_with(&key_prefix));
    assert_ne!(key_hash, created.raw);
    assert!(revoked_at.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn x_api_key_variants_do_not_change_bearer_chat_authorization() {
    let app = spawn_app().await;
    let bearer = app.login_admin().await;
    let valid = app.provision_wildcard_api_key(false).await;
    let revoked = app.provision_wildcard_api_key(false).await;
    let expired = app.provision_wildcard_api_key(false).await;
    let ownerless = app.insert_legacy_api_key_without_user().await;
    let other_user = app.provision_wildcard_api_key(false).await;
    let other_user_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role) VALUES ($1, $2, 'unused', 'admin')",
    )
    .bind(other_user_id)
    .bind(format!("api-key-owner-{other_user_id}"))
    .execute(&app.app_pool)
    .await
    .expect("insert other API-key owner");
    sqlx::query("UPDATE api_keys SET revoked_at = now() WHERE id = $1")
        .bind(revoked.id)
        .execute(&app.app_pool)
        .await
        .expect("revoke api key");
    sqlx::query("UPDATE api_keys SET expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(expired.id)
        .execute(&app.app_pool)
        .await
        .expect("expire api key");
    sqlx::query("UPDATE api_keys SET user_id = $1 WHERE id = $2")
        .bind(other_user_id)
        .bind(other_user.id)
        .execute(&app.app_pool)
        .await
        .expect("reassign api key");

    let variants = [
        ("absent", None),
        ("valid", Some(valid.raw.as_str())),
        ("invalid", Some("air_test_invalid")),
        ("revoked", Some(revoked.raw.as_str())),
        ("expired", Some(expired.raw.as_str())),
        ("ownerless", Some(ownerless.as_str())),
        ("other-user", Some(other_user.raw.as_str())),
    ];
    for (name, api_key) in variants {
        let response = app
            .get_bearer_with_api_key("/chat/sessions", &bearer, api_key)
            .await;
        assert_eq!(response.status(), 200, "variant {name}");
    }
}
