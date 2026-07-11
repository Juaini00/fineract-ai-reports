# Project Setup: 4. core Crate

Source: `docs-old/project-setup.md`

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
