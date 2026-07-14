//! Integration-test harness for the `chat` crate.
//!
//! Boots a real axum server against a real Postgres via the containers already
//! present on the dev host (see docker-compose.yml + local postgres). The app
//! DB is created fresh per test and dropped in `Drop`. The Fineract DB is
//! **read-only by contract** — we point at an existing Fineract database and
//! never write to it.
//!
//! To skip integration tests when infrastructure is not available, set
//! `AI_REPORT_SKIP_INTEGRATION=1`; each `spawn_app` panics with a clear
//! message if the DB is unreachable.

#![allow(dead_code)] // parts of the harness are used by only some test files

use std::net::SocketAddr;

use app_core::api::AppState;
use app_core::auth::api_key;
use app_core::config::{
    AppConfig, AuthConfig, CatalogConfig, LlmConfig, QueryConfig, RedisConfig, ServerConfig,
    VoyageAiConfig,
};
use app_core::db::DatabasePools;
use reqwest::header;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgPool, postgres::PgPoolOptions};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use uuid::Uuid;

pub const ADMIN_TOKEN: &str = "test-bootstrap-admin-token";

/// One live server + fresh app DB. Dropping it tears down.
pub struct TestApp {
    pub base_url: String,
    pub http: reqwest::Client,
    pub app_pool: PgPool,
    pub app_db_name: String,
    admin_db_url: String,
    server: Option<JoinHandle<()>>,
}

pub struct ApiKey {
    pub raw: String,
    pub id: Uuid,
}

impl TestApp {
    /// GET `path` with an optional `X-API-Key`.
    pub async fn get(&self, path: &str, api_key: Option<&str>) -> reqwest::Response {
        let mut req = self.http.get(format!("{}{path}", self.base_url));
        if let Some(key) = api_key {
            let access_token = self.login_admin().await;
            req = req
                .header("X-API-Key", key)
                .header(header::AUTHORIZATION, format!("Bearer {access_token}"));
        }
        req.send().await.expect("http get")
    }

    /// GET `path` with a user bearer token.
    pub async fn get_bearer(&self, path: &str, token: &str) -> reqwest::Response {
        self.http
            .get(format!("{}{path}", self.base_url))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .await
            .expect("http get bearer")
    }

    /// POST JSON with an optional `X-API-Key`.
    pub async fn post_json(
        &self,
        path: &str,
        api_key: Option<&str>,
        body: &Value,
    ) -> reqwest::Response {
        let mut req = self
            .http
            .post(format!("{}{path}", self.base_url))
            .json(body);
        if let Some(key) = api_key {
            let access_token = self.login_admin().await;
            req = req
                .header("X-API-Key", key)
                .header(header::AUTHORIZATION, format!("Bearer {access_token}"));
        }
        req.send().await.expect("http post")
    }

    /// POST JSON with a bootstrap admin bearer.
    pub async fn post_json_admin(&self, path: &str, body: &Value) -> reqwest::Response {
        self.http
            .post(format!("{}{path}", self.base_url))
            .header(header::AUTHORIZATION, format!("Bearer {ADMIN_TOKEN}"))
            .json(body)
            .send()
            .await
            .expect("http post admin")
    }

    pub async fn login_admin(&self) -> String {
        let resp = self
            .http
            .post(format!("{}/auth/login", self.base_url))
            .json(&json!({ "username": "admin", "password": "password123" }))
            .send()
            .await
            .expect("login admin");
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let payload: Value = resp.json().await.expect("login json");
        payload["data"]["access_token"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// Create an API key via logged-in dashboard user auth.
    pub async fn provision_api_key(
        &self,
        allowed_capabilities: &[&str],
        allowed_office_ids: Vec<i64>,
        can_view_pii: bool,
    ) -> ApiKey {
        let access_token = self.login_admin().await;
        let body = json!({
            "name": format!("harness-{}", &Uuid::new_v4().to_string()[..8]),
            "allowed_capabilities": allowed_capabilities,
            "allowed_office_ids": allowed_office_ids,
            "can_view_pii": can_view_pii,
        });
        let resp = self
            .http
            .post(format!("{}/auth/api-keys", self.base_url))
            .header(header::AUTHORIZATION, format!("Bearer {access_token}"))
            .json(&body)
            .send()
            .await
            .expect("post api key");
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::CREATED,
            "provision_api_key failed: {}",
            resp.text().await.unwrap_or_default()
        );
        let payload: Value = resp.json().await.expect("api key json");
        ApiKey {
            raw: payload["data"]["api_key"].as_str().unwrap().to_string(),
            id: Uuid::parse_str(payload["data"]["id"].as_str().unwrap()).unwrap(),
        }
    }

    pub async fn provision_wildcard_api_key(&self, can_view_pii: bool) -> ApiKey {
        let access_token = self.login_admin().await;
        let body = json!({
            "name": format!("harness-{}", &Uuid::new_v4().to_string()[..8]),
            "allowed_capabilities": [],
            "allowed_office_ids": [],
            "allow_all_offices": true,
            "allow_all_capabilities": true,
            "can_view_pii": can_view_pii,
        });
        let resp = self
            .http
            .post(format!("{}/auth/api-keys", self.base_url))
            .header(header::AUTHORIZATION, format!("Bearer {access_token}"))
            .json(&body)
            .send()
            .await
            .expect("post wildcard api key");
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::CREATED,
            "provision_wildcard_api_key failed: {}",
            resp.text().await.unwrap_or_default()
        );
        let payload: Value = resp.json().await.expect("api key json");
        ApiKey {
            raw: payload["data"]["api_key"].as_str().unwrap().to_string(),
            id: Uuid::parse_str(payload["data"]["id"].as_str().unwrap()).unwrap(),
        }
    }

    pub async fn insert_legacy_api_key_without_user(&self) -> String {
        let raw = api_key::generate_api_key("air_test");
        sqlx::query(
            r#"
            INSERT INTO api_keys (
                id,
                user_id,
                name,
                owner,
                key_prefix,
                key_hash,
                allowed_office_ids,
                allowed_capabilities,
                allow_all_offices,
                allow_all_capabilities,
                can_view_pii
            )
            VALUES ($1, NULL, 'legacy', 'legacy', $2, $3, '[]'::jsonb, '[]'::jsonb, true, true, false)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(api_key::key_display_prefix(&raw))
        .bind(api_key::hash_api_key(&raw))
        .execute(&self.app_pool)
        .await
        .expect("insert legacy api key");
        raw
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        if let Some(handle) = self.server.take() {
            handle.abort();
        }
        // Drop the throwaway DB in the background — best effort. sqlx's runtime
        // may already be gone, so we spawn a blocking task from a fresh runtime.
        let admin_url = self.admin_db_url.clone();
        let db_name = self.app_db_name.clone();
        std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Runtime::new() else {
                return;
            };
            rt.block_on(async {
                let Ok(pool) = PgPoolOptions::new()
                    .max_connections(1)
                    .connect(&admin_url)
                    .await
                else {
                    return;
                };
                let _ = sqlx::query(AssertSqlSafe(format!(
                    "DROP DATABASE IF EXISTS \"{db_name}\" WITH (FORCE)"
                )))
                .execute(&pool)
                .await;
            });
        });
    }
}

/// Spin up a fresh app DB, run migrations, boot axum on `127.0.0.1:0`.
pub async fn spawn_app() -> TestApp {
    spawn_app_with_llm_api_key("__ai_report_test_llm__").await
}

pub async fn spawn_app_with_llm_api_key(llm_api_key: &str) -> TestApp {
    let admin_db_url = std::env::var("TEST_ADMIN_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://root:password@127.0.0.1:5432/postgres".into());
    let fineract_db_url = std::env::var("TEST_FINERACT_DATABASE_URL").unwrap_or_else(|_| {
        std::env::var("FINERACT_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://root:password@127.0.0.1:5432/fineract_default".into())
    });

    // Create per-test app DB
    let db_name = format!("ai_report_test_{}", Uuid::new_v4().simple());
    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&admin_db_url)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "cannot reach Postgres at {admin_db_url}; is docker running? original error: {error}"
            )
        });
    sqlx::query(AssertSqlSafe(format!("CREATE DATABASE \"{db_name}\"")))
        .execute(&admin_pool)
        .await
        .expect("create test database");
    drop(admin_pool);

    let app_db_url = admin_db_url.replace("/postgres", &format!("/{db_name}"));

    // Build AppConfig directly — no process env, no cross-test race.
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = AppConfig {
        app: ServerConfig {
            env: "test".into(),
            host: "127.0.0.1".into(),
            port: 0,
        },
        app_database_url: app_db_url.clone(),
        app_database_migrate_on_startup: true,
        fineract_database_url: fineract_db_url,
        redis: RedisConfig {
            enabled: false,
            url: "redis://127.0.0.1:6379/0".into(),
        },
        auth: AuthConfig {
            bootstrap_admin_token: ADMIN_TOKEN.into(),
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
        },
        query: QueryConfig {
            default_timeout_ms: 3000,
        },
        llm: LlmConfig {
            provider: "test".into(),
            api_key: llm_api_key.into(),
            chat_completions_url: "https://example.invalid".into(),
            base_url: "https://example.invalid".into(),
            model: "test".into(),
            timeout_ms: 5000,
            max_retries: 1,
            max_output_tokens: 100,
            temperature: 0.0,
        },
        embedding: app_core::config::EmbeddingConfig {
            provider: "test".into(),
            api_key: String::new(),
            base_url: "https://example.invalid".into(),
            model: "test".into(),
            timeout_ms: 5000,
            max_retries: 1,
            dimensions: 1024,
        },
        voyage_ai: VoyageAiConfig {
            api_key: String::new(),
            base_url: "https://example.invalid".into(),
            embedding_model: "voyage-3-large".into(),
            timeout_ms: 5000,
            embedding_dimensions: 1024,
        },
        catalog: CatalogConfig {
            path: workspace_root.join("knowledge").to_string_lossy().into(),
            query_path: workspace_root.join("queries").to_string_lossy().into(),
            validate_on_startup: true,
            sync_on_startup: false,
        },
        chat_features: app_core::config::ChatFeatureConfig {
            lqr_enabled: false,
            context_soft_token_limit: 6000,
            context_hard_token_limit: 8000,
            context_max_recent_messages: 12,
            context_max_relevant_jobs: 3,
        },
    };
    let pools = DatabasePools::connect(&config)
        .await
        .expect("connect databases");
    pools
        .run_app_migrations()
        .await
        .expect("run app migrations");
    let app_pool = pools.app.clone();

    // Compose router the same way main.rs does
    let core_state = AppState::new(config.clone(), pools);
    core_state
        .auth_service
        .bootstrap_admin()
        .await
        .expect("bootstrap admin");
    let chat_state = chat::api::ChatAppState::new(core_state.clone())
        .await
        .expect("build ChatAppState");
    let router = app_core::api::router(core_state).merge(chat::api::router(chat_state));

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr: SocketAddr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    TestApp {
        base_url: format!("http://{addr}"),
        http: reqwest::Client::new(),
        app_pool,
        app_db_name: db_name,
        admin_db_url,
        server: Some(handle),
    }
}
