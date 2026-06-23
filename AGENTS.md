# AGENTS.md

## Current Architecture

- This is a Rust workspace with exactly three crates for now: `crates/app`, `crates/core`, and `crates/chat`. Do not add `api`, `infra`, `runtime`, `knowledge`, `reporting`, or any `ai_report_*` crates yet.
- The root `Cargo.toml` is workspace-only; it must not contain `[package]`.
- Crate names must stay short and direct: `app`, `core`, `chat`. Do not use names like `ai_report_core`, `ai_report_chat`, or `chat_service`.
- `crates/app` is the binary entrypoint and composition root. It wires `core` foundation pieces and the `chat` feature crate.
- `crates/core` owns shared foundation: config, tracing, DB pools, API primitives, auth, extractors, response envelope, validation primitives, and the API key `ClientContext`.
- `crates/chat` owns the main chat-driven reporting feature: API routes/handlers/DTOs, chat sessions/messages/jobs, knowledge catalog/index usage, report policy helpers, checkpoints/events, and future pipeline orchestration.
- Knowledge remains folders/YAML under `knowledge/` and SQL remains under `queries/`; do not create `crates/knowledge` yet.
- Reporting remains part of the chat-driven flow for now; do not create `crates/reporting` yet.
- Keep the existing boundaries: route -> service -> repository -> database. Do not put `sqlx` calls directly in route handlers or services.

## Ponytail Mode

- Use Ponytail by default: prefer the smallest correct change that moves the roadmap forward.
- Stop at the first solution that holds: existing code, stdlib/native feature, already-installed dependency, then minimal new code.
- Do not add speculative abstractions, extra crates, future scaffolding, factories, or interfaces with one implementation.
- Delete or reuse before adding. Keep code boring and local unless reuse is already real.
- Non-trivial logic needs one small runnable check. Do not create broad test scaffolding unless the feature needs it.
- Mark deliberate shortcuts only when there is a real ceiling, for example `// ponytail: global lock, per-job locks if throughput matters`.

## Commands

- Build/check everything: `cargo check`
- Run tests: `cargo test`
- Run the app: `cargo run -p app`
- Format Rust code: `cargo fmt`
- Run migrations manually: `sqlx migrate run --database-url "postgres://root:password@127.0.0.1:5432/ai_reports"`
- Start Redis: `docker compose up -d redis`
- Check Redis: `docker compose exec -T redis redis-cli ping`

## Local Runtime

- Local app port is `3007` from `.env`; use `http://127.0.0.1:3007` in examples.
- Redis must run through Docker Compose, not Homebrew/local install. It maps host port `6380` to container port `6379` because local port `6379` may be occupied.
- Health endpoints: `GET /health`, `GET /ready`.
- `/ready` checks App DB, Fineract DB, pgvector, and Redis when `REDIS_ENABLED=true`.
- Startup logs should show environment, address, health URL, ready URL, and dependency readiness.

## Database And Migrations

- App DB is PostgreSQL database `ai_reports`; Fineract DB is read-only/replica via `FINERACT_DATABASE_URL`.
- `pgvector` is a PostgreSQL extension in the app DB, not a separate vector service.
- Schema changes belong in `migrations/*.sql`. Do not create or alter tables from application startup code.
- `.env` currently has `APP_DATABASE_MIGRATE_ON_STARTUP=true` for local/dev. Default policy should remain false outside local/dev.

## API And Validation Conventions

- All API responses use the envelope: `{ "success": bool, "data": ..., "error": ... }`.
- Use `validator` derive plus the global `ValidatedJson<T>` extractor for request validation. Do not hand-roll per-route JSON validators unless there is no reasonable crate support.
- Keep client-facing errors sanitized. Log parser/internal details with tracing, but do not return raw Serde/Axum parser messages, stack traces, SQL, prompts, or secrets to clients.
- MVP user-facing language is English only. Do not add Indonesian classifier phrases, clarification text, response templates, or examples unless multilingual support is explicitly added later.

## Auth Status And Rules

- Implemented: `POST /auth/api-keys`, bootstrap admin token auth, API key hashing, `ApiKeyRepository`, `AuthService`, API key authentication extractor, `GET /auth/me`, consistent response envelope.
- Raw API keys are returned once and never stored. DB stores `key_hash` and `key_prefix` only.
- Minimal authorization helpers exist in `crates/chat/src/policy/authorization.rs` for capability, office-scope, and PII checks. Enforce them in protected chat/report execution before approved queries run.
- `allowed_office_ids` is stored and helper-validated, but report SQL filtering is not implemented yet. Office filtering must be enforced inside approved report queries once report execution exists.

## Chat/Job Design Decisions

- Durable chat state belongs in PostgreSQL: `chat_sessions`, `chat_messages`, `chat_jobs`, `chat_job_checkpoints`, `chat_job_events`.
- Redis is only for live progress/SSE coordination: `chat_job:{job_id}:live_state`, `chat_job:{job_id}:latest_event`, `chat_job:{job_id}:lock`.
- Memory is never the source of truth for resumable jobs.
- Save PostgreSQL checkpoints only at important boundaries; do not write every heartbeat/progress update to PostgreSQL.
- Clarification must continue the same job via `POST /chat/jobs/{job_id}/responses`; do not create a new job for clarification answers.

## Current Implementation Order

- Follow `docs/implementation-steps.md` as the active roadmap.
- Completed: baseline, app bootstrap, DB pools/readiness, API key generation/authentication, reporting scope/capability/PII docs, chat session/job migrations, workspace alignment to `app` + `core` + `chat`, and current chat module split.
- Partially done: Phase 9 chat job API foundation, Phase 10 catalog foundation, and knowledge index persistence without embeddings.
- Next: finish Phase 9 checkpoint/event coverage and real Redis-backed SSE behavior, then finish catalog config and `POST /catalog/validate`, then Phase 11 SQL safety validation.

## Important References

- `docs/project-setup.md`: current workspace/crate setup rules.
- `docs/implementation-steps.md`: active phase roadmap.
- `docs/chat-data-model.md`: chat/session/job tables and Redis state rules.
- `docs/ai-reporting-design.md`: broader AI reporting architecture.
- `docs/reporting-data-scope.md`: approved/deferred reporting data scope.
- `docs/reporting-capabilities.md`: executable capability rules.
- `docs/reporting-pii-policy.md`: PII/masking/never-expose rules.
