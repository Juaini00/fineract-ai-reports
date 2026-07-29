# Issue 007 — Bundle 4: Savings Catalog Completeness, Defaults & Currency (Design)

**Bundle:** B4 (W-A2 verify-close, W-A4 defaults, W-J decisions 1/4/5).
**Program:** `docs/superpowers/plans/2026-07-27-issue-007-program-roadmap.md` (row 4).
**Issue:** `docs/issues/active/007-analyst-grade-knowledge-and-request-mapping.md`
(W-A2 §202–223, W-A4 §230–243, W-J §728–791, A.1.3–A.1.7 §1671–1849, A.5 §2120–2166).

## Goal

Bring `savings_pending_charges_clients` to analyst grade and give it correct
currency semantics **in the data payload**, plus perform the per-capability
temporal/limit default review (E4). Concretely:

1. Add `amount_levied_total` (A.1.3 Finding 1) — the only reconstructable
   "total ever levied" figure Fineract can produce.
2. Carry currency **precision and symbol** in the row payload, sourced correctly
   (`sa.currency_digits` for precision, `m_organisation_currency.display_symbol`
   for symbol) and joined **fan-out-safe** (W-J decisions 1, 4, 5).
3. Enforce the fan-out-safe currency join in the catalog validator so the next
   author cannot reintroduce the latent duplication bug.
4. Review every capability's date and `limit` defaults per E4 and record the
   decision, replacing the uniform `business_today` migration artifact where it
   produces a wrong answer.

**Explicitly NOT in this bundle:** money *rendering* (rounding to
`currency_digits`, `multi_currency` warning, per-currency subtotal cards). Those
are W-J decisions 2 and 3 and belong to Bundle 9 (`W-G + W-J rest`). B4 only
makes the raw data *available and correct*; the presentation layer is untouched.

## Background — current state (verified 2026-07-27)

Read against the working tree, not the issue text. The issue (dated 2026-07-24)
is stale on several points.

### Already shipped (issue evidence E3 / W-A2 table is out of date)

`queries/savings/pending_charges_clients.sql` already exports, in order:
`client_id, client_display_name, office_id, office_name, savings_account_id,
savings_account_charge_id, charge_definition_id, charge_name, is_penalty,
charge_timing_enum, currency_code, amount_due_current, amount_paid,
amount_waived, amount_written_off, amount_outstanding, due_date, days_overdue`.

- The A.1.3 **Finding 2** hotfix already shipped: the `charge_due_date <= $2`
  filter is **gone**. The `WHERE` is exactly the A.1.4 predicate
  (`waived = false AND is_paid_derived = false AND is_active = true AND
  amount_outstanding_derived > 0`) plus `c.office_id = ANY($1::bigint[])`.
- `days_overdue` already uses the resolved clamp convention: `NULL` when
  `charge_due_date IS NULL`, `$2::date - charge_due_date` when past due, else `0`.
- The W-A2 rename corrections already landed: `savings_account_charge_id`,
  `charge_definition_id`, `savings_account_id`, `is_penalty`, `charge_timing_enum`
  are all present. The naming footgun `amount_original` was **not** shipped
  (correct — see A.1.3 Finding 1).
- `ORDER BY` differs from the A.1.6 draft: the shipped query orders by
  `amount_outstanding_derived DESC, charge_due_date NULLS LAST, sac.id`. This is
  the analyst-sensible "biggest debt first" ordering and is **kept as-is** — the
  A.1.6 draft ordering was illustrative.

### Remaining gaps (the real B4 work)

1. **`amount_levied_total` is absent.** Fineract stores no lifetime-levied total
   for recurring charges; the only exact reconstruction is
   `paid + waived + writtenoff + outstanding` (A.1.3 Finding 1). Not shipped.
2. **No currency precision/symbol in the payload.** The query selects
   `sa.currency_code` only. There is no `m_organisation_currency` join, no
   `currency_digits`, no `display_symbol`. A downstream renderer therefore has
   no way to render at the correct precision or with the correct symbol.
3. **No validator guard against the currency fan-out.** `m_organisation_currency.code`
   has no unique constraint (A.5 point 5); a plain equality join can duplicate
   rows. The validator does not forbid it.
4. **Temporal/limit defaults were migrated uniformly (E4).**
   `scripts/migrate_capability_policies.py` set `default: business_today,
   fill_when_missing: true` on **every** date parameter across all 30
   capabilities. For point-in-time capabilities that is right; for rolling-window
   capabilities it collapses the window to a single day — a wrong answer, not a
   clarification (E4 §124–130). Every `limit` is `default: unbounded` with a
   `hard_cap`, including genuine top-N rankings where "unbounded" means "not
   actually a ranking".

### Machinery facts that constrain the design

- **`parameter_policies` are live.** `catalog/loader.rs` (`parse_parameters_block`,
  `read_default_expr`) lifts each capability's `parameters:` block into
  `CapabilityKnowledge::parameter_policies`, and `validate_capability_parameter_contract`
  consumes them. The default grammar is the fixed allowlist in
  `catalog/parameter_policy.rs::DefaultExpr::parse` — it already supports
  `business_today`, `business_today - Nm`, `start_of_month(business_today)`,
  `unbounded`, and integer literals. **No new grammar is needed.** So A4 edits
  are real config changes, not documentation.
- **`checks:` YAML blocks are declarative only.** The validator does not
  interpret arbitrary `check.rule` strings; enforcement is hard-coded Rust in
  `validator.rs::validate_sql_safety` (office `ANY`, `from/to BETWEEN`, `LIMIT`).
  So W-J decision 4 ("add a catalog check") means **both** a declarative
  `checks:` entry in the query YAML (for the human reader) **and** a real
  hard-coded rule in `validate_sql_safety` (for enforcement).
- **The output-column contract is enforced by `validate_runtime`.** It prepares
  each Fineract query against the live pool and asserts the prepared column names
  equal `query.output_fields` in order. Every new SQL column must be added to the
  query YAML `output_fields` in the **exact same position**, or `validate_runtime`
  fails. This is the DB-backed regression guard for the row shape.
- **`management/knowledge.rs` detail projection is an allowlist** (name +
  sensitivity only): derived columns like `amount_levied_total` are safe to add;
  the SQL expression never leaves the server (issue §843–854).

## Constraints (invariants — never violated)

- Approved-SQL only; no AI-generated SQL. Office scope stays bound **inside** SQL
  via `c.office_id = ANY($1::bigint[])` — never a Rust post-filter.
- SQL lives only in `queries/**` (executed by the repository layer). No SQL added
  to handlers, services, or `assistant/**`.
- PII gating stays field-level: `client_id` / `client_display_name` remain
  `pii` / `pii_conditional`; every new column here (`amount_levied_total`,
  `currency_digits`, `currency_display_symbol`) is `public_business` (A.5.7
  decision — none identifies a person).
- "Today" = Fineract tenant business date. `as_of_date` stays a reference point
  for `days_overdue` only, never a row filter.
- Sanitized errors; PostgreSQL durable truth; Redis live-SSE only; same-job
  clarification via `POST /chat/jobs/{job_id}/responses` — none of this bundle
  touches those paths.
- Exactly three crates (`app`, `core`, `chat`). **No new dependency, no
  migration.** This bundle does not add a *new* capability or query (it enriches
  the one existing savings-charges query), so no new `knowledge/**` or
  `queries/**` surface beyond editing the existing files.
- English-only product copy.
- Single-statement `SELECT`, parameterized-only, `require_office_filter: true`
  preserved.

## Design

### A. `amount_levied_total` (A.1.3 Finding 1)

Add one computed column to `queries/savings/pending_charges_clients.sql`,
positioned immediately after `amount_written_off` and before `amount_outstanding`
(matching the A.1.6 layout):

```sql
  COALESCE(sac.amount_paid_derived, 0)
+ COALESCE(sac.amount_waived_derived, 0)
+ COALESCE(sac.amount_writtenoff_derived, 0)
+ sac.amount_outstanding_derived            AS amount_levied_total,
```

`amount_outstanding_derived` is `NOT NULL` in the source, so no `COALESCE` around
it — matching the existing `amount_outstanding` select. Sensitivity
`public_business`, type `decimal`.

### B. Currency precision & symbol in the payload (W-J decisions 1, 5)

Add two columns:

- `currency_digits` ← `sa.currency_digits` (`smallint NOT NULL` on
  `m_savings_account`; the precision the account was *booked* at — A.5.4). Type
  `integer`, sensitivity `public_business`. This is the value a renderer rounds
  to. **Not** `m_organisation_currency.decimal_places` (current tenant config,
  can drift — the A.1.6 draft's use of `cur.decimal_places` is corrected here).
- `currency_display_symbol` ← `cur.display_symbol` (nullable; AED has none —
  A.5.2). Type `string`, sensitivity `public_business`. Kept nullable in the
  payload; the `NULL → code` fallback is a *rendering* decision (Bundle 9), not a
  SQL decision, so SQL emits the raw nullable symbol.

Both placed right after `currency_code`. `currency_multiplesof` is **not** added
(A.5.3: only relevant for sub-unit-less currencies, no analyst question needs it
yet — YAGNI, add when a question demands it).

### C. Fan-out-safe currency join (W-J decision 4)

Replace the (absent-today) plain equality join with a `LEFT JOIN LATERAL … LIMIT 1`
so an unconstrained `m_organisation_currency.code` cannot duplicate charge rows:

```sql
LEFT JOIN LATERAL (
    SELECT display_symbol
    FROM m_organisation_currency
    WHERE code = sa.currency_code
    LIMIT 1
) cur ON true
```

`currency_digits` comes from `sa` directly (no join needed), so the LATERAL
subselect carries only `display_symbol`.

### D. Enforce the fan-out-safe join in the validator

Add a hard-coded rule to `validator.rs`: any query whose SQL references
`m_organisation_currency` must reference it through `LATERAL`. Implemented as a
small pure helper `currency_join_is_fanout_safe(sql_upper) -> bool` called from
`validate_sql_safety`, unit-tested directly. Also add a declarative `checks:`
entry to the query YAML so the human contract is visible next to the SQL.

This is deliberately a coarse guard (presence of `LATERAL` when
`M_ORGANISATION_CURRENCY` is present) — it is one line, it catches the exact
regression described in A.5.5, and it needs no SQL parser.
`// ponytail: string-level guard; a real join-graph analysis is overkill for one table.`

### E. Query & capability YAML output-field contract

`knowledge/queries/savings/pending_charges_clients.yaml` `output_fields` gains
`currency_digits`, `currency_display_symbol` (after `currency_code`) and
`amount_levied_total` (after `amount_written_off`), in the exact SQL column order,
or `validate_runtime` fails. `knowledge/capabilities/savings/pending_charges_clients.yaml`
`output_fields.public` gains the same three names (order there is not
contract-enforced but is kept consistent).

### F. W-A4 — per-capability temporal & limit default review (closes E4)

Walk all 30 capability YAMLs. Classification and the resulting action:

| Class | Rule (issue §234–240) | Default policy |
| --- | --- | --- |
| **Point-in-time** | single reference date | `required: false, default: business_today, fill_when_missing: true` (unchanged) |
| **Rolling — monthly-grouped** | `from/to` feeding a `GROUP BY month` or month-series | `from_date: business_today - 12m`, `to_date: business_today` |
| **Rolling — single-period** | `from/to` bounding one aggregate/top-N over a period | `from_date: start_of_month(business_today)`, `to_date: business_today` |
| **No date param** | snapshot / lookup | no date action |
| **top-N limit** | genuine ranking | `default: 10` numeric (keep `hard_cap`) |
| **detail-list limit** | analyst detail dump | `default: unbounded` + `hard_cap` (unchanged) |

Per-capability decision table (the E4 deliverable — appended to
`docs/product/analyst-question-inventory.md`, created by Bundle 3):

| Capability | Date class | Date action | Limit class → action |
| --- | --- | --- | --- |
| savings/pending_charges_clients | point-in-time (`as_of_date`) | keep | detail-list → keep `unbounded`/10000 |
| client/activation_monthly_breakdown | rolling-monthly | `from -12m`, `to today` | (no limit) |
| client/activation_top_n_offices | rolling-single | `from MTD`, `to today` | top-N → `default: 10` |
| client/top_n_by_deposit_volume | rolling-single | `from MTD`, `to today` | top-N → `default: 10` |
| client/top_n_by_savings_account_count | no date | — | top-N → `default: 10` |
| client/top_n_by_savings_balance | no date | — | top-N → `default: 10` |
| client/client_list_recent | no date | — | detail-list → keep |
| client/client_random_sample | no date | — | detail-list → keep |
| client/lifecycle_summary | no date | — | (no limit) |
| client/name_lookup | no date | — | (no limit; `search` required) |
| client/summary_by_office | no date | — | detail-list → keep |
| organization/office_activity_ranking | rolling-single | `from MTD`, `to today` | top-N → `default: 10` |
| organization/office_dormant | rolling-single | `from MTD`, `to today` | detail-list → keep |
| organization/office_opening_monthly_breakdown | rolling-monthly | `from -12m`, `to today` | (no limit) |
| organization/hierarchy_summary | no date | — | (no limit) |
| organization/office_client_summary | no date | — | detail-list → keep |
| organization/office_hierarchy_tree | no date | — | detail-list → keep |
| organization/office_list_basic | no date | — | detail-list → keep |
| organization/office_savings_summary | no date | — | (per file) |
| organization/office_summary | no date | — | (per file) |
| savings/activity_list | rolling-single | `from MTD`, `to today` | detail-list → keep |
| savings/balance_summary | no date | — | (no limit) |
| savings/deposit_monthly_breakdown | rolling-monthly | `from -12m`, `to today` | (no limit) |
| savings/deposit_monthly_top_n | rolling-monthly | `from -12m`, `to today` | top-N → `default: 10` |
| savings/deposit_top_n | rolling-single | `from MTD`, `to today` | top-N → `default: 10` |
| savings/deposit_total | rolling-single | `from MTD`, `to today` | (no limit) |
| savings/withdrawal_monthly_breakdown | rolling-monthly | `from -12m`, `to today` | (no limit) |
| savings/withdrawal_monthly_top_n | rolling-monthly | `from -12m`, `to today` | top-N → `default: 10` |
| savings/withdrawal_top_n | rolling-single | `from MTD`, `to today` | top-N → `default: 10` |
| savings/withdrawal_total | rolling-single | `from MTD`, `to today` | (no limit) |

Every edit keeps `required: false` and (for date params) `fill_when_missing:
true`, so `validate_capability_parameter_contract` and `validate_policies` stay
satisfied — a defaulted param still "covers" a query-required param. No query
SQL changes for A4 (defaults are capability-config, the `BETWEEN
$n::date AND $m::date` shape is unchanged).

**One open decision for spec-review** (§ Open decisions): the trailing-window
sizes (`12m` monthly, MTD single-period) and the top-N numeric default (`10`) are
product judgments. They are concrete in the table above so the plan has no
placeholders, but the user confirms or adjusts them before the plan runs.

### G. Bundle-3 dependency (marked, not invented)

W-A3 ("close the gaps A1 identified" — *which extra capabilities to enrich*) is
**not** in B4. That set is finalized against the W-A1 inventory
(`docs/product/analyst-question-inventory.md`, Bundle 3). B4 touches only the one
already-existing savings-charges capability plus the defaults review of the 30
existing capabilities. Any additional enrichment is deferred to Bundle 8 (W-A3).

## Testing strategy

Static (no DB, run in CI on every change):

- `cargo test -p chat --lib` — covers `parameter_policy.rs` and the new
  `validator.rs` currency-join unit test (`currency_join_is_fanout_safe`).
- A new validator unit test asserts: SQL mentioning `M_ORGANISATION_CURRENCY`
  without `LATERAL` fails; the same SQL with the LATERAL form passes.
- `cargo test -p chat --test catalog_validation` — static catalog load + validate;
  confirms the enriched YAML still passes referential integrity and the new
  `checks:` entry loads.

DB-backed (gated on a reachable Fineract pool — the row-shape regression guard):

- `validate_runtime` (exercised by the catalog-validation integration path when
  `FINERACT_DATABASE_URL` is set) prepares the enriched SQL and asserts the
  prepared columns equal the new `output_fields` order — this is the guard that
  `amount_levied_total`, `currency_digits`, `currency_display_symbol` exist,
  are spelled correctly, and are in the right position. A missing/renamed column
  fails the prepare-vs-contract comparison.

Acceptance:

- The enriched SQL prepares against Fineract and its columns match the query YAML
  `output_fields` exactly (via `validate_runtime`).
- The catalog validator rejects any query that joins `m_organisation_currency`
  without `LATERAL` (unit test).
- `amount_levied_total = paid + waived + writtenoff + outstanding` for every row
  by construction (guaranteed by the expression; A.1.3 census showed no source
  column carries this total).
- Every changed capability YAML still loads and passes `KnowledgeValidator`.

## Out of scope

- Money **rendering**: rounding to `currency_digits`, `multi_currency` warning,
  per-currency subtotal cards, `TableColumnKind::Money` wiring in
  `presentation/builder.rs` — all Bundle 9 (W-J decisions 2, 3 / W-G).
- Any new capability / query / metric (Bundle 8, W-A3, gated on W-A1).
- Loan capabilities (issue 008).
- `currency_multiplesof` payload column (no backing question yet).
- Runtime application of the new defaults at request-resolution time (the values
  load and validate here; the fill-when-missing execution path is Bundle 5/10).
- Changing the shipped `ORDER BY` (the "biggest debt first" ordering is kept).
