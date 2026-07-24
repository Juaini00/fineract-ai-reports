mod common;

use common::spawn_app;
use reqwest::header;

#[tokio::test(flavor = "multi_thread")]
async fn management_knowledge_requires_admin_bearer_authentication() {
    let app = spawn_app().await;

    let response = app.get("/management/knowledge", None).await;

    assert_eq!(response.status(), 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn management_status_returns_safe_configured_identity() {
    let app = spawn_app().await;
    let token = app.login_admin().await;

    let response = app
        .http
        .get(format!("{}/management/status", app.base_url))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["data"]["provider"]["name"], "test");
    assert_eq!(body["data"]["features"]["reference_knowledge"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn management_knowledge_projects_catalog_without_sql_and_reference_is_disabled() {
    let app = spawn_app().await;
    let client = &app.http;
    let token = app.login_admin().await;

    let response = client
        .get(format!("{}/management/knowledge", app.base_url))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["success"], true);
    let item = body["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "catalog:savings_deposit_total")
        .unwrap();
    assert!(item.get("sql_file").is_none());

    let detail = client
        .get(format!(
            "{}/management/knowledge/catalog:savings_deposit_total",
            app.base_url
        ))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(detail.status(), 200);
    let detail: serde_json::Value = detail.json().await.unwrap();
    assert!(detail["data"].get("sql_file").is_none());
    assert!(detail["data"].get("sql").is_none());

    let reference = client
        .get(format!(
            "{}/management/knowledge?kind=reference",
            app.base_url
        ))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(reference.status(), 200);
    let reference: serde_json::Value = reference.json().await.unwrap();
    assert_eq!(reference["data"]["items"], serde_json::json!([]));
    assert_eq!(reference["data"]["reference_knowledge_status"], "disabled");
}

#[tokio::test(flavor = "multi_thread")]
async fn management_knowledge_validates_page_size() {
    let app = spawn_app().await;
    let token = app.login_admin().await;
    let response = app
        .http
        .get(format!("{}/management/knowledge?limit=0", app.base_url))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], "validation_error");
}
