# AI Gateway State and Authorization Redesign

**Status:** approved architecture specification  
**Scope:** chat request intake, clarification, authorization, planning, execution, and recovery  
**Supersedes:** the conflicting portions of `2026-07-12-semantic-assistant-platform-migration-design.md`

## 1. Decision and supersession

This specification is authoritative where the 2026-07-12 design differs on authentication, principal construction, intent/clarification state, capability relevance, temporal extraction, planner inputs, transaction boundaries, recovery, compatibility, language, observability, or acceptance gates. The earlier design remains valid only for compatible constraints: exactly three crates, structured LLM boundaries, graph validation, curated retrieval, approved SQL, repository-owned SQLx, SQL-bound office scope, and structured responses.

Specifically replaced are the earlier design's API-key-required chat context, API-key/bearer ownership coupling, mutable `AssistantIntent`/`SourceIntentSnapshot`/`PendingClarification` memory, semantic option matching as normal clarification behavior, prompt-shape relevance, advisory date extraction, English-only behavior, transition-by-transition persistence without the atomicity rules below, and its clarification/deletion acceptance gates.

## 2. Invariants and ownership

- The workspace remains exactly `app`, `core`, and `chat`; this redesign adds no crate.
- Flow remains route → service → repository → database. Only repositories use SQLx.
- PostgreSQL is authoritative for jobs, immutable inputs, revisions, turns, observations, checkpoints, events, and emitted assistant messages. Redis remains live SSE coordination only.
- SQL is selected only from approved catalog metadata and `queries/**/*.sql`; the LLM never writes SQL.
- Authorized `office_ids` are bound inside approved SQL. Results are never office-filtered in Rust.
- The existing principal-neutral authorization policy boundary is preserved: planner/executor code receives an authorized principal projection, not HTTP credentials or an admin-specific branch.
- Client errors use the standard `{ success, data, error }` envelope and contain no raw SQL, prompts, stack traces, tokens, or hidden PII.

## 3. Gateway authentication and principal projection

Every `/chat/**` endpoint requires a bearer access token whose signature, issuer, audience, and expiry are verified. Principal construction then authoritatively loads the user, login session, and role from PostgreSQL on every request: the user must exist and be active, and the session must exist, belong to that user, be unrevoked, and be unexpired. The database role, never a JWT role claim, determines authorization; a forged or stale `admin` claim grants nothing. This per-request lookup is required for initial rollout so user deactivation, session revocation, and role changes take effect on the next request. A later cache is permitted only with a documented short upper-bound staleness and invalidation on those changes, and must fail closed when authoritative state cannot be confirmed. `X-API-Key` is optional and ignored for chat: it neither authenticates, narrows, expands, nor links the bearer principal. Missing, expired, malformed, or revoked bearer credentials or authoritative sessions fail before job/session access. API-key lifecycle endpoints retain their existing rules outside this scope.

The current user database admits only role `admin`. Principal projection is therefore:

```text
authenticated admin
  -> capabilities = every currently approved capability
  -> office_ids = real expanded office IDs from the authoritative office resolver
  -> pii_allowed = true

authenticated non-admin (including unexpected future/legacy role values)
  -> deny before request planning or SQL until RBAC is implemented and approved
```

An empty or failed admin office expansion is a policy failure, never an unscoped query. The projection is represented as a neutral `PrincipalContext` consumed by policy. It records user ID, role, capability set, expanded office IDs, PII permission, and only where legacy audit attribution requires it an optional legacy API-key ID; it does not carry the bearer token. API-key IDs never substitute for user IDs and never affect policy or ownership. Admin defaults do not bypass capability existence, parameter validation, approved-query selection, or office predicates.

Phase 1 adds nullable user ownership to chat sessions, jobs, LLM traces, and job audit events. New bearer rows record the authenticated `user_id`; sessions, jobs, and traces leave legacy `api_key_id` null. During the admin-only rollout, DB-authoritative admins may read retained legacy chat rows whose user cannot be proven, but those rows are not adopted or mutated. Non-admin principals remain denied.

## 4. Canonical job state

### 4.1 Immutable `OriginalIntent`

Created once from the accepted initial user request and never rewritten:

```text
OriginalIntent { job_id, schema_version, raw_message_id, locale,
  action, entities, metrics, groupings, output, parameters, pii_request,
  extraction_provenance, created_at }
```

Raw text remains in `chat_messages`; derived fields retain source spans or extractor identifiers without duplicating raw prompts in logs.

### 4.2 Append-only `FactObservation`

Each extraction, deterministic resolution, user clarification, or approved default adds a typed observation:

```text
FactObservation { id, job_id, sequence, source_kind, source_id,
  field_path, typed_value, confidence, extractor_version, observed_at }
```

`source_kind` is `original_request | clarification | deterministic_resolver | approved_default | llm_advisory`. Rows are immutable. Corrections append a later observation; provenance is never overwritten.

### 4.3 Versioned `EffectiveConstraints`

One immutable row per job revision contains the canonical merged constraint document plus per-field winning observation IDs. Merge is field-by-field, not whole-object replacement:

1. latest explicit clarification for that exact field;
2. original-request value for that field;
3. approved capability/default value;
4. absent, requiring clarification or unsupported routing as applicable.

Unmentioned clarification fields preserve prior winners. `null` clears a field only when that field's contract explicitly permits clear. Lists use the field contract's replace/add/remove operation; no implicit concatenation. Merge is deterministic, associative for disjoint fields, idempotent for the same `submission_id`, and replayable solely from persisted observations.

### 4.4 Immutable `PlannerInputSnapshot`

Before policy or execution, the gateway persists:

```text
PlannerInputSnapshot { id, job_id, revision, original_intent_id,
  effective_constraints_id, capability_catalog_version,
  principal_projection, reference_instant, timezone,
  selected_capability_id, normalized_parameters, created_at }
```

Planner, policy, and executor consume exactly this snapshot. A later clarification creates a new revision and snapshot; in-flight components never read mutable “latest” state.

## 5. Structural relevance and routing

Capability retrieval and validation compare structured dimensions: action, entity, metric, grouping, output shape, typed parameters, and PII requirement. A candidate is relevant only when required dimensions are compatible; lexical or embedding similarity may rank compatible candidates but cannot make an incompatible candidate executable.

Routing outcomes are distinct:

- `relevant`: exactly one structurally compatible approved capability, or a deterministic choice among equivalent aliases;
- `ambiguous`: two or more compatible capabilities or a required field with multiple valid interpretations; create a clarification;
- `unsupported_in_domain`: understood request but no approved capability satisfies all structural dimensions;
- `out_of_domain`: request is outside supported business domains;
- `blocked_by_policy`: capability exists but principal projection forbids it.

`5 random clients this year` is `unsupported_in_domain` until a capability explicitly approving random sampling, client entity, limit, and temporal semantics exists. It must not degrade to top-N, recent clients, or arbitrary SQL ordering.

## 6. Temporal resolution and Indonesian support

Each job persists the original UTC `reference_instant` captured at creation and timezone `Asia/Jakarta`; retries and clarifications never replace it. All relative dates are resolved deterministically against the Jakarta local date containing that instant and retain phrase, locale, grammar version, resolved bounds, and observation provenance. Query bounds are half-open Jakarta-local calendar intervals `[start, end)`, converted to instants only after resolution:

- today and yesterday are their single local calendar days;
- this/last week are Monday-Sunday containing the reference date / immediately preceding it;
- this/last month are the containing / preceding calendar month;
- this/last quarter are the containing / preceding Jan-Mar, Apr-Jun, Jul-Sep, or Oct-Dec quarter;
- this year / `tahun ini` is the full Jan 1-Dec 31 calendar year containing the reference date; last year is the preceding full calendar year;
- last N days includes today and the preceding `N-1` local dates, with positive bounded integer `N`;
- an explicit valid calendar date is that single day, while an explicit `from…to` / `dari…sampai` range includes both endpoint dates.

Calendar operations use real Gregorian validation, timezone-aware local boundaries, start ≤ end, and capability-specific range/retention limits. Invalid dates and excessive ranges produce structured clarification or validation errors; they are never normalized silently. `next` periods may be parsed for understanding but are non-executable unless the selected capability explicitly permits future bounds.

This redesign intentionally promotes Indonesian alongside English for request understanding, temporal extraction, and clarification templates. The approved grammar includes equivalent EN/ID forms for today/hari ini, yesterday/kemarin, this/last/next day-week-month-quarter-year, explicit ISO and locale-approved calendar dates, and bounded `from…to` / `dari…sampai` ranges. Locale is detected once with confidence; an ambiguous locale asks a localized neutral clarification. Mixed EN/ID phrases are accepted only where the grammar parses them unambiguously. LLM date output is advisory: it may add `llm_advisory` observations but cannot set executable bounds without deterministic parsing and validation.

## 7. Clarification contract

A job has versioned, append-only `ClarificationTurn` records:

```text
ClarificationTurn { clarification_id, job_id, revision, attempt,
  reason_code, question_template_id, locale, expected_revision,
  choices[{ opaque_choice_id, label, typed_patch }],
  parameter_schema, status, created_at, resolved_at }
```

Choice IDs are opaque, job/turn-scoped, and reveal no capability or SQL identifier. `typed_patch` and free-form parameters are validated against the turn schema. Response request:

```json
{
  "clarification_id": "opaque-id",
  "expected_revision": 3,
  "submission_id": "client-generated-idempotency-key",
  "action": "answer",
  "answer": { "choice_id": "opaque-choice-id" }
}
```

`action` is tagged and required. `cancel` permits no `answer`; `answer` requires exactly one answer form, either `{ "choice_id": "..." }` or `{ "parameters": { ... } }`. Choice and parameter forms cannot be combined. Cancel moves the job to `cancelled` and performs no execution. Unknown actions, choices, or fields, wrong types, missing/combined answer content, or invalid parameters are rejected without revision change. A response to a closed turn is stale unless it is an exact idempotent replay.

Attempts are bounded at three per unresolved field set. Each unsuccessful attempt must narrow or change the question. At the limit the job ends in the distinct terminal state and response code `clarification_exhausted` with a safe explanation; it is never classified as `unsupported_in_domain`. Identical clarification loops are forbidden.

## 8. State machine and transaction boundaries

```text
received -> extracting -> routing
routing -> waiting_for_clarification | snapshot_ready
routing -> unsupported_in_domain | out_of_domain | blocked_by_policy | clarification_exhausted
waiting_for_clarification -> applying_response -> routing
waiting_for_clarification -> cancelled
snapshot_ready -> policy_checked -> queued -> executing
policy_checked -> blocked_by_policy
executing -> completed | failed_retryable | failed_terminal
failed_retryable -> queued
```

Every transition validates expected revision. Applying a clarification is one PostgreSQL transaction that: locks the job/active turn; checks ownership, status, `expected_revision`, and `submission_id`; inserts response and observations; computes/inserts new effective constraints; closes the turn; increments revision; writes checkpoint/event; and requeues the same job. Commit makes all effects visible together. Rollback leaves the turn open.

Executor claim uses one atomic conditional update/row lock from `queued` to `executing` with a lease/attempt token. Only the token holder may checkpoint or complete. Expired claims are recoverable; live claims cannot be stolen. Checkpoints occur at durable semantic boundaries and contain snapshot ID, state, revision, attempt token, and sanitized result references.

On restart, workers replay from the last committed checkpoint and immutable snapshot. External/read execution may repeat, but assistant output is idempotent: the final assistant message has a unique `(job_id, revision, output_kind)` key and is inserted in the same completion transaction as job state/event. Duplicate claims, retries, and response submissions cannot emit duplicate messages or execute a different snapshot.

## 9. Persistence and migration strategy

Schema changes are forward-only migrations under `migrations/*.sql`. Phase 1 first adds nullable `user_id` foreign keys and user lookup indexes to `chat_sessions`, `chat_jobs`, `assistant_llm_traces`, and `chat_job_audit_events`; backfills only ownership provable through existing session/job relations or `api_keys.user_id`; drops `NOT NULL` from legacy `api_key_id` on sessions, jobs, and traces; and adds checks requiring at least one owner on those three tables. This nullable `user_id`/API-key-only check is an intentional transitional legacy invariant, not authoritative DB-level ownership: application and repository writes must set `user_id` on every new bearer row. Unprovable rows remain null and API-key IDs are never copied into user IDs; after legacy rows are drained, a later migration may enforce stronger non-null DB ownership. Migration integration tests and a production-like rehearsal must cover mixed legacy/new rows, forward-schema application rollback, and row/ownership preservation.

Later phases add normalized tables (names may follow existing prefix conventions) for original intents, fact observations, effective-constraint revisions, clarification turns/choices/submissions, planner snapshots, and executor claims; add foreign keys to `chat_jobs`, monotonic unique `(job_id, revision)`/`(job_id, sequence)` constraints, unique `(job_id, submission_id)`, unique assistant-output keys, JSON schema-version columns, status checks, and indexes for active turns and queued claims.

Migration first adds nullable references and new tables without changing traffic. Existing jobs remain on the legacy reader and cannot be converted by guessing missing provenance. New jobs are dual-recorded only during rollout validation, with the new model authoritative behind a feature flag. No startup DDL. Backfill is limited to mechanically provable identifiers/timestamps; unprovable intent facts remain legacy. After drain, legacy rows remain audit-readable until retention permits deletion.

## 10. Error semantics

| Condition | HTTP / code | Job effect |
| --- | --- | --- |
| Missing/invalid bearer | `401 authentication_required` | none |
| Authenticated non-admin | `403 role_not_authorized` | none |
| Session/job not owned or hidden | `404 resource_not_found` | none |
| Invalid typed response/date/range | `422 clarification_validation_failed` | unchanged |
| Wrong revision/closed turn | `409 stale_clarification` | unchanged |
| Reused submission ID, different body | `409 idempotency_conflict` | unchanged |
| Exact duplicate submission | original successful envelope | no duplicate effects |
| Competing executor/response transaction | `409 state_conflict` or worker no-op | unchanged/reload |
| Three unsuccessful clarification attempts | successful structured `clarification_exhausted` response | terminal |
| Unsupported understood request | successful structured `unsupported` response | terminal |
| Policy denial after routing | successful structured `policy_blocked` response | terminal |
| Repository/provider failure | sanitized `500 operational_failure` or retry event | retryable/terminal by policy |

Errors include safe code, message, and correlation ID only. Expected/current revisions may be returned; raw values, SQL, credentials, hidden capability internals, and PII may not.

## 11. Observability

Emit correlated structured events for extraction, provenance/merge winner selection, retrieval candidate compatibility, clarification create/apply/stale/duplicate, temporal resolution, policy decision, executor claim/replay, SQL capability ID, and completion. Metrics cover outcome/locale, clarification attempts, stale and duplicate rates, merge/replay mismatches, unsupported dimensions, claim contention, recovery latency, and execution duration.

Logs/traces store IDs, schema/grammar/catalog versions, dimension names, counts, timings, decision codes, and hashes where correlation is required. They never store raw prompts, bearer/API keys, SQL text, bound values, account/client identifiers, result rows, or PII. Access to audit tables follows existing privileged operational controls.

## 12. Compatibility and deletion gates

Legacy `message`/`option_id` clarification payloads may be accepted only by an isolated adapter that resolves an active legacy turn, synthesizes a submission ID, validates ownership/revision, and writes through the new atomic application path. It may not perform label similarity or reconstruct intent. New turns expose only the new contract.

Delete the adapter only after: all pre-cutover open turns are drained/cancelled; no legacy payload is observed for 30 consecutive production days; supported clients advertise the new contract; replay/stale/idempotency tests pass in production-like CI; and an explicit migration release records the removal. Delete old mutable intent/pending-clarification readers after no active job references them and restart-recovery tests pass solely on new tables. Remove API-key chat auth only when bearer-only HTTP tests pass and telemetry shows no gateway path authenticates chat by API key. These are deletion gates, not permanent dual paths.

## 13. Phased rollout

1. **Schema and contracts:** deploy additive migrations, typed contracts, deterministic EN/ID temporal grammar, and read-disabled repositories.
2. **Shadow derivation:** for new bearer-admin jobs, derive observations/merges/relevance without affecting decisions; compare sanitized decision codes and exact normalized parameters.
3. **Clarification cutover:** issue new turns, enable atomic apply/requeue, retain the gated legacy adapter for existing turns.
4. **Planner/executor cutover:** make immutable snapshots authoritative; enable single-claim execution and idempotent completion; canary then expand after zero snapshot/replay mismatches.
5. **Enforcement and cleanup:** enforce bearer-only chat and non-admin fail-closed, drain legacy jobs, satisfy deletion gates, then remove old readers/adapters. Rollback before cleanup switches new-job routing to the prior path; committed new-model jobs continue with their recorded model and are never downgraded.

## 14. Acceptance criteria

The redesign is accepted only when all are automated and passing:

- Merge algebra proves field precedence, preservation of unmentioned fields, permitted clear/list operations, disjoint associativity, replay determinism, and submission idempotency.
- HTTP tests prove bearer-admin success; missing/invalid bearer `401`; revoked/expired/missing authoritative session and inactive/missing user fail closed; authenticated non-admin `403`; forged or stale JWT `admin` role claims never override the database role; role changes take effect on the next request; `X-API-Key` absent/present/invalid never changes chat authorization; cross-owner resources remain hidden.
- Admin projection contains all approved capabilities, real expanded office IDs, and PII permission; empty expansion fails closed; policy remains principal-neutral.
- Structural routing covers every dimension and separates ambiguous, unsupported, out-of-domain, and blocked outcomes; `5 random clients this year` is unsupported and executes no SQL.
- Temporal tests cover the specified Asia/Jakarta semantics for today, yesterday, this/last week/month/quarter/year (including full-year `this year`/`tahun ini`), last N days, explicit date/range inclusivity, day boundaries, DST-independent local behavior, leap day, invalid dates, reversed/excessive ranges, EN/ID and accepted mixed phrases, unchanged original reference instant, provenance, and rejection of unverified LLM dates.
- Clarification tests cover tagged answer/cancel actions, mutually exclusive choice/parameter content, opaque IDs, typed parameters, wrong turn/revision, malformed fields, exact duplicate, conflicting duplicate, concurrent submissions, three-attempt bound ending only as `clarification_exhausted`, and no repeated identical question.
- Transaction tests prove response/observations/merge/revision/checkpoint/requeue are all-or-nothing and only one executor claim succeeds.
- Kill/restart tests at every durable boundary replay the same snapshot, recover expired claims, produce one assistant output, and never reopen completed/cancelled jobs.
- Repository integration tests prove only approved SQL executes, every scoped query binds expanded `office_ids`, no Rust post-filter exists, and SQLx is absent from handlers/services.
- Golden Fineract integration fixtures assert exact values, ordering, range boundaries, office isolation, PII inclusion for admin, and no approximate/LLM-generated result values.
- Observability tests prove required decision events/metrics exist and captured logs/traces contain no raw SQL, prompt text, bound values, tokens, result rows, or PII.
- Migration rehearsal proves mixed legacy/new jobs, rollback before cleanup, drain, and every compatibility deletion gate without data loss.

No implementation phase is complete, and no legacy path is deleted, until its corresponding criteria above pass.
