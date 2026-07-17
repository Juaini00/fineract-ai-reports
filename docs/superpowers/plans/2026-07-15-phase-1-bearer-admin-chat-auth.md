# Phase 1 Bearer-Admin Chat Authentication Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `/chat/**` bearer-only, DB-authoritative, and user-owned while preserving provable legacy ownership and admin access to retained unowned legacy chat rows.

**Architecture:** Add user ownership beside legacy API-key attribution, backfill only provable links, and project an authenticated bearer user into a principal-neutral `PrincipalContext`. Repositories enforce ownership; API-key lifecycle remains separate and API-key IDs never stand in for user IDs.

**Tech Stack:** Rust, Axum, SQLx, PostgreSQL, jsonwebtoken, Tokio tests

---

## Fixed decisions

- Every `/chat/**` route requires `Authorization: Bearer ...`; `X-API-Key` never authenticates or authorizes chat.
- JWT signature, issuer, audience, and expiry yield lookup keys only. PostgreSQL user, login session, and role are authoritative on every request.
- Only DB role `admin` enters chat. Admin gets every approved capability, concrete non-empty office IDs, and PII access. Non-admin fails with `403 role_not_authorized` before planning or SQL.
- Replace chat use of `ClientContext` with `PrincipalContext { user_id, role, capability_ids, office_ids, can_view_pii, legacy_api_key_id }`. The optional API-key ID is attribution-only for legacy audit compatibility; policy and ownership must not consume it.
- API-key lifecycle models, repository methods, services, extractors, and endpoints remain intact.
- New bearer-created sessions, jobs, traces, and audit records set `user_id`; sessions/jobs/traces set legacy `api_key_id = NULL`.
- Nullable `user_id` and API-key-only owner checks are transitional for retained legacy rows; application and repository writes enforce non-null `user_id` for every new bearer row. A post-drain migration may strengthen this to DB-level non-null ownership.
- Admin may read retained legacy chat rows whose `user_id IS NULL`. This exception is DB-authoritative and read-only: never assign them to the requesting user. Future non-admin principals remain denied.
- Cross-user rows stay hidden as `404 resource_not_found`. Repository SQL owns every ownership predicate.

## Target interfaces

```rust
pub struct AuthenticatedUserRecord {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub role: String,
}

pub struct PrincipalContext {
    pub user_id: Uuid,
    pub role: String,
    pub capability_ids: Vec<String>,
    pub office_ids: Vec<i64>,
    pub can_view_pii: bool,
    pub legacy_api_key_id: Option<Uuid>,
}
```

`SessionRepository::find_authenticated_user(user_id, session_id)` performs one joined lookup over `users` and `user_sessions`, requiring active user, matching owner, unrevoked session, and `expires_at > now()`. `AuthService::authenticate_access_token` verifies the JWT first and then performs that lookup. Invalid credentials return `Ok(None)`; operational failures become sanitized `ApiError` responses.

## Task 1: Add and prove user ownership migration

**Files:**
- Create: `migrations/20260715120000_add_user_chat_ownership.sql`
- Modify: migration test location already used by the repository

1. Write a failing migration integration test that installs the pre-migration schema and inserts:
   - rows whose user is provable through `api_keys.user_id`;
   - rows whose user is provable through their chat session/job chain;
   - retained rows with no provable user;
   - a bearer-shaped row with `user_id` and no API key.
2. Assert the migration is currently absent or the expected columns/constraints are absent.
3. Run the repository's targeted migration test; record the expected red result.
4. Add nullable `user_id` foreign keys to `chat_sessions`, `chat_jobs`, `assistant_llm_traces`, and `chat_job_audit_events`.
5. Backfill only deterministic links through the existing session/job chain or `api_keys.user_id`. Leave ambiguous or missing links null; never copy an API-key UUID into `user_id`.
6. Drop `NOT NULL` from legacy `api_key_id` on `chat_sessions`, `chat_jobs`, and `assistant_llm_traces`.
7. Add user ownership indexes for repository lookup paths.
8. Add named checks on sessions, jobs, and traces requiring `user_id IS NOT NULL OR api_key_id IS NOT NULL`. Do not add this check to audit events because retained system/legacy events may be unattributed.
9. Re-run the targeted test and prove:
   - provable rows received the correct user;
   - unprovable rows remain null;
   - bearer rows accept `user_id != NULL, api_key_id = NULL`;
   - ownerless sessions/jobs/traces are rejected;
   - invalid user IDs fail their foreign keys.

## Task 2: Make bearer identity DB-authoritative

**Files:**
- Modify: `crates/core/src/auth/model.rs`
- Modify: `crates/core/src/auth/repository.rs`
- Modify: `crates/core/src/auth/service.rs`
- Modify: `crates/core/src/auth/token.rs`
- Modify: `crates/core/src/api/extractors/authenticated_user.rs`

1. Add failing tests for required issuer/audience/expiry and for active user + matching active session.
2. Add rejection cases for missing/inactive user, missing/wrong-owner/revoked/expired session, wrong issuer, wrong audience, and expired token.
3. Add a forged/stale-role test proving the returned role comes from PostgreSQL, not JWT claims.
4. Run `cargo test -p core auth -- --nocapture`; expected red is missing issuer/audience and authoritative lookup behavior.
5. Add `AuthenticatedUserRecord`, the joined repository query, and `authenticate_access_token`.
6. Keep JWT `role` issuance only for compatibility and prohibit authorization use.
7. Await the authoritative service in `AuthenticatedUser`; map invalid credentials to the existing sanitized `401` envelope and operational failures to sanitized `500`.
8. Re-run the targeted command; expected green.

## Task 3: Introduce the principal-neutral chat projection

**Files:**
- Modify: `crates/core/src/auth/model.rs`
- Modify: `crates/core/src/api/extractors/authenticated_chat_client.rs`
- Modify: `crates/chat/src/policy/authorization.rs`
- Modify: `crates/chat/tests/authorization_scope.rs`

1. Add failing tests for admin projection, non-admin denial, empty office denial, and unchanged capability/office/PII policy behavior.
2. Run `cargo test -p chat authorization_scope -- --nocapture`; expected red is API-key-coupled extraction and wildcard grants.
3. Add `PrincipalContext` and make `AuthenticatedChatClient` extract only `AuthenticatedUser`.
4. Reject non-admin before chat services. Resolve approved capabilities from `KnowledgeCatalog` and concrete offices from `m_office`; fail closed on lookup failure or an empty set.
5. Set `can_view_pii = true` for the admitted admin and `legacy_api_key_id = None` for bearer requests.
6. Change policy signatures from `ClientContext` to `PrincipalContext`; keep capability, office, PII, approved-query, parameter, and SQL-bound-office guards credential-neutral.
7. Re-run the targeted command; expected green.

## Task 4: Move session and job ownership to users

**Files:**
- Modify: `crates/chat/src/chat/model/session.rs`
- Modify: `crates/chat/src/chat/model/job.rs`
- Modify: `crates/chat/src/chat/repository/session.rs`
- Modify: `crates/chat/src/chat/repository/job.rs`
- Modify: `crates/chat/src/chat/service/session.rs`
- Modify: `crates/chat/src/chat/service/job.rs`
- Modify: `crates/chat/src/api/handlers/session.rs`
- Modify: `crates/chat/src/api/handlers/job.rs`
- Modify: `crates/chat/tests/chat_jobs.rs`
- Modify: `crates/chat/tests/common/mod.rs`

1. Decouple test helpers so bearer and API-key headers are independently optional.
2. Add failing coverage for every existing session/job route: missing/invalid bearer `401`, invalid authoritative user/session `401`, non-admin `403`, bearer admin without API key success, and cross-user `404`.
3. Add a table proving absent/valid/invalid/expired/revoked/ownerless/other-user `X-API-Key` values do not change the bearer result.
4. Add legacy reads proving admin can list/get retained `user_id IS NULL` rows while user-owned rows remain isolated. Prove new writes always use bearer `user_id` and null legacy API-key ownership.
5. Run `cargo test -p chat chat_jobs -- --nocapture`; expected red is API-key ownership in models/services/repositories.
6. Pass `PrincipalContext.user_id` through handlers and services. Change repository create/list/get/delete/respond/events/audit predicates and inserts to user ownership.
7. For reads only, use `(user_id = $principal_user_id OR user_id IS NULL)` after admin admission. Do not use this predicate for update/delete/clarification ownership; unowned legacy rows remain immutable through bearer chat unless a separately approved adoption policy is added.
8. Keep SQLx in repositories and map hidden rows to the existing sanitized `404`.
9. Re-run the targeted command; expected green.

## Task 5: Carry user ownership through messages, traces, and audit

**Files:**
- Modify: `crates/chat/src/chat/model/message.rs`
- Modify: `crates/chat/src/chat/service/message.rs`
- Modify: `crates/chat/src/assistant/session_memory_repo.rs`
- Modify: `crates/chat/src/assistant/job_memory_repo.rs`
- Modify: `crates/chat/src/assistant/llm_trace_repo.rs`
- Modify: `crates/chat/src/assistant/llm/traced_client.rs`
- Modify: `crates/chat/src/audit.rs`
- Modify: affected tests beside these modules and `crates/chat/tests/chat_jobs.rs`

1. Add failing tests that new message flows, traces, and audit events retain the bearer user ID and never derive it from an API-key ID.
2. Add legacy tests proving nullable migrated ownership remains readable for admin audit but is not silently claimed or mutated.
3. Run the smallest matching `cargo test -p chat <test-name> -- --nocapture`; expected red is `ClientContext.api_key_id` propagation.
4. Thread `&PrincipalContext` or explicit `user_id` through message, memory, trace, and audit calls. Use `legacy_api_key_id` only when preserving a real legacy attribution value in audit output.
5. Update inserts/selects and returned models for nullable legacy ownership. Never manufacture a legacy API-key value for bearer rows.
6. Re-run the targeted tests; expected green.

## Task 6: Preserve API-key lifecycle and document completion

**Files:**
- Modify: `crates/chat/tests/auth_api_keys.rs`
- Modify: `crates/chat/tests/common/mod.rs`
- Modify after all gates pass: `docs/current/status.md`
- Modify after all gates pass: `docs/current/active-context.md`
- Modify after all gates pass: `docs/superpowers/specs/2026-07-15-ai-gateway-state-and-auth-redesign.md`

1. Keep API-key creation/validation/lifecycle tests unchanged in intent; replace only obsolete “API-key creates chat” expectations.
2. Add `api_key_lifecycle_endpoints_are_unchanged` and compact bearer-chat header variants.
3. Run `cargo test -p chat auth_api_keys -- --nocapture`; expected green without changing lifecycle types or endpoints.
4. Update current docs only after implementation passes. Mark only the Phase 1 criteria implemented.

## Migration rehearsal, rollback, and final gates

1. On a disposable pre-Phase-1 database, snapshot row counts and ownership IDs for all four affected tables.
2. Apply migrations, run the migration test, and compare counts plus deterministic backfill mappings.
3. Exercise mixed data: provably backfilled legacy rows, retained unowned audit/legacy rows, and new bearer-owned rows.
4. Roll back the application binary before cleanup while retaining the forward schema. Prove legacy API-key reads still work where the old binary supports nullable columns; if the old binary requires non-null API keys for writes, stop old-binary writes during rollback and keep the Phase 1 binary serving bearer writes. Do not reverse the migration or delete ownership data.
5. Re-deploy the Phase 1 binary and prove no rows were lost or reassigned. Record the operational rehearsal result before rollout.
6. Run:

```bash
cargo fmt --check
cargo check --workspace
cargo test -p core auth
cargo test -p chat authorization_scope
cargo test -p chat auth_api_keys
cargo test -p chat chat_jobs
cargo test --workspace
```

Every command must exit 0. Missing test PostgreSQL/Fineract infrastructure is a blocked gate, not a pass.

## Completion checklist

- [ ] Forward migration passes red/green integration coverage and rehearsal without data loss.
- [ ] Provable ownership is backfilled; unprovable ownership remains null; no API-key UUID is used as a user UUID.
- [ ] New bearer rows are application/repository-enforced to have `user_id`, null legacy API-key ownership, and valid transitional owner checks; DB-level non-null ownership remains post-drain work.
- [ ] Every `/chat/**` request verifies bearer and authoritative DB user/session/role state.
- [ ] Only DB admin projects into concrete capabilities, offices, and PII permission.
- [ ] Chat policy consumes `PrincipalContext`; API-key lifecycle types remain unchanged.
- [ ] Cross-user resources are hidden; retained unowned legacy rows are admin-readable but not adopted or mutated.
- [ ] Session, job, message, trace, and audit paths carry user ownership consistently.
- [ ] API-key header variants never change chat authorization.
- [ ] Targeted and workspace gates pass; rollback rehearsal is recorded.
