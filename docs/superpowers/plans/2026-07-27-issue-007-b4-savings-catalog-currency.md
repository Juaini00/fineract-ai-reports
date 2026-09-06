# Issue 007 — Bundle 4 Implementation Plan: Savings Catalog Completeness, Defaults & Currency

> **For agentic workers:** REQUIRED SUB-SKILL — use `superpowers:subagent-driven-development`
> or `superpowers:executing-plans`. Steps use `- [ ]` checkboxes. **No commit steps** — the
> user commits manually.

**Goal:** Ship `amount_levied_total` + currency precision/symbol on
`savings_pending_charges_clients` (fan-out-safe), enforce the currency join in
the validator, and apply the per-capability temporal/limit default review (E4).

**Authoritative spec:** `docs/superpowers/specs/2026-07-27-issue-007-b4-savings-catalog-currency-design.md`.

**Architecture:** Edit one approved SQL file + its two YAMLs; add one hard-coded
validator rule + unit test; edit ~15 capability YAMLs for defaults; append the
decision table to the W-A1 inventory doc. No new crate, dependency, migration,
capability, or query.

**Tech Stack:** Rust edition 2024, Cargo workspace, sqlx, PostgreSQL. Existing
dependencies only.

## Global Constraints

- Approved-SQL only; office scope bound inside SQL via `c.office_id = ANY($1::bigint[])`.
- SQL only under `queries/**`; none in handlers/services/`assistant/**`.
- PII field-level: new columns are all `public_business`; client identity stays PII.
- `as_of_date` is a reference for `days_overdue` only, never a row filter.
- Single-statement `SELECT`, parameterized-only, `require_office_filter: true`.
- English-only copy. No new dependency/migration/capability/query.
- **Do not include commit steps.** A task is done when its listed checks exit `0`.

## ⚠️ Confirm before running (open decision from spec §F)

The A4 default values below are the spec's recommendation. If spec-review changed
the trailing-window sizes or the top-N default, update the literals in Task 6
before running it. Recommended: monthly-grouped `from = business_today - 12m`;
single-period `from = start_of_month(business_today)`; top-N `default: 10`; all
`to = business_today`.

---

## Task 1: Record a green baseline

**Files:** read only.

- [ ] **Step 1: Format + compile**

```bash
cargo fmt --check
cargo check -p chat
```
Expected: both exit `0`.

- [ ] **Step 2: Baseline the tests this plan must keep green**

```bash
cargo test -p chat --lib
cargo test -p chat --test catalog_validation
```
Expected: pass. If `catalog_validation` skips or fails only on a missing
`FINERACT_DATABASE_URL` (the `validate_runtime` path), record that exact message —
the DB-backed column-contract check in Task 3 will need a reachable Fineract pool
to fully verify; the static checks still run without it.

---

## Task 2: Enrich the approved SQL (`amount_levied_total` + currency, fan-out-safe)

**Files:**
- Modify: `queries/savings/pending_charges_clients.sql`

**Interfaces:** produces three new output columns in this order —
`currency_digits`, `currency_display_symbol` (after `currency_code`);
`amount_levied_total` (after `amount_written_off`, before `amount_outstanding`).

- [ ] **Step 1: Replace the SQL with the enriched query**

Overwrite `queries/savings/pending_charges_clients.sql` with exactly:

```sql
SELECT
    c.id AS client_id,
    c.display_name AS client_display_name,
    c.office_id,
    o.name AS office_name,
    sa.id AS savings_account_id,
    sac.id AS savings_account_charge_id,
    ch.id AS charge_definition_id,
    ch.name AS charge_name,
    sac.is_penalty,
    sac.charge_time_enum AS charge_timing_enum,
    sa.currency_code,
    sa.currency_digits,
    cur.display_symbol AS currency_display_symbol,
    sac.amount AS amount_due_current,
    COALESCE(sac.amount_paid_derived, 0) AS amount_paid,
    COALESCE(sac.amount_waived_derived, 0) AS amount_waived,
    COALESCE(sac.amount_writtenoff_derived, 0) AS amount_written_off,
      COALESCE(sac.amount_paid_derived, 0)
    + COALESCE(sac.amount_waived_derived, 0)
    + COALESCE(sac.amount_writtenoff_derived, 0)
    + sac.amount_outstanding_derived AS amount_levied_total,
    sac.amount_outstanding_derived AS amount_outstanding,
    sac.charge_due_date AS due_date,
    CASE
        WHEN sac.charge_due_date IS NULL THEN NULL
        WHEN $2::date > sac.charge_due_date THEN $2::date - sac.charge_due_date
        ELSE 0
    END AS days_overdue
FROM m_savings_account_charge sac
JOIN m_savings_account sa ON sa.id = sac.savings_account_id
JOIN m_client c ON c.id = sa.client_id
JOIN m_office o ON o.id = c.office_id
JOIN m_charge ch ON ch.id = sac.charge_id
LEFT JOIN LATERAL (
    SELECT display_symbol
    FROM m_organisation_currency
    WHERE code = sa.currency_code
    LIMIT 1
) cur ON true
WHERE sac.waived = false
  AND sac.is_paid_derived = false
  AND sac.is_active = true
  AND sac.amount_outstanding_derived > 0
  AND c.office_id = ANY($1::bigint[])
ORDER BY sac.amount_outstanding_derived DESC, sac.charge_due_date NULLS LAST, sac.id
LIMIT $3;
```

Note: the `WHERE`/`ORDER BY`/`LIMIT $3`/`ANY($1::bigint[])`/`$2::date` are byte-for-byte
the shipped versions — only the SELECT list and the LATERAL join are added.

- [ ] **Step 2: Sanity-check the diff is additive only**

```bash
git diff queries/savings/pending_charges_clients.sql
```
Expected: additions are the three columns and the `LEFT JOIN LATERAL` block; the
`WHERE`, office `ANY`, `$2::date`, and `LIMIT $3` are unchanged (no removed filter,
no reintroduced `charge_due_date <= $2`).

---

## Task 3: Update the query YAML output-field contract + declarative currency check

**Files:**
- Modify: `knowledge/queries/savings/pending_charges_clients.yaml`

`output_fields` order MUST match the SQL SELECT order or `validate_runtime` fails.

- [ ] **Step 1: Insert the two currency fields after `currency_code`**

In `knowledge/queries/savings/pending_charges_clients.yaml`, immediately after the
`currency_code` block (the one whose `name: currency_code`), insert:

```yaml
  - name: currency_digits
    type: integer
    sensitivity: public_business
    description: m_savings_account.currency_digits, the precision the account was booked
      at (NOT NULL). Renderers round money columns to this, not to organisation config.
  - name: currency_display_symbol
    type: string
    sensitivity: public_business
    description: m_organisation_currency.display_symbol; nullable (e.g. AED has none).
      The NULL-to-currency_code fallback is a rendering decision, not applied in SQL.
```

- [ ] **Step 2: Insert `amount_levied_total` after `amount_written_off`**

Immediately after the `amount_written_off` block and before `amount_outstanding`,
insert:

```yaml
  - name: amount_levied_total
    type: decimal
    sensitivity: public_business
    description: paid + waived + written_off + outstanding. The only exact reconstruction
      of total-ever-levied; Fineract stores no lifetime levied total for recurring charges.
```

- [ ] **Step 3: Add the declarative currency-join check**

In the `checks:` list of the same file, append:

```yaml
  - id: currency_join_fanout_safe
    rule: Any join to m_organisation_currency must use LEFT JOIN LATERAL (... LIMIT 1)
      because m_organisation_currency.code has no unique constraint and a plain equality
      join can duplicate rows.
```

- [ ] **Step 4: Validate the YAML loads**

```bash
cargo test -p chat --test catalog_validation
```
Expected: passes the static load/validate. If a Fineract pool is reachable, the
`validate_runtime` step also confirms the prepared columns equal the new
`output_fields` order (the row-shape regression guard). If it fails with a
column-mismatch message, the SQL SELECT order and this file's `output_fields`
order disagree — reconcile them.

---

## Task 4: Update the capability YAML output fields

**Files:**
- Modify: `knowledge/capabilities/savings/pending_charges_clients.yaml`

- [ ] **Step 1: Add the three names to `output_fields.public`**

In `knowledge/capabilities/savings/pending_charges_clients.yaml`, under
`output_fields.public`, add `currency_digits` and `currency_display_symbol` after
`currency_code`, and `amount_levied_total` after `amount_written_off`. Result of
that block:

```yaml
output_fields:
  public:
  - office_id
  - office_name
  - savings_account_id
  - savings_account_charge_id
  - charge_definition_id
  - charge_name
  - is_penalty
  - charge_timing_enum
  - currency_code
  - currency_digits
  - currency_display_symbol
  - amount_due_current
  - amount_paid
  - amount_waived
  - amount_written_off
  - amount_levied_total
  - amount_outstanding
  - due_date
  - days_overdue
  pii_conditional:
  - client_id
  - client_display_name
```

- [ ] **Step 2: Validate**

```bash
cargo test -p chat --test catalog_validation
```
Expected: passes.

---

## Task 5: Enforce the fan-out-safe currency join in the validator

**Files:**
- Modify: `crates/chat/src/knowledge/catalog/validator.rs`

**Interfaces:** adds `fn currency_join_is_fanout_safe(sql_upper: &str) -> bool` and
one call site in `validate_sql_safety`.

- [ ] **Step 1: Add the guard helper**

In `crates/chat/src/knowledge/catalog/validator.rs`, add near `sql_tokens`
(before the `#[cfg(test)]` module):

```rust
/// A query may reference `m_organisation_currency` only through a `LATERAL` join,
/// because `m_organisation_currency.code` has no unique constraint and a plain
/// equality join can fan out and duplicate rows (issue 007 A.5.5).
///
/// ponytail: string-level guard; a real join-graph analysis is overkill for one table.
fn currency_join_is_fanout_safe(sql_upper: &str) -> bool {
    !sql_upper.contains("M_ORGANISATION_CURRENCY") || sql_upper.contains("LATERAL")
}
```

- [ ] **Step 2: Call it from `validate_sql_safety`**

In `validate_sql_safety`, after the office-scope block (the
`if has_parameter(query, "office_ids")` block) and before the `from_date`/`to_date`
block, insert:

```rust
    if !currency_join_is_fanout_safe(&upper) {
        bail!(
            "query {} joins m_organisation_currency without LATERAL; use LEFT JOIN LATERAL (... LIMIT 1) to avoid fan-out",
            query.id
        );
    }
```

(`upper` is already in scope — it is the uppercased SQL computed at the top of
`validate_sql_safety`.)

- [ ] **Step 3: Add unit tests**

In the `#[cfg(test)] mod tests` block at the bottom of the file, add:

```rust
    #[test]
    fn currency_join_lateral_is_safe() {
        let sql = "SELECT sa.currency_code FROM m_savings_account sa \
                   LEFT JOIN LATERAL (SELECT display_symbol FROM m_organisation_currency \
                   WHERE code = sa.currency_code LIMIT 1) cur ON true";
        assert!(currency_join_is_fanout_safe(&sql.to_ascii_uppercase()));
    }

    #[test]
    fn plain_currency_equality_join_is_rejected() {
        let sql = "SELECT sa.currency_code FROM m_savings_account sa \
                   LEFT JOIN m_organisation_currency cur ON cur.code = sa.currency_code";
        assert!(!currency_join_is_fanout_safe(&sql.to_ascii_uppercase()));
    }

    #[test]
    fn query_without_currency_table_is_unaffected() {
        let sql = "SELECT c.id FROM m_client c";
        assert!(currency_join_is_fanout_safe(&sql.to_ascii_uppercase()));
    }
```

- [ ] **Step 4: Validate**

```bash
cargo fmt --check
cargo test -p chat --lib knowledge::catalog::validator
cargo test -p chat --test catalog_validation
```
Expected: all exit `0`; the three new unit tests pass; the enriched
`pending_charges_clients` SQL (which now uses LATERAL) still passes.

---

## Task 6: W-A4 — apply the per-capability temporal & limit defaults (E4)

**Files (capability YAMLs, `parameters:` block only):**
- Rolling-monthly → `from_date: business_today - 12m`, `to_date: business_today`:
  - `knowledge/capabilities/client/activation_monthly_breakdown.yaml`
  - `knowledge/capabilities/organization/office_opening_monthly_breakdown.yaml`
  - `knowledge/capabilities/savings/deposit_monthly_breakdown.yaml`
  - `knowledge/capabilities/savings/deposit_monthly_top_n.yaml`
  - `knowledge/capabilities/savings/withdrawal_monthly_breakdown.yaml`
  - `knowledge/capabilities/savings/withdrawal_monthly_top_n.yaml`
- Rolling-single → `from_date: start_of_month(business_today)`, `to_date: business_today`:
  - `knowledge/capabilities/client/activation_top_n_offices.yaml`
  - `knowledge/capabilities/client/top_n_by_deposit_volume.yaml`
  - `knowledge/capabilities/organization/office_activity_ranking.yaml`
  - `knowledge/capabilities/organization/office_dormant.yaml`
  - `knowledge/capabilities/savings/activity_list.yaml`
  - `knowledge/capabilities/savings/deposit_top_n.yaml`
  - `knowledge/capabilities/savings/deposit_total.yaml`
  - `knowledge/capabilities/savings/withdrawal_top_n.yaml`
  - `knowledge/capabilities/savings/withdrawal_total.yaml`
- top-N limit → `default: 10` (keep existing `hard_cap`):
  - `activation_top_n_offices`, `top_n_by_deposit_volume`,
    `top_n_by_savings_account_count`, `top_n_by_savings_balance`,
    `office_activity_ranking`, `deposit_monthly_top_n`, `deposit_top_n`,
    `withdrawal_monthly_top_n`, `withdrawal_top_n`

Point-in-time (`pending_charges_clients` `as_of_date`) and all no-date /
detail-list capabilities are **unchanged**.

- [ ] **Step 1: Rewrite each `from_date` block for the rolling-monthly group**

In each of the six rolling-monthly files, change the `from_date` default. Example
for `knowledge/capabilities/savings/deposit_monthly_breakdown.yaml` — replace:

```yaml
  from_date:
    type: date
    required: false
    default: business_today
    fill_when_missing: true
```
with:
```yaml
  from_date:
    type: date
    required: false
    default: business_today - 12m
    fill_when_missing: true
```
Leave `to_date` as `default: business_today` (already correct). Apply the
identical `from_date` edit to the other five rolling-monthly files.

- [ ] **Step 2: Rewrite each `from_date` block for the rolling-single group**

In each of the nine rolling-single files, replace the `from_date` block's
`default: business_today` with `default: start_of_month(business_today)`. Leave
`to_date` as `default: business_today`. Example for
`knowledge/capabilities/savings/deposit_top_n.yaml`:

```yaml
  from_date:
    type: date
    required: false
    default: start_of_month(business_today)
    fill_when_missing: true
```

- [ ] **Step 3: Set a numeric top-N limit default**

In each of the nine top-N files, replace the `limit` block's `default: unbounded`
with `default: 10`, keeping the existing `hard_cap`. Example for
`knowledge/capabilities/savings/deposit_top_n.yaml`:

```yaml
  limit:
    type: integer
    required: false
    default: 10
    hard_cap: 100
```
(Keep each file's own `hard_cap` value — e.g. `deposit_monthly_top_n` keeps
`hard_cap: 10000`, `activation_top_n_offices` keeps `hard_cap: 100`.)

- [ ] **Step 4: Confirm the whitelist accepts every new default expression**

```bash
git grep -nE 'default: (business_today - 12m|start_of_month\(business_today\)|10)' -- knowledge/capabilities
cargo test -p chat --lib parameter_policy
```
Expected: the grep lists exactly the edited lines; `DefaultExpr::parse` unit tests
pass (the expressions `business_today - 12m`, `start_of_month(business_today)`, and
integer `10` are all in the existing allowlist — no grammar change needed).

- [ ] **Step 5: Validate the whole catalog still passes**

```bash
cargo test -p chat --test catalog_validation
```
Expected: passes — every edited capability still declares `required: false` with a
default, so `validate_capability_parameter_contract` and `validate_policies` stay
satisfied.

---

## Task 7: Append the E4 decision table to the analyst inventory

**Files:**
- Modify: `docs/product/analyst-question-inventory.md` (created by Bundle 3)

> Depends on Bundle 3 having created `docs/product/analyst-question-inventory.md`.
> If it does not yet exist, STOP and run Bundle 3 first — do not create it here
> (it is that bundle's deliverable).

- [ ] **Step 1: Append the decision table**

Append a `## W-A4 temporal & limit default decisions (E4)` section containing the
30-row table from spec §F (capability, date class, date action, limit action),
plus a one-paragraph rationale: point-in-time keeps `business_today`; rolling
windows use a trailing default (`-12m` for month-grouped, MTD for single-period)
instead of collapsing to one day; genuine top-N rankings carry a numeric default
(`10`); analyst detail lists stay `unbounded` under a `hard_cap`. Also record the
A.5.7 derived-column sensitivity rule (`amount_levied_total`, `days_overdue`,
`charge_timing_enum` are `public_business` — none identifies a person).

- [ ] **Step 2: Validate the doc is well-formed**

```bash
git diff --check docs/product/analyst-question-inventory.md
```
Expected: no whitespace errors.

---

## Task 8: Final phase-gate validation

- [ ] **Step 1: Full check**

```bash
cargo fmt --check
cargo check -p chat
cargo test -p chat --lib
cargo test -p chat --test catalog_validation
git diff --check
```
Expected: all exit `0`.

- [ ] **Step 2: Confirm the SQL invariants held**

```bash
git grep -n 'charge_due_date <= ' -- queries/savings/pending_charges_clients.sql || echo "clean: no due-date row filter reintroduced"
git grep -n 'ANY($1::bigint\[\])' -- queries/savings/pending_charges_clients.sql
git grep -n 'LEFT JOIN LATERAL' -- queries/savings/pending_charges_clients.sql
```
Expected: the first prints `clean: ...` (no debt-hiding filter); the second and
third each print one match (office scope bound in SQL; currency join is fan-out-safe).

- [ ] **Step 3: DB-backed column-contract (when a Fineract pool is reachable)**

With `FINERACT_DATABASE_URL` set to the read replica, run the catalog-validation
path that invokes `validate_runtime`. Expected: the enriched query prepares and
its prepared columns equal the query YAML `output_fields` order
(`amount_levied_total`, `currency_digits`, `currency_display_symbol` present and
positioned correctly). If no pool is reachable, record that this final DB check is
deferred to CI/staging where the replica exists.

---

## Completion gate

Done when: all tasks checked; `cargo fmt --check`, `cargo check -p chat`,
`cargo test -p chat --lib`, and `cargo test -p chat --test catalog_validation`
exit `0`; the validator rejects a non-LATERAL `m_organisation_currency` join
(unit tests green); the enriched SQL keeps office scope in `ANY($1::bigint[])`,
carries no `charge_due_date` row filter, and joins currency via LATERAL; the 15
A4 capability edits load; the E4 decision table is appended to the W-A1 inventory;
and no new crate/dependency/migration/capability/query was introduced.
