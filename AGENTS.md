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

## Multi-Agent Workflow

Primary session = orchestrator. Delegate to protect the main context window, not as procedure. Default: do it yourself.

### When to delegate

- Task touches > 3 files or > 1 module → delegate the implementation.
- Investigation that requires reading many files → delegate to an explorer.
- Single file, < 50 lines, path is clear → do it inline, no agent.

### Two roles only

- **Brainstorm / Plan (strong model)** — primary session. Understand the problem, map the files that will change, write a short plan.
- **Executor (cheap model, e.g. haiku)** — mechanical implementation against the plan. Does not make architectural decisions.

Review, test, and debug are not mandatory steps. Call them on demand: build fails → debugger; sensitive change (auth, SQL, migrations) → reviewer. Do not run them as a fixed pipeline.

## Development Principles

- Keep changes minimal.
- Respect existing architecture.
- Preserve project conventions.
- Never expand scope unnecessarily.
- Validate changes before reporting completion.
- Report assumptions whenever uncertainty exists.

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

## File Editing

- Do not use Python/Node/shell scripts to edit files unless editing many files, doing a mechanical transformation, or native edit/`ctx_edit` failed.
- For normal file edits, use native Edit/StrReplace first, then lean-ctx `ctx_edit`.
- Use shell for runtime commands only (`cargo test`, `cargo check`, `cargo fmt`, Docker, migrations), not for file rewrites.
- Never use `python3 - <<'PY'`, `node -e`, `perl -pi`, or shell heredocs for file edits when Edit/StrReplace or `ctx_edit` can do the change.

## Postman MCP Workflow

- Use Postman MCP for API collection discovery, request inspection, and request/collection updates when verifying API behavior.
- Read `postman://instructions` before Postman API work.
- Primary local verification collection: `fineract report` in workspace `cms-rivolta`, folder `chat`.
- Collection variables are expected for local use: `BASE_URL`, `LOCAL_ADMIN_TOKEN`, `API_KEY`, `SESSION_ID`, and `JOB_ID`.
- Preferred Postman MCP flow: `searchPostmanElements` for the collection, `getCollection` with `model=full`, then inspect/update the relevant request definitions.
- If the active Postman MCP tool profile has no request/collection runner, execute the same request sequence locally against `http://127.0.0.1:3007` and keep secrets out of command output.

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
- Authorization helpers in `crates/chat/src/policy/authorization.rs` (capability, office-scope, PII) are wired into the chat job pipeline: `chat::chat::planner::evaluate_policy` runs before `chat::chat::executor::execute_plan`, and execution is blocked when the decision is not `Allowed`.
- Office filtering is enforced inside approved SQL: `queries/savings/*.sql` use `office_id = ANY($3::bigint[])` and the executor binds `policy_decision.office_ids` to that parameter. New approved queries must follow the same pattern — do not post-filter office scope in Rust.

## Chat/Job Design Decisions

- Durable chat state belongs in PostgreSQL: `chat_sessions`, `chat_messages`, `chat_jobs`, `chat_job_checkpoints`, `chat_job_events`.
- Redis is only for live progress/SSE coordination: `chat_job:{job_id}:live_state`, `chat_job:{job_id}:latest_event`, `chat_job:{job_id}:lock`.
- Memory is never the source of truth for resumable jobs.
- Save PostgreSQL checkpoints only at important boundaries; do not write every heartbeat/progress update to PostgreSQL.
- Clarification must continue the same job via `POST /chat/jobs/{job_id}/responses`; do not create a new job for clarification answers.

## Current Implementation Order

- Follow `docs/implementation-steps.md` as the active roadmap.
- Completed: baseline, app bootstrap, DB pools/readiness, API key generation/authentication, reporting scope/capability/PII docs, chat session/job migrations, workspace alignment to `app` + `core` + `chat`, and current chat module split.
- Partially done: Phase 10 catalog foundation (schema/metrics/policies/responses load as generic knowledge; typed field schemas still pending), Phase 17 LLM provider integration (constrained OpenAI-compatible planner fallback; response formatting fallback pending), Phase 18 retrieval breadth.
- Done since last update: Phase 9 background worker + Redis-backed SSE (`JobService::emit_event` + spawned `run_pipeline`), Phase 11 runtime SQL validation via `validate_runtime` wired into `POST /catalog/validate`, Phase 18 admin endpoints `POST /vector-index/rebuild` and `GET /vector-index/status`, Phase 19 savings runtime matrix with 9 approved capabilities.
- Next: typed schema/metric/policy/response validation, broader LLM context consumption, then new non-savings capabilities after data-scope promotion.

## Important References

- `docs/project-setup.md`: current workspace/crate setup rules.
- `docs/implementation-steps.md`: active phase roadmap.
- `docs/chat-data-model.md`: chat/session/job tables and Redis state rules.
- `docs/ai-reporting-design.md`: broader AI reporting architecture.
- `docs/reporting-data-scope.md`: approved/deferred reporting data scope.
- `docs/reporting-capabilities.md`: executable capability rules.
- `docs/reporting-pii-policy.md`: PII/masking/never-expose rules.
