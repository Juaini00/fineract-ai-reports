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

## Appendix A — Fineract schema reference (verified against `fineract_qicard_default`, 2026-07-24)

### A.0 Method and caveats

**What was inspected.** Local PostgreSQL, user `root`, databases `fineract_qicard_default`
(schema authority) and `fineract_default` (value/semantics authority). Read-only:
`\d <table>`, `information_schema.columns`, `pg_tables`, and `SELECT` probes. No writes.

**Caveat 1 — `fineract_qicard_default` is schema-only.** It has the tables but almost no
rows:

| Table | Rows in `fineract_qicard_default` | Rows in `fineract_default` |
| --- | --- | --- |
| `m_office` | 1 | 8 |
| `m_client` | 0 | 39 |
| `m_savings_account` | 0 | 198 |
| `m_savings_account_charge` | 0 | 204 |
| `m_savings_account_transaction` | 0 | (many) |
| `m_loan` | 0 | 116 |
| `m_loan_arrears_aging` | — | 86 |
| `m_charge` | 6 | 51 |
| `m_savings_product` | 1 | (several) |
| `m_business_date` | **0** | 2 |

**`m_business_date` is empty in `fineract_qicard_default`.** Any `BusinessDateProvider`
pointed at this database resolves nothing and falls back to wall clock on **every**
request, permanently emitting `business_date.fallback_used`. In `fineract_default` the
rows are `BUSINESS_DATE = 2026-07-23` and `COB_DATE = 2026-07-22`. This is a deployment
fact that W-B must account for; it is not a bug in the provider.

Consequence for this appendix: **column names, types, nullability, indexes and foreign
keys are taken from `fineract_qicard_default`. Column *semantics* (what a value actually
means) were established by querying `fineract_default`, because qicard has no data to
observe.** Every semantic claim below states which database produced the evidence.

**Caveat 2 — the two databases are different Fineract builds, not different tenants of
the same build.** 295 public tables in qicard vs 293 in `fineract_default`, with
non-overlapping extras on both sides (qicard has `m_merchant_payout_*`, `m_solar_*`,
`m_charge_gl_map`, `m_external_bank`; `fineract_default` has `m_odoo_export_*`,
`m_iban_config*`, `m_loan_capitalized_income_balance`, `m_loan_buy_down_fee_balance`).
Column-level deltas on the tables this issue cares about:

| Table | Only in `fineract_qicard_default` | Only in `fineract_default` | Verdict |
| --- | --- | --- | --- |
| `m_savings_account_charge` | `external_id`, `is_overdraft`, `is_deduct`, `is_ratio` | `settlement_priority` | Every column used by the corrected SQL in A.1 exists in both. |
| `m_charge` | `is_overdraft`, `is_deduct`, `is_ratio`, `percentage_amount` | `settlement_priority` | `id`, `name`, `is_penalty`, `charge_time_enum`, `charge_applies_to_enum`, `currency_code` exist in both. |
| `m_savings_account` | `external_client_id`, `internal_saving_account`, `is_tsys`, `is_used_as_escrow`, `is_virtual_account` | `accrued_till_date`, `available_balance_derived`, `iban`, `receivable_settlement_mode` | `client_id`, `group_id`, `gsim_id`, `currency_code`, `currency_digits`, `status_enum`, `sub_status_enum` exist in both. |
| `m_savings_account_transaction` | `super_qi_transaction_id`, `super_qi_dateTime`, `gl_map_charge_id` | `value_date`, `transaction_date_time`, `hold_status`, `hold_type`, `parent_hold_transaction_id`, `posted_by_transaction_id`, `running_balance_on_reversal` | **Approved SQL must never reference `value_date` or `transaction_date_time`** — they do not exist in qicard. |
| `m_client` | `escrow_account_id`, `escrow_product_id`, `payout_method` | — | Identity/office columns identical. |
| `m_office` | `branch_code` | — | Identical otherwise. |
| `m_loan` | `is_shariah_compliant` | ~20 columns (capitalized income, buy-down fee, `iban`, `total_principal_derived`, …) | No column needed for arrears/outstanding reporting differs. |
| `m_loan_charge` | — | — | **Identical.** |
| `m_loan_arrears_aging` | — | — | **Identical.** |
| `m_loan_repayment_schedule` | — | `credited_interest` | All schedule columns used below exist in both. |
| `m_business_date`, `m_working_days`, `m_holiday`, `m_holiday_office`, `m_organisation_currency` | — | — | **Identical.** |

**Caveat 3 — enum values are not stored in the database.** Fineract keeps `*_enum`
meanings in Java source. Every enum label in this appendix is *inferred* by joining
observed values to charge/product names in `fineract_default` and cross-checking against
Fineract's `ChargeTimeType` / `SavingsAccountStatusType` / `LoanStatus`. Labels are marked
**inferred** where they were not directly corroborated by a name in the data. Do not put
an inferred label into user-facing output without confirming it against Fineract source.

---

### A.1 Savings charges

#### A.1.1 `m_savings_account_charge` — full column reference (`fineract_qicard_default`)

| Column | Type | Null | Meaning | Analyst relevance |
| --- | --- | --- | --- | --- |
| `id` | `bigint` | no | PK of the *account charge instance*. | Row key. **Not** the catalogue charge id. |
| `savings_account_id` | `bigint` | no | FK → `m_savings_account.id`. | Join to owner/office/currency. |
| `charge_id` | `bigint` | no | FK → `m_charge.id`, the catalogue definition. | Join for `charge_name`. |
| `is_penalty` | `boolean` | no | Denormalised from `m_charge.is_penalty`. | "Jenis charge": fee vs penalty. |
| `charge_time_enum` | `smallint` | no | When the charge is levied. See A.1.2. | Distinguishes one-off from recurring — **required** to interpret `amount` and `charge_due_date` correctly. |
| `charge_due_date` | `date` | **yes** | Due date. For one-off charges, the date it fell due. For recurring charges, the **next** occurrence. See A.1.3. | The analyst's "tanggal berapa due nya". |
| `fee_on_month` | `smallint` | yes | Recurrence: month of year. | Populated for monthly/annual timings. |
| `fee_on_day` | `smallint` | yes | Recurrence: day (of week for weekly, of month for monthly). | Recurrence description. |
| `fee_interval` | `smallint` | yes | Recurrence: every N periods. | Recurrence description. |
| `free_withdrawal_count` | `integer` | yes (dflt 0) | Withdrawal-fee free allowance consumed. | Withdrawal-fee scope only. |
| `charge_reset_date` | `date` | yes | Free-withdrawal counter reset date. | Withdrawal-fee scope only. |
| `charge_calculation_enum` | `smallint` | no | Flat vs percentage-of-X. | Explains why `amount` is what it is. |
| `calculation_percentage` | `numeric(19,6)` | yes | Percentage rate when percentage-based. | Optional detail. |
| `calculation_on_amount` | `numeric(19,6)` | yes | Base the percentage was applied to. | Optional detail. |
| `amount` | `numeric(19,6)` | no | **Per-occurrence** charge amount. **Not** a lifetime total for recurring charges. See A.1.3. | The candidate for "amount originally due" — with a caveat that invalidates the naive reading. |
| `amount_paid_derived` | `numeric(19,6)` | yes (NULL) | **Cumulative** amount paid across all occurrences. | "yg dibayar berapa". `COALESCE(...,0)`. |
| `amount_waived_derived` | `numeric(19,6)` | yes (NULL) | Cumulative amount waived. | "amount waived". `COALESCE(...,0)`. |
| `amount_writtenoff_derived` | `numeric(19,6)` | yes (NULL) | Cumulative amount written off. | Not in the analyst's field list, but needed to reconcile the arithmetic. |
| `amount_outstanding_derived` | `numeric(19,6)` | no (dflt 0) | **Currently unsettled amount.** The authoritative "still owed" figure. | "sisa berapa". Never NULL. |
| `is_paid_derived` | `boolean` | no (dflt f) | Charge fully settled. | Exclusion filter. |
| `waived` | `boolean` | no (dflt f) | Whole charge waived. | Exclusion filter. |
| `is_active` | `boolean` | no (dflt t) | Charge not inactivated. | Exclusion filter. |
| `inactivated_on_date` | `date` | yes | Set when `is_active` flips false. | Audit only. |
| `created_by` / `last_modified_by` | `bigint` | no | FK → `m_appuser.id`. | Audit only. |
| `created_on_utc` / `last_modified_on_utc` | `timestamptz` | no | Row audit timestamps. | **Wall clock**, not business date. Never use for domain filtering. |
| `external_id` | `varchar(255)` | yes | External system id. **qicard-only column.** | Listed in `never_return` on the capability — keep it that way. |
| `is_overdraft` / `is_deduct` / `is_ratio` | `boolean` | yes | qicard-only flags. | Undocumented; out of scope. |

There is **no** `due_for_collection_as_of_date` on `m_savings_account_charge`. That column
exists only on `m_loan_charge` (see A.2). Savings has exactly one due-date concept:
`charge_due_date`.

#### A.1.2 `charge_time_enum` — observed values (evidence: `fineract_default`, 204 rows)

| Value | Rows | `charge_due_date` set? | Recurrence cols set? | Inferred label | Corroborating charge names |
| --- | --- | --- | --- | --- | --- |
| 2 | 95 | always | no | `SPECIFIED_DUE_DATE` | `Account_Fee_…`, `Fixed_Deposit_charge` |
| 3 | 11 | never | no | `SAVINGS_ACTIVATION` (**inferred**) | `…_charge2` |
| 5 | 40 | never | no | `WITHDRAWAL_FEE` | `Withdrawal fee` |
| 7 | 22 | always | `fee_interval`, `fee_on_day`, `fee_on_month` | `MONTHLY_FEE` | `Monthly Fee - US`, `Account maintenance Fee` |
| 10 | 1 | yes | no | `OVERDRAFT_FEE` | `overdraft fee` |
| 11 | 35 | always | `fee_interval`, `fee_on_day` | `WEEKLY_FEE` | `weekly charge`, `Weekly Fee - US` |

`6` (`ANNUAL_FEE`) is defined in Fineract but was not observed. Values `1`, `4`, `8`, `9`
were not observed on savings.

`m_charge.charge_applies_to_enum` observed values in `fineract_default`: `1` (27 rows),
`2` (20), `3` (2), `4` (2) — inferred as Loan / Savings / Client / Shares. A savings
capability that joins `m_charge` should not need this filter (the account-charge table is
already savings-only), but a *catalogue* capability listing available charges must.

#### A.1.3 Two findings that invalidate assumptions in W-A2

**Finding 1 — `amount` is per-occurrence, so `amount_original` is a misnomer for recurring
charges.**

W-A2's table maps `amount_original` ← `sac.amount`. That is correct for one-off charges
and wrong for recurring ones. Evidence (`fineract_default`, row `id = 1`):
`amount = 2.000000` but `amount_paid_derived = 14.000000` — seven monthly occurrences of a
2.00 fee. The identity
`amount = paid + waived + writtenoff + outstanding` holds or breaks strictly by timing:

| `charge_time_enum` | Rows | Rows where the identity **breaks** |
| --- | --- | --- |
| 2 (specified due date) | 95 | 0 |
| 3 (activation) | 11 | 0 |
| 10 (overdraft) | 1 | 0 |
| 5 (withdrawal fee) | 40 | **34** |
| 7 (monthly) | 22 | **10** |
| 11 (weekly) | 35 | **26** |

70 of 204 rows break it. So there is no column that means "total ever levied on this
charge across all occurrences" — Fineract does not store it. The reconstructable total is
`paid + waived + writtenoff + outstanding`, which is exact for every row by construction.

**Recommendation:** do not ship a field called `amount_original` sourced from
`sac.amount`. Ship both, named honestly:
- `amount_due_current` ← `sac.amount` — "the amount of the current/next occurrence"
- `amount_levied_total` ← `COALESCE(paid,0) + COALESCE(waived,0) + COALESCE(writtenoff,0) + outstanding`

and expose `charge_timing` so the analyst can tell which reading applies.

**Finding 2 — the current due-date predicate silently drops outstanding recurring
charges.**

`queries/savings/pending_charges_clients.sql` filters
`(sac.charge_due_date IS NULL OR sac.charge_due_date <= $2::date)`. For recurring charges
`charge_due_date` is the *next* occurrence and therefore in the **future**, even while
`amount_outstanding_derived > 0` right now. Measured on `fineract_default` at
`BUSINESS_DATE = 2026-07-23`, all offices:

- with the predicate: **37 rows**
- without it: **74 rows**

The capability currently answers "who owes savings charges?" by hiding half the debt. This
is a wrong answer, not a narrower one.

**Recommendation:** drop the `charge_due_date <= as_of` predicate entirely. "Outstanding as
of the business date" is expressed by the flags plus
`amount_outstanding_derived > 0`; the due date belongs in the *output* (with
`days_overdue`), not in the *filter*. If a strictly-overdue variant is wanted later, make it
a separate capability or an explicit boolean parameter — do not bake it into the default.

#### A.1.4 Exact filter predicate for "unpaid and outstanding as of the business date"

```sql
sac.is_active         = true
AND sac.waived        = false
AND sac.is_paid_derived = false
AND sac.amount_outstanding_derived > 0
```

Flag-combination census (`fineract_default`, 204 rows) confirming these are the right four:

| `is_paid_derived` | `waived` | `is_active` | Rows | Σ outstanding |
| --- | --- | --- | --- | --- |
| f | f | t | 151 | 2 343.81 |
| f | t | t | 3 | 0.00 |
| t | f | t | 49 | 0.00 |
| t | f | f | 1 | 0.00 |

`amount_outstanding_derived > 0` alone would give the same 151 rows here, but the flags are
kept because they are the semantic statement and the data set is small.

The business date enters only as the reference point for `days_overdue`, never as a row
filter.

#### A.1.5 Join path to client, office and currency

```
m_savings_account_charge sac
  → m_savings_account sa      ON sa.id = sac.savings_account_id
  → m_client c                ON c.id  = sa.client_id       -- client-owned only, see A.3
  → m_office o                ON o.id  = c.office_id
  → m_charge ch               ON ch.id = sac.charge_id
  → m_organisation_currency   ON code  = sa.currency_code    -- LEFT, see A.5
```

Office scope binds on `c.office_id = ANY($1::bigint[])`.

#### A.1.6 Corrected SQL for `pending_charges_clients`

Executed successfully against `fineract_default` (returns rows; see sample below). Every
column referenced exists in `fineract_qicard_default`.

```sql
SELECT
    c.id                                       AS client_id,
    c.display_name                             AS client_display_name,
    c.office_id                                AS office_id,
    o.name                                     AS office_name,
    sa.id                                      AS savings_account_id,
    sac.id                                     AS savings_account_charge_id,
    ch.id                                      AS charge_definition_id,
    ch.name                                    AS charge_name,
    sac.is_penalty                             AS is_penalty,
    sac.charge_time_enum                       AS charge_timing_enum,
    sa.currency_code                           AS currency_code,
    cur.decimal_places                         AS currency_decimal_places,
    cur.display_symbol                         AS currency_display_symbol,
    sac.amount                                 AS amount_due_current,
    COALESCE(sac.amount_paid_derived, 0)       AS amount_paid,
    COALESCE(sac.amount_waived_derived, 0)     AS amount_waived,
    COALESCE(sac.amount_writtenoff_derived, 0) AS amount_written_off,
      COALESCE(sac.amount_paid_derived, 0)
    + COALESCE(sac.amount_waived_derived, 0)
    + COALESCE(sac.amount_writtenoff_derived, 0)
    + sac.amount_outstanding_derived           AS amount_levied_total,
    sac.amount_outstanding_derived             AS amount_outstanding,
    sac.charge_due_date                        AS due_date,
    CASE
        WHEN sac.charge_due_date IS NULL           THEN NULL
        WHEN $2::date > sac.charge_due_date        THEN $2::date - sac.charge_due_date
        ELSE 0
    END                                        AS days_overdue
FROM m_savings_account_charge sac
JOIN m_savings_account sa ON sa.id = sac.savings_account_id
JOIN m_client c           ON c.id  = sa.client_id
JOIN m_office o           ON o.id  = c.office_id
JOIN m_charge ch          ON ch.id = sac.charge_id
LEFT JOIN m_organisation_currency cur ON cur.code = sa.currency_code
WHERE sac.is_active = true
  AND sac.waived = false
  AND sac.is_paid_derived = false
  AND sac.amount_outstanding_derived > 0
  AND c.office_id = ANY($1::bigint[])
ORDER BY sac.charge_due_date NULLS FIRST, c.display_name, sac.id
LIMIT $3;
```

Sample output (`fineract_default`, `$2 = 2026-07-23`, all offices):

| client | office | charge_name | timing | ccy | due_current | paid | waived | outstanding | due_date | days_overdue |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Nour Hashem Ismail | Head Office | `…_charge2` | 3 | EUR | 50.00 | 0 | 0 | 50.00 | *(null)* | *(null)* |
| Elias Farah | Branch 004 | Monthly Fee - US | 7 | USD | 4.00 | 0 | 0 | 4.00 | 2026-01-21 | 183 |
| Elias Farah | Branch 004 | overdraft fee | 10 | USD | 5.00 | 0 | 0 | 5.00 | 2026-01-22 | 182 |
| marita louis | Branch 003 | Account_Maintenance_Fee_…_AED | 2 | AED | 100.00 | 0 | 0 | 100.00 | 2026-03-05 | 140 |

Three naming corrections against the shipped SQL:

1. The existing query emits `sac.id AS charge_id`. That is the *account charge instance* id,
   not the catalogue charge id, and an analyst reading "charge_id 6 = overdraft fee" will be
   misled. Renamed to `savings_account_charge_id`, with `charge_definition_id` added.
2. `savings_account_id` was absent; without it two charges on two different accounts of the
   same client are indistinguishable in the output.
3. `is_penalty` and `charge_timing_enum` were absent; "sebutkan jenis charge tersebut" needs
   more than a free-text name.

`days_overdue` is clamped at `0` and `NULL` when `charge_due_date IS NULL`. Rationale in the
Open questions resolution below.

#### A.1.7 Charge payment history

`m_savings_account_charge_paid_by` is the only per-payment record:

| Column | Type | Null | Meaning |
| --- | --- | --- | --- |
| `id` | `bigint` | no | PK. |
| `savings_account_transaction_id` | `bigint` | no | FK → `m_savings_account_transaction.id`. |
| `savings_account_charge_id` | `bigint` | no | FK → `m_savings_account_charge.id`. |
| `amount` | `numeric(19,6)` | no | Amount of this charge settled by that transaction. |

`amount_paid_derived` is the rollup of these rows. A payment-history capability joins
through to `m_savings_account_transaction` for `transaction_date` and **must** exclude
reversals defensively (`t.is_reversed = false`) — the paid-by row is not deleted when its
transaction is reversed. Observed: charge `id = 1` has 7 paid-by rows summing to 14.00,
matching its `amount_paid_derived`.

---

### A.2 Loan domain

#### A.2.1 Verdict: the catalog needs loan capabilities, and currently has zero

`knowledge/capabilities/` contains only `client/`, `organization/` and `savings/`. There is
no loan capability, no loan query, and no loan metric. Meanwhile `fineract_default` holds
116 loans, 87 of them active, **86 rows in `m_loan_arrears_aging`**, and 299 overdue
instalments. Every question the representative savings question asks — who owes, how much,
since when, how much paid, how much left — has a direct loan analogue with better source
data than savings has.

First five capabilities, in priority order:

1. **`loans_in_arrears_clients`** — clients with loans in arrears: principal / interest /
   fee / penalty overdue, total overdue, days in arrears. Source: `m_loan_arrears_aging`
   joined to `m_loan` → `m_client` → `m_office`. The single highest-value loan question and
   the closest analogue to the representative savings question.
2. **`loan_overdue_installments`** — instalment-level detail: instalment number, due date,
   days overdue, amount due, amount paid, amount remaining. Source:
   `m_loan_repayment_schedule`.
3. **`loan_outstanding_balances_clients`** — outstanding principal / interest / fees /
   penalties / total per active loan. Source: `m_loan` derived columns; no joins beyond
   client/office.
4. **`loan_unpaid_charges_clients`** — the exact loan mirror of
   `savings_pending_charges_clients`. Source: `m_loan_charge`.
5. **`loan_portfolio_summary_by_office`** — counts and totals by office and loan status, for
   the "berapa total pinjaman di kantor X" class of question.

#### A.2.2 `m_loan` — columns that matter (all verified present in both databases)

Ownership and identity: `id`, `account_no`, `external_id`, `client_id` (nullable),
`group_id` (nullable), `glim_id`, `product_id`, `loan_officer_id`, `currency_code`,
`currency_digits`, `currency_multiplesof`.

**`m_loan` has no `office_id`.** Its only office-bearing column is `loan_officer_id`
(→ `m_staff.office_id`, the officer's office, *not* the loan's). Office scope must go
through the client or the group — see A.3.

Status and dates: `loan_status_id`, `loan_type_enum`, `submittedon_date`, `approvedon_date`,
`disbursedon_date`, `expected_maturedon_date`, `maturedon_date`, `closedon_date`.

`loan_status_id` observed in `fineract_default`: `100` (14 rows), `200` (7), `300` (87),
`600` (8) — inferred as Submitted-and-pending-approval / Approved / **Active** / Closed
(obligations met). Active loans are `loan_status_id = 300`; use that, it is the only value
corroborated by the presence of repayment activity.

Derived balance rollups (all `numeric(19,6) NOT NULL DEFAULT 0`):

| Component | charged/disbursed | repaid | waived | written off | **outstanding** |
| --- | --- | --- | --- | --- | --- |
| Principal | `principal_disbursed_derived` | `principal_repaid_derived` | — | `principal_writtenoff_derived` | `principal_outstanding_derived` |
| Interest | `interest_charged_derived` | `interest_repaid_derived` | `interest_waived_derived` | `interest_writtenoff_derived` | `interest_outstanding_derived` |
| Fees | `fee_charges_charged_derived` | `fee_charges_repaid_derived` | `fee_charges_waived_derived` | `fee_charges_writtenoff_derived` | `fee_charges_outstanding_derived` |
| Penalties | `penalty_charges_charged_derived` | `penalty_charges_repaid_derived` | `penalty_charges_waived_derived` | `penalty_charges_writtenoff_derived` | `penalty_charges_outstanding_derived` |

Plus `total_expected_repayment_derived`, `total_repayment_derived`,
`total_expected_costofloan_derived`, `total_costofloan_derived`, `total_waived_derived`,
`total_writtenoff_derived`, `total_outstanding_derived`, and
`total_charges_due_at_disbursement_derived`. Also `arrearstolerance_amount` (grace amount
below which a loan is not treated as in arrears).

#### A.2.3 `m_loan_arrears_aging` — the arrears answer, prebuilt

| Column | Type | Null | Meaning |
| --- | --- | --- | --- |
| `loan_id` | `bigint` | no | **PK** — one row per loan in arrears; loans not in arrears have no row. |
| `principal_overdue_derived` | `numeric(19,6)` | no | Overdue principal. |
| `interest_overdue_derived` | `numeric(19,6)` | no | Overdue interest. |
| `fee_charges_overdue_derived` | `numeric(19,6)` | no | Overdue fees. |
| `penalty_charges_overdue_derived` | `numeric(19,6)` | no | Overdue penalties. |
| `total_overdue_derived` | `numeric(19,6)` | no | Sum of the four. |
| `overdue_since_date_derived` | `date` | yes | Oldest unmet due date. |

`days_in_arrears = business_date − overdue_since_date_derived` (same clamp convention as
`days_overdue` in A.1).

**Caveat:** this table is *batch-maintained* by a Fineract scheduled job, not a view. Its
freshness is bounded by when that job last ran, which the reporting service cannot see. For
"as of the business date" precision, `m_loan_repayment_schedule` (A.2.4) is authoritative;
`m_loan_arrears_aging` is the fast path. A capability built on it should say so in its
`description`.

#### A.2.4 `m_loan_repayment_schedule` — instalment-level truth

Key columns: `loan_id`, `installment`, `fromdate`, `duedate` (NOT NULL),
`completed_derived` (NOT NULL), `obligations_met_on_date`, `total_paid_late_derived`,
`total_paid_in_advance_derived`, `is_down_payment`, `is_re_aged`, `is_additional`.

Per component, `<c>_amount` / `<c>_completed_derived` / `<c>_writtenoff_derived` /
`<c>_waived_derived` for `principal` (no `waived`), `interest`, `fee_charges`,
`penalty_charges`, plus `accrual_*_derived` variants and `credits_amount`, `credited_fee`,
`credited_penalty`.

Remaining on an instalment:
`(principal_amount − principal_completed_derived − principal_writtenoff_derived)` and the
analogous expressions per component. **All of these are nullable** — `COALESCE(...,0)` is
mandatory.

Overdue instalment predicate:
`r.completed_derived = false AND r.duedate < $business_date AND l.loan_status_id = 300`
→ 299 rows in `fineract_default` at `2026-07-23`.

#### A.2.5 `m_loan_charge`

Same shape as `m_savings_account_charge` with three differences that matter:

- the due-date column is **`due_for_collection_as_of_date`**, not `charge_due_date`;
- there is a `charge_payment_mode_enum` and `charge_amount_or_percentage`;
- there is a `submitted_on_date`.

Same money columns (`amount`, `amount_paid_derived`, `amount_waived_derived`,
`amount_writtenoff_derived`, `amount_outstanding_derived`) and same flags
(`is_paid_derived`, `waived`, `is_active`, `is_penalty`). Related tables:
`m_loan_charge_paid_by`, `m_loan_installment_charge`, `m_loan_overdue_installment_charge`.

Evidence (`fineract_default`): 38 unpaid loan charges, **33 of which have
`due_for_collection_as_of_date IS NULL`** — these are `charge_time_enum = 1`
(disbursement-time) charges that have no due date at all. A loan-charge capability must not
filter on the due date for the same reason A.1.3 gives for savings.

#### A.2.6 Delinquency classification (native, better than hand-rolled buckets)

- `m_delinquency_range(id, classification, min_age_days, max_age_days)` — the bucket
  definitions, e.g. 1–30 / 31–60.
- `m_delinquency_bucket`, `m_delinquency_bucket_mappings` — buckets assembled from ranges;
  `m_product_loan.delinquency_bucket_id` attaches one to a product.
- `m_loan_delinquency_tag_history(loan_id, delinquency_range_id, addedon_date, liftedon_date)`
  — the loan's classification over time; the current tag is the row with
  `liftedon_date IS NULL`.
- `m_loan_installment_delinquency_tag` — same at instalment level.

If an analyst asks for aging buckets, read them from `m_delinquency_range` rather than
inventing 0–30/31–60/61–90 in SQL — the institution has already configured its own.

---

### A.3 Scoping and joins

#### A.3.1 Definitive office-scope join path per ownership type

| Ownership | Discriminator | Office join path | Bind |
| --- | --- | --- | --- |
| Client-owned savings | `sa.client_id IS NOT NULL` | `sa.client_id → m_client.office_id` | `c.office_id = ANY($n::bigint[])` |
| Group-owned savings | `sa.client_id IS NULL AND sa.group_id IS NOT NULL` | `sa.group_id → m_group.office_id` | `g.office_id = ANY($n::bigint[])` |
| GSIM savings | **both** `client_id` and `group_id` set | prefer `m_client.office_id` | `c.office_id = ANY($n::bigint[])` |
| Client-owned loan | `l.client_id IS NOT NULL` | `l.client_id → m_client.office_id` | `c.office_id = ANY($n::bigint[])` |
| Group-owned loan | `l.client_id IS NULL AND l.group_id IS NOT NULL` | `l.group_id → m_group.office_id` | `g.office_id = ANY($n::bigint[])` |
| Savings transaction | — | `m_savings_account_transaction.office_id` is a **direct NOT NULL column** | see caveat |
| Loan transaction | — | `m_loan_transaction.office_id` is a direct column | see caveat |

**Transaction-office caveat.** `m_savings_account_transaction.office_id` and
`m_loan_transaction.office_id` are the office where the transaction was *performed*, which
can differ from the office that *owns* the account. For "which office does this client
belong to" always route through `m_client` / `m_group`. Use the transaction office only when
the question is explicitly about where activity happened (teller/branch throughput).

**Office hierarchy.** `m_office.hierarchy` is a materialised path (`.`, `.2.`, `.9.` in
`fineract_default`) with `parent_id` as the tree edge. Subtree scope could be expressed as
`o.hierarchy LIKE :parent_hierarchy || '%'`, but the system's existing
`office_ids = ANY($n::bigint[])` binding is preferable and should stay: the authorized set
is resolved once, outside SQL, and cannot be widened by a malformed prefix.

`m_client.office_id` is `NOT NULL`; `m_group.office_id` is `NOT NULL`. `m_staff.office_id`
is nullable — do not scope through staff.

#### A.3.2 Answer to the group-owned-savings open question

**The `INNER JOIN m_client` does structurally exclude group-owned savings accounts, and in
the observed data it excludes nothing, because no group-owned savings account carries any
charge at all.**

Evidence (`fineract_default`, the only database with data):

- 198 savings accounts; 5 have `group_id IS NOT NULL`.
- Of those 5: ids 11, 12, 13 have **both** `client_id` and `group_id` set (GSIM — they still
  join to `m_client` fine and are *not* excluded); ids 16 and 158 have `client_id IS NULL`
  (pure group-owned) and **are** excluded by the inner join.
- Charge rows on any of those 5 accounts: **0**.
- Savings charges that are outstanding, active, unwaived, unpaid **and** attached to an
  account with `client_id IS NULL`: **0**.

So the inner join costs zero rows today, and the GSIM case — the one most likely to
surprise — is already handled correctly.

**Recommendation:** keep `savings_pending_charges_clients` client-scoped and inner-joined.
Its output contract is keyed on client identity; LEFT JOINing would emit rows with
`client_id = NULL` and `client_display_name = NULL` into a PII-gated client column, which is
worse than omitting them. If group-owned charges ever materialise, add a parallel
`savings_pending_charges_groups` capability with `m_group.display_name` /
`m_group.account_no` as the identity fields. Do not build it speculatively — there is no
data for it.

---

### A.4 Business date, working days, holidays

#### A.4.1 `m_business_date`

| Column | Type | Null | Notes |
| --- | --- | --- | --- |
| `id` | `bigint` | no | PK. |
| `type` | `varchar(100)` | no | **UNIQUE** — at most one row per type. |
| `date` | `date` | no | The date. |
| `created_by`, `last_modified_by` | `bigint` | no | FK → `m_appuser.id`. |
| `created_date`, `lastmodified_date` | `timestamp` | yes | Legacy audit. |
| `created_on_utc`, `last_modified_on_utc` | `timestamptz` | no | Audit. |
| `version` | `bigint` | no | Optimistic lock. |

Observed `type` values (`fineract_default`): `BUSINESS_DATE` = 2026-07-23,
`COB_DATE` = 2026-07-22. **`fineract_qicard_default` has zero rows** — see A.0.

`SELECT date FROM m_business_date WHERE type = 'BUSINESS_DATE'` returns at most one row by
the unique constraint, so the provider does not need `LIMIT 1` for correctness (harmless to
keep).

#### A.4.2 `m_working_days`

Single-row configuration table. Identical in both databases.

| Column | Type | Null | Meaning |
| --- | --- | --- | --- |
| `id` | `bigint` | no | PK. |
| `recurrence` | `varchar(100)` | yes | **RFC-5545 RRULE string.** |
| `repayment_rescheduling_enum` | `smallint` | yes | What to do when a repayment lands on a non-working day. |
| `extend_term_daily_repayments` | `boolean` | yes | Term-extension behaviour. |
| `extend_term_holiday_repayment` | `boolean` | no | Term-extension behaviour. |

Value in `fineract_qicard_default` (1 row):

```
FREQ=WEEKLY;INTERVAL=1;BYDAY=MO,TU,WE,TH,FR,SA,SU    repayment_rescheduling_enum = 2
```

i.e. **all seven days are working days in this tenant.**

**Consequence for a "hari kerja hari apa saja" capability:** there is no per-day boolean
column. The working-day set lives inside a semicolon-delimited RRULE string in the `BYDAY=`
segment. Approved SQL should return `recurrence` verbatim (plus the three flags); parsing
`BYDAY` into a day list belongs in the response-formatting layer, not in SQL. Do not attempt
`split_part`/`regexp` gymnastics in an approved query — it is unreadable and brittle for a
one-row table.

#### A.4.3 `m_holiday` and `m_holiday_office`

| `m_holiday` column | Type | Null | Meaning |
| --- | --- | --- | --- |
| `id` | `bigint` | no | PK. |
| `name` | `varchar(100)` | no | Holiday name. UNIQUE with `from_date`. |
| `from_date` | `date` | no | First day. |
| `to_date` | `date` | no | Last day (inclusive range, not a point). |
| `repayments_rescheduled_to` | `date` | yes | Where repayments move to. |
| `status_enum` | `integer` | no | Draft/active — **inferred**, default 100. |
| `processed` | `boolean` | no | Reschedule job has run. |
| `description` | `varchar(100)` | yes | Free text. |
| `rescheduling_type` | `integer` | no | Reschedule strategy, default 2. |

`m_holiday_office(holiday_id, office_id)` is the office association, PK on both columns.
**A holiday with no `m_holiday_office` row applies to no office** — office scope is an inner
join, not a filter.

Both tables have **0 rows** in `fineract_qicard_default` and in `fineract_default`.

A `organization_working_days_and_holidays` capability would therefore return: the single
`m_working_days` row, plus holidays where
`h.from_date <= $to AND h.to_date >= $from` joined through `m_holiday_office` on
`office_id = ANY($offices)`. It will legitimately return an empty holiday list today.

---

### A.5 Currency and formatting

`m_organisation_currency` — the currencies **enabled for this tenant**:

| Column | Type | Null | Use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | PK. |
| `code` | `varchar(3)` | no | ISO code. **No unique constraint** — see below. |
| `decimal_places` | `smallint` | no | Rounding for display. |
| `currency_multiplesof` | `smallint` | yes | Rounding multiple (e.g. round to nearest 50). |
| `name` | `varchar(50)` | no | Display name. |
| `display_symbol` | `varchar(10)` | yes | Symbol; **nullable** — AED has none in `fineract_default`. |
| `internationalized_name_code` | `varchar(50)` | no | i18n key. |

`m_currency` is the full ISO reference list with the identical column set, and **does** have
`UNIQUE(code)`.

`fineract_qicard_default` has exactly one enabled currency: `USD`, 2 decimal places,
symbol `$`, `currency_multiplesof` NULL. `fineract_default` has USD, EUR, AED and others.

**What the output layer needs:**

1. **`decimal_places`** — every money column in Fineract is `numeric(19,6)`. Rendering six
   decimals is wrong for every real currency. Round to `decimal_places`.
2. **`display_symbol`**, which may be NULL — fall back to `code`, never to a hardcoded `$`.
3. **`currency_multiplesof`**, which may be NULL — only relevant for currencies without
   subunits.
4. **Prefer the account's own snapshot for precision.** `m_savings_account`,
   `m_loan`, `m_savings_product` and `m_product_loan` each carry `currency_code`,
   `currency_digits` and `currency_multiplesof` frozen at creation.
   `m_savings_account.currency_digits` is `NOT NULL` and is the precision the account was
   actually booked at; `m_organisation_currency.decimal_places` is the current tenant
   configuration and can drift from it. Use the account column for arithmetic/rounding and
   `m_organisation_currency` only for `display_symbol` / `name`.
5. **`m_organisation_currency.code` has no unique constraint.** A plain
   `LEFT JOIN m_organisation_currency ON code = sa.currency_code` can in principle fan out
   and duplicate rows. It does not today (one row per code observed), but a defensive
   capability should use `DISTINCT ON (code)` in a subselect or a `LEFT JOIN LATERAL (… LIMIT 1)`.
   Flagged rather than fixed — the corrected SQL in A.1.6 uses the plain join and would need
   this hardening before it becomes the approved query.
6. **Multi-currency output is real.** In `fineract_default` a single office holds accounts in
   USD, EUR and AED. Money columns must never be summed across `currency_code`, and any
   totals row must be per-currency. `savings_pending_charges_clients` returns
   `currency_code` per row and does not aggregate — correct as-is.

---

### A.6 Newly discovered tables worth capability coverage

Each with the analyst question it would serve. None of these appear in
`docs/reporting-data/*.md`.

| Table(s) | Analyst question it answers |
| --- | --- |
| `m_client_charge`, `m_client_charge_paid_by` | "Which clients owe fees that are not attached to any account?" — same column shape as `m_savings_account_charge` (with `charge_due_date`). Directly relevant: the analyst's "hutang" is not scoped to savings. 0 rows in both databases. |
| `m_loan_arrears_aging` | "Which clients are in arrears, for how much, and since when?" (A.2.3) |
| `m_delinquency_range`, `m_delinquency_bucket`, `m_delinquency_bucket_mappings`, `m_loan_delinquency_tag_history`, `m_loan_installment_delinquency_tag` | "Show the loan book by the institution's own delinquency classification." (A.2.6) |
| `m_account_transfer_standing_instructions` (+ `_history`, `m_account_transfer_details`, `m_account_transfer_transaction`) | "Which standing instructions are active, when did each last run, and which failed?" Columns include `status`, `valid_from`, `valid_till`, `recurrence_*`, `last_run_date`. |
| `m_tellers`, `m_cashiers`, `m_cashier_transactions` | "What is the cash position per teller / per branch today?" `m_tellers.office_id` is NOT NULL, so office scoping is direct. |
| `m_note` | "What notes are recorded against this client / loan / savings account?" Polymorphic: `client_id`, `group_id`, `loan_id`, `loan_transaction_id`, `savings_account_id`, `savings_account_transaction_id`, `share_account_id` — exactly one is set, plus `note_type_enum`. |
| `m_savings_product.is_dormancy_tracking_active` / `days_to_inactive` / `days_to_dormancy` / `days_to_escheat` + `m_savings_account.sub_status_enum` | "Which savings accounts are dormant or about to become dormant?" 11 accounts have `sub_status_enum = 100` in `fineract_default` (**inferred** = dormant/inactive; unconfirmed). There is no dedicated dormancy table — it is a product config plus an account sub-status. |
| `m_calendar`, `m_calendar_instance`, `m_calendar_history` | "What is the meeting schedule for this group/centre?" |
| `m_guarantor`, `m_guarantor_funding_details`, `m_guarantor_transaction` | "Who guarantees this loan and how much is held?" |
| `m_collateral_management`, `m_client_collateral_management`, `m_loan_collateral_management`, `m_loan_collateral` | "What collateral secures the portfolio?" |
| `m_creditreport` | "Do we hold a credit bureau report for this client?" |
| `request_audit_table`, `m_report_mailing_job*`, `stretchy_report`, `stretchy_report_parameter` | Operational/audit surface. Explicitly a **non-goal** of this issue; noted so a future reader does not rediscover it. |

**Interest posting has no dedicated table.** Interest postings are rows in
`m_savings_account_transaction` distinguished by `transaction_type_enum`, supported by
`m_savings_account.interest_posted_till_date`, `last_interest_calculation_date`,
`total_interest_earned_derived` and `total_interest_posted_derived`. Any "how much interest
was posted" capability must go through the transaction type enum, whose mapping is **not
yet verified** — see the new open question below.

---

### A.7 Documentation drift

Every item below is a concrete disagreement between `docs/reporting-data/*.md` and the live
`fineract_qicard_default` schema.

1. **Wrong verification target, systemically.** Every file in `docs/reporting-data/` states
   it was "Verified from … local database `information_schema.columns` on
   `fineract_default`" (`savings-charges-fees.md:41`, `loans.md:31`, and the same line in the
   siblings). The deployment target is `fineract_qicard_default`, which is a **different
   Fineract build**, not another tenant. Every one of these documents needs re-verification.
   This is the root cause of items 2–4.

2. **`settlement_priority` is documented but does not exist.**
   `savings-charges-fees.md:85` (`m_savings_account_charge.settlement_priority`),
   `:162` and `:210` (`m_charge.settlement_priority`), `:259`, `:272`, and
   `savings-core.md:283` all describe a `settlement_priority` column. **That column does not
   exist in `fineract_qicard_default`** (it exists only in `fineract_default`). Any SQL
   written from these docs would fail at prepare time.

3. **`value_date` and `hold_status` are documented but do not exist.**
   `savings-transactions.md:89` and `:261` describe
   `m_savings_account_transaction.value_date`; `:116` and `:258` describe `hold_status`.
   Neither exists in `fineract_qicard_default`. `transaction_date_time`,
   `running_balance_on_reversal`, `posted_by_transaction_id`, `parent_hold_transaction_id`
   and `hold_type` are in the same category — present in `fineract_default` only.

4. **qicard-only columns are undocumented.** `m_savings_account_charge.external_id`,
   `.is_overdraft`, `.is_deduct`, `.is_ratio`; `m_charge.is_overdraft`, `.is_deduct`,
   `.is_ratio`, `.percentage_amount`; `m_office.branch_code`;
   `m_savings_account.external_client_id`, `.internal_saving_account`, `.is_tsys`,
   `.is_used_as_escrow`, `.is_virtual_account`; `m_client.escrow_account_id`,
   `.escrow_product_id`, `.payout_method`;
   `m_savings_account_transaction.super_qi_transaction_id`, `.super_qi_dateTime`,
   `.gl_map_charge_id`. `external_id` in particular is already named in the capability's
   `pii.never_return` list, so it is load-bearing that it exists.

5. **`charge_time_enum` is still "Needs enum mapping".** `savings-charges-fees.md:75`
   and `:189` both defer it, and `:228` lists it as blocked on that mapping. A.1.2 supplies
   an inferred mapping. Until it is confirmed against Fineract source, output should expose
   the raw enum value alongside any label.

6. **`m_loan_arrears_aging` is absent from `loans.md` entirely** (`grep` over the whole
   `docs/` tree returns nothing). It is the single most useful loan table for the analyst
   questions this issue is about, and it holds 86 rows in `fineract_default`.

7. **No document covers `m_business_date`, `m_working_days`, `m_holiday`, or
   `m_holiday_office`** — yet "today means the tenant business date" is a product decision
   recorded in this very issue, and "hari kerja hari apa saja" is a named analyst question.

8. **No document covers `m_organisation_currency` or `m_currency`**, despite every money
   output needing `decimal_places` to render correctly (A.5).

9. **No document covers `m_client_charge`**, which is the non-account "hutang" surface.

10. **`savings-core.md` treats group ownership as conditional.** Lines 59–60, 68, 78, 324 and
    368 gate `group_id` / `gsim_id` behind "if group scope is enabled" / "out of MVP unless
    GSIM is approved". A.3 shows GSIM accounts (both `client_id` and `group_id` set) already
    exist in the data and are silently included by the current client join. The docs should
    state that GSIM accounts are in scope via their client, and that pure group-owned
    accounts are out of scope by design — not leave it as an open toggle.


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

- **Days-overdue sign convention.** *Resolved (Appendix A.1.6): clamp at zero, and emit
  `NULL` when `charge_due_date IS NULL`.* A signed value is actively misleading here:
  for recurring charges `charge_due_date` is the **next** occurrence and is normally in
  the future, so a signed `days_overdue` would report "−28 days" on a charge that is
  genuinely outstanding today. The not-yet-due case is carried by `due_date` plus the
  new `charge_timing_enum` output field, not by the sign. Apply the same clamp to
  `days_in_arrears` on the loan side (`business_date − overdue_since_date_derived`,
  Appendix A.2.3) so every aging capability agrees.
- **Group-owned savings accounts.** *Resolved (Appendix A.3.2): keep the inner join,
  keep the capability client-scoped.* Measured on `fineract_default` (the only populated
  database): 5 of 198 savings accounts are group-linked; 3 of those are GSIM with **both**
  `client_id` and `group_id` set and are therefore **not** excluded by the join; only 2 are
  pure group-owned; and **zero** group-linked accounts carry any charge row at all. The
  inner join costs no rows today. LEFT JOINing would emit `client_id = NULL` /
  `client_display_name = NULL` into PII-gated client columns, which is worse than omitting
  them. If group-owned charges ever appear, add a parallel `savings_pending_charges_groups`
  capability keyed on `m_group.display_name`. Do not build it speculatively.
- **Composite capabilities.** If several analyst questions each need the same three
  sub-views stitched together, is the answer one wide capability or a new composite
  execution mode? Defer until the W-A1 inventory shows how often it actually happens —
  do not build the abstraction speculatively.
- **`Unbounded` representation.** Currently `i64::MAX` bound into `LIMIT $n`
  (commit `27669e1`, carries a `ponytail:` note). Revisit only if a query plan
  regression appears; the alternative is a LIMIT-less SQL variant per capability,
  which doubles the approved-SQL surface.
- **Loan-domain parity.** *Resolved (Appendix A.2): there are **zero** loan capabilities,
  zero loan queries and zero loan metrics — `knowledge/capabilities/` holds only `client/`,
  `organization/` and `savings/`.* Meanwhile `fineract_default` carries 116 loans (87
  active), 86 rows in `m_loan_arrears_aging` and 299 overdue instalments. The gap is real
  and large. Recommended first five, in order: `loans_in_arrears_clients`,
  `loan_overdue_installments`, `loan_outstanding_balances_clients`,
  `loan_unpaid_charges_clients`, `loan_portfolio_summary_by_office` (rationale and source
  tables in A.2.1). **Scope decision still needed:** these are five full capabilities —
  fold them into W-A3, or split them into a follow-up issue so W-A3 stays savings-only?
  Appendix A.2 is the reference either way.

### New open questions raised by Appendix A

- **Which database is the real target?** `fineract_qicard_default` has 0 clients, 0 savings
  accounts, 0 charges, 0 loans and — critically — **0 rows in `m_business_date`**
  (Appendix A.0). If that is the production shape, `FineractBusinessDateProvider` falls back
  to wall clock on every request and `business_date.fallback_used` fires continuously. W-B
  must decide whether that is an alert-worthy condition or the expected dev-time state.
- **What does "amount originally due" mean for a recurring charge?** W-A2 maps
  `amount_original` ← `sac.amount`, but `amount` is the **per-occurrence** amount and
  `amount_paid_derived` is cumulative; the identity
  `amount = paid + waived + writtenoff + outstanding` breaks on 70 of 204 rows, entirely on
  recurring timings (Appendix A.1.3). Ship `amount_due_current` +
  `amount_levied_total` (reconstructed) instead of a single misleading `amount_original`?
  Blocks the W-A2 field naming.
- **Should the `charge_due_date <= as_of` predicate be dropped outright?** It halves the
  result set (37 rows vs 74 on `fineract_default` at the business date) by hiding
  outstanding *recurring* charges whose next occurrence is in the future
  (Appendix A.1.3). Appendix A recommends dropping it and moving the due date into output
  only. Confirm before W-A2 rewrites the SQL.
- **`m_organisation_currency.code` has no unique constraint.** The corrected SQL in
  A.1.6 uses a plain `LEFT JOIN ... ON code = sa.currency_code`, which could fan out. Harden
  to `DISTINCT ON` / `LEFT JOIN LATERAL (… LIMIT 1)` before it becomes an approved query?
- **Enum labels remain unmapped.** `charge_time_enum`, `m_savings_account.sub_status_enum`
  (is `100` dormant?), `m_savings_account_transaction.transaction_type_enum` (which value is
  an interest posting?) and `loan_status_id` are all inferred in Appendix A, not confirmed
  against Fineract source. Any user-facing label needs confirmation; until then output the
  raw value alongside.
- **`docs/reporting-data/*.md` re-verification.** All of it was verified against
  `fineract_default` and documents at least three columns that do not exist in
  `fineract_qicard_default` (`settlement_priority`, `value_date`, `hold_status` —
  Appendix A.7). Is fixing those docs in scope for this issue, or a separate one?

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
