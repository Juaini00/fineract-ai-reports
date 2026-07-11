# Project Setup: 3. app Crate

Source: `docs-old/project-setup.md`

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
