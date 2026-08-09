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
use app_core::auth::{api_key, token::TokenService};
use app_core::config::{
    AppConfig, AuthConfig, CatalogConfig, LlmConfig, QueryConfig, RedisConfig, ServerConfig,
    VoyageAiConfig,
};
use app_core::db::DatabasePools;
use chrono::{Duration, Utc};
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
    pub fineract: PgPool,
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

    pub async fn get_bearer_with_api_key(
        &self,
        path: &str,
        token: &str,
        api_key: Option<&str>,
    ) -> reqwest::Response {
        let mut req = self
            .http
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(token);
        if let Some(api_key) = api_key {
            req = req.header("X-API-Key", api_key);
        }
        req.send().await.expect("http get bearer with api key")
    }

    pub async fn post_json_bearer(
        &self,
        path: &str,
        token: &str,
        body: &Value,
    ) -> reqwest::Response {
        self.http
            .post(format!("{}{path}", self.base_url))
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .expect("http post bearer")
    }

    pub async fn patch_json_bearer(
        &self,
        path: &str,
        token: &str,
        body: &Value,
    ) -> reqwest::Response {
        self.http
            .patch(format!("{}{path}", self.base_url))
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .expect("http patch bearer")
    }

    pub async fn delete_bearer(&self, path: &str, token: &str) -> reqwest::Response {
        self.http
            .delete(format!("{}{path}", self.base_url))
            .bearer_auth(token)
            .send()
            .await
            .expect("http delete bearer")
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

    pub async fn admin_user_id(&self) -> Uuid {
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'admin'")
            .fetch_one(&self.app_pool)
            .await
            .expect("admin user id")
    }

    pub async fn create_test_user_bearer(&self) -> String {
        let user_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role) VALUES ($1, $2, 'unused', 'admin')",
        )
        .bind(user_id)
        .bind(format!("test-{user_id}"))
        .execute(&self.app_pool)
        .await
        .expect("insert test user");
        sqlx::query("INSERT INTO user_sessions (id, user_id, expires_at) VALUES ($1, $2, $3)")
            .bind(session_id)
            .bind(user_id)
            .bind(Utc::now() + Duration::hours(1))
            .execute(&self.app_pool)
            .await
            .expect("insert test user session");
        TokenService::new(AuthConfig {
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
        })
        .issue_access_token(user_id, session_id, "admin")
        .expect("issue test user token")
        .token
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

/// Run an async test body on a runtime whose worker threads have a 16 MB stack.
///
/// The chat execution pipeline's future is deep: `JobMemory`, `ExecutionWorkflow`,
/// and large JSON all live in linear (non-recursive) async poll frames, overflowing
/// tokio's default 2 MB worker stack whenever a request actually executes a
/// workflow. `main.rs` builds the production runtime with a 16 MB stack for this
/// reason; `#[tokio::test]` does not inherit that, so execution-path integration
/// tests must run through this helper instead of the attribute macro.
/// ponytail: 16 MB mirrors main.rs; bump both together if a deeper path appears.
pub fn block_on_big_stack<F>(fut: F) -> F::Output
where
    F: std::future::Future,
{
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .expect("build big-stack test runtime")
        .block_on(fut)
}

/// Spin up a fresh app DB, run migrations, boot axum on `127.0.0.1:0`.
pub async fn spawn_app() -> TestApp {
    spawn_app_with_options("__ai_report_test_llm__").await
}

pub async fn spawn_app_with_llm_api_key(llm_api_key: &str) -> TestApp {
    spawn_app_with_options(llm_api_key).await
}

async fn spawn_app_with_options(llm_api_key: &str) -> TestApp {
    let admin_db_url = std::env::var("TEST_ADMIN_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://root:password@127.0.0.1:5432/postgres".into());
    let fineract_db_url = std::env::var("TEST_FINERACT_DATABASE_URL").unwrap_or_else(|_| {
        std::env::var("FINERACT_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://root:password@127.0.0.1:5432/fineract_default".into())
    });

    // Real embedding retrieval requires a Voyage key. When `VOYAGEAI_API_KEY`
    // is set in the environment, wire the live client so full-stack tests can
    // bootstrap `ChatAppState` (which syncs the vector index on boot). When it
    // is absent (default CI), keep the empty key + unreachable URL — the same
    // behaviour these tests had before, so nothing that ran without a key
    // starts requiring one.
    let voyage_api_key = std::env::var("VOYAGEAI_API_KEY").unwrap_or_default();
    let voyage_base_url = if voyage_api_key.is_empty() {
        "https://example.invalid".to_string()
    } else {
        std::env::var("VOYAGEAI_BASE_URL").unwrap_or_else(|_| "https://api.voyageai.com/v1".into())
    };

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
            global_max_rows: 50000,
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
            api_key: voyage_api_key.clone(),
            base_url: voyage_base_url.clone(),
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
    let fineract = pools.fineract.clone();

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
        fineract,
        app_db_name: db_name,
        admin_db_url,
        server: Some(handle),
    }
}
