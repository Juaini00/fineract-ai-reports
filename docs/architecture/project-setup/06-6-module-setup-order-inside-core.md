# Project Setup: 6. Module Setup Order Inside core

Source: `docs-old/project-setup.md`

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
