# Project Setup

This document defines the exact project setup we will use. It intentionally avoids the larger multi-crate design for now.

The current implementation should use only two crates:

```text
app
core
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
```

Meaning:

```text
app  = binary entrypoint
core = application library
```

Do not add more crates yet.

Do not use crate names like `ai_report_core` or `ai_report_app`.

The crate names must be:

```text
app
core
```

## 2. Root Cargo.toml

The root `Cargo.toml` must be a workspace manifest only.

It must not contain `[package]`.

Correct root structure:

```toml
[workspace]
members = [
    "crates/app",
    "crates/core",
]
resolver = "3"

[workspace.package]
version = "0.0.1"
edition = "2024"

[workspace.dependencies]
anyhow = "1.0.102"
async-trait = "0.1.89"
tokio = { version = "1.52.3", features = ["full"] }
tracing = "0.1.44"
tracing-subscriber = { version = "0.3.23", features = ["env-filter"] }
axum = "0.8.9"
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"
serde_yaml = "0.9.34"
sqlx = { version = "0.9.0", features = ["runtime-tokio", "tls-rustls", "postgres", "macros", "chrono", "uuid", "rust_decimal"] }
dotenvy = "0.15.7"
config = "0.15.23"
thiserror = "2.0.18"
tower-http = { version = "0.6.11", features = ["trace", "tracing"] }
uuid = { version = "1.23.3", features = ["serde", "v4"] }
chrono = { version = "0.4.45", features = ["serde"] }
redis = { version = "1.2.3", features = ["tokio-comp"] }
pgvector = { version = "0.4.2", features = ["sqlx"] }
rand = "0.8.6"
sha2 = "0.10.9"
hex = "0.4.3"
validator = { version = "0.20.0", features = ["derive"] }
```

Why root has no `[package]`:

```text
The root is not an app.
The root only manages the workspace.
The runnable binary is crates/app.
```

## 3. app Crate

`app` is only the binary launcher.

Path:

```text
crates/app/src/main.rs
```

`app` should not contain business logic.

`app` should only call the `core` crate through the local alias `app_core`.

Expected `crates/app/Cargo.toml`:

```toml
[package]
name = "app"
version.workspace = true
edition.workspace = true

[dependencies]
app_core = { package = "core", path = "../core" }
anyhow.workspace = true
tokio.workspace = true
```

Expected `crates/app/src/main.rs`:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app_core::run().await
}
```

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

`core` is the application library.

Path:

```text
crates/core/src/lib.rs
```

`core` owns:

```text
config loading
tracing setup
database pool setup
HTTP router setup
auth service
health/readiness handlers
future reporting/catalog logic
```

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

## 5. Module Setup Order Inside core

Do not create all modules at once.

Add them in this order:

```text
1. config
2. telemetry
3. db
4. api
5. auth
6. catalog
7. reporting
```

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

## 6. Initial run() Target

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

## 7. Validation Commands

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

## 8. What Not To Do Yet

Do not create these crates yet:

```text
api
infra
runtime
ai_report_core
ai_report_api
ai_report_runtime
```

Do not create all modules upfront.

Do not add reporting code before health/readiness/auth are working.

Do not add dynamic SQL generation.

## 9. Immediate Next Implementation

The immediate next implementation should be:

```text
1. Add crates/core.
2. Add core::run().
3. Make app call app_core::run().
4. Make cargo check pass.
5. Add config module.
6. Add telemetry module.
7. Add /health endpoint.
8. Run app and curl /health.
```
