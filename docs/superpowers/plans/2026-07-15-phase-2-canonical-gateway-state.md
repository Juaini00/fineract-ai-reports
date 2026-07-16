# Phase 2 Canonical Gateway State Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make original intent, fact observations, effective constraints, and planner inputs immutable, replayable, job-scoped PostgreSQL state, then gate planner/policy/executor authority on one persisted snapshot.

**Architecture:** Add four append-only/immutable tables and typed Rust contracts beside the legacy assistant memory. Derive effective constraints with a pure field-level merge, shadow-write and compare before cutover, and in authoritative mode reload one persisted planner snapshot for planning, policy, and execution. Session pending clarification remains a Phase 4 compatibility concern and gains no new authority or dependency here.

**Tech Stack:** Rust, Serde/Schemars, SQLx, PostgreSQL, proptest, Tokio integration tests

---

## Fixed decisions

- `OriginalIntent` is created once from the accepted initial user message. It references `chat_messages.id`; it never embeds, rewrites, or reparses the raw prompt.
- Extraction of every later message remains an audit input only. It appends observations and cannot replace the original baseline.
- `FactObservation` is append-only. Corrections append a later row; no repository update/delete API exists.
- `EffectiveConstraints` is an immutable revision. Every present field names its winning observation ID.
- Precedence is exact-field latest explicit clarification, original request, approved default, then absent. A clarification changes only fields explicitly present in its typed patch.
- `null` clears only fields whose contract declares `clearable`; list changes require explicit `replace`, `add`, or `remove`. Unknown fields and implicit list concatenation fail validation.
- Replaying persisted observations is deterministic. Reusing the same source/submission ID and field is idempotent; a different value for that key is a conflict.
- Phase 2 does not introduce `ClarificationTurn` or move active-turn ownership. Existing session pending data may keep the legacy path alive until Phase 4, but canonical reads and snapshot creation are keyed only by `job_id`.
- Planner, policy, and executor use the same persisted `PlannerInputSnapshot`; they do not reread mutable `JobMemory`, `SessionMemory`, current extraction metadata, catalog latest, clock, or live principal while executing it.
- `assistant_job_memory.current_user_message_metadata_json.deterministic_extraction` remains audit-readable during shadowing. It is not authoritative and is deleted only after the explicit gate below.
- Existing jobs stay legacy. No intent/provenance backfill is guessed.

## Target contracts and interfaces

Create `crates/chat/src/assistant/canonical_state.rs` and export it from `crates/chat/src/assistant/mod.rs`:

```rust
pub struct OriginalIntent {
    pub id: Uuid, pub job_id: Uuid, pub schema_version: i32,
    pub raw_message_id: Uuid, pub locale: AssistantLanguage,
    pub action: AssistantIntentKind, pub entities: Vec<AssistantEntity>,
    pub metrics: Vec<String>, pub groupings: Vec<String>, pub output: Option<String>,
    pub parameters: BTreeMap<String, TypedFactValue>, pub pii_request: bool,
    pub extraction_provenance: Vec<ExtractionProvenance>, pub created_at: DateTime<Utc>,
}

pub enum FactSourceKind {
    OriginalRequest, Clarification, DeterministicResolver,
    ApprovedDefault, LlmAdvisory,
}

pub struct FactObservation {
    pub id: Uuid, pub job_id: Uuid, pub sequence: i64,
    pub source_kind: FactSourceKind, pub source_id: String,
    pub field_path: ConstraintField, pub typed_value: TypedFactValue,
    pub confidence: Option<f32>, pub extractor_version: String,
    pub observed_at: DateTime<Utc>,
}

pub struct EffectiveConstraints {
    pub id: Uuid, pub job_id: Uuid, pub revision: i64, pub schema_version: i32,
    pub values: BTreeMap<ConstraintField, TypedFactValue>,
    pub winning_observation_ids: BTreeMap<ConstraintField, Uuid>,
    pub created_at: DateTime<Utc>,
}

pub struct PlannerInputSnapshot {
    pub id: Uuid, pub job_id: Uuid, pub revision: i64,
    pub original_intent_id: Uuid, pub effective_constraints_id: Uuid,
    pub capability_catalog_version: Uuid, pub principal_projection: PrincipalContext,
    pub reference_instant: DateTime<Utc>, pub timezone: String,
    pub selected_capability_id: String, pub normalized_parameters: Value,
    pub created_at: DateTime<Utc>,
}

pub enum CanonicalGatewayMode { Disabled, Shadow, Authoritative }

pub fn merge_observations(
    job_id: Uuid,
    revision: i64,
    observations: &[FactObservation],
    contracts: &ConstraintContracts,
) -> Result<EffectiveConstraints, MergeError>;
```

`ConstraintField` and tagged `TypedFactValue` must enumerate the executable dimensions currently represented by `AssistantIntent`, `AssistantConstraints`, and `PayloadField`; they must not accept arbitrary executable JSON. `ListPatch<T>` is tagged `replace | add | remove`. `ExtractionProvenance` stores extractor/version and source spans or identifiers, never raw prompt text.

Create `crates/chat/src/assistant/canonical_state_repo.rs` with repository-owned SQLx:

```rust
pub async fn insert_original_intent(&self, value: &OriginalIntent) -> Result<OriginalIntent>;
pub async fn append_observations(&self, job_id: Uuid, values: &[NewFactObservation]) -> Result<Vec<FactObservation>>;
pub async fn list_observations(&self, job_id: Uuid) -> Result<Vec<FactObservation>>;
pub async fn insert_effective_constraints(&self, value: &EffectiveConstraints) -> Result<EffectiveConstraints>;
pub async fn insert_planner_snapshot(&self, value: &PlannerInputSnapshot) -> Result<PlannerInputSnapshot>;
pub async fn get_planner_snapshot(&self, id: Uuid, job_id: Uuid) -> Result<Option<PlannerInputSnapshot>>;
```

Insert methods use `ON CONFLICT ... DO NOTHING` followed by exact readback comparison. Exact replay returns the existing row; changed content returns a typed conflict. There are no update/delete methods.

## Task 1: Add immutable canonical-state schema

**Files:**
- Create: `migrations/20260715130000_create_canonical_gateway_state.sql`
- Modify: `crates/chat/tests/assistant_repositories.rs`

- [ ] Add a failing integration test that creates a job plus initial user message and proves all four tables, foreign keys, checks, and uniqueness rules are absent.
- [ ] Create `assistant_original_intents` with UUID primary key, unique `job_id`, FK to `chat_jobs`, FK `raw_message_id` to `chat_messages`, positive `schema_version`, typed document/provenance JSONB, and immutable timestamps.
- [ ] Create `assistant_fact_observations` with UUID primary key, job FK, positive `sequence`, checked source kind, non-empty source/extractor identifiers, field path, typed value JSONB, bounded nullable confidence, and unique `(job_id, sequence)` plus `(job_id, source_kind, source_id, field_path)`.
- [ ] Create `assistant_effective_constraints` with UUID primary key, job FK, non-negative revision, positive schema version, values/provenance JSONB, and unique `(job_id, revision)`.
- [ ] Create `assistant_planner_input_snapshots` with UUID primary key; job, original-intent, and effective-constraint FKs; non-negative revision; catalog version; principal projection; fixed reference instant/timezone; selected capability; normalized parameters; and unique `(job_id, revision)`.
- [ ] Add job-ordered observation and latest-revision indexes. Add no trigger or startup DDL and no legacy backfill.
- [ ] Assert cross-job references, duplicate revisions/sequences, invalid source kinds/confidence, and mismatched raw message/job relationships are rejected. Use a composite uniqueness/FK or repository transaction check so a job cannot reference another job's message/state.
- [ ] Run `cargo test -p chat --test assistant_repositories canonical_state_schema -- --nocapture`; expected green.

## Task 2: Add typed schemas and pure merge algebra

**Files:**
- Create: `crates/chat/src/assistant/canonical_state.rs`
- Modify: `crates/chat/src/assistant/mod.rs`
- Modify: `crates/chat/src/assistant/intent.rs`
- Modify: `crates/chat/src/assistant/extraction.rs`
- Modify: `crates/chat/Cargo.toml`

- [ ] Add failing example tests for precedence, preservation of unmentioned fields, allowed clear, rejected clear, and explicit list replace/add/remove.
- [ ] Add `proptest` as a dev-dependency and property tests for disjoint-field associativity, same-source idempotency, sequence replay determinism, and permutation invariance where relative order within each field is preserved.
- [ ] Implement the contracts above with `Serialize`, `Deserialize`, and `JsonSchema`; reject unknown fields and non-finite/out-of-range confidence at deserialization/validation boundaries.
- [ ] Map initial `AssistantIntent` and `DeterministicExtraction.candidates` into original-request observations without copying `SourceIntentSnapshot.prompt`.
- [ ] Implement clarification patch conversion so only keys physically present in the validated patch emit observations. Do not use default-filled `AssistantConstraints` to infer supplied keys.
- [ ] Implement merge as a pure sort/fold over persisted observations. Resolve each field independently; keep the winning observation UUID beside the value; never read session pending state or wall clock.
- [ ] Keep `DeterministicExtraction::merge_into` only for legacy/shadow comparison. Mark its output non-authoritative in type/call-site naming rather than comments alone.
- [ ] Run `cargo test -p chat canonical_state -- --nocapture`; expected green.

## Task 3: Add immutable repositories and replay proof

**Files:**
- Create: `crates/chat/src/assistant/canonical_state_repo.rs`
- Modify: `crates/chat/src/assistant/mod.rs`
- Modify: `crates/chat/tests/assistant_repositories.rs`

- [ ] Add failing tests for create/read of every type, ordered observation reads, exact idempotent replay, conflicting replay, ownership isolation through the job, and absent update/delete APIs.
- [ ] Implement the target repository methods with SQLx only in the repository. Decode JSON into typed values and fail closed on invalid persisted schema versions or malformed documents.
- [ ] Add `derive_and_insert_effective(job_id, revision, contracts)` that reads ordered observations, runs the pure merge, and inserts exactly one immutable revision in one transaction.
- [ ] Add `insert_initial_state(original, observations, effective)` as one transaction so partial baseline state cannot be visible.
- [ ] Verify concurrent inserts produce one sequence/revision winner; exact duplicates read back identically and conflicting duplicates return a sanitized repository conflict.
- [ ] Drop and recreate the test database from migrations, then run `cargo test -p chat --test assistant_repositories canonical -- --nocapture`; expected green.

## Task 4: Shadow-write job-scoped state without changing decisions

**Files:**
- Modify: `crates/core/src/config.rs`
- Modify: `crates/chat/src/chat/service/job.rs`
- Modify: `crates/chat/src/assistant/job_memory_repo.rs`
- Modify: `crates/chat/src/assistant/session_memory_repo.rs`
- Modify: `crates/chat/src/assistant/context_builder.rs`
- Modify: `crates/chat/src/assistant/runtime/mod.rs`
- Modify: `crates/chat/tests/chat_full_flow.rs`

- [ ] Add `CHAT_CANONICAL_GATEWAY_MODE=disabled|shadow|authoritative`, default `disabled`, with invalid values failing config startup.
- [ ] Add failing full-flow tests proving shadow mode preserves the legacy response/status while writing one original intent, original/deterministic observations, effective revision 0, and no duplicate baseline on retry.
- [ ] Capture `raw_message_id`, accepted initial parse, `reference_instant = chat_jobs.created_at`, and timezone `Asia/Jakarta` once. Never derive the baseline from `chat_jobs.message`, a clarification message, or `SourceIntentSnapshot.prompt` after creation.
- [ ] On every later message, append only that message's extraction as audit observations. If it is a validated clarification patch, label only explicitly supplied fields `clarification`; otherwise label advisory/deterministic facts by their real source.
- [ ] Compute shadow effective revisions from the job's observations. Compare sanitized selected capability, decision code, and exact normalized parameters against legacy output; emit IDs, field names, counts, and hashes only.
- [ ] Keep `SessionMemory.pending_clarification` available to the legacy runtime through Phase 4, but remove it from canonical derivation inputs. Add a test where two jobs in one session produce independent effective revisions despite the singleton pending value.
- [ ] Add `planner_snapshot_id: Option<Uuid>` to `JobMemory` and checkpoint JSON for audit/recovery; in shadow mode it is informational and cannot alter execution.
- [ ] Run `cargo test -p chat --test chat_full_flow canonical_shadow -- --nocapture`; expected green with unchanged legacy assertions.

## Task 5: Persist one planner snapshot and gate authority

**Files:**
- Modify: `crates/chat/src/assistant/runtime/mod.rs`
- Modify: `crates/chat/src/assistant/tool.rs`
- Modify: `crates/chat/src/chat/service/job.rs`
- Modify: `crates/chat/src/chat/planner.rs`
- Modify: `crates/chat/src/chat/executor.rs`
- Modify: `crates/chat/tests/chat_full_flow.rs`
- Modify: `crates/chat/tests/assistant_repositories.rs`

- [ ] Add failing tests that mutate session memory, job memory, current principal, catalog latest, and clock after snapshot insertion; planner/policy/executor must still use the snapshot's principal, catalog version, capability, reference instant, timezone, and normalized parameters.
- [ ] Build normalized parameters only from the selected catalog version plus typed effective constraints. Persist the snapshot before policy or SQL and reload it by `(snapshot_id, job_id)`.
- [ ] Replace authoritative `plan_selected_capability_verified(..., intent, deterministic_extraction)` use with `plan_from_snapshot(catalog_version, &PlannerInputSnapshot)`. Keep the old function only behind disabled/shadow mode.
- [ ] Pass the snapshot's principal projection to principal-neutral `evaluate_policy`; pass the resulting plan parameters unchanged to `execute_plan`. No component may supplement them from live `AssistantIntent`, `current_user_message_metadata_json`, or `SessionMemory`.
- [ ] In `authoritative` mode, missing/malformed/mismatched snapshot state fails closed before SQL. There is no fallback to the legacy plan. Shadow mismatch also blocks promotion but does not change shadow traffic.
- [ ] Assert one snapshot per job revision, a new immutable snapshot for a later effective revision, checkpointed snapshot ID, and zero SQL calls for snapshot validation failure.
- [ ] Run `cargo test -p chat --test chat_full_flow canonical_snapshot -- --nocapture`; expected green.

## Task 6: Prove cutover and define the legacy-slot deletion gate

**Files:**
- Modify: `crates/chat/tests/assistant_context_window.rs`
- Modify: `crates/chat/tests/chat_full_flow.rs`
- Modify after gates pass: `docs/current/status.md`
- Modify after gates pass: `docs/current/active-context.md`

- [ ] Add a context-window regression test proving canonical planner input is job-scoped and independent of `pending_clarification` and `source_intent` session fields.
- [ ] Replay persisted observations into a fresh effective revision and assert byte-equivalent canonical JSON, winning observation IDs, selected capability, and normalized parameters.
- [ ] Run shadow mode against all `chat_full_flow` cases and a production-like fixture. Promotion requires zero capability/decision/parameter mismatches and zero missing canonical rows for new jobs.
- [ ] Canary `authoritative` mode, then expand only after snapshot/replay mismatch metrics remain zero. Rollback changes new jobs to `shadow`; jobs already marked canonical continue from their persisted snapshot and are never downgraded.
- [ ] Do not remove session pending ownership in Phase 2. Record Phase 4 as the owner of job-scoped `ClarificationTurn`, atomic response application, and legacy pending drain.
- [ ] Delete `current_user_message_metadata_json.deterministic_extraction`, `DeterministicExtraction::merge_into`, and the `plan_selected_capability_verified` extraction argument only after: all active canonical jobs read snapshots; restart/replay passes without the slot; telemetry shows no authoritative read for 30 production days; shadow mismatches are zero; and an explicit cleanup migration/release is approved.
- [ ] Update current docs only after implementation and canary gates pass; do not mark Phase 4 clarification ownership complete.

## Validation and rollout gates

- [ ] Run:

```bash
cargo fmt --check
cargo check --workspace
cargo test -p chat canonical_state
cargo test -p chat --test assistant_repositories canonical
cargo test -p chat --test assistant_context_window
cargo test -p chat --test chat_full_flow canonical_shadow
cargo test -p chat --test chat_full_flow canonical_snapshot
cargo test --workspace
```

- [ ] Every command exits 0. Missing PostgreSQL/Fineract infrastructure is blocked, not passed.
- [ ] Migration rehearsal covers clean install, mixed legacy/new jobs, duplicate/concurrent inserts, forward schema with old application mode, and no guessed backfill or row loss.
- [ ] Observability inspection proves comparisons contain no raw prompt, SQL, bound values, principal token, result row, or PII.
- [ ] Authoritative promotion is forbidden until shadow equality, immutable replay, repository integrity, snapshot isolation, no-fallback failure, and restart recovery all pass.

## Completion checklist

- [ ] Additive schema and typed repositories preserve immutable job-scoped provenance.
- [ ] Original intent is created once from the initial message reference and never rewritten/reparsed.
- [ ] Clarification merge changes explicitly supplied fields only; clear/list contracts are enforced.
- [ ] Merge algebra and persisted replay properties pass.
- [ ] Legacy extraction is audit/shadow-only and session pending has no canonical authority.
- [ ] Planner, policy, and executor consume one reloaded immutable snapshot.
- [ ] Shadow comparison and authoritative cutover/rollback gates pass.
- [ ] The deterministic extraction slot remains until its measured deletion gate is satisfied.
