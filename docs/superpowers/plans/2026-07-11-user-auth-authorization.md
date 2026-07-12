# User Authentication and Authorization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add username/password user auth, refresh-cookie sessions, bootstrap admin seeding, and user-owned default API keys while keeping chat/reporting on API-key auth.

**Architecture:** Keep all shared auth in `crates/core`; `app` remains composition root and `chat` continues consuming `ClientContext`. PostgreSQL is authoritative for users/sessions/refresh tokens; Redis is optional cache/revocation acceleration only. Use small focused modules under `core::auth` and keep existing route → service → repository → database boundaries.

**Tech Stack:** Rust, Axum, SQLx, PostgreSQL, Redis optional, SHA-256 for API/refresh token hashes, `jsonwebtoken`, `argon2`, `time`/`cookie` if needed.

## Global Constraints

- Keep exactly three crates: `app`, `core`, `chat`; do not add crates.
- Chat/reporting stays authenticated by API key.
- User tokens do not carry reporting permission/scope; API keys do.
- `api_keys.owner` remains for backward compatibility; add nullable `api_keys.user_id`.
- Bootstrap admin comes from env and auto-generates one default API key.
- `.env.example` and `.env` must both get concrete local values.
- PostgreSQL is durable source of truth for `user_sessions` and `refresh_tokens`.
- Redis stores no raw tokens and auth must degrade to PostgreSQL if Redis is unavailable.
- Do not commit unless explicitly requested.
- Implementation should be delegated per agent workflow because this touches more than 3 files; use cheap/fast task-executor for mechanical tasks, primary session for architecture decisions/review.

---

## File map

- Modify `Cargo.toml`: add workspace deps `argon2`, `jsonwebtoken`, `cookie`, `time` if not already present.
- Modify `crates/core/Cargo.toml`: consume the new deps.
- Create `migrations/20260711120000_create_users_sessions_permissions.sql`: users, permissions, role_permissions, user_sessions, refresh_tokens, `api_keys.user_id`.
- Modify `.env.example` and `.env`: user auth/bootstrap/JWT/cookie values.
- Modify `crates/core/src/config.rs`: extend `AuthConfig`.
- Modify `crates/core/src/auth/model.rs`: user/session/token DTO-domain structs and `ClientContext.user_id`.
- Create `crates/core/src/auth/password.rs`: Argon2 hash/verify.
- Create `crates/core/src/auth/token.rs`: JWT access token and opaque refresh token helpers.
- Modify `crates/core/src/auth/repository.rs`: add `UserRepository`, `SessionRepository`, extend `ApiKeyRepository`.
- Modify `crates/core/src/auth/service.rs`: login, refresh, logout, bootstrap admin, user-owned API-key creation.
- Modify `crates/core/src/auth/mod.rs`: export new modules.
- Modify `crates/core/src/api/dto/auth.rs`: login/refresh/logout/me response DTOs; move API-key me response name.
- Modify `crates/core/src/api/extractors/authenticated_client.rs`: keep API-key extractor; no user-token behavior here.
- Create `crates/core/src/api/extractors/authenticated_user.rs`: bearer access-token user extractor.
- Modify `crates/core/src/api/extractors/mod.rs`: export user extractor.
- Modify `crates/core/src/api/handlers/auth.rs`: login, refresh, logout, user me, api-key me, API-key create with user/admin compatibility.
- Modify `crates/core/src/api/routes/auth.rs`: add routes and remove API-key me from `/auth`; chat/reporting use `X-API-Key`.
- Modify `crates/core/src/api/mod.rs`: construct enhanced `AuthService`, expose `FromRef` for it.
- Modify `crates/app/src/main.rs`: run auth bootstrap before serving.
- Update tests in `crates/chat/tests/auth_api_keys.rs` and/or add `crates/chat/tests/user_auth.rs`.

---

### Task 1: Schema and configuration

**Files:**
- Create: `migrations/20260711120000_create_users_sessions_permissions.sql`
- Modify: `.env.example`
- Modify: `.env`
- Modify: `Cargo.toml`
- Modify: `crates/core/Cargo.toml`
- Modify: `crates/core/src/config.rs`

**Interfaces:**
- Produces: `AuthConfig` fields used by later tasks:
  - `bootstrap_admin_enabled: bool`
  - `bootstrap_admin_username: String`
  - `bootstrap_admin_password: String`
  - `bootstrap_admin_email: String`
  - `jwt_access_secret: String`
  - `jwt_refresh_secret: String`
  - `jwt_access_token_expiry_seconds: i64`
  - `jwt_refresh_token_expiry_seconds: i64`
  - `refresh_cookie_name: String`
  - `refresh_cookie_secure: bool`
  - `refresh_cookie_same_site: String`
  - `refresh_cookie_path: String`

- [ ] **Step 1: Add migration**

Create `migrations/20260711120000_create_users_sessions_permissions.sql`:

```sql
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    email TEXT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    full_name TEXT NULL,
    role TEXT NOT NULL DEFAULT 'admin',
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_login_at TIMESTAMPTZ NULL,
    CONSTRAINT chk_users_role CHECK (role IN ('admin'))
);

CREATE TABLE IF NOT EXISTS permissions (
    id UUID PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    description TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS role_permissions (
    role TEXT NOT NULL,
    permission_id UUID NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (role, permission_id),
    CONSTRAINT chk_role_permissions_role CHECK (role IN ('admin'))
);

CREATE TABLE IF NOT EXISTS user_sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_agent TEXT NULL,
    ip_address TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ NULL
);

CREATE TABLE IF NOT EXISTS refresh_tokens (
    id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES user_sessions(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ NULL
);

ALTER TABLE api_keys
    ADD COLUMN IF NOT EXISTS user_id UUID NULL REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_user_sessions_user_id ON user_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_user_sessions_revoked_at ON user_sessions(revoked_at);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_token_hash ON refresh_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_session_id ON refresh_tokens(session_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_user_id ON api_keys(user_id);
```

- [ ] **Step 2: Add dependencies**

In root `Cargo.toml` `[workspace.dependencies]`, add:

```toml
argon2 = "0.5.3"
cookie = "0.18.1"
jsonwebtoken = "9.3.1"
time = "0.3.44"
```

In `crates/core/Cargo.toml`, add:

```toml
argon2.workspace = true
cookie.workspace = true
jsonwebtoken.workspace = true
time.workspace = true
```

- [ ] **Step 3: Update env files**

Append under `# Authentication` in both `.env.example` and `.env`:

```dotenv
AUTH_BOOTSTRAP_ADMIN_ENABLED=true
AUTH_BOOTSTRAP_ADMIN_USERNAME=admin
AUTH_BOOTSTRAP_ADMIN_PASSWORD=password123
AUTH_BOOTSTRAP_ADMIN_EMAIL=admin@example.com
JWT_ACCESS_SECRET=local-access-secret-change-me
JWT_REFRESH_SECRET=local-refresh-secret-change-me
JWT_ACCESS_TOKEN_EXPIRY_SECONDS=900
JWT_REFRESH_TOKEN_EXPIRY_SECONDS=604800
AUTH_REFRESH_COOKIE_NAME=refresh_token
AUTH_REFRESH_COOKIE_SECURE=false
AUTH_REFRESH_COOKIE_SAME_SITE=strict
AUTH_REFRESH_COOKIE_PATH=/
```

- [ ] **Step 4: Extend config**

Modify `crates/core/src/config.rs` `AuthConfig` and parser:

```rust
#[derive(Clone, Debug)]
pub struct AuthConfig {
    pub bootstrap_admin_token: String,
    pub bootstrap_admin_enabled: bool,
    pub bootstrap_admin_username: String,
    pub bootstrap_admin_password: String,
    pub bootstrap_admin_email: String,
    pub jwt_access_secret: String,
    pub jwt_refresh_secret: String,
    pub jwt_access_token_expiry_seconds: i64,
    pub jwt_refresh_token_expiry_seconds: i64,
    pub refresh_cookie_name: String,
    pub refresh_cookie_secure: bool,
    pub refresh_cookie_same_site: String,
    pub refresh_cookie_path: String,
    pub api_key_prefix: String,
    pub api_key_default_expiration_days: u32,
}
```

Add corresponding parsing in `AppConfig::from_env()` auth block:

```rust
bootstrap_admin_enabled: get_env_or("AUTH_BOOTSTRAP_ADMIN_ENABLED", "false")
    .parse()
    .context("AUTH_BOOTSTRAP_ADMIN_ENABLED must be true or false")?,
bootstrap_admin_username: get_env_or("AUTH_BOOTSTRAP_ADMIN_USERNAME", "admin"),
bootstrap_admin_password: get_env_or("AUTH_BOOTSTRAP_ADMIN_PASSWORD", "password123"),
bootstrap_admin_email: get_env_or("AUTH_BOOTSTRAP_ADMIN_EMAIL", "admin@example.com"),
jwt_access_secret: get_required_env("JWT_ACCESS_SECRET")?,
jwt_refresh_secret: get_required_env("JWT_REFRESH_SECRET")?,
jwt_access_token_expiry_seconds: get_env_or("JWT_ACCESS_TOKEN_EXPIRY_SECONDS", "900")
    .parse()
    .context("JWT_ACCESS_TOKEN_EXPIRY_SECONDS must be an integer")?,
jwt_refresh_token_expiry_seconds: get_env_or("JWT_REFRESH_TOKEN_EXPIRY_SECONDS", "604800")
    .parse()
    .context("JWT_REFRESH_TOKEN_EXPIRY_SECONDS must be an integer")?,
refresh_cookie_name: get_env_or("AUTH_REFRESH_COOKIE_NAME", "refresh_token"),
refresh_cookie_secure: get_env_or("AUTH_REFRESH_COOKIE_SECURE", "true")
    .parse()
    .context("AUTH_REFRESH_COOKIE_SECURE must be true or false")?,
refresh_cookie_same_site: get_env_or("AUTH_REFRESH_COOKIE_SAME_SITE", "strict"),
refresh_cookie_path: get_env_or("AUTH_REFRESH_COOKIE_PATH", "/"),
```

- [ ] **Step 5: Verify config compiles**

Run: `cargo check -p core`

Expected: either PASS, or compile errors only in call sites that now need the new config fields. Fix field construction in tests by copying the local values above.

---

### Task 2: Auth domain, password, and token helpers

**Files:**
- Modify: `crates/core/src/auth/model.rs`
- Create: `crates/core/src/auth/password.rs`
- Create: `crates/core/src/auth/token.rs`
- Modify: `crates/core/src/auth/mod.rs`

**Interfaces:**
- Produces:
  - `hash_password(password: &str) -> Result<String>`
  - `verify_password(password: &str, password_hash: &str) -> Result<bool>`
  - `TokenService::issue_access_token(user_id: Uuid, session_id: Uuid) -> Result<IssuedAccessToken>`
  - `TokenService::issue_refresh_token() -> IssuedRefreshToken`
  - `TokenService::verify_access_token(token: &str) -> Result<AccessTokenClaims>`

- [ ] **Step 1: Extend models**

Add to `crates/core/src/auth/model.rs`:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub full_name: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct UserRecord {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub full_name: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewUserRecord {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub full_name: Option<String>,
    pub role: String,
}

#[derive(Debug, Clone)]
pub struct LoginInput {
    pub username: String,
    pub password: String,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LoginResult {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
    pub user: UserProfile,
}

#[derive(Debug, Clone)]
pub struct RefreshResult {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
}

#[derive(Debug, Clone)]
pub struct IssuedRefreshToken {
    pub id: Uuid,
    pub raw_token: String,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewSessionRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewRefreshTokenRecord {
    pub id: Uuid,
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
}
```

Add `user_id` to API key structs:

```rust
pub user_id: Option<Uuid>,
```

for `NewApiKeyRecord`, `ActiveApiKeyRecord`, and `ClientContext`.

- [ ] **Step 2: Add password helpers**

Create `crates/core/src/auth/password.rs`:

```rust
use anyhow::Result;
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)?
        .to_string();
    Ok(hash)
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool> {
    let parsed = PasswordHash::new(password_hash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}
```

- [ ] **Step 3: Add token helpers**

Create `crates/core/src/auth/token.rs`:

```rust
use anyhow::Result;
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{auth::model::IssuedRefreshToken, config::AuthConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    pub sub: Uuid,
    pub sid: Uuid,
    pub role: String,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug, Clone)]
pub struct IssuedAccessToken {
    pub token: String,
    pub expires_in: i64,
}

#[derive(Clone)]
pub struct TokenService {
    config: AuthConfig,
}

impl TokenService {
    pub fn new(config: AuthConfig) -> Self {
        Self { config }
    }

    pub fn issue_access_token(&self, user_id: Uuid, session_id: Uuid, role: &str) -> Result<IssuedAccessToken> {
        let now = Utc::now();
        let expires_in = self.config.jwt_access_token_expiry_seconds;
        let claims = AccessTokenClaims {
            sub: user_id,
            sid: session_id,
            role: role.to_string(),
            iat: now.timestamp(),
            exp: (now + Duration::seconds(expires_in)).timestamp(),
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.config.jwt_access_secret.as_bytes()),
        )?;
        Ok(IssuedAccessToken { token, expires_in })
    }

    pub fn issue_refresh_token(&self) -> IssuedRefreshToken {
        let mut bytes = [0_u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let raw_token = hex::encode(bytes);
        IssuedRefreshToken {
            id: Uuid::new_v4(),
            token_hash: hash_token(&raw_token),
            raw_token,
            expires_at: Utc::now() + Duration::seconds(self.config.jwt_refresh_token_expiry_seconds),
        }
    }

    pub fn verify_access_token(&self, token: &str) -> Result<AccessTokenClaims> {
        let claims = decode::<AccessTokenClaims>(
            token,
            &DecodingKey::from_secret(self.config.jwt_access_secret.as_bytes()),
            &Validation::default(),
        )?
        .claims;
        Ok(claims)
    }
}

pub fn hash_token(raw_token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    hex::encode(hasher.finalize())
}
```

- [ ] **Step 4: Export modules**

Modify `crates/core/src/auth/mod.rs`:

```rust
pub mod api_key;
pub mod model;
pub mod password;
pub mod repository;
pub mod service;
pub mod token;
```

- [ ] **Step 5: Verify**

Run: `cargo check -p core`

Expected: PASS after fixing missing struct initializers by setting `user_id: None` in tests/fixtures.

---

### Task 3: Repositories and API-key ownership

**Files:**
- Modify: `crates/core/src/auth/repository.rs`
- Modify: test fixtures that construct `ClientContext`

**Interfaces:**
- Consumes task 2 structs.
- Produces methods:
  - `UserRepository::find_by_username(&self, username: &str) -> Result<Option<UserRecord>>`
  - `UserRepository::find_by_id(&self, id: Uuid) -> Result<Option<UserRecord>>`
  - `UserRepository::insert(&self, record: NewUserRecord) -> Result<()>`
  - `UserRepository::touch_last_login_at(&self, id: Uuid) -> Result<()>`
  - `SessionRepository::insert_session(...)`
  - `SessionRepository::insert_refresh_token(...)`
  - `SessionRepository::find_active_refresh_token(hash: &str)`
  - `SessionRepository::revoke_session(session_id: Uuid)`
  - `ApiKeyRepository::count_for_user(user_id: Uuid) -> Result<i64>`

- [ ] **Step 1: Extend API-key SQL**

In `ApiKeyRow`, add:

```rust
user_id: Option<Uuid>,
```

In `insert`, include `user_id` column and bind `record.user_id`.

In `find_active_by_hash`, select `user_id`.

In `From<ApiKeyRow>`, set `user_id: row.user_id`.

- [ ] **Step 2: Add user row mapping**

Add to `repository.rs`:

```rust
#[derive(Debug, FromRow)]
struct UserRow {
    id: Uuid,
    username: String,
    email: Option<String>,
    password_hash: String,
    full_name: Option<String>,
    role: String,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    last_login_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<UserRow> for UserRecord {
    fn from(row: UserRow) -> Self {
        Self {
            id: row.id,
            username: row.username,
            email: row.email,
            password_hash: row.password_hash,
            full_name: row.full_name,
            role: row.role,
            is_active: row.is_active,
            created_at: row.created_at,
            last_login_at: row.last_login_at,
        }
    }
}
```

- [ ] **Step 3: Add repositories**

Add `UserRepository` and `SessionRepository` structs using the same `PgPool` pattern as `ApiKeyRepository`. Implement SQL methods exactly against tables from Task 1.

Use this active refresh query:

```sql
SELECT rt.id, rt.session_id, rt.user_id, rt.expires_at
FROM refresh_tokens rt
JOIN user_sessions us ON us.id = rt.session_id
WHERE rt.token_hash = $1
  AND rt.revoked_at IS NULL
  AND rt.expires_at > now()
  AND us.revoked_at IS NULL
  AND us.expires_at > now()
```

- [ ] **Step 4: Fix `ClientContext` constructors**

Search for `ClientContext {` and add:

```rust
user_id: None,
```

where tests create synthetic contexts.

- [ ] **Step 5: Verify**

Run: `cargo check -p core`

Expected: PASS.

---

### Task 4: Auth service login, refresh, logout, bootstrap

**Files:**
- Modify: `crates/core/src/auth/service.rs`
- Modify: `crates/core/src/api/mod.rs`
- Modify: `crates/app/src/main.rs`

**Interfaces:**
- Produces:
  - `AuthService::bootstrap_admin(&self) -> Result<()>`
  - `AuthService::login(&self, input: LoginInput) -> Result<(LoginResult, String)>` where `String` is raw refresh token.
  - `AuthService::refresh(&self, raw_refresh_token: &str) -> Result<Option<RefreshResult>>`
  - `AuthService::logout(&self, raw_refresh_token: &str) -> Result<()>`
  - `AuthService::get_user(&self, user_id: Uuid) -> Result<Option<UserProfile>>`

- [ ] **Step 1: Update service fields and constructor**

Make `AuthService` hold:

```rust
config: AuthConfig,
api_key_repository: ApiKeyRepository,
user_repository: UserRepository,
session_repository: SessionRepository,
token_service: TokenService,
```

Update `AuthService::new(config, api_key_repository, user_repository, session_repository)`.

- [ ] **Step 2: Implement user profile conversion**

Add helper:

```rust
fn user_profile(user: UserRecord) -> UserProfile {
    UserProfile {
        id: user.id,
        username: user.username,
        email: user.email,
        full_name: user.full_name,
        role: user.role,
        is_active: user.is_active,
        created_at: user.created_at,
        last_login_at: user.last_login_at,
    }
}
```

- [ ] **Step 3: Implement bootstrap admin**

Logic:

```rust
pub async fn bootstrap_admin(&self) -> Result<()> {
    if !self.config.bootstrap_admin_enabled {
        return Ok(());
    }
    if self.user_repository.find_by_username(&self.config.bootstrap_admin_username).await?.is_some() {
        return Ok(());
    }

    let user_id = Uuid::new_v4();
    let password_hash = password::hash_password(&self.config.bootstrap_admin_password)?;
    self.user_repository.insert(NewUserRecord {
        id: user_id,
        username: self.config.bootstrap_admin_username.clone(),
        email: Some(self.config.bootstrap_admin_email.clone()),
        password_hash,
        full_name: Some("Administrator".to_string()),
        role: "admin".to_string(),
    }).await?;

    if self.api_key_repository.count_for_user(user_id).await? == 0 {
        let _created = self.create_api_key(CreateApiKeyInput {
            name: "Default admin API key".to_string(),
            owner: self.config.bootstrap_admin_username.clone(),
            expires_at: None,
            allowed_office_ids: Vec::new(),
            allowed_capabilities: Vec::new(),
            allow_all_offices: true,
            allow_all_capabilities: true,
            can_view_pii: true,
            user_id: Some(user_id),
        }).await?;
        tracing::warn!("bootstrap admin API key was generated but is only shown in logs if explicitly logged; create a new key from the API if needed");
    }
    Ok(())
}
```

Do not log raw API key unless product explicitly asks.

- [ ] **Step 4: Implement login**

Pseudo-code:

```rust
pub async fn login(&self, input: LoginInput) -> Result<(LoginResult, String)> {
    let user = self.user_repository
        .find_by_username(input.username.trim())
        .await?
        .ok_or_else(|| anyhow::anyhow!("invalid credentials"))?;
    if !user.is_active || !password::verify_password(&input.password, &user.password_hash)? {
        anyhow::bail!("invalid credentials");
    }
    let session_id = Uuid::new_v4();
    let refresh = self.token_service.issue_refresh_token();
    let access = self.token_service.issue_access_token(user.id, session_id, &user.role)?;
    self.session_repository.insert_session(NewSessionRecord {
        id: session_id,
        user_id: user.id,
        user_agent: input.user_agent,
        ip_address: input.ip_address,
        expires_at: refresh.expires_at,
    }).await?;
    self.session_repository.insert_refresh_token(NewRefreshTokenRecord {
        id: refresh.id,
        session_id,
        user_id: user.id,
        token_hash: refresh.token_hash,
        expires_at: refresh.expires_at,
    }).await?;
    self.user_repository.touch_last_login_at(user.id).await?;
    Ok((LoginResult {
        access_token: access.token,
        token_type: "Bearer",
        expires_in: access.expires_in,
        user: user_profile(user),
    }, refresh.raw_token))
}
```

Map invalid credentials to unauthorized in handlers, not raw internal messages.

- [ ] **Step 5: Implement refresh/logout/get_user**

Refresh:
- Hash raw token with `token::hash_token`.
- Find active refresh token.
- Load user.
- Issue access token using existing session id.
- Return `None` for invalid/revoked/expired.

Logout:
- Hash raw token.
- If active token found, revoke its session and token.
- Return OK even if token missing, for idempotency.

Get user:
- `find_by_id`, return active user only.

- [ ] **Step 6: Wire state and startup**

In `crates/core/src/api/mod.rs`, construct repos and enhanced service.

In `crates/app/src/main.rs`, after state creation and before router:

```rust
core_state.auth_service.bootstrap_admin().await?;
```

- [ ] **Step 7: Verify**

Run: `cargo check -p core && cargo check -p app`

Expected: PASS.

---

### Task 5: User extractors, routes, cookies, DTOs

**Files:**
- Create: `crates/core/src/api/extractors/authenticated_user.rs`
- Modify: `crates/core/src/api/extractors/mod.rs`
- Modify: `crates/core/src/api/dto/auth.rs`
- Modify: `crates/core/src/api/handlers/auth.rs`
- Modify: `crates/core/src/api/routes/auth.rs`

**Interfaces:**
- Consumes `AuthService::login/refresh/logout/get_user`.
- Produces HTTP routes:
  - `POST /auth/login`
  - `POST /auth/refresh`
  - `POST /auth/logout`
  - `GET /auth/me`
- [ ] **Step 1: Add DTOs**

In `dto/auth.rs`, add:

```rust
#[derive(Debug, Deserialize, Validate)]
pub(crate) struct LoginRequest {
    #[validate(length(min = 1, message = "username is required"))]
    pub(crate) username: String,
    #[validate(length(min = 1, message = "password is required"))]
    pub(crate) password: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct LoginResponse {
    pub(crate) access_token: String,
    pub(crate) token_type: &'static str,
    pub(crate) expires_in: i64,
    pub(crate) user: crate::auth::model::UserProfile,
}

#[derive(Debug, Serialize)]
pub(crate) struct RefreshResponse {
    pub(crate) access_token: String,
    pub(crate) token_type: &'static str,
    pub(crate) expires_in: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct UserMeResponse {
    pub(crate) auth_type: &'static str,
    pub(crate) user: crate::auth::model::UserProfile,
}

#[derive(Debug, Serialize)]
pub(crate) struct ApiKeyMeResponse {
    pub(crate) auth_type: &'static str,
    pub(crate) client: crate::auth::model::ClientContext,
}
```

Rename old `AuthMeResponse` to `ApiKeyMeResponse`.

- [ ] **Step 2: Add user extractor**

Create `authenticated_user.rs`:

```rust
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header, request::Parts},
};
use uuid::Uuid;

use crate::{api::error::ApiError, auth::service::AuthService};

pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub role: String,
}

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
    AuthService: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let token = parts.headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::unauthorized("missing access token"))?;

        let auth_service = AuthService::from_ref(state);
        let claims = auth_service
            .verify_access_token(token)
            .map_err(|_| ApiError::unauthorized("invalid access token"))?;

        Ok(Self {
            user_id: claims.sub,
            session_id: claims.sid,
            role: claims.role,
        })
    }
}
```

Add `pub mod authenticated_user;` to extractors mod.

- [ ] **Step 3: Add cookie helpers in handler**

In `handlers/auth.rs`, add helpers:

```rust
fn refresh_cookie(state: &AppState, raw_token: String) -> Result<HeaderValue, ApiError> {
    let same_site = match state.config.auth.refresh_cookie_same_site.as_str() {
        "lax" => cookie::SameSite::Lax,
        "none" => cookie::SameSite::None,
        _ => cookie::SameSite::Strict,
    };
    let cookie = cookie::Cookie::build((state.config.auth.refresh_cookie_name.clone(), raw_token))
        .http_only(true)
        .secure(state.config.auth.refresh_cookie_secure)
        .same_site(same_site)
        .path(state.config.auth.refresh_cookie_path.clone())
        .max_age(time::Duration::seconds(state.config.auth.jwt_refresh_token_expiry_seconds))
        .build();
    HeaderValue::from_str(&cookie.to_string()).map_err(ApiError::internal)
}

fn clear_refresh_cookie(state: &AppState) -> Result<HeaderValue, ApiError> {
    let cookie = cookie::Cookie::build((state.config.auth.refresh_cookie_name.clone(), ""))
        .http_only(true)
        .secure(state.config.auth.refresh_cookie_secure)
        .path(state.config.auth.refresh_cookie_path.clone())
        .max_age(time::Duration::seconds(0))
        .build();
    HeaderValue::from_str(&cookie.to_string()).map_err(ApiError::internal)
}

fn extract_refresh_cookie(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    cookie_header
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(&format!("{}=", state.config.auth.refresh_cookie_name)))
        .map(ToString::to_string)
}
```

- [ ] **Step 4: Add handlers**

Implement:

```rust
pub(crate) async fn login(...)
pub(crate) async fn refresh(...)
pub(crate) async fn logout(...)
pub(crate) async fn get_user_me(...)
pub(crate) async fn get_api_key_me(...)
```

Rules:
- Login maps invalid credentials to `ApiError::unauthorized("invalid username or password")`.
- Refresh maps missing/invalid cookie to 401.
- Logout always returns success and clears cookie when cookie is present or missing.
- User `me` returns `auth_type: "user"`.
- API key `me` returns `auth_type: "api_key"`.

- [ ] **Step 5: Update routes**

Modify `routes/auth.rs`:

```rust
Router::new()
    .route("/auth/login", post(login))
    .route("/auth/refresh", post(refresh))
    .route("/auth/logout", post(logout))
    .route("/auth/me", get(get_user_me))
    .route("/auth/api-keys", post(create_api_key))
```

- [ ] **Step 6: Verify**

Run: `cargo check`

Expected: PASS.

---

### Task 6: Tests and compatibility

**Files:**
- Modify: `crates/chat/tests/common/mod.rs`
- Modify: `crates/chat/tests/auth_api_keys.rs`
- Create: `crates/chat/tests/user_auth.rs`

**Interfaces:**
- Consumes all previous HTTP routes.
- Produces regression coverage for user auth and API-key compatibility.

- [ ] **Step 1: Update existing API-key me test**

In `auth_api_keys.rs`, change old API-key identity call:

```rust
let session = app.post_json("/chat/sessions", Some(&created.raw), &json!({ "title": "API key session" })).await;
```

Assert response path remains:

```rust
assert_eq!(body["data"]["auth_type"], "api_key");
```

- [ ] **Step 2: Add user auth tests**

Create `crates/chat/tests/user_auth.rs`:

```rust
mod common;

use common::spawn_app;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn bootstrap_admin_can_login_and_call_user_me() {
    let app = spawn_app().await;

    let login = app
        .post_json(
            "/auth/login",
            None,
            &json!({ "username": "admin", "password": "password123" }),
        )
        .await;
    assert_eq!(login.status(), 200);
    assert!(login.headers().get(reqwest::header::SET_COOKIE).is_some());

    let body: serde_json::Value = login.json().await.unwrap();
    let token = body["data"]["access_token"].as_str().unwrap();
    assert_eq!(body["data"]["user"]["username"], "admin");

    let me = app.get("/auth/me", Some(token)).await;
    assert_eq!(me.status(), 200);
    let me_body: serde_json::Value = me.json().await.unwrap();
    assert_eq!(me_body["data"]["auth_type"], "user");
    assert_eq!(me_body["data"]["user"]["username"], "admin");
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_password_is_unauthorized() {
    let app = spawn_app().await;

    let resp = app
        .post_json(
            "/auth/login",
            None,
            &json!({ "username": "admin", "password": "wrong" }),
        )
        .await;

    assert_eq!(resp.status(), 401);
}
```

- [ ] **Step 3: Add refresh/logout tests if helpers support cookies**

If existing test harness exposes raw `reqwest::Client`, add:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn refresh_cookie_issues_new_access_token_and_logout_revokes() {
    let app = spawn_app().await;
    let login = app.http
        .post(format!("{}/auth/login", app.base_url))
        .json(&json!({ "username": "admin", "password": "password123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);
    let cookie = login.headers().get(reqwest::header::SET_COOKIE).unwrap().to_str().unwrap().to_string();

    let refresh = app.http
        .post(format!("{}/auth/refresh", app.base_url))
        .header(reqwest::header::COOKIE, cookie.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(refresh.status(), 200);

    let logout = app.http
        .post(format!("{}/auth/logout", app.base_url))
        .header(reqwest::header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(logout.status(), 200);
}
```

- [ ] **Step 4: Verify focused tests**

Run:

```bash
cargo test -p chat auth_api_keys user_auth
```

Expected: PASS.

- [ ] **Step 5: Verify full workspace**

Run:

```bash
cargo fmt
cargo check
cargo test
```

Expected: PASS.

---

### Task 7: Redis optional cache/revocation, only if simple

**Files:**
- Modify: `crates/core/src/auth/service.rs`
- Modify: `crates/core/src/api/mod.rs` only if needed to pass Redis client/state.

**Interfaces:**
- Consumes existing `DatabasePools`/Redis config if a Redis connection is already available.
- Produces no public API changes.

- [ ] **Step 1: Check existing Redis pool/client**

Inspect `crates/core/src/db.rs`. If there is no reusable Redis client already exposed, skip Redis implementation for this pass and document: PostgreSQL-only auth is correct; Redis auth cache can be added when a shared Redis abstraction exists.

- [ ] **Step 2: If Redis client exists, add best-effort revocation cache**

On logout, write:

```text
auth:revoked_refresh:{token_hash} = 1 EX <remaining_refresh_ttl>
```

On refresh, check that key first. If Redis errors, log warning and continue PostgreSQL check.

- [ ] **Step 3: Verify fallback**

Run Redis disabled test path by setting `REDIS_ENABLED=false` for one focused run:

```bash
REDIS_ENABLED=false cargo test -p chat user_auth
```

Expected: PASS.

---

## Self-review

- Spec coverage: schema, env, bootstrap, login/refresh/logout/me, API-key ownership, Redis optional behavior, and tests are covered by Tasks 1-7.
- Placeholder scan: no TBD/TODO placeholders; Redis task has an explicit skip condition because no shared Redis auth abstraction may exist.
- Type consistency: all task interfaces use `Uuid`, `DateTime<Utc>`, `UserProfile`, `LoginResult`, and existing `AuthService` patterns consistently.
