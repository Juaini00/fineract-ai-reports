# 007 — Analyst-grade knowledge catalog and request mapping

Status: active — requirements defined; execution pending
Severity: high
Area: knowledge | catalog | retrieval | clarification | temporal | LLM extraction | SQL | client contract
Created: 2026-07-24
Resolved:

Depends on: 005 (unified clarification contract, shipped), 006 (management observability, shipped)
Design reference: `docs/superpowers/specs/2026-07-24-llm-extraction-gateway-design.md`
Plan reference: `docs/superpowers/plans/2026-07-24-llm-extraction-gateway.md`

## Problem

The service now has a correct clarification *mechanism* but an inadequate knowledge
*catalog* and an incomplete request *mapping* layer. The real user of this system is
a banking analyst, not a casual chat user. Analyst questions are single-shot,
multi-dimensional, and expect a complete answer without follow-up interrogation.

A representative real question:

> "saya ingin mengetahui pada saving tersebut apakah ada nasabah (client) yg masih
> memiliki hutang, sebutkan jenis charge tersebut, tanggal berapa due nya atau jika
> terlewat sudah berapa, yg dibayar berapa dan sisa berapa?"

That one prompt requires: client identity, charge type, due date, days overdue,
amount originally due, amount already paid, amount waived, amount still outstanding,
and currency — scoped to the caller's offices, as of the tenant business date, with
no row cap. It must not produce a clarification asking "summary or top-N?" or
"what date range?" or "how many rows?".

Today the system fails this in three independent ways:

1. **Catalog gap.** No capability returns that full field set. The closest one
   (`savings_pending_charges_clients`, added 2026-07-24) returns only 9 shallow
   columns and omits `days_overdue`, `amount_paid`, `amount_waived`, and
   `amount_original`.
2. **Mapping gap.** The LLM extraction gateway described in the design spec
   (Layer 1 gateway → Layer 2 resolver → Layer 3 decider) is only partially
   realised. Parameter defaults are now applied, but candidate selection still runs
   through the legacy semantic scorer with no verified mapping from analyst phrasing
   to capability.
3. **Temporal gap.** "Today" is now the tenant business date at the parameter-fill
   boundary, but the deterministic temporal extractors that handle "kemarin",
   "minggu lalu", "bulan lalu" have not been re-pointed at business date, and the
   Phase-3 YAML auto-migration applied `default: business_today` uniformly without
   per-capability review.

## Product decisions already made

- **"Today" means the Fineract tenant business date**, read from
  `m_business_date WHERE type = 'BUSINESS_DATE'`. Wall-clock time is only for audit
  timestamps (`occurred_at`, trace `created_at`, outbox `created_at`), never for
  domain filtering. Falling back to wall clock is allowed but must emit
  `business_date.fallback_used`.
- **Analyst-class capabilities execute without clarification by default.** Where a
  parameter has a sane default (`business_today`, `unbounded`, `authorized_scope`),
  the system fills it silently and proceeds. Clarification is reserved for genuine
  ambiguity of *intent*, not for parameter harvesting.
- **One analyst question maps to one complete capability.** Capabilities are designed
  per analyst workflow, not per atomic SQL fragment. If answering a common analyst
  question needs three joins and eight columns, that is one capability with eight
  output fields — not three capabilities the analyst must stitch together.
- **Office scope stays bound inside approved SQL** via `office_ids = ANY($n::bigint[])`.
  Never post-filtered in Rust.
- **PII gating stays field-level** via `pii.allowed_fields_when_can_view_pii` /
  `omitted_when_cannot_view_pii`. Chat's admin projection sets `can_view_pii = true`;
  that is intentional and documented in `AGENTS.md`.
- **No arbitrary AI-generated SQL.** Execution remains restricted to approved
  `queries/*.sql` selected by capability id.

## Current state — what is already shipped

Landed on `feature/management-observability-audit`:

| Commit | Deliverable |
| --- | --- |
| `4992fe3` | `BusinessDateProvider` trait, `FineractBusinessDateProvider`, `AuditingBusinessDateProvider`, `StaticBusinessDateProvider`, `AuditEventType::BusinessDateFallback` |
| `2f0fecf` | `ParameterPolicy`, `DefaultExpr` parser + evaluator, `validate_policies` (11 unit tests) |
| `0e00a4c` | `CapabilityKnowledge::parameter_policies` field |
| `9ec535a` | Loader parses the `parameters:` YAML block into policies (3 unit tests) |
| `0df6fed` | `scripts/migrate_capability_policies.py` |
| `61c0a72` | 29 capability YAMLs migrated to the per-parameter policy block; validator accepts it |
| `9f5c8c2` | Capability `savings_pending_charges_clients` + query YAML + SQL + metric |
| `197c331` | `BusinessDateProvider` wired into `ChatAppState` → `JobService` → `CanonicalRuntimeContext.business_today` |
| `27669e1` | `params_from_verified` consults `parameter_policies` and applies defaults before demanding clarification (4 unit tests) |

Verified green at the time of writing: `cargo test -p chat --lib` (191 pass),
`cargo test -p chat --tests`, `cargo clippy --workspace --all-targets -D warnings`.

## Evidence — concrete observed failures

### E1. Wrong capability offered for a clear analyst intent

Input: `"beritahukan saya siapa saja yg masih memiliki charge yg belum dibayar hari ini pada savings"`

Observed clarification options: `savings_balance_summary`, `savings_activity_list`,
`savings_withdrawal_top_n`, `savings_withdrawal_total`, `others`.

None of these answer "unpaid charges". The scorer fell back to generic savings
capabilities. `savings_pending_charges_clients` now exists but has **not been proven**
to score above the gap threshold for Indonesian or English phrasings of this intent.

### E2. Clarification demanded for a parameter that has a default

Same input produced a `date_range` field requirement on `savings_activity_list`,
whose YAML declares `from_date`/`to_date` with `default: business_today`. Fixed at the
`params_from_verified` boundary by `27669e1`, but **no end-to-end runtime test proves
the clarification no longer appears**.

### E3. Field-level incompleteness in the newest capability

`savings_pending_charges_clients` output fields today:

```
client_id, client_display_name, office_id, office_name,
charge_id, charge_name, currency_code, amount_outstanding, due_date
```

Missing for the representative analyst question: `amount_original` (`amount` on
`m_savings_account_charge`), `amount_paid` (`amount_paid_derived`), `amount_waived`
(`amount_waived_derived`), and a derived `days_overdue`.

### E4. Uniform temporal default applied without review

`scripts/migrate_capability_policies.py` assigned `default: business_today` to every
date parameter across all 29 capabilities. For point-in-time capabilities that is
correct. For historical-range capabilities (for example a quarter-over-quarter office
ranking) it silently narrows the window to a single day instead of asking, which is a
**wrong answer instead of a clarification** — strictly worse than the original bug.

### E5. Frontend cannot submit a `date_range` answer

The dashboard posts `answers.date_range` as a plain string. The backend contract
requires an object `{ from, to }`. Result:

```json
{ "success": false, "data": null,
  "error": { "code": "clarification_validation_error",
             "message": "Clarification response is invalid.",
             "details": { "fields": ["answers.date_range"] } } }
```

Tracked here for completeness; frontend work is explicitly out of scope for this
issue's backend workstreams (see W-F).

## Goals

- An analyst can ask a complex, multi-dimensional question and receive a complete
  answer in one turn, with no parameter-harvesting follow-up.
- Every capability that a real analyst question maps to returns the full field set
  that question implies.
- "Today", "kemarin", "minggu lalu", "bulan lalu", "bulan ini", "tahun ini" all
  resolve against the tenant business date, consistently, everywhere.
- Every date/limit/scope parameter default is correct **per capability**, reviewed by
  a human, not applied uniformly by a script.
- When no capability genuinely matches, the system says so plainly rather than
  offering unrelated options.
- The mapping from user phrasing to capability is testable and regression-guarded.

## Non-goals

- Knowledge authoring, approval, or publishing workflow.
- Arbitrary AI-generated SQL.
- Multi-provider LLM routing or fallback.
- Cost quotas or request blocking.
- Frontend implementation (tracked, not executed here).
- A general Fineract operational/security audit surface.

---

## Workstreams

Each workstream is independently executable and independently reviewable. Suggested
order is W-A → W-B → W-C → W-D → W-E, but W-A and W-B can run in parallel (disjoint
file sets), as can W-C and W-D.

### W-A — Analyst question inventory and catalog completeness

**Objective:** Prove the catalog answers the questions analysts actually ask, and
close the gaps it does not.

**A1. Build the analyst question inventory.**
Create `docs/product/analyst-question-inventory.md`. Enumerate the real questions an
admin/analyst will ask across savings, client, organization, and loan domains. For
each: the Indonesian phrasing, the English phrasing, the field set the answer must
contain, and whether any parameter is genuinely required from the user. Source
material: existing `examples:` blocks in `knowledge/capabilities/**/*.yaml`, the
representative question in this issue, and the domain docs under `docs/reporting-data/`.

Minimum 25 questions. Mark each `covered` / `partial` / `missing` against the current
catalog.

**A2. Enrich `savings_pending_charges_clients` to analyst grade.**
Files: `queries/savings/pending_charges_clients.sql`,
`knowledge/queries/savings/pending_charges_clients.yaml`,
`knowledge/capabilities/savings/pending_charges_clients.yaml`.

Add output fields (verify exact column names against
`psql -U root -d fineract_default -c "\d m_savings_account_charge"`):

| Output field | Source | Notes |
| --- | --- | --- |
| `amount_original` | `sac.amount` | Charge amount as levied |
| `amount_paid` | `sac.amount_paid_derived` | Already settled |
| `amount_waived` | `sac.amount_waived_derived` | Waived portion |
| `days_overdue` | `$2::date - sac.charge_due_date` | `NULL` when `charge_due_date IS NULL`; negative means not yet due — clamp to `0` or expose signed, decide and document |

Keep `amount_outstanding` (`sac.amount_outstanding_derived`). Preserve
`require_office_filter`, single-statement SELECT, parameterized-only.

Update `output_fields` in both the query YAML and the capability YAML, keeping
`sensitivity` labels coherent (`client_id`/`client_display_name` remain
`pii_conditional`; monetary and date fields are `public_business`).

**A3. Close the gaps A1 identified.**
For every question marked `missing` or `partial`, either enrich an existing capability
or add a new one. Each new capability needs: capability YAML with the per-parameter
policy block, query YAML, approved SQL under `queries/`, and any new metric YAML.
Do not add a capability without a real question from A1 backing it.

**A4. Analyst-class default review (closes E4).**
Walk all 30 capability YAMLs. For each date parameter decide, per capability:

- **point-in-time** → `required: false, default: business_today, fill_when_missing: true`
- **rolling window** → `required: false, default: business_today - Nd|Nm` for the
  `from` side and `business_today` for the `to` side
- **historical, user must specify** → `required: true`, no default

Same treatment for `limit`: analyst-facing detail lists get `default: unbounded`
with a `hard_cap`; genuine top-N rankings keep `required: true` or a sensible
numeric default.

Record the decision per capability in a table appended to
`docs/product/analyst-question-inventory.md` so the reasoning survives.

**Acceptance:**
- `docs/product/analyst-question-inventory.md` exists with ≥25 questions, each mapped
  to a capability id and a coverage verdict.
- Zero questions remain `missing` for the savings and client domains.
- The representative question in this issue's Problem section is answered completely
  by exactly one capability.
- `cargo test -p chat --test catalog_validation` green.

---

### W-B — Business date correctness end to end

**Objective:** Every temporal expression resolves against the tenant business date.

**B1. Audit the deterministic temporal extractors.**
File: `crates/chat/src/assistant/understanding/extraction/temporal.rs` (291 lines).
Find every place that computes a date from `Utc::now()` or an injected reference
instant. Establish whether the reference instant already carries business date or
wall clock.

**B2. Re-point relative expressions at business date.**
`hari ini`, `kemarin`, `minggu ini`, `minggu lalu`, `bulan ini`, `bulan lalu`,
`tahun ini`, `tahun lalu`, `N hari terakhir`, and their English equivalents must all
derive from `CanonicalRuntimeContext.business_today`, matching the mapping table in
design spec §5.4. One unit test per expression, both languages.

**B3. Surface the business date to the user when it differs from wall clock.**
When `business_date_source == Fineract` and `business_today != Utc::now().date_naive()`,
the response should carry a short note (for example a `warnings` entry, or a metadata
field the client can render) stating the reporting date used. Never silently answer
"today" with a date the analyst does not expect. Keep the copy English-only.

**B4. Prove the fallback path is observable.**
Extend `crates/chat/tests/business_date_provider.rs` so a forced fallback provably
enqueues `business_date.fallback_used` into `management_audit_outbox` and it becomes
visible via `GET /management/audit`.

**Acceptance:**
- Every relative temporal expression has a passing unit test asserting a
  business-date-derived value.
- No production code path computes a domain-filter date from `Utc::now()`.
- Audit timestamps still use `Utc::now()` — assert this explicitly in a test so a
  future refactor cannot silently swap them.
- A response whose reporting date differs from wall clock says so.

---

### W-C — Request mapping: LLM extraction gateway Phases 4–6

**Objective:** Replace the inline legacy extraction/classification path with the
three-layer gateway from the design spec, so analyst phrasing maps to capability
reliably and auditably.

Read `docs/superpowers/specs/2026-07-24-llm-extraction-gateway-design.md` §§3–6 and
`docs/superpowers/plans/2026-07-24-llm-extraction-gateway.md` Phases 4–7 before
starting. Phases 0–3 are shipped; do not redo them.

**C1. Layer 1 — LLM gateway (plan Phase 4).**
Create `crates/chat/src/assistant/understanding/gateway/{mod,schema,prompt,client}.rs`.

- `schema.rs`: `LlmGatewayExtraction` and subtypes exactly per spec §4.1, with
  `schemars` derives.
- `prompt.rs`: renders the user message, recent-turn summary, and a **safe** capability
  catalogue (`id`, `display_name`, `description`, `use_when`). Never SQL, never
  parameter internals, never PII policy.
- `client.rs`: schema-constrained call through the existing `SharedLlmClient` /
  `TracedLlmClient`. One retry on schema-invalid, then `GatewayError::SchemaInvalidAfterRetry`.
  Sanitize: drop entities whose `value` is not a substring of the user message; drop
  candidates whose `capability_id` is not in the caller-visible catalogue. Emit
  `llm_gateway.entity_dropped` / `llm_gateway.candidate_dropped` through the management
  outbox, mirroring the `business_date.fallback_used` wrapper pattern.

Do not invent a new LLM abstraction. If `SharedLlmClient` lacks a structured-output
helper, add one that wraps the existing call and enforces `schema_for!`.

**C2. Layer 2 — deterministic resolver (plan Phase 5).**
Create `crates/chat/src/assistant/understanding/resolver.rs` with `ResolverRequest`,
`ResolvedRequest`, `ResolvedParameter { value, source }`. Precedence per spec §5.4:
user-typed → LLM hint (confidence ≥ 0.7, via the §5.4 mapping table) → YAML default.
Tag every filled parameter with a `PayloadSource`
(`user_text | llm_claim | catalog_default | business_date | authorized_scope | wall_clock_fallback`).

One unit test per row of the §5.4 mapping table.

The existing `params_from_verified` policy-default logic (commit `27669e1`) is the
seed for this module — move it, do not duplicate it. `params_from_verified` should
end up delegating to the resolver rather than owning precedence logic.

**C3. Layer 3 — clarification decider (plan Phase 6).**
Create `crates/chat/src/assistant/understanding/decider.rs` with
`DecisionOutcome::{Execute, Clarify, Reject}` per spec §6. Consume the resolver output
plus the existing gap-based `decide_from_scores`. Ask only when a `required: true`
parameter is genuinely unfilled, or the classifier signalled `Clarify`, or the intent
is unsafe/unsupported.

**C4. Runtime wiring (plan Phase 7).**
Wire gateway → resolver → decider into
`crates/chat/src/assistant/execution/runtime/mod.rs`. Persist
`state_json.llm_extraction` (Layer 1 output) and `state_json.resolved_request`
(Layer 2 output, including per-parameter `source`) on job memory. Demote the legacy
deterministic extractors in
`crates/chat/src/assistant/understanding/extraction/` to verification helpers that
sanity-check LLM output (substring and span validation) rather than being the primary
extraction path. Do not delete them.

**C5. Scenario tests (plan Phase 8).**
Create `crates/chat/tests/extraction_gateway_scenarios.rs` covering all seven rows of
design spec §7 plus the representative analyst question from this issue. Use a stub
LLM client returning canned `LlmGatewayExtraction` JSON per scenario. Assert the
Execute-vs-Clarify decision, the resolved parameter values, and that no
`chat.clarification_requested` audit event fires for the auto-execute rows.

**Acceptance:**
- All eight scenarios pass end to end.
- `state_json.llm_extraction` and `state_json.resolved_request` are populated on every
  executed job.
- No production path reads the legacy extractors as a primary source.
- `cargo clippy --workspace --all-targets -D warnings` clean with zero `#[allow]`.

---

### W-D — Retrieval and classification verification

**Objective:** Prove analyst phrasing reaches the right capability, in both languages.

**D1. Bilingual retrieval regression suite.**
Extend or create a fixture-driven suite (`crates/chat/tests/retrieval_scoring.rs` is
the existing home) that asserts, for each question in the W-A inventory, that the
intended capability id ranks first and clears the gap threshold. Both Indonesian and
English phrasings.

**D2. Close scoring gaps found by D1 (closes E1).**
Where a question does not reach its capability, fix it in the catalog, not in the
scorer: enrich `examples:`, `supported_intents:`, `description:`, and domain/metric
tags with the vocabulary analysts actually use — `belum bayar`, `hutang`, `tunggakan`,
`jatuh tempo`, `terlambat`, `outstanding`, `overdue`, `arrears`, `unpaid`, `pending`.

Only touch `ClassificationPolicy` thresholds if the catalog fix genuinely cannot
work, and justify it in the commit message.

**D3. Honest unsupported.**
When no candidate clears the floor, the response must be a plain sanitized
"not supported" message naming what the system *can* do in that domain — not a
clarification listing unrelated capabilities. Add a test for a deliberately
out-of-catalog question asserting `Unsupported`, not `Clarify`.

**Acceptance:**
- Every W-A inventory question has a passing retrieval assertion in both languages.
- The E1 input maps to `savings_pending_charges_clients` at rank 1.
- An out-of-catalog question yields `Unsupported`, and the test proves the offered
  options list is empty.

---

### W-E — Clarification suppression for analyst-class capabilities

**Objective:** Guarantee that a fully-defaulted capability never asks the analyst
anything (closes E2 with a durable guard).

**E1. Runtime-level no-clarification test.**
Add a graph-level test (not just `params_from_verified` unit level) that runs a
capability whose every parameter carries a default through the full runtime and
asserts the terminal state is `Completed`, not `WaitingForUserInput`, and that no
clarification payload is attached to the response.

**E2. Catalog invariant.**
Add a `KnowledgeValidator` rule: if a capability declares
`output_mode` in the analyst-detail family **and** every parameter has
`required: false` with a default, then it must be flagged as
non-clarifying — and a corresponding runtime assertion must hold. Decide the exact
marker (a `clarification.never_ask: true` field, or derivation from the policy block —
prefer derivation, no new YAML surface unless needed).

**E3. Regression guard against the old behaviour.**
Assert that no capability which previously demanded `from_date`/`to_date` still does,
unless its policy block explicitly marks those `required: true` with no default.
This is the guard the plan's self-review checklist calls for.

**Acceptance:**
- A capability with all-defaulted parameters provably completes in one turn.
- The validator rejects a configuration that would reintroduce the E2 behaviour.

---

### W-F — Frontend contract alignment (tracked, not executed here)

Backend-owned obligations only:

**F1.** Publish the exact `answers.<field>` value shapes per `field_type` in
`docs/current/management-dashboard-integration.md` — specifically that `date_range`
takes `{ "from": "YYYY-MM-DD", "to": "YYYY-MM-DD" }`, not a string. Include a worked
request/response pair for each `field_type`.

**F2.** Ensure `clarification_validation_error` `details.fields` entries are precise
enough for a client to highlight the offending input.

Frontend implementation (the actual date-range picker) is tracked in the dashboard
repo, not here. E5 stays open until that lands.

---

## Cross-cutting constraints

Every workstream inherits these:

- Branch: continue on `feature/management-observability-audit` unless the executor is
  told otherwise. No new crates — `app`, `core`, `chat` only.
- Layering `route → service → repository → database`; no `sqlx` outside repositories.
- Schema changes only via new `migrations/*.sql`; startup never creates schema. None
  of these workstreams is expected to need a migration.
- Approved SQL only, from `queries/*.sql`, single statement, parameterized, office
  scope bound inside SQL.
- Sanitized errors: never leak SQL, prompts, provider text, stack traces, or secrets.
- English-only user-facing copy.
- Pre-commit runs `cargo fmt --check` and
  `cargo clippy --workspace --all-targets -D warnings`. Fix warnings at the source;
  `#[allow]` is not acceptable.
- Small, reviewable commits — one logical unit each.
- Never `git push` without an explicit request.
- Deliberate simplifications get a `ponytail:` comment naming the ceiling and the
  upgrade path.

## Overall acceptance criteria

This issue is resolved when all of the following hold:

1. The representative analyst question in the Problem section executes in a single
   turn and returns client identity, charge type, due date, days overdue, amount
   originally due, amount paid, amount waived, and amount outstanding.
2. No clarification is emitted for any parameter that has a declared default.
3. Every relative temporal expression resolves against the tenant business date, and a
   test asserts audit timestamps still use wall clock.
4. Every question in the analyst inventory maps to its intended capability at rank 1
   in both Indonesian and English.
5. An out-of-catalog question returns a plain unsupported message with no unrelated
   options.
6. `state_json.llm_extraction` and `state_json.resolved_request` are populated on
   every executed job, with per-parameter source labels.
7. Per-capability date and limit defaults have been human-reviewed and the reasoning
   is recorded.
8. `cargo test -p chat`, `cargo test -p core`, and
   `cargo clippy --workspace --all-targets -D warnings` are all green.
9. `docs/current/management-dashboard-integration.md` documents the exact
   `answers.<field>` value shape for every `field_type`.

## Open questions

- **Days-overdue sign convention.** Expose signed (negative = not yet due) or clamp at
  zero and rely on `due_date` for the not-yet-due case? Pick one, document it in the
  capability YAML `description`, and stay consistent across every aging capability.
- **Group-owned savings accounts.** `savings_pending_charges_clients` inner-joins
  `m_client`, which excludes group-owned accounts. Is that the intended analyst
  semantics, or should there be a parallel group-scoped capability? Needs a domain
  decision before W-A3.
- **Composite capabilities.** If several analyst questions each need the same three
  sub-views stitched together, is the answer one wide capability or a new composite
  execution mode? Defer until the W-A1 inventory shows how often it actually happens —
  do not build the abstraction speculatively.
- **`Unbounded` representation.** Currently `i64::MAX` bound into `LIMIT $n`
  (commit `27669e1`, carries a `ponytail:` note). Revisit only if a query plan
  regression appears; the alternative is a LIMIT-less SQL variant per capability,
  which doubles the approved-SQL surface.
- **Loan-domain parity.** This issue's examples are savings-centric. The loan domain
  likely has the same analyst questions (arrears, overdue instalments, outstanding
  interest). Confirm during W-A1 whether loan capabilities exist at all, and scope
  loan work into A3 or split it into a follow-up issue.

## Suggested execution order for a fresh session

1. **W-A1** first, always. The inventory is the specification for everything after it.
2. **W-A2 + W-A4** (savings enrichment and default review) — highest user-visible value.
3. **W-B** (business date everywhere) — can run in parallel with W-A, disjoint files.
4. **W-D1** (retrieval suite) — needs W-A1 to exist; will surface the real gaps.
5. **W-A3 + W-D2** together — close catalog and scoring gaps found by D1.
6. **W-E** — lock in the no-clarification guarantee with tests.
7. **W-C** — the gateway architecture refactor. Deliberately last: it is cleanup that
   makes the system auditable and maintainable, but it does not by itself fix any
   currently-observed user-facing failure. Do not let it block W-A through W-E.
8. **W-F1/F2** — documentation, any time after W-A2 stabilises the field shapes.
