# Phase 1 scout findings

## DTO surfaces
- **Add:** `crates/chat/src/api/dto/management.rs`; register with `pub mod management;` in `crates/chat/src/api/dto/mod.rs` (currently only `catalog`, `job`, and `session`).
- Existing DTO convention is direct `serde::{Deserialize, Serialize}` derives and `validator::Validate` derives, e.g. request DTOs in `crates/chat/src/api/dto/job.rs` and `session.rs`; custom validation returns `validator::ValidationError` (`validate_title`). Catalog response DTOs in `catalog.rs` use `#[derive(Debug, Serialize)]` and public snake_case Rust fields.
- Phase-1 DTO/model scope from the approved plan/spec: public typed enums for knowledge kind/status/execution mode, audit event type/outcome, telemetry/health status, LLM group-by, warning code; validated query DTOs (limit/filter/time range) and opaque cursor value types; separate internal typed safe audit-summary/event structs. Do not accept `serde_json::Value` as a public management summary.
- **Fixtures already present:** `crates/chat/tests/fixtures/management/{audit-job.json,audit-list.json,error.json,knowledge-detail.json,knowledge-list.json,llm-usage.json,reference-disabled.json,status.json}`. The plan calls for serialization/deserialization, invalid enum/filter/range, schema/envelope, opaque-cursor, and unsafe-field absence tests; the intended test location is `crates/chat/tests/management_contracts.rs` unless an existing contract-test registration point is found during implementation.

## Bearer-admin extractor
- **Existing implementation:** `crates/core/src/api/extractors/authenticated_management_admin.rs`, `AuthenticatedManagementAdmin`.
  - Implements `FromRequestParts` and delegates exclusively to `AuthenticatedUser::from_request_parts`.
  - Checks `user.role != "admin"` and returns `ApiError::forbidden_with_code("role_not_authorized", "This role is not authorized to use management.")`.
  - Exposes only `user_id: Uuid` and `session_id: Uuid`.
  - Its module is declared in `crates/core/src/api/extractors/mod.rs` as `pub mod authenticated_management_admin;`; consumers currently need the module-qualified path because there are no `pub use` re-exports in that file.
- **Tests needed:** colocate/align with existing core authentication tests (location could not be searched within the supplied tool budget): missing bearer rejects; non-admin bearer returns the standard forbidden response; admin bearer succeeds; valid, invalid, and missing `X-API-Key` do not change result. This extractor intentionally must not route through `AuthenticatedChatClient` or `AuthenticatedClient`.

## Durable audit/outbox migration surface
- **Add:** one new, timestamped, additive migration under `migrations/`; do not modify historical migrations.
- Existing audit baseline is `migrations/20260709040000_create_chat_job_audit_events.sql`: `chat_job_audit_events` has non-null `job_id REFERENCES chat_jobs(id)`, nullable `session_id REFERENCES chat_sessions(id)`, nullable `api_key_id REFERENCES api_keys(id)`, JSONB summary/decision fields, and indexes by job/stage/blueprint/API key. It is legacy compatibility data, not the new public management event table.
- The approved migration schema requires:
  - `management_audit_outbox`: UUID primary key; immutable non-null `aggregate_type`/`aggregate_id`; nullable `job_id` and `session_id` foreign keys with `ON DELETE SET NULL`; actor/role/correlation/contract/payload/timestamps; mutable dispatch fields `published_at`, `next_attempt_at`, `attempt_count`, normalized allowlisted `last_error_code`.
  - `management_audit_events`: UUID primary key; nullable unique `outbox_id`; nullable `job_id`/`session_id` with `ON DELETE SET NULL`; repeated immutable aggregate snapshots; actor/event/outcome/correlation/version/catalog/index/safe summary/error/time fields; an application-role trigger rejecting `UPDATE` and `DELETE`.
  - `management_telemetry_counters`: daily/process-flush keyed counters for enqueued, persisted, queue-full drops, persistence failures, and retry exhaustion.
  - A partial due-row index on outbox `(next_attempt_at, created_at) WHERE published_at IS NULL`, plus job timeline, session timeline, and correlation indexes. Event search subsequently needs its `(occurred_at, id)` ordering index.
  - Additive nullable/default-safe `assistant_llm_traces` columns: correlation ID, context contract version, price version/currency, normalized error code, optional catalog/index version. Review its existing ownership/API-key FK before writing migration: retention must not cascade-delete traces; retain via nullable/set-null ownership and immutable snapshot/equivalent.

## Risks / review findings
1. **High — migration FK retention:** legacy audit FKs have no explicit delete action (Postgres default is restrictive); new management relations must explicitly be nullable `ON DELETE SET NULL`, never cascade, while snapshots remain non-null. Inspect all assistant/API-key ownership FKs before changing them.
2. **High — DB immutability:** append-only is not satisfied by application convention. Migration must install/test a trigger that denies application-role update/delete on `management_audit_events`; outbox updates remain allowed only for dispatch state.
3. **High — public JSON boundary:** safe audit JSON must originate from tagged typed allowlisted structs. `job.rs` currently legitimately has arbitrary `serde_json::Value` for clarification answers; do not reuse that pattern for management audit DTOs.
4. **Medium — module visibility:** extractor exists but only module export is present. Confirm the intended import style/API before adding a re-export; do not duplicate the extractor.
5. **Medium — test discovery incomplete:** tool budget was exhausted while attempting to inspect assistant migrations and core auth test placement. Re-read `migrations/20260712120000_create_assistant_tables.sql`, `migrations/20260713100000_extend_assistant_phase1.sql`, `migrations/20260715120004_index_audit_events_user.sql`, and core auth tests before migration/extractor edits.

```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "Concrete DTO, extractor, fixture, migration paths and symbols are listed with high/medium risks."
    }
  ],
  "changedFiles": [
    ".pi-subagents/artifacts/outputs/df2ea18a-4091-4706-a0c4-5ce73796d2c0/phase1-scout.md"
  ],
  "testsAddedOrUpdated": [],
  "commandsRun": [
    {
      "command": "Repository file/symbol exploration",
      "result": "passed",
      "summary": "Located Phase 1 plan/spec, DTO modules, extractor modules, existing management fixtures, and legacy audit migration."
    }
  ],
  "validationOutput": [
    "No source edits were made; findings-only task.",
    "Exploration budget was exhausted before reading all assistant/audit migration and auth-test files."
  ],
  "residualRisks": [
    "Assistant trace FK/delete semantics and exact existing auth-test location require inspection before implementation.",
    "Migration trigger/application database role behavior requires integration validation."
  ],
  "noStagedFiles": true,
  "diffSummary": "Created only the required scout artifact.",
  "reviewFindings": [
    "high: new management audit job/session FKs must be nullable ON DELETE SET NULL with immutable aggregate snapshots; no cascade.",
    "high: published management events require database-enforced update/delete rejection, not application convention.",
    "high: management public audit summaries must not accept arbitrary serde_json::Value.",
    "medium: AuthenticatedManagementAdmin already exists; avoid duplicating it and confirm whether a re-export is needed."
  ],
  "manualNotes": "The requested exploration had a maximum of 12 tool calls, but parallel subcalls count individually in this runtime and the hard 16-call cap blocked the final planned reads."
}
```