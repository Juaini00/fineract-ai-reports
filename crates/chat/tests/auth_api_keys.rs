//! Verifies the bootstrap-admin → API-key flow, and that authentication
//! actually gates protected routes.

mod common;

use common::spawn_app;
use serde_json::json;

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
async fn admin_can_create_api_key_and_client_can_call_me() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let created = app
        .provision_api_key(SCENARIO_CAPABILITIES, vec![1, 2], true)
        .await;

    let me = app.get("/auth/me", Some(&created.raw)).await;

    // Assert
    assert_eq!(me.status(), 200);
    let body: serde_json::Value = me.json().await.unwrap();
    assert_eq!(body["data"]["auth_type"], "api_key");
    assert_eq!(body["data"]["client"]["allowed_office_ids"], json!([1, 2]));
    assert_eq!(
        body["data"]["client"]["allowed_capabilities"],
        json!(SCENARIO_CAPABILITIES)
    );
    assert_eq!(body["data"]["client"]["can_view_pii"], true);
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
async fn create_api_key_with_wrong_admin_token_is_forbidden() {
    let app = spawn_app().await;

    let resp = app
        .http
        .post(format!("{}/auth/api-keys", app.base_url))
        .header(reqwest::header::AUTHORIZATION, "Bearer wrong-token")
        .json(&json!({ "name": "x", "owner": "y" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 403);
}

#[tokio::test(flavor = "multi_thread")]
async fn me_without_api_key_is_unauthorized() {
    let app = spawn_app().await;

    let resp = app.get("/auth/me", None).await;

    assert_eq!(resp.status(), 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn me_with_invalid_api_key_is_unauthorized() {
    let app = spawn_app().await;

    let resp = app.get("/auth/me", Some("air_test_bogus")).await;

    assert_eq!(resp.status(), 401);
}
