# 008 — Loan domain analyst capabilities

Status: active — scope defined; execution pending
Severity: high
Area: knowledge | catalog | retrieval | SQL | scoping | currency | presentation
Created: 2026-07-27
Resolved:

Split from: 007 (analyst-grade knowledge and request mapping) — W-M decision, LOCKED 2026-07-27
Design reference: issue 007 Appendix A.2 (loan schema, verified against `fineract_qicard_default` / `fineract_default`, 2026-07-24)

## Why this is a separate issue

Issue 007's W-M decision split the loan domain out of 007 so W-A3 stays bounded to
savings + client. The catalog today has **zero** loan capabilities, queries, or metrics
(`knowledge/capabilities/` holds only `client/`, `organization/`, `savings/`), while
`fineract_default` carries 116 loans (87 active), 86 rows in `m_loan_arrears_aging`, and
299 overdue instalments. The gap is real and large, and the loan domain carries its own
design questions that are not the savings ones. 007's W-A1 inventory still enumerates
loan questions marked `missing`, so the gap stays visible from 007.

## Scope — five capabilities in priority order (007 Appendix A.2.1)

1. `loans_in_arrears_clients`
2. `loan_overdue_installments`
3. `loan_outstanding_balances_clients`
4. `loan_unpaid_charges_clients`
5. `loan_portfolio_summary_by_office`

Each needs: capability YAML with the per-parameter policy block, query YAML, approved SQL
under `queries/`, any new metric YAML, and bilingual retrieval assertions (Indonesian +
English), matching 007's W-A/W-D shape.

## Design questions this issue must resolve (from 007 W-M "must contain")

- **Office scope per ownership type.** `m_loan` has **no `office_id`** (A.2.2), so office
  scope must route through `m_client` or `m_group`. The group-owned case is a real branch
  (unlike savings charges, where A.3.2 measured zero group-owned charge rows). Measure how
  many loans are group-owned before choosing the join, per the A.3.1 table.
- **Arrears source per capability.** `m_loan_arrears_aging` is batch-maintained by a
  Fineract scheduled job whose freshness the reporting service cannot observe;
  `m_loan_repayment_schedule` is authoritative but more expensive (A.2.3, A.2.4). Choose
  per capability and document the freshness caveat in each `description`.
- **`loan_status_id` is inferred, not confirmed** (A.2.2). Confirm against Fineract source
  before any capability filters `loan_status_id = 300`, or output the raw value alongside
  any label.
- **`days_in_arrears` clamp convention** = `business_date − overdue_since_date_derived`,
  clamped at zero, `NULL` when the source date is NULL — matching 007's resolved
  `days_overdue` convention so every aging capability agrees.
- **`loan_unpaid_charges_clients` must not filter on the due date.** A.2.5: 33 of 38 unpaid
  loan charges have `due_for_collection_as_of_date IS NULL`; filtering on it hides most of
  the debt, exactly as A.1.3 established for savings.
- **Delinquency buckets read from `m_delinquency_range`** (A.2.6), never hardcoded.

## Inherited infrastructure

Loan capabilities are wider and more money-dense than savings ones, so they are the first
real test of 007's W-G (presentation), W-I (`hard_cap`/`timeout_ms`/backstop), W-J
(currency/money semantics), and W-L (management observability). This issue depends on those
landing in 007 first; do not duplicate them here.

## Cross-cutting invariants

Approved-SQL only (no AI-generated SQL); office scope bound in SQL via
`office_ids = ANY($n::bigint[])`, never Rust post-filter; PII field-level gating;
"today" = tenant business date; sanitized errors; three crates unchanged; English-only copy.

## Acceptance

- All five capabilities exist with capability YAML, query YAML, approved SQL, and metrics.
- Office scope is bound inside SQL for client-owned and group-owned loans, with the
  group-owned branch measured and handled (or explicitly deferred with a count).
- Each capability's `description` states its arrears source and freshness caveat.
- `loan_status_id` filtering is confirmed against source, or the raw value is output.
- `loan_unpaid_charges_clients` does not filter on `due_for_collection_as_of_date`.
- Bilingual retrieval assertions pass for each capability (rank 1, clears gap threshold).
- `cargo test -p chat --test catalog_validation` green.
