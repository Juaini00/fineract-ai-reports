# Project Setup

This document defines the exact project setup we will use. It keeps crate boundaries small and intentional.

The current implementation should use exactly three crates:

```text
app
core
chat
```

## 1. Final Rule For The Initial Setup

Use this structure:

```text
ai_report/
  Cargo.toml
  .env
  docs/

  crates/
    app/
      Cargo.toml
      src/
        main.rs

    core/
      Cargo.toml
      src/
        lib.rs

    chat/
      Cargo.toml
      src/
        lib.rs
```

Meaning:

```text
app  = binary entrypoint and composition root
core = shared application foundation
chat = main chat-driven reporting feature
```

Do not add more crates yet.

Do not use crate names like `ai_report_core`, `ai_report_app`, `ai_report_chat`, `chat_service`, `knowledge`, or `reporting`.

The crate names must be:

```text
app
core
chat
```

Knowledge remains a folder-based catalog under `knowledge/`.

SQL remains under `queries/`.

Reporting remains part of the chat-driven feature until there is a concrete non-chat report API or scheduling surface.

## 2. Root Cargo.toml

The root `Cargo.toml` must be a workspace manifest only.

It must not contain `[package]`.

Correct root structure:

```toml
[workspace]
members = [
    "crates/app",
    "crates/core",
    "crates/chat",
]
resolver = "3"

[workspace.package]
version = "0.0.1"
edition = "2024"

[workspace.dependencies]
anyhow = "1.0.102"
async-trait = "0.1.89"
axum = "0.8.9"
chrono = { version = "0.4.45", features = ["serde"] }
config = "0.15.23"
dotenvy = "0.15.7"
futures = "0.3.32"
hex = "0.4.3"
pgvector = { version = "0.4.2", features = ["sqlx"] }
redis = { version = "1.2.3", features = ["tokio-comp"] }
rand = "0.8.6"
reqwest = { version = "0.13.4", features = ["json", "rustls"] }
rust_decimal = { version = "1.42.1", features = ["serde"] }
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"
serde_yaml = "0.9.34"
sha2 = "0.10.9"
sqlx = { version = "0.9.0", features = ["runtime-tokio", "tls-rustls", "postgres", "macros", "migrate", "chrono", "uuid", "rust_decimal"] }
thiserror = "2.0.18"
tokio = { version = "1.52.3", features = ["full"] }
tower-http = { version = "0.6.11", features = ["trace", "tracing"] }
tracing = "0.1.44"
tracing-subscriber = { version = "0.3.23", features = ["env-filter"] }
uuid = { version = "1.23.3", features = ["serde", "v4"] }
validator = { version = "0.20.0", features = ["derive"] }
```

Why root has no `[package]`:

```text
The root is not an app.
The root only manages the workspace.
The runnable binary is crates/app.
```

## 3. app Crate

`app` is the binary launcher and composition root.

Path:

```text
crates/app/src/main.rs
```

`app` should not contain business logic.

`app` wires the `core` foundation and `chat` feature crate.

Expected `crates/app/Cargo.toml`:

```toml
[package]
name = "app"
version.workspace = true
edition.workspace = true

[dependencies]
app_core = { package = "core", path = "../core" }
chat = { path = "../chat" }
anyhow.workspace = true
axum.workspace = true
tokio.workspace = true
tower-http.workspace = true
tracing.workspace = true
```

Current `crates/app/src/main.rs` responsibility:

```text
call app_core::bootstrap()
build core AppState
build chat ChatAppState
merge core and chat routers
apply the global HTTP TraceLayer
bind and serve the configured address
log startup/readiness status
```

`app_core::run()` may remain as a fallback helper, but the active composition root is now `crates/app/src/main.rs`.

The intended dependency direction is:

```text
app -> core
app -> chat
chat -> core
```

Do not make `core -> chat` and `chat -> core` depend on each other.

Why the alias is needed:

```text
The crate package is named core, but Rust also has a built-in core crate.
The app crate uses the alias app_core to avoid macro/name resolution confusion.
```

`app` does not need `src/lib.rs`.

Reason:

```text
app is a binary crate, not a library crate.
```

## 4. core Crate

`core` is the shared application foundation.

Path:

```text
crates/core/src/lib.rs
```

`core` owns:

```text
config loading
tracing setup
database pool setup
shared HTTP/API primitives
auth service
health/readiness handlers
validated JSON extractor
API key authentication extractor
response envelope
API key context model shared by protected feature crates
```

`core` must not own chat-specific job orchestration once `crates/chat` exists.

Expected `crates/core/Cargo.toml`:

```toml
[package]
name = "core"
version.workspace = true
edition.workspace = true

[dependencies]
anyhow.workspace = true
axum.workspace = true
config.workspace = true
dotenvy.workspace = true
hex.workspace = true
rand.workspace = true
redis.workspace = true
serde.workspace = true
serde_json.workspace = true
sha2.workspace = true
sqlx.workspace = true
thiserror.workspace = true
tokio.workspace = true
tower-http.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
uuid.workspace = true
chrono.workspace = true
validator.workspace = true
```

Initial `crates/core/src/lib.rs`:

```rust
pub async fn run() -> anyhow::Result<()> {
    Ok(())
}
```

After that works, we add modules gradually.

## 5. chat Crate

`chat` owns the main chat-driven reporting feature.

Path:

```text
crates/chat/src/lib.rs
```

`chat` owns:

```text
api routes, handlers, and DTOs for chat endpoints
chat sessions, messages, and jobs
chat job repositories and services
future chat job checkpoints and events usage
future pipeline orchestration
chat-local policy guards for capability, office scope, and PII checks
chat-driven catalog loading, validation, and retrieval document building
chat-driven knowledge index persistence
chat-driven approved query/report execution usage
```

`chat` does not own:

```text
global config loading
telemetry initialization
database pool creation
API key hashing/storage
base response envelope
raw knowledge YAML files
raw SQL catalog files
```

Knowledge and SQL are project-level assets:

```text
knowledge/
queries/
```

They are consumed by the chat pipeline later, but they are not separate crates for now.

Initial `crates/chat/Cargo.toml` should use the short crate name:

```toml
[package]
name = "chat"
version.workspace = true
edition.workspace = true

[dependencies]
app_core = { package = "core", path = "../core" }
anyhow.workspace = true
axum.workspace = true
chrono.workspace = true
serde.workspace = true
serde_json.workspace = true
serde_yaml.workspace = true
sha2.workspace = true
sqlx.workspace = true
tracing.workspace = true
uuid.workspace = true
validator.workspace = true
```

Current internal module layout:

```text
crates/chat/src/
  api/
    dto/
      catalog.rs
      job.rs
      session.rs
    handlers/
      catalog.rs
      job.rs
      session.rs
    routes/
      catalog.rs
      job.rs
      session.rs
  chat/
    classifier.rs
    planner.rs
    model/
      job.rs
      message.rs
      session.rs
    repository/
      job.rs
      message.rs
      session.rs
    service/
      job.rs
      message.rs
      session.rs
  knowledge/
    catalog/
      loader.rs
      validator.rs
    index/
      repository.rs
      sync.rs
    model.rs
    retrieval.rs
  policy/
    authorization.rs
```

Boundary rules inside `chat`:

```text
api = HTTP mapping only, split by catalog/job/session
chat/classifier = deterministic local intent classification before AI/vector
chat/planner = deterministic conversion from matched classification into an atomic execution plan
api::ChatAppState = composition for chat services and the cached validated knowledge catalog
chat/model = durable session/message/job data types, split by concern
chat/repository = PostgreSQL access, split by concern
chat/service = application logic, split by concern
knowledge/catalog = load and validate source YAML/SQL metadata
knowledge/retrieval = build retrieval documents from validated catalog data
knowledge/index = persist generated retrieval documents to app DB search/index tables
policy = chat/report execution guard helpers
```

## 6. Module Setup Order Inside core

Do not create all modules at once.

Add them in this order:

```text
1. config
2. telemetry
3. db
4. api
5. auth
```

Do not add `catalog` or `reporting` modules to `core` now. Catalog files live under `knowledge/`, and reporting execution belongs inside the chat-driven pipeline until a separate non-chat reporting surface exists.

### Step 1: config

Files:

```text
crates/core/src/config.rs
```

Then expose it in `lib.rs`:

```rust
pub mod config;
```

### Step 2: telemetry

Files:

```text
crates/core/src/telemetry.rs
```

Expose:

```rust
pub mod telemetry;
```

### Step 3: db

Files:

```text
crates/core/src/db.rs
```

Expose:

```rust
pub mod db;
```

### Step 4: api

Files:

```text
crates/core/src/api/mod.rs
crates/core/src/api/routes/mod.rs
crates/core/src/api/routes/health.rs
```

Expose in `lib.rs`:

```rust
pub mod api;
```

Expose in `api/mod.rs`:

```rust
pub mod routes;
```

Expose in `api/routes/mod.rs`:

```rust
pub mod health;
```

### Step 5: auth

Files:

```text
crates/core/src/auth/mod.rs
crates/core/src/auth/api_key.rs
crates/core/src/auth/model.rs
crates/core/src/auth/repository.rs
crates/core/src/auth/service.rs
```

Expose in `lib.rs`:

```rust
pub mod auth;
```

Expose in `auth/mod.rs`:

```rust
pub mod api_key;
pub mod model;
pub mod repository;
pub mod service;
```

Current API support modules:

```text
crates/core/src/api/error.rs
crates/core/src/api/response.rs
crates/core/src/api/extractors/validated_json.rs
```

Current implementation note:

```text
Authorization helpers that are specific to report/chat execution live in crates/chat/src/policy/authorization.rs.
Core still owns API key authentication and the ClientContext model.
```

## 7. Initial run() Target

The first real `run()` should do only this:

```text
load .env
load config
init tracing
start HTTP server
serve /health
```

Do not connect databases yet in the first implementation if the HTTP server is not running.

After `/health` works, add database pools and `/ready`.

## 8. Validation Commands

Check workspace:

```bash
cargo metadata --verbose --format-version 1 --all-features --filter-platform aarch64-apple-darwin
```

Check compile:

```bash
cargo check
```

Run app:

```bash
cargo run -p app
```

Test health endpoint after server exists:

```bash
curl http://127.0.0.1:3007/health
```

## 9. What Not To Do Yet

Do not create these crates yet:

```text
api
infra
runtime
knowledge
reporting
ai_report_core
ai_report_api
ai_report_chat
ai_report_runtime
```

Do not create all modules upfront.

Do not split `knowledge` or `reporting` into crates before there is a concrete need.

Do not add reporting execution before health/readiness/auth and chat job foundations are working.

Do not add dynamic SQL generation.

## 10. Current Implementation Position

The initial setup described in this document is complete:

```text
1. Root Cargo.toml is workspace-only.
2. crates/app is the binary entrypoint.
3. crates/core owns shared foundation.
4. crates/chat exists and owns chat-driven reporting feature code.
5. health/readiness/auth foundations are implemented.
6. chat now has separate api, chat, knowledge, and policy modules.
```

Continue with `docs/implementation-steps.md` for the active roadmap.
