# Active Context

Read this before changing code or docs.

## Non-negotiable boundaries

- Keep exactly three crates: `app`, `core`, `chat`.
- Root `Cargo.toml` remains workspace-only and must not contain `[package]`.
- Knowledge remains under `knowledge/`; SQL remains under `queries/`.
- Do not create `crates/knowledge` or `crates/reporting` yet.
- Keep route → service → repository → database. No `sqlx` in route handlers or services.

## Runtime source-of-truth rules

- PostgreSQL is the durable source of truth for chat sessions, messages, jobs, checkpoints, and events.
- Redis is only for live progress/SSE coordination.
- Clarification continues the same job through `POST /chat/jobs/{job_id}/responses`.
- Runtime SQL must come from approved catalog query metadata and approved SQL files.
- The LLM must not generate SQL.

## Security and policy rules

- Raw API keys are returned once and never stored.
- Chat authentication is bearer session JWT plus `role == "admin"`; optional `X-API-Key` is only a voluntary office-scope opt-down and never causes a chat 401.
- Office filtering belongs inside approved SQL using authorized office ids. Do not post-filter in Rust.
- Client-facing errors must be sanitized.
- User-facing language is English only until multilingual extraction, classification, and templates are implemented and tested.

## Documentation rules

- Keep files focused: one file, one responsibility.
- Put current state in `docs/current/`, not in architecture docs.
- Put stable design in `docs/architecture/`.
- Put roadmap/progress in `docs/roadmap/`.
- Put active problems in `docs/issues/active/`.
- Every significant implementation should have a spec in `docs/superpowers/specs/` and a plan in `docs/superpowers/plans/`.
