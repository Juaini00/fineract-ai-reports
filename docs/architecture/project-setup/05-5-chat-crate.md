# Project Setup: 5. chat Crate

Source: `docs-old/project-setup.md`

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
