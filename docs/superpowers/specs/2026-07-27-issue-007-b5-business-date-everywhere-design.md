# Bundle 5 (W-B) — Business date correctness end to end — Design

**Issue:** 007 §W-B (lines 255–289), context §A.4 (lines 2045–2119).
**Roadmap row:** program roadmap bundle 5, "W-B business date", type code, no dependency, runs parallel with bundle 4.
**Design reference:** `docs/superpowers/specs/2026-07-24-llm-extraction-gateway-design.md` §5.4 (relative-expression → business-date mapping table).

## Goal

Every relative temporal expression the deterministic extractor understands
(`hari ini`/`today`, `kemarin`/`yesterday`, `minggu ini`/`this week`,
`minggu lalu`/`last week`, `bulan ini`/`this month`, `bulan lalu`/`last month`,
`tahun ini`/`this year`, `tahun lalu`/`last year`, `N hari terakhir`/`last N days`,
plus `kuartal ini`/`this quarter`, `kuartal lalu`/`last quarter`) must resolve
against the tenant **business date** (`CanonicalRuntimeContext.business_today`),
not the wall clock. When the reporting date differs from the calendar date, the
answer says so. Audit timestamps must still be wall-clock (`Utc::now()`), proven
by a test so a future refactor cannot silently swap them.

## Background / Current state (verified 2026-07-27)

Read against the working tree. Trust this section over the (2026-07-24) issue text.

### The two date paths, and only one is correct today

1. **Default-expression path (correct).**
   `crates/chat/src/knowledge/catalog/parameter_policy.rs` resolves YAML
   `default: business_today` (and `business_today - Nd/Nm/Ny`,
   `start_of_month(...)`, `end_of_month(...)`) against
   `EvaluationContext.business_today`. This is the value a query parameter gets
   when the user does **not** state a date. Verified: `ResolvedValue::Date(ctx.business_today)`
   at `parameter_policy.rs:103`.

2. **Deterministic-extraction path (BROKEN — the target of this bundle).**
   `crates/chat/src/assistant/understanding/extraction/temporal.rs:20` computes
   the anchor as:
   ```rust
   let jakarta = FixedOffset::east_opt(7 * 3600).expect("valid Jakarta offset");
   let today = reference_instant.with_timezone(&jakarta).date_naive();
   ```
   `reference_instant` is a wall-clock `DateTime<Utc>` — it flows from
   `job_created_at` / `canonical_turn.reference_instant`
   (`job/service/mod.rs:189`, `job/service/run.rs:76`). So when a user *types*
   "kemarin" or "bulan lalu", the resolved `from`/`to` derive from the **wall
   clock**, contradicting path 1. The `reference_instant` carries **wall clock,
   not business date** — this is the audit finding B1 asks for.

### How the two dates reach the runtime

`CanonicalRuntimeContext` (`assistant/execution/runtime/mod.rs:82,86,87`) already
carries **both**:
- `reference_instant: DateTime<Utc>` (wall clock),
- `business_today: NaiveDate` (from `BusinessDateProvider.today()`, set in `job/service/run.rs:80`),
- `business_date_source: BusinessDateSource` (`Fineract` | `WallClockFallback`, `run.rs:81`).

`business_today` is populated by `FineractBusinessDateProvider` reading
`SELECT date FROM m_business_date WHERE type = 'BUSINESS_DATE'`, wrapped by
`AuditingBusinessDateProvider` which enqueues `business_date.fallback_used` when it
falls back. All of that is already wired (`api/mod.rs:77`). This bundle does **not**
touch the provider chain except to add the audit-timestamp guard test.

### The single production call site that must change

`assistant/execution/runtime/extraction.rs:111` (`extract_for_context`):
```rust
.map(|context| extract_message_facts_at(message, context.reference_instant, 366))
```
`context.business_today` is in scope here but is **not** passed to the extractor.
This is the one production path where a typed relative phrase is resolved. Fixing
`resolve_temporal`'s anchor plus threading `business_today` through
`extract_message_facts_at` closes the gap.

### Response note surface exists

`AssistantResponse.warnings: Vec<ResponseWarning { code, message }>`
(`assistant/presentation/response.rs:24,94`) is already client-renderable and is
already used for `pii_hidden` (`presentation/builder.rs:44`). The successful table
response is built in exactly one place —
`ResponseBuilder::from_tool_result` called at
`assistant/execution/runtime/execution.rs:238`, where `canonical:
Option<&CanonicalRuntimeContext>` is in scope. This is the single injection site
for the reporting-date note.

### Audit timestamp is already wall clock

`BusinessDate.resolved_at` is set to `Utc::now()` in every provider
(`business_date.rs:64,96,101`) and is used as `occurred_at` for the
`business_date.fallback_used` event (`business_date.rs:153`). No production code
uses `business_today` for an audit timestamp. This bundle adds a regression test
that pins the invariant; no production change is required for it.

### Drift vs the issue text

- **B1 "establish whether the reference instant carries business date or wall
  clock":** it carries **wall clock**. Confirmed above; issue leaves it open.
- **W-A2 / charge_due_date / F7:** unrelated to this bundle and already shipped
  (per roadmap); not touched here.
- **B4 references `crates/chat/tests/business_date_provider.rs`:** that file
  **does not exist** in the tree. The provider unit tests live inside
  `business_date.rs`; the outbox → `GET /management/audit` round trip is covered by
  `crates/chat/tests/management_audit.rs`. See "Out of scope".
- Existing test `temporal_uses_jakarta_date_and_exact_period_boundaries`
  (`extraction/tests.rs:83`) asserts the *old* wall-clock-via-Jakarta anchor. It
  must be updated to pass an explicit `business_today` (behavioural intent
  preserved: same expected dates, but now sourced from `business_today`).

## Constraints (invariants preserved)

- Approved-SQL only; office scope bound in SQL — untouched (this bundle changes only
  date *derivation* feeding bound parameters, never SQL text or filtering).
- No SQL in handlers/services/assistant orchestration — no new SQL anywhere.
- "today" = Fineract tenant business date; wall clock only for audit — this bundle
  *enforces* that invariant on the extraction path.
- Sanitized errors — the reporting-date note is fixed English copy, no SQL/prompt/stack.
- 3 crates, no new deps, no migrations, no new knowledge/queries YAML surface.
- English-only product copy.

## Design

### D1 — Re-anchor `resolve_temporal` on `business_today`

Add a `business_today: NaiveDate` parameter and use it as the relative anchor.
`reference_instant` is retained solely for `TemporalProvenance` (audit) and the
`timezone: "Asia/Jakarta"` label stays. Explicit ISO ranges (`from DATE to DATE`)
do not use the anchor and are unaffected.

Mapping (matches design spec §5.4, `relative_range` already implements the math):

| Rule | from | to |
| --- | --- | --- |
| `today` | `business_today` | `business_today` |
| `yesterday` | `business_today - 1d` | `business_today - 1d` |
| `this_week` | Monday of `business_today`'s week | `+6d` |
| `last_week` | Monday of previous week | `+6d` |
| `this_month` | `start_of_month(business_today)` | `end_of_month(business_today)` |
| `last_month` | start of previous month | end of previous month |
| `this_quarter` | first day of `business_today`'s quarter | last day of quarter |
| `last_quarter` | first day of previous quarter | last day of previous quarter |
| `this_year` | Jan 1 of business year | Dec 31 |
| `last_year` | Jan 1 of previous year | Dec 31 |
| `last_n_days_inclusive` | `business_today - (n-1)d` | `business_today` |

The `FixedOffset`/Jakarta conversion line is deleted (the anchor is now a
`NaiveDate` directly); `FixedOffset` is dropped from the `chrono` import.

### D2 — Thread `business_today` through the extractor

- `extract_message_facts_at(message, reference_instant, business_today, max_range_days)`.
- `extract_message_facts(message)` (no-canonical convenience) uses
  `let now = Utc::now(); extract_message_facts_at(message, now, now.date_naive(), 366)` —
  wall clock for both, since no tenant context exists on that path.
- `extract_for_context` passes `context.business_today` at
  `runtime/extraction.rs:111`.

### D3 — Reporting-date note when business date ≠ wall clock

New pure helper `ResponseBuilder::reporting_date_note(business_today, source,
wall_today) -> Option<ResponseWarning>`:
- Returns `Some` only when `source == BusinessDateSource::Fineract` **and**
  `business_today != wall_today`.
- `code: "reporting_date"`, English message naming both dates.

Injected once at `execution.rs:238` after `from_tool_result`, using the
`canonical` context in scope. `wall_today` is derived from
`canonical.reference_instant` converted to Asia/Jakarta (see open decision 1).

### D4 — Audit-timestamp regression guard

Unit test in `business_date.rs`: a `StaticBusinessDateProvider` configured with a
stale business date (e.g. `2000-01-01`) must still return `resolved_at` within a
`[before, after]` `Utc::now()` window, and `resolved_at.date_naive() != date`.
This pins "audit timestamps use `Utc::now()`, never `business_today`".

## Testing strategy

- **Per-expression, per-language (RED-first):** table-driven test in
  `extraction/tests.rs` with one case per phrase per language (11 rules × 2
  languages = 22 cases), each asserting `from`/`to` derived from a
  `business_today` that is **deliberately different** from the wall-clock
  `reference_instant`, so the test fails under today's wall-clock anchor.
  Fixed fixture: `reference_instant = 2026-07-25T02:00:00Z`,
  `business_today = 2026-07-23` (Thursday). Expected boundaries are enumerated in
  the plan.
- **Provenance keeps wall clock:** one case asserts
  `temporal_provenance.reference_instant == reference_instant` while `from`/`to`
  track `business_today`.
- **Response note:** unit test on `reporting_date_note` — `Some` when Fineract &&
  differing, `None` when equal, `None` when `WallClockFallback`.
- **Audit timestamp:** D4 unit test above.
- **Update** `temporal_uses_jakarta_date_and_exact_period_boundaries` and all
  existing `extract_message_facts_at(...)` call sites to the new 4-arg signature.
- Commands: `cargo test -p chat temporal`, `cargo test -p chat business_date`,
  `cargo test -p chat reporting_date`, then `cargo check` / `cargo test -p chat`.

## Out of scope

- The provider chain (`FineractBusinessDateProvider` / `AuditingBusinessDateProvider`)
  — already correct; only guarded by a new test.
- **B4 full round-trip** (forced fallback → `management_audit_outbox` → `GET
  /management/audit`): requires a live app+Fineract DB and belongs with the
  DB-backed suites. The `business_date.fallback_used` enqueue already exists and is
  exercised by `management_audit.rs`. This bundle proves the audit-timestamp
  invariant at unit level (D4) and does not add a new DB integration test. (Open
  decision 2.)
- LLM gateway resolver/decider (bundle 12, W-C) — the deterministic path is the
  only relative-date resolver today; nothing else to re-point.
- Currency/money and presentation rework (bundles 4, 9) — untouched.

## Open decisions for spec review

1. **Wall-clock basis for the note.** Issue text says compare against
   `Utc::now().date_naive()` (UTC date). Recommendation: compare against
   `canonical.reference_instant` in **Asia/Jakarta**, so the note is deterministic
   (fixed by the job's own reference instant, not a fresh `now()`) and matches the
   tenant zone the extractor labels. Confirm UTC vs Jakarta.
2. **B4 depth.** Confirm that unit-level audit-timestamp proof (D4) plus the
   existing `management_audit.rs` coverage is sufficient for this bundle, deferring
   any new forced-fallback DB integration test. If a dedicated
   `business_date_provider.rs` integration test is wanted, it is additive and
   DB-gated.
