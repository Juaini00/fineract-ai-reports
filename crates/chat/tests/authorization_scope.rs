mod common;

use app_core::auth::model::PrincipalContext;
use chat::knowledge::catalog::loader::KnowledgeLoader;
use chat::policy::authorization::{
    AuthorizationError, effective_office_scope, ensure_capability_allowed, ensure_pii_allowed,
    project_admin_principal,
};
use common::spawn_app;
use reqwest::header;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread")]
async fn authorization_scope_projects_admin_to_concrete_grants() {
    let app = spawn_app().await;
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalog = KnowledgeLoader::new(
        workspace_root.join("knowledge"),
        workspace_root.join("queries"),
    )
    .load()
    .expect("load catalog");
    let mut principal = principal();

    project_admin_principal(&mut principal, &catalog, &app.fineract)
        .await
        .expect("project admin");

    let expected_capability_ids: Vec<_> = catalog
        .capabilities
        .iter()
        .filter(|capability| capability.status == "approved_mvp")
        .map(|capability| capability.id.clone())
        .collect();
    let expected_office_ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM m_office ORDER BY id")
        .fetch_all(&app.fineract)
        .await
        .expect("load authoritative offices");

    assert_eq!(principal.capability_ids, expected_capability_ids);
    assert_eq!(principal.office_ids, expected_office_ids);
    assert!(principal.can_view_pii);
    assert_eq!(principal.legacy_api_key_id, None);
}

#[tokio::test]
async fn authorization_scope_lookup_failure_grants_nothing() {
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalog = KnowledgeLoader::new(
        workspace_root.join("knowledge"),
        workspace_root.join("queries"),
    )
    .load()
    .expect("load catalog");
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://localhost/unreachable")
        .expect("lazy pool");
    pool.close().await;
    let mut principal = principal();

    assert!(
        project_admin_principal(&mut principal, &catalog, &pool)
            .await
            .is_err()
    );
    assert!(principal.capability_ids.is_empty());
    assert!(principal.office_ids.is_empty());
    assert!(!principal.can_view_pii);
}

#[tokio::test(flavor = "multi_thread")]
async fn authorization_scope_rejects_non_admin_with_stable_code() {
    let app = spawn_app().await;
    let token = app.login_admin().await;
    sqlx::query("ALTER TABLE users DROP CONSTRAINT chk_users_role")
        .execute(&app.app_pool)
        .await
        .expect("allow non-admin fixture");
    sqlx::query("UPDATE users SET role = 'user' WHERE username = 'admin'")
        .execute(&app.app_pool)
        .await
        .expect("demote admin");

    let response = app
        .http
        .post(format!("{}/catalog/validate", app.base_url))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("X-API-Key", "ignored-invalid-key")
        .send()
        .await
        .expect("validate request");

    sqlx::query("UPDATE users SET role = 'admin' WHERE username = 'admin'")
        .execute(&app.app_pool)
        .await
        .expect("restore admin");
    sqlx::query("ALTER TABLE users ADD CONSTRAINT chk_users_role CHECK (role IN ('admin'))")
        .execute(&app.app_pool)
        .await
        .expect("restore role constraint");

    assert_eq!(response.status(), 403);
    let body: Value = response.json().await.expect("error body");
    assert_eq!(body["error"]["code"], "role_not_authorized");
}

#[test]
fn authorization_scope_policy_uses_only_concrete_principal_grants() {
    let mut principal = principal();
    principal.capability_ids = vec!["savings_deposit_total".into()];
    principal.office_ids = vec![1, 2];
    principal.can_view_pii = true;

    assert!(ensure_capability_allowed(&principal, "savings_deposit_total").is_ok());
    assert_eq!(
        ensure_capability_allowed(&principal, "savings_deposit_top_n"),
        Err(AuthorizationError::CapabilityNotAllowed(
            "savings_deposit_top_n".into()
        ))
    );
    assert_eq!(effective_office_scope(&principal, Some(&[2])), Ok(vec![2]));
    assert_eq!(
        effective_office_scope(&principal, Some(&[3])),
        Err(AuthorizationError::OfficeNotAllowed(3))
    );
    assert!(ensure_pii_allowed(&principal, true).is_ok());

    principal.office_ids.clear();
    assert_eq!(
        effective_office_scope(&principal, None),
        Err(AuthorizationError::MissingOfficeScope)
    );
}

fn principal() -> PrincipalContext {
    PrincipalContext {
        user_id: Uuid::new_v4(),
        role: "admin".into(),
        capability_ids: Vec::new(),
        office_ids: Vec::new(),
        can_view_pii: false,
        legacy_api_key_id: None,
    }
}
