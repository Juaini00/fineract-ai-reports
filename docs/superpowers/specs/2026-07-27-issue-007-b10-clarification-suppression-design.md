# Issue 007 — Bundle 10: Clarification Suppression Guarantee (both directions) — Design

**Bundle:** B10 (W-E + W-O F8) of `docs/superpowers/plans/2026-07-27-issue-007-program-roadmap.md`.

**Goal:** Make the two halves of one contract provable and durable, over the real
catalog, under `cargo test`:

1. **W-E — a fully-defaulted capability never asks.** Every capability whose
   required user parameters all carry a policy default must plan straight through
   to a completed answer with **no** clarification payload — and a durable,
   catalog-wide guard must fail if a future edit reintroduces a
   `from_date`/`to_date` clarification.
2. **W-O F8 — a defaultless required parameter always asks.** A capability with a
   `required: true` parameter that has **no** default (today: `search` on
   `client_name_lookup`) must return a `collect_fields` clarification on the
   confident routing path, **not** a confident empty table and **not** a generic
   operational error.

Both halves are asserted by the same test pass, because a one-directional
guarantee is exactly what lets the broken half look finished.

## Background — why this bundle exists

W-E (issue lines 399–431) closes E2: analyst-class capabilities were originally
demanding `from_date`/`to_date` even though the migration to per-parameter
policies gave those parameters `default: business_today`. F8 (issue lines
1389–1548) is the inverse defect: `client_name_lookup.search` is `required: true`
with no default, yet the system never asks for it — a heuristic invents a value
and the request returns an authoritative-looking empty result.

## Current state (verified 2026-07-27)

Verified against the working tree with `Read`/`grep`. The issue text is dated
2026-07-24; several of its claims have drifted.

### W-E side

- **The policy-default auto-fill works.** `params_from_verified`
  (`crates/chat/src/assistant/execution/tool/parameters.rs:216-273`) resolves each
  query parameter, falling back to `resolve_policy_default` (`:278-290`) then
  `default_required_parameter` (`:7-10`, for `limit`/`top_n`). `from_date`/`to_date`
  with `default: business_today` are filled from the `EvaluationContext`, so the
  `bail!("missing parameter …")` at `:268` is never reached for them.
- **DRIFT — E1's runtime test already exists.** The issue asks for "a graph-level
  test (not just `params_from_verified` unit level)". That test is already in the
  tree: `date_parameters_with_a_policy_default_are_auto_filled_without_asking`
  (`crates/chat/tests/chat_jobs.rs:247`) drives `savings_deposit_total` through the
  full HTTP + job-service + canonical-authoritative stack and asserts
  `status == "completed"` and `result_json…structured_response.clarification` is
  null. What is genuinely **missing** is a *durable, catalog-wide* guard — that
  test pins exactly one capability, so a new capability (e.g. from W-A3) can
  reintroduce the E2 behaviour with a green suite.
- **DRIFT — there is no "analyst-detail output_mode family".** E2's wording keys
  the invariant off "`output_mode` in the analyst-detail family". The catalog's
  actual `output_mode` values are only `list`, `summary`, `total`, `top_n`,
  `monthly_breakdown`, `monthly_top_n` (`knowledge/capabilities/**`). No
  "analyst-detail" family exists. Per the issue's own stated preference
  ("prefer derivation from the policy block — no new YAML surface unless needed"),
  the invariant is derived **entirely from the parameter-policy block**, with no
  new YAML field and no dependence on `output_mode`.

### F8 side

- **CONFIRMED — the heuristic still invents a value.** `extract_person_name`
  (`crates/chat/src/assistant/understanding/extraction/token.rs:31-53`) still has
  the second-pass adjacency rule (`:43-51`): the token after `client`/`find`
  (excluding only `client`/`name`/`named`) is returned as a person name. On
  `"Search client by display name."` it returns `by`; on
  `"ada gak nama Tony di client kita?"` it returns `kita`. This scavenges a filler
  token from any phrasing, so `search` is never seen as missing.
- **CONFIRMED — `search` is the one defaultless required parameter.**
  `knowledge/capabilities/client/name_lookup.yaml` declares `search: { required: true }`
  with no `default`; `knowledge/queries/client/name_lookup.yaml` mirrors it as a
  required parameter. It is the only such parameter in the catalogue.
- **DRIFT — the modern (canonical) path errors, it does not re-clarify.** The
  issue describes the ask as "a planning `bail!` that is afterwards translated back
  into a clarification". That translation exists **only on the legacy
  (`canonical = None`) path**: `execute_selected_capability`'s `Ok(None)` branch
  (`crates/chat/src/assistant/execution/runtime/execution.rs:105-183`) catches the
  `params_from_verified` bail and re-clarifies. In canonical **authoritative** mode
  (production), `normalize_effective_parameters`
  (`parameters.rs:59-95`) bails, `authoritative_plan`
  (`runtime/planning.rs:218`) propagates the `Err`, and the `Err(error)` arm at
  `execution.rs:184-204` returns `TerminalState::FailedOperational`
  (`"canonical_snapshot_invalid"`) — a generic error, **not** a `collect_fields`
  clarification. So the fix must gate required inputs **before** the
  authoritative/legacy split, not inside one branch.
- **CONFIRMED — F7 is already shipped and green.** `409 clarification_not_active`
  is exercised by `chat_jobs.rs:213`. This bundle leaves it untouched; it only
  keeps the same-job-continuation contract intact for the new ask path.

### The trap the design must avoid

`ClarificationPlanner` (`crates/chat/src/assistant/context/clarification_planner.rs`)
already computes missing inputs, but its `input_satisfied` for `date_range`
(`:207-215`) requires actual `FromDate`/`ToDate` **facts** — it does **not** know
about policy defaults. So a naïve "run the planner before execution" gate would
mark `date_range` as missing and **re-introduce the exact E2 date clarification
that W-E removes**. The gate must ask **only** for inputs whose backing parameter
policy is `required: true` with **no** default — the same policy-derived set that
defines E2's invariant. This is why W-E and F8 are one contract: the set that
"never asks" and the set that "always asks" are complements of each other,
computed from one source (the policy block).

## Constraints (invariants preserved)

- Approved-SQL only; office scope bound **inside** SQL via
  `office_ids = ANY($n::bigint[])`; never a Rust post-filter.
- No `sqlx` in handlers, services, or `assistant/**`. This bundle adds none.
- PII gating stays field-level; "today" = Fineract tenant business date
  (`EvaluationContext.business_today`); wall clock only for audit.
- Sanitized errors — the ask exposes the stable input registry field metadata,
  never a raw `bail!` string, parser error, SQL, or prompt.
- PostgreSQL durable truth, Redis live-SSE only. Same-job clarification via
  `POST /chat/jobs/{job_id}/responses` — the ask reuses the existing
  `is_missing_execution_parameters` continuation, spawning no new job.
- Exactly 3 crates. **No new dependency, migration, or knowledge/queries YAML
  surface.** No new YAML field — the invariant is derived from existing
  `parameter_policies`.
- English-only product copy.

## Design

### A. One source of truth: the policy-derived "must-ask" set

Add a single pure helper that both halves consume. For a selected capability it
returns the required user inputs that carry **no** default and are therefore the
only legitimate things to ask for:

`crates/chat/src/assistant/context/clarification_planner.rs`

```rust
/// Required user inputs for `capability_id` that (a) map to a query parameter
/// declared `required: true`, (b) carry no policy default, and (c) are not yet
/// satisfied by `facts`. These are the only parameters the runtime may ask for
/// on the confident routing path — parameters with a policy default are filled
/// silently (W-E) and must never appear here (else E2 date clarifications return).
/// Today this set is `{ search }` for `client_name_lookup` and empty for every
/// other approved capability.
/// ponytail: `search` is the only member today; the loop stays general so a new
/// defaultless required parameter is covered without another code change.
pub fn defaultless_missing_fields(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
    facts: &ClarificationFacts,
) -> Vec<ClarificationField>
```

Membership is computed from existing data only:
- `required_inputs(query, parameter_inputs)` already yields the required
  non-`authorized_scope` inputs.
- An input is **defaulted** (excluded) when *every* query parameter it covers is
  either filled by a `ParameterPolicy` with `required == false && default.is_some()`
  or is a `limit`/`top_n` parameter under a capability with a `default_limit`
  (the existing `default_required_parameter` / `limit_default` convention).
- Remaining inputs are kept only when `!input_satisfied(input, facts, &defaults)`.

The helper reuses `field_for`/`field_validation` so the emitted
`ClarificationField` is byte-identical to what `ClarificationPlanner` already
produces — the payload shape, the registry, and the same-job continuation all
already exist; only this filtered call site is new.

### B. F8 fix — narrow the extractor, gate before execution

**B1. Delete the adjacency scavenger.** In
`crates/chat/src/assistant/understanding/extraction/token.rs`, remove the second
`windows(2)` loop (`:43-51`). A one-token person name survives only from the
explicit `named X` / `name X` rule (`:36-42`). Positional adjacency to a domain
noun is deleted — it produced `by`/`kita`, never a name.

**B2. Gate required inputs before the mode split.** At the top of
`execute_selected_capability` (`runtime/execution.rs`), after the intent guard and
before the authoritative/legacy branch, build `ClarificationFacts` from the routed
intent + deterministic extraction and call `defaultless_missing_fields`. If it is
non-empty, return `TerminalState::WaitingForUserInput` with a `CollectFields`
payload carrying those fields (`is_missing_execution_parameters = true`, so the
existing continuation in `run_with_router` `:217-255` routes the answer back to the
**same** capability and job). Because the gate sits above the split, both the
canonical-authoritative and legacy paths ask identically — this is the root-cause
fix, not a per-branch patch. When the set is empty, execution proceeds exactly as
today.

This removes the reliance on the authoritative `Err → FailedOperational` arm and
on the legacy bail-recovery for the *missing-required-input* case; those arms
remain for genuinely invalid snapshots/temporal errors.

### C. W-E durable guard — validator rule + catalog-wide planning test

**C1. Validator rule (E2/E3 reintroduction guard).** Add
`validate_clarification_contract` to `KnowledgeValidator::validate`
(`crates/chat/src/knowledge/catalog/validator.rs`). For each `approved_mvp`
capability:
- For every required, non-`authorized_scope` query parameter mapped to the
  `date_range` input (`from_date`, `to_date`), the capability's
  `parameter_policies` **must** carry a matching policy with a `default`
  (`required == false && default.is_some()`). Otherwise `bail!` — this is the
  rule that makes reintroducing the E2 date clarification a load-time failure.
- Every member of the policy-derived **must-ask** set (required user parameter
  with no default) must be covered exactly once by the parameter-input registry,
  so a `collect_fields` ask is always constructible. (Extends the coverage check
  already in `validate_capability_parameter_contract`.)

**C2. Catalog-wide planning test (E1 + E3 durable guard).** Add a test to
`crates/chat/tests/catalog_validation.rs` that loads the real catalog and, for
**every** approved capability whose must-ask set is empty (the "non-clarifying"
capabilities), calls `plan_selected_capability_verified` with a bare
`AssistantIntent` (no entities/constraints) and a synthesized `EvaluationContext`
(a fixed `business_today`, empty office scope) and asserts it returns `Ok` with
every non-`authorized_scope` query parameter present in `plan.params`. This proves,
over the whole catalog and under `cargo test`, that a fully-defaulted capability
plans to completion without asking — and it fails the day a new capability adds a
defaultless required date parameter. The existing full-stack
`date_parameters_with_a_policy_default_are_auto_filled_without_asking`
(chat_jobs.rs) stays as the end-to-end witness; its doc comment is updated to point
at this durable guard.

### D. F8 runtime tests (both directions, one pass)

In `crates/chat/src/assistant/execution/runtime/tests.rs`, using the existing
`runtime_test_catalog()` + `pending_context` + `run_with_router` harness (no DB —
`fineract_pool = None`):

- **Positive (must-ask):** drive `client_name_lookup` with a message naming no
  person; assert terminal `WaitingForUserInput`, a `CollectFields` payload whose
  fields carry `search`, and that `memory.selected_tool` / `execution_summary`
  show **no** execution ran with a scavenged search term. (F8 acceptance, both
  sub-assertions.)
- **Negative (never-ask):** drive a fully-defaulted capability (e.g.
  `organization_office_activity_ranking`) the same way; assert terminal
  `Completed` (or `execution_not_configured` when the pool is absent) and **no**
  clarification payload. (W-E runtime witness at graph level.)

## Testing strategy

Run from repo root:

```bash
cargo fmt --check
cargo check -p chat
cargo test -p chat --lib clarification            # unit: defaultless_missing_fields, extractor
cargo test -p chat --lib execution::runtime::tests # F8 positive + W-E negative graph tests
cargo test -p chat --test catalog_validation       # C1 validator rule + C2 catalog-wide guard
cargo test -p chat date_parameters_with_a_policy_default_are_auto_filled_without_asking
```

Each unit/catalog test runs without a database. The full-stack witness
(`chat_jobs.rs`) needs the read-only Fineract DB and Redis, as it does today.
A change is complete only when all listed checks exit `0` and
`KnowledgeValidator::validate` on the real catalog stays green.

## Out of scope

- Adding a deterministic Indonesian `nama X` name rule to replace the deleted
  adjacency scavenger — name extraction for phrasings without `named`/`name` is
  the LLM router's job; deterministic re-point belongs to W-B, not here.
- Any change to `output_mode`, presentation, money formatting, or the answer
  builder (W-G / W-J / F4 / F6).
- Adding a `clarification.never_ask` YAML field — explicitly rejected in favour of
  policy-block derivation (no new YAML surface).
- F1–F7 of W-O (they ride other bundles); this bundle only keeps F7's 409
  contract green while adding a new ask path.
- Loan capabilities (issue 008).
