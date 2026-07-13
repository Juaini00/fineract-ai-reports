# AGENTS.md

Operating contract for AI agents. Read fully once per session; do not re-scan the repo before consulting the docs referenced here.

## Before you scan the code

1. Read `docs/current/status.md` (what's implemented / partial / pending).
2. Read `docs/current/active-context.md` (rules for edits happening now).
3. Consult the **Doc trigger table** below — jump to the specific doc for the area you're changing.
4. Only scan source code if the answer isn't already in the docs.

Re-scanning the repo when a doc file already answers the question is the #1 way tokens are wasted here.

## Doc trigger table (read the file when its trigger fires)

| Trigger | Read |
| --- | --- |
| Any edit today | `docs/current/status.md`, `docs/current/active-context.md` |
| Deciding what to work on next | `docs/current/next-work.md` |
| Any architectural / cross-crate change | `docs/architecture/overview.md` |
| Touching chat / session / job code | `docs/architecture/chat-data-model.md` |
| Touching a response formatter or SQL result path | `docs/product/pii-policy/` |
| Adding / changing a capability | `docs/product/reporting-capabilities/`, `docs/product/reporting-data-scope/`, `knowledge/**/*.yaml`, `queries/**/*.sql` |
| Running / verifying the app | `docs/runtime/README.md` |
| Migration or schema question | `migrations/*.sql` (source of truth), `docs/architecture/chat-data-model.md` |
| Planning a phase / picking next task | `docs/roadmap/implementation-roadmap.md` |
| Filing or reading an issue | `docs/issues/` |
| Writing a spec or implementation plan | `docs/superpowers/` |

## Workspace layout (locked)

Three crates only: `crates/app`, `crates/core`, `crates/chat`. Names stay short — never `ai_report_*`. Do not add `api`, `infra`, `runtime`, `knowledge`, `reporting` crates.

- `app` — binary entrypoint + composition root. Wires `core` + `chat`.
- `core` — config, tracing, DB pools, Redis, API envelope, `ValidatedJson<T>`, `ApiError`, auth (`ApiKeyRepository`, `AuthService`, `AuthenticatedClient`, `ClientContext`). `AuthenticatedClient` is generic over any `S: FromRef<AuthService>`.
- `chat` — chat-driven reporting: `chat::{model, repository, service}`, `chat::knowledge::{catalog, index}`, `chat::api::{handlers, routes, dto}`, `chat::policy::authorization` (wired via `chat::chat::planner::evaluate_policy` before `executor::execute_plan`).

Knowledge stays as YAML under `knowledge/`, SQL under `queries/`, schema changes only via `migrations/*.sql`.

## Architectural invariants

- Layer order: `route → service → repository → database`. **No `sqlx` calls in handlers or services** — repositories only.
- All HTTP responses: envelope `{ success, data, error }`. Errors via `ApiError`. Never leak raw Serde / Axum / SQL / stack / prompt text.
- Request validation: `validator` derive + global `ValidatedJson<T>`. No hand-rolled per-route validators.
- Durable chat/job state → PostgreSQL (`chat_sessions`, `chat_messages`, `chat_jobs`, `chat_job_checkpoints`, `chat_job_events`). Redis is only for live SSE keys (`chat_job:{id}:live_state` / `:latest_event` / `:lock`). Checkpoints at meaningful boundaries, not per heartbeat.
- Clarification continues the same job via `POST /chat/jobs/{job_id}/responses`. Never spawn a new job for clarification.
- Office scope enforced inside approved SQL via bound `office_ids` parameter — never post-filter in Rust.
- API keys: raw key returned once, DB stores `key_hash` + `key_prefix` only. `POST /auth/api-keys` gated by `AUTH_BOOTSTRAP_ADMIN_TOKEN`.
- MVP language: English only. No Indonesian classifier / clarification / template text.
- Schema changes only via `migrations/*.sql`. App startup must never create/alter tables. `APP_DATABASE_MIGRATE_ON_STARTUP=true` is local/dev only.

## Multi-agent workflow

Primary session = orchestrator. Delegate to protect context, not as procedure. Default: do it yourself.

### Delegation threshold

| Situation | Action |
| --- | --- |
| Single file, <50 LOC, path clear | Inline. No subagent. |
| Rename / format / typo / obvious diff | Inline. No subagent. No skill load. |
| >3 files or >1 module change | `task-executor` |
| Reading many files to answer a question | `code-explorer` |
| Doc lookup | `documentation-researcher` |
| Build / test failure with unknown root cause | `debugger` — on demand only |
| Touching auth / SQL / migrations / money paths, OR diff >100 LOC | `code-reviewer` — on demand only |
| Rest of the time | No review. No debugger. Not a pipeline. |

Model is chosen once at session level (opencode TUI / `opencode.json`). Agents do not override it. Efficiency comes from *when* to delegate, not from swapping models mid-task. After a plan is agreed, delegate mechanical implementation to `task-executor` so the primary session doesn't burn context on rename / format / edit.

### Subagent input contract

When delegating, ALWAYS pass:

- `task`: one-sentence goal.
- `target_files`: explicit `path:line-range` list.
- `already_known`: symbols / decisions / conventions parent has already established.
- `budget`: max tool calls or LOC.

Subagents MUST NOT re-explore paths in `already_known` without stating a reason.

## Development principles (Ponytail default)

- Smallest correct change that moves the roadmap forward.
- Stop at the first working rung: existing code → stdlib/native → installed dep → then minimal new code.
- No speculative abstractions, factories, one-impl interfaces, "for later" scaffolding.
- Delete or reuse before adding. Boring and local wins.
- Non-trivial logic needs one small runnable check. No broad test scaffolding unless the feature needs it.
- Mark deliberate shortcuts only with a real ceiling: `// ponytail: global lock, per-job locks if throughput matters`.

## File editing

- Native `Edit` / `Write` first. `ctx_edit` fallback. Never MCP write when native works.
- Shell heredocs / `sed` / `python3 -c` for edits are forbidden unless doing a bulk mechanical transform.
- `ctx_read` / `ctx_search` / `ctx_shell` / `ctx_tree` before native `Read` / `Grep` / `Bash`.

## Commands

```bash
cargo check                    # workspace type-check
cargo test                     # all tests
cargo test -p chat <name>      # one crate, substring match
cargo run -p app               # HTTP on :3007
cargo fmt
docker compose up -d redis     # Redis on host :6380 → container :6379
sqlx migrate run --database-url "postgres://root:password@127.0.0.1:5432/ai_reports"
```

Local app: `http://127.0.0.1:3007`. Health: `GET /health`, `GET /ready` (checks app DB, Fineract DB, pgvector, Redis when `REDIS_ENABLED=true`).

## Postman MCP (only when doing API verification)

- Not loaded by default. Enable in `~/.config/opencode/opencode.json` per-session when needed.
- Read `postman://instructions` first.
- Collection: `fineract report`, workspace `cms-rivolta`, folder `chat`.
- Vars: `BASE_URL`, `LOCAL_ADMIN_TOKEN`, `API_KEY`, `SESSION_ID`, `JOB_ID`.
- Prefer `searchPostmanElements` → `getCollection model=full` → inspect/update.
- Fallback: hit `http://127.0.0.1:3007` locally, keep secrets out of output.
