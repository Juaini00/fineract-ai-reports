# Management observability and audit implementation plan

> **Status:** proposed. Do not implement until the reviewer accepts
> `docs/superpowers/specs/2026-07-23-management-observability-and-audit-design.md`.

**Goal:** Deliver a secure admin management API for knowledge inventory, audit detail/search, LLM token-cost analytics, and operational status, backed by durable decision audit and measurable telemetry.

**Architecture:** Keep all feature code in `chat`, shared bearer-admin extraction/config/API primitives in `core`, and wiring in `app` only where composition requires it. Material job/session decisions write a PostgreSQL transactional outbox in the same transaction; a dispatcher idempotently publishes immutable management audit events. High-volume LLM/context data stays telemetry with persisted loss counters. Management routes read safe projections through service/repository layers.

**Tech stack:** Rust, Axum, SQLx/PostgreSQL, Serde/Schemars, Tokio, existing `KnowledgeCatalog`, existing job/audit/LLM trace infrastructure, `validator`, existing API envelope/auth extractors.

## Preconditions and non-negotiable constraints

- Work on branch `feature/management-observability-audit`; do **not** create a worktree.
- Read the issue, design spec, `docs/current/status.md`, and `docs/current/active-context.md` before each implementation phase.
- Keep exactly `app`, `core`, and `chat` crates.
- Preserve `route -> service -> repository -> database`; no `sqlx` in handlers/services.
- PostgreSQL is durable; Redis/SSE are never audit authority.
- Management authentication is bearer JWT plus `role == admin`; API keys never authenticate or scope management calls.
- Preserve current `/chat/jobs/{job_id}/audit` behavior during this work.
- Preserve approved-catalog SQL, bound office scope, PII policy, sanitized external errors, and English-only user-facing copy.
- Do not add a knowledge approval workflow, reference-document authoring, global session summary, provider fallback, or cost quotas.
- Add migrations only under `migrations/`; startup must not create schema.
- Every task must run its focused tests before proceeding. Run `cargo fmt`, `cargo check`, and relevant broader tests at phase boundaries.

## Orchestrator execution model

Implementation will be delegated task-by-task by the primary orchestrator; no separate worktree agent is required.

For every delegated task, the orchestrator supplies:

- `task`: one-sentence result;
- `target_files`: exact paths and line ranges discovered at execution time;
- `already_known`: the decisions in this plan/spec and relevant existing symbols;
- `budget`: maximum tool calls/LOC.

Suggested allocation:

| Work | Agent type | Model |
| --- | --- | --- |
| Source exploration, contract-test design, documentation verification | code explorer/reviewer | `openai-codex/gpt-5.6-sol` |
| Isolated DTO/repository/migration/API implementation | task executor | `openai-codex/gpt-5.6-terra` |
| Auth, transactional audit/outbox, redaction, migration review | code reviewer | `openai-codex/gpt-5.5` |
| Test-log summary and formatting checks | lightweight checker | `openai-codex/gpt-5.6-luna` |

The primary session retains integration decisions, merges each task, runs final verification, and does not delegate a broad unbounded “implement the ticket” request.

## Phase 0 — Baseline and contract lock

### Task 0.1: Record repository baseline

**Read:**
- `docs/current/status.md`
- `docs/current/active-context.md`
- issue 006 and its design spec
- current audit migrations and `crates/chat/src/audit/`

**Steps:**

1. Confirm current branch and working-tree status. Existing documentation changes for issue/spec/plan belong to this branch.
2. Record `git rev-parse --short HEAD` in execution notes.
3. Run:

```bash
cargo fmt --check
cargo check
cargo test -p chat --lib
```

4. Stop and record exact output if baseline fails; do not hide a pre-existing failure by changing expectations.

**Acceptance:** Baseline result is known and branch is correct before implementation begins.

### Task 0.2: Freeze client contract fixtures

**Files:**
- Add `crates/chat/tests/fixtures/management/*.json`
- Add `crates/chat/tests/management_contracts.rs` or the repository's existing API contract test location
- Read `crates/chat/src/api/dto/`, response conventions, and existing test fixtures

**Steps:**

1. Add fixture JSON for `management/status`, catalog knowledge list/detail, audit job detail, audit list cursor page, LLM usage, disabled reference knowledge, warnings, and sanitized errors.
2. Write failing serialization/schema tests that assert field names, stable enum spellings, envelope behavior, opaque cursors, and absence of unsafe fields.
3. Do not expose endpoint implementations yet; fixtures are the frontend integration baseline.

**Acceptance:** Client payload shapes are reviewable and testable before database/API work.

## Phase 1 — Shared contracts, admin auth, and migrations

### Task 1.1: Add management DTOs and pure domain contracts

**Files:**
- Add `crates/chat/src/management/mod.rs`
- Add `crates/chat/src/management/model.rs`
- Add `crates/chat/src/api/dto/management.rs`
- Modify `crates/chat/src/api/dto/mod.rs`
- Tests from Task 0.2

**Steps:**

1. Define typed public enums: knowledge kind/status/execution mode, audit event type/outcome, telemetry/health status, LLM group-by, and warning code.
2. Define request query DTOs with `validator` constraints for limit/filter/time range; define opaque cursor value types.
3. Define internal safe audit summary types separately from public DTOs. JSONB fields must be built from typed allowlisted structs.
4. Use Serde/Schemars consistently with current response DTO conventions.
5. Prove old JSON does not accidentally deserialize into a privileged/executable mode.

**Focused tests:** DTO serialization/deserialization, invalid enum/filter/range, schema snapshots.

**Acceptance:** No handler/repository accepts arbitrary `serde_json::Value` as a public management event summary.

### Task 1.2: Add a management-admin extractor

**Files:**
- Add/modify the appropriate `crates/core/src/api/extractors/` module
- Modify extractor re-exports
- Add extractor tests near existing authentication tests

**Steps:**

1. Create `AuthenticatedManagementAdmin` using bearer `AuthenticatedUser` only.
2. Require `role == admin`; return the project-standard sanitized forbidden error otherwise.
3. Do not invoke `AuthenticatedClient`, consume `X-API-Key`, or project office scope.
4. Expose only actor data needed by management services.

**Focused tests:** no bearer -> rejection; non-admin bearer -> forbidden; admin bearer -> success; valid/invalid/missing `X-API-Key` has no effect.

**Acceptance:** Management auth behavior is independent of the chat API-key compatibility behavior.

### Task 1.3: Add additive audit/outbox migrations

**Files:**
- Add a timestamped SQL migration under `migrations/`
- Add SQLx model/repository tests as supported by the repository
- Read all existing assistant/audit/user ownership migrations first

**Steps:**

1. Add `management_audit_outbox`, `management_audit_events`, and `management_telemetry_counters` with foreign keys, constraints, indexes, and timestamps from the spec.
2. Extend `assistant_llm_traces` only with additive nullable/default-safe columns necessary for correlation/version/price/error normalization.
3. Use check constraints for public status/outcome where stable enough; preserve forward-compatible internal event handling via a migration-reviewed strategy.
4. Ensure audit references survive session deletion: do not use `ON DELETE CASCADE` from session/job to new decision audit rows. Choose `SET NULL` or snapshot identifiers according to spec and test it.
5. Run migration on an empty local database and, if available, migration smoke tests/up/down policy checks. Do not edit old migration files.

**Reviewer checkpoint:** Assign a `gpt-5.5` reviewer for FK/delete behavior, indexes, transactional boundary, and data migration safety before proceeding.

**Acceptance:** Schema supports durable events/outbox and indexed management reads without weakening existing ownership/auth data.

## Phase 2 — Durable decision audit foundation

### Task 2.1: Implement safe event builder and repositories

**Files:**
- Add `crates/chat/src/management/audit.rs`
- Add `crates/chat/src/management/repository.rs` or focused repository modules
- Modify `crates/chat/src/management/mod.rs`
- Tests colocated with modules

**Steps:**

1. Implement typed event construction with an allowlist per `event_type`.
2. Normalize errors to stable code/category and optional safe message; never pass `anyhow`/provider error strings straight to persistence or response.
3. Implement repository methods to enqueue within a caller-owned SQL transaction, publish idempotently by `outbox_id`, list event pages, read job timeline, and update telemetry counters.
4. Add cursor ordering on `(occurred_at, id)` and encode both values opaquely.
5. Ensure repository methods are the sole SQL boundary.

**Focused tests:** event redaction/validation, deterministic cursor round trip, no duplicate publication on retry, safe error conversion.

**Acceptance:** Events cannot contain raw prompt/SQL/result content through typed construction APIs.

### Task 2.2: Implement outbox dispatcher and health counters

**Files:**
- Add/modify `crates/chat/src/management/outbox.rs`
- Modify `crates/chat/src/api/mod.rs` composition/startup
- Tests for dispatcher behavior

**Steps:**

1. Add a bounded, restart-safe polling dispatcher that leases unpublished outbox rows transactionally and publishes idempotently.
2. Retry with bounded backoff and record attempt/error code; do not discard decision outbox records on failure.
3. Expose pending/oldest pending/outcome health values through a repository query.
4. Keep existing `AuditHandle` operational only as telemetry during transition. Add persisted counters for queue/full/persist failures instead of warning-only loss.
5. Ensure graceful shutdown flushes/finishes safely without claiming all queued telemetry persisted when it did not.

**Focused tests:** dispatcher resumes after simulated failure, duplicate dispatch idempotency, outbox event survives process restart boundary through DB state, counter updates.

**Acceptance:** A material decision is never represented only by a best-effort mpsc send.

### Task 2.3: Instrument material chat/session lifecycle events

**Files:**
- `crates/chat/src/job/service/`
- `crates/chat/src/conversation/service/`
- job/session repositories as required for transaction ownership
- assistant policy/execution boundary modules discovered during implementation
- Tests for job and session lifecycle

**Steps:**

1. Identify every transaction that creates a job, transitions terminal state, writes clarification state, authorizes/blocks/executes a query, and archives/deletes a session.
2. In the same transaction, enqueue the corresponding required decision event. If a current service cannot supply a transaction, refactor minimally so repository methods own the atomic write.
3. Capture actor, correlation ID, catalog/index snapshot, capability/query/policy identifiers, safe office scope mode, outcome, and sanitized summaries.
4. Do not duplicate issue 003 field provenance storage; link only its final safe verified-payload summary when available.
5. Preserve the old job audit event write/API until compatibility mapping is verified.

**Focused tests:** completed, clarification, blocked, unsupported, failed, execution-completed, archived, and deleted paths each persist correct ordered events; no raw SQL/PII leaks.

**Reviewer checkpoint:** `gpt-5.5` review for atomicity, policy/execution placement, and redaction.

**Acceptance:** Required job outcomes have a durable decision timeline even if the old telemetry queue is full.

## Phase 3 — Context and LLM telemetry normalization

### Task 3.1: Add context-assembly summary

**Files:**
- Context builder/runtime modules under `crates/chat/src/assistant/`
- Management event builder/repository
- Assistant scenario/contract tests

**Steps:**

1. Find the one context assembly boundary used by runtime LLM calls.
2. Emit a typed `context.assembled` decision event or linked telemetry record with budget, estimated tokens, categories, selected identifiers/scores, and truncation reason.
3. Do not log raw user messages, history, prompt templates, document chunks, policy text, or authorization payloads.
4. Represent missing token estimates explicitly and preserve technical context-window exceeded behavior.

**Focused tests:** normal context, truncated history/evidence, no raw content in persisted JSON, context-window failure.

**Acceptance:** The audit explains context selection without turning audit into another knowledge store.

### Task 3.2: Normalize LLM trace semantics and cost estimates

**Files:**
- `crates/chat/src/audit/llm_trace_repository.rs`
- `crates/chat/src/assistant/llm/traced_client.rs`
- LLM configuration modules under `crates/core/src/config/` as needed
- Tests for trace persistence/aggregation inputs

**Steps:**

1. Attach correlation/catalog/index/context contract data where available.
2. Replace externally visible arbitrary `error_kind` text with normalized error code/status; retain only safe internal diagnostics where explicitly permitted.
3. Represent usage as provider-reported, estimated, or unavailable. Do not convert unavailable values into zero.
4. Add versioned price metadata. Cost is null/unavailable if price data is absent/stale; never fabricate an estimate.
5. Instrument dropped telemetry counters on every queue and persistence failure path.

**Focused tests:** successful/malformed/timeout/error traces, unavailable usage, unavailable pricing, different price versions, and error redaction.

**Acceptance:** LLM analytics can distinguish zero use from unknown use/cost and cannot leak provider input through errors.

## Phase 4 — Management read services and HTTP APIs

### Task 4.1: Knowledge inventory service and endpoints

**Files:**
- Add `crates/chat/src/management/service.rs` or focused modules
- Add `crates/chat/src/api/routes/management.rs`
- Add `crates/chat/src/api/handlers/management.rs`
- Modify `crates/chat/src/api/{routes,handlers,mod}.rs`
- DTO/contract tests

**Steps:**

1. Project validated in-memory catalog items into `catalog:<id>` inventory rows, including availability and execution mode.
2. Map current catalog statuses deterministically; do not reinterpret `approved_mvp` as a human approval workflow.
3. Return an explicit disabled/empty reference-knowledge state until a reference source exists.
4. Add list/detail routes protected by `AuthenticatedManagementAdmin`; validate cursor/filter/limit through DTOs.
5. Reuse catalog metadata; never duplicate YAML parsing or expose query SQL.

**Focused tests:** catalog list/detail, deferred/unavailable filter, invalid ID, disabled reference kind, admin-only auth, no SQL/output secret fields.

**Acceptance:** A frontend can build a complete knowledge browser without guessing executability or reading YAML directly.

### Task 4.2: Audit detail/search service and endpoints

**Files:**
- Management repositories/services/DTOs/routes/handlers
- Existing job audit DTO/handler only if compatibility adapter is needed
- API integration tests

**Steps:**

1. Implement job detail from `management_audit_events` plus safe LLM trace projection.
2. Implement newest-first audit search with required bounded time range, filters, opaque cursor, and indexes established in Phase 1.
3. Treat unknown/non-visible job IDs as sanitized `404` according to current resource-hiding conventions.
4. Keep `GET /chat/jobs/{id}/audit` stable. If it shares data, create an adapter; do not silently replace its JSON shape.

**Focused tests:** event order, cursor continuation/no duplicates, all filters, invalid date span/cursor, empty result, missing job, redaction, admin-only access.

**Acceptance:** Client audit screens need no database access and no SSE replay to recover history.

### Task 4.3: Status and LLM usage endpoints

**Files:**
- Management repositories/services/DTOs/routes/handlers
- Configuration access modules only as needed
- API/repository tests

**Steps:**

1. Read safe configured provider/model identity, latest catalog/index version/status, outbox/telemetry health, and feature flags for `/management/status`.
2. Implement LLM aggregate SQL in repositories for required time range and one group dimension at a time.
3. Aggregate cost only for compatible currency/price version; otherwise return groups/warnings rather than an invalid grand total.
4. Implement advisory warnings deterministically from config/known counters. Do not add quotas or job blocking.
5. Return p95 latency only when SQL/database capabilities and sample size are defined; otherwise return null with an explicit warning/metadata contract.

**Focused tests:** daily/model/purpose/status groupings, missing usage, price mismatch, telemetry drop warning, invalid dates/group, no secret config exposure.

**Acceptance:** Client can render cost/usage/error/latency dashboards accurately without treating estimates as invoices.

## Phase 5 — Retention semantics, compatibility, and documentation

### Task 5.1: Define and test archive/delete retention behavior

**Files:**
- Session service/repository and management audit repository/tests
- `docs/architecture/audit-trail/` updates as appropriate
- Runtime/API docs

**Steps:**

1. Verify actual current archive/delete semantics and distinguish soft archive from permanent delete.
2. Ensure an audit event is enqueued atomically before the operation completes.
3. Prove management audit remains queryable after a session is hidden/deleted according to the chosen FK strategy.
4. Document that automatic audit/telemetry purging is not implemented; do not add a scheduled purge without a separately approved retention policy.

**Acceptance:** No management user can assume chat deletion silently destroys audit evidence.

### Task 5.2: Publish client integration materials

**Files:**
- `docs/api/README.md`
- Add `docs/current/management-client-integration.md`
- Update issue 006 implementation notes/current status only after verification
- Contract fixtures from Phase 0

**Steps:**

1. Document endpoint authentication, query validation, cursors, error handling, all response shapes, enum fallback rules, and warnings.
2. Include TypeScript-ready interface examples or JSON Schema/OpenAPI references consistent with existing documentation conventions.
3. State clearly which fields are estimates, which are unavailable, which records are decision audit vs telemetry, and that reference knowledge may be disabled.
4. Document migration path from legacy job audit endpoint without forcing clients to parse internal fields.

**Acceptance:** A client team can implement all management screens from the published contract and fixtures alone.

## Phase 6 — Final verification and release gate

### Task 6.1: Full test and security review

**Steps:**

1. Run focused tests after each task, then run:

```bash
cargo fmt --check
cargo check
cargo test -p core
cargo test -p chat
cargo test
```

2. Run migration smoke/`sqlx migrate info` against the approved local database when available.
3. Run `git diff --check`.
4. Request final `gpt-5.5` adversarial review covering auth, outbox durability, SQL/migration correctness, retention/FKs, redaction, cursor query safety, and cost aggregation.
5. Resolve findings or document explicitly deferred findings in issue 006; do not call the feature complete with unresolved security/durability defects.

### Task 6.2: Manual API acceptance

Use a seeded admin session and known jobs to verify:

1. management routes reject no bearer, non-admin bearer, and API-key-only requests;
2. knowledge inventory/detail renders catalog items and disabled reference state;
3. completed, blocked, clarified, unsupported, and failed job timelines appear in detail/search;
4. no response contains raw SQL, API-key material, secrets, raw prompt/history/chunk text, result rows, or stack traces;
5. LLM usage accurately reports known versus unavailable values and price-version warnings;
6. audit/status reports telemetry loss/outbox delay if deliberately simulated;
7. archive/delete retains the documented audit timeline.

**Release acceptance:** Every design acceptance gate is backed by automated test evidence or recorded manual API evidence. Update `docs/current/status.md` and issue 006 only after this gate passes.

## Commit boundaries

Use small reviewable commits after each completed phase or independent task:

1. `docs: add management observability contract fixtures`
2. `feat(core): add management admin authentication`
3. `feat(chat): add durable management audit outbox schema`
4. `feat(chat): publish immutable management audit events`
5. `feat(chat): instrument job and session decision audit`
6. `feat(chat): expose management knowledge and audit APIs`
7. `feat(chat): expose LLM usage and management status`
8. `docs: publish management client integration guide`

Stage only task-owned files; do not combine unrelated issue 003/005 changes unless an explicit dependency requires it.

## Risks and stop conditions

Stop and return to design review if any of these occurs:

- the only feasible way to guarantee a decision event is a best-effort in-memory queue;
- implementing context audit requires storing raw prompt/history/document content;
- an endpoint requires API-key authentication or changes chat auth semantics;
- migration requires cascade deletion of decision audit with session/job data;
- provider cost cannot be versioned/categorized safely but a total is requested;
- issue 003 provenance or issue 005 clarification state has no stable safe summary boundary;
- a requested “reference knowledge” feature becomes authoring/approval/global-memory scope.
