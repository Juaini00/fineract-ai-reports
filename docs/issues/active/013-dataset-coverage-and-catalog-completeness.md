# 013 — Dataset completeness: format conformance, stale removal, and full-domain coverage

Status: active
Severity: high
Area: chat | catalog | datasets | knowledge | docs
Created: 2026-08-09

> **Goal (owner's bar).** No dataset may be non-conforming to format, stale, or
> left incomplete because of the MVP boundary. The product must not be "savings
> only", must not "filter by name but not by id", and must not present
> `Unsupported` that is really a self-inflicted knowledge gap. This issue is the
> single source of truth for **every dataset that must be fixed or authored**, so
> that "not supported" becomes a shrinking, scheduled set — never a silent
> permanent ceiling.

Successor to **012** (runtime that outgrew the catalog) and **011** (filter/shape
coverage). Every gap below is verified against the live Fineract database
(schema via the `postgresql` MCP = `fineract_local_default`; row counts from
`fineract_default`) and the catalog source. No 012 security/PII/office-scope/
budget invariant may be weakened by any change here.

---

## 0. Executive summary — three defect classes

1. **Format is schema-valid but has a completeness loophole.** All 10 datasets
   load (`validate_dataset` passes), but the validator forces a filter only for
   narrowable **string** columns (issue 011) — **not for stable-id (`bigint`)
   columns.** That is exactly why "filter by name, not by id" passes today. Plus
   vestigial junk (`order_by: []`/`output_fields: []`/`parameters: []`) and
   dataset-vs-shape output-field inconsistency.
2. **Stale / unwired surface.** 5 of 10 datasets have **zero** consuming
   capability; 5 of 6 resolver/probe shapes are dead; 6 fragment SQL files back
   nothing. The conditional/sequential machinery 012 built has essentially one
   live path.
3. **Whole domains uncovered.** Loan (data-rich: `m_loan` 65 / `m_loan_transaction`
   3721 / `m_loan_repayment_schedule` 2167 / `m_loan_arrears_aging` 48),
   Accounting/GL (`acc_gl_journal_entry` **21846** — the largest table),
   payments/transfers, the savings **deposit** sub-domain, group/center, tax —
   none have a dataset. Only **share** is genuinely empty (skip).

Root cause of (3): a **deliberate, locked MVP scope decision** (§1) — not an
oversight. But the owner has ruled it must no longer be left incomplete: this
issue therefore **specifies every domain's datasets**, and records the
**activation gate** each deferred domain must pass (§7).

---

## 1. Why it looks like this (scope decision, with the two "missing" kinds)

Every data area carries a machine-readable `status` (`knowledge/data-scope/areas/`):

| Area | status |
| --- | --- |
| `organization_foundation`, `client_foundation` | `included_mvp_foundation` |
| `savings_core`, `savings_transactions` | `included_mvp_domain` |
| `savings_charges_fees` | `secondary` |
| `group_center_foundation` | `conditional` |
| `loans`, `accounting_gl`, `tax`, `custom_datatables`, `audit_users_operations` | **`deferred`** |
| `out_of_scope_areas` (arbitrary SQL, schema search, writes, doc/address/user-audit) | `rejected_group` |

012 §Required-dataset-migration (line 359): *"Add loan/audit datasets only when
those domains are approved."* Loan already has active issue **008**. So the
domain omissions were on-plan. **Two distinct "missing" kinds:**
- **Deferred-by-decision** (loan, GL, tax, custom-datatables, audit) — correct
  today, but must be specified now and activated on schedule (§7).
- **In-scope-but-incomplete** (savings/client/org half-wired, missing id
  filters, stale shapes) — the actionable rot inside the approved surface (§3–§5).

---

## 2. Definition of "conforming" (format contract) + the loophole

Canonical dataset format (`crates/chat/src/knowledge/dataset/model.rs`,
enforced by `dataset/validate.rs`):

- Required: `id`, `database`, `source_sql`, `shapes` (≥1). Optional: `tables`,
  `filters`, `entity`, `filters_exempt`, `order_by`, `output_fields`,
  `parameters`, `timeout_ms`.
- **Filter**: `id`, `expr` (grammar-checked), `type` ∈
  {date,integer,boolean,string,decimal}, non-empty `operators`. `in` ⇒ every
  shape needs a non-zero `row_cap`. `exact_identifier` ⇒ string, `eq`-only,
  case-sensitive, needs a `masked_output` field + authorized `office_ids`.
- **Shape**: unique id, `request_shape`, `role` (default terminal), `fragment`
  required when the dataset declares filters, ≥1 output field, ≥1 `core` field,
  no `filter_only`/`never_use` projected. **Any narrowable string column must
  have a filter or be in `filters_exempt`** (011). Resolver ⇒ non-empty `produces`.
- **Entity**: `id_field` must be an output field, `bigint public_business`;
  `label_fields` must be output fields.

**Conformance verdict:** all 10 datasets pass. But three format defects the
validator does **not** catch:

- **D1 — the id-filter loophole.** The 011 rule covers **string** columns only.
  A `bigint` FK column (`office_id`, `product_id`, `savings_account_id`) can be
  returned or joined with **no filter slot** and still validate. This is the
  structural reason "filter by name, not by id" survives. **Fix:** extend
  `validate_dataset` so a returned/joinable stable-id column must be a filter
  (or explicitly exempt), symmetric with the string rule.
- **D2 — vestigial junk.** Dataset-level empty `order_by: []`/`output_fields: []`/
  `parameters: []` in `savings/account_activity`, `savings/account_charges`,
  `savings/accounts`. Valid (serde default) but misleading — delete.
- **D3 — no "unconsumed shape" check.** A dataset shape with no consuming
  capability validates silently (§3). **Fix:** a catalog lint failing on any
  dataset shape referenced by zero capabilities, and warning on any
  resolver/probe not wired to a `probe:`.

---

## 3. STALE / unwired (declared, zero live consumers)

| Dataset / shape | Role | Consumers | Action |
| --- | --- | --- | --- |
| `client.portfolio_counts` / `counts_by_client` (grouped, `savings_account_count`) | terminal (grouped) | **none** | wire to a `clients_with_account_counts` capability, or retire |
| `organization.offices` / `office_candidates` | resolver | **none** | wire as an office probe (office-name resolution), or retire |
| `savings.products` / `products_by_client` | resolver | **none** | wire as a product probe on a per-client product report |
| `savings.transactions` / `activity_rows` | resolver | **none** | wire to the general activity path (§5, replaces account_activity fixture) |
| `savings.charge_definitions` / `charge_type_candidates` | **probe** | **none** | wire as the A3 charge-type probe |

6 fragment SQL files back zero live capabilities. **Only** wired probe today:
`client_relationship_by_id.client_id → client.identity/identity_candidates`.
Rule: **wire or retire with a recorded decision** — no authored-but-unreachable
fragment may remain (enforced by the D3 lint).

---

## 4. INCOMPLETE filters on the live shapes (DB-verified)

Verified columns (`fineract_local_default`):
`m_savings_account(id, account_no, client_id, group_id, product_id,
field_officer_id, status_enum, deposit_type_enum, currency_code)` — **no direct
`office_id`** (office is via `m_client.office_id`); `m_savings_account_transaction`
**has** `office_id`; `m_savings_product(id, name, short_name, deposit_type_enum,
currency_code)`.

| Live shape | Missing / weak | Fix (DB-verified) |
| --- | --- | --- |
| `savings.accounts` (identity, terms) | `product_id`, group_id, field_officer_id not filterable; office by name only | add `savings_product_id`, `group_id`, `staff_id` filters `[eq,in]` (direct columns); add `office_id` filter as `mc.office_id` via the client join |
| `savings.account_charges` (recent_list, pending_clients, overdue_clients, type_count) | **no** `client_id`/`savings_account_id`/`office_id` filter; office by name; **no `in`** | add `savings_account_id`/`client_id` `[eq,in]`; office via client join |
| `savings.account_activity` (account_match) | eq-only name+amount, **no stable-id** filter | replace contract (§5) — the fixture-specific shape 012 #9 said to remove |
| `savings.transactions` (activity_rows, resolver) | fine, but **dead** | wire (§3) |
| `organization.office_summary` (summary) | aggregate-only, no entity resolver | keep for summary; office existence answered by `organization.offices` once wired |

`office_id` is absent as a filter on **every** savings dataset; bounded `IN`
sits only on the dead resolver surface.

---

## 5. General savings-activity path (replace the fixture contract)

Retire `savings.account_activity/account_match`'s `search + product_name +
latest_transaction_amount` fingerprint. Replace with the two-stage resolution
012 scenario A1 requires, using shapes that already exist:
1. `client.identity/identity_candidates` (resolver, live) → resolve client.
2. `savings.accounts/accounts_by_client` (resolver, **currently dead**) → resolve
   account, wired as a `probe:` so a `CardinalityBranch` picks 0/1/many.
3. `savings.transactions/activity_rows` (resolver, **currently dead**) or the
   `savings.activity_list` capability → the activity itself.
Keep `latest_transaction_amount` only as an optional disambiguator, never required.

---

## 6. Missing-domain dataset SPECS (authorable, DB-verified)

Each spec is enough to write the dataset YAML directly. All columns verified in
`fineract_local_default`. Row counts from `fineract_default`. Skip **share**
(`m_share_account`/`m_share_product` = 0 rows).

### 6.1 LOAN *(deferred → issue 008; highest value)*
- **`loan.accounts`** — tables `m_loan, m_client, m_office, m_product_loan`.
  Entity `kind: loan_account, id_field: loan_id`, labels `[loan_account_no,
  client_display_name]`, fallback `"Loan {loan_id}"`.
  Filters `[eq,in]`: `loan_id`, `client_id`, `group_id`, `savings_product_id`→`product_id`,
  `loan_officer_id`; `[eq]`: `loan_status_id`, `loan_type_enum`, `currency_code`;
  office via `m_client.office_id`. Verified columns:
  `m_loan(id, account_no, client_id, group_id, product_id, loan_officer_id,
  loan_status_id, loan_type_enum, currency_code, disbursedon_date, closedon_date,
  principal_disbursed_derived, principal_outstanding_derived,
  total_repayment_derived, total_outstanding_derived)`.
  Shapes: **resolver** `loan_account_by_client` (produces `loan_id:integer`),
  terminal `portfolio` (outputs: account_no, principal_outstanding_derived,
  total_outstanding_derived, loan_status_id, disbursedon_date, currency_code),
  grouped `counts_by_client` (`grouped_by: client_id`).
- **`loan.arrears`** — `m_loan_arrears_aging` (48 rows) joined `m_loan`.
  Outputs: `total_overdue_derived, principal_overdue_derived,
  interest_overdue_derived, overdue_since_date_derived`. Filters: `loan_id`,
  `client_id`. Powers PAR / delinquency.
- **`loan.transactions`** — `m_loan_transaction` (3721). Filters `loan_id`,
  `savings`/`office_id`, `transaction_type_enum`, date range. Resolver
  `repayment_rows`.
- **`loan.repayment_schedule`** — `m_loan_repayment_schedule` (2167). Filters
  `loan_id`; outputs duedate, installment, principal/interest amounts,
  `completed_derived`.
- **`loan.products`** — `m_product_loan` (10): `id, name, short_name,
  currency_code, principal_amount, nominal_interest_rate_per_period,
  annual_nominal_interest_rate, accounting_type`. Resolver `product_by_name`.
- **`loan.charges`** — `m_loan_charge` (236) + `m_charge`. Mirrors
  `savings.account_charges`.

### 6.2 ACCOUNTING / GL *(deferred; 2nd value)*
- **`accounting.gl_accounts`** — `acc_gl_account` (70): `id, name, gl_code,
  classification_enum, account_usage, parent_id`. Entity `kind: gl_account,
  id_field: gl_account_id`. Resolver `gl_account_by_code` (produces
  `gl_account_id`). Chart-of-accounts listing.
- **`accounting.journal_entries`** — `acc_gl_journal_entry` (21846): `id,
  account_id, office_id, entry_date, type_enum (debit/credit), amount,
  transaction_id, loan_transaction_id, savings_transaction_id, entity_type_enum,
  office_running_balance`. Filters `[eq,in]`: `gl_account_id`→`account_id`,
  `office_id`; `[between]`: `entry_date`; `[eq]`: `type_enum`. Powers
  trial-balance, GL-account activity, debit/credit by office/date.

### 6.3 PAYMENTS / TRANSFERS *(no data area yet — add one)*
- **`payments.account_transfers`** — `m_account_transfer_transaction` (272)
  `+ m_account_transfer_details`. Verified: transaction has `amount,
  transaction_date, currency_code, is_reversed, from/to_loan_transaction_id`;
  details has `from_office_id, to_office_id, from_client_id, to_client_id,
  from_savings_account_id, to_savings_account_id, transfer_type`. Filters:
  from/to office/client/account, date range. Cross-account movement report.
- **`payments.payment_details`** — `m_payment_detail` (473): `id, payment_type_id,
  account_number, check_number, receipt_number` (thin; amount is on the linked
  transaction). Join savings/loan transaction → payment_detail → `m_payment_type`.
  Note `account_number`/`check_number` are sensitive — mask/gate.

### 6.4 DEPOSITS (FD/RD sub-domain of savings) *(in-scope, unsurfaced)*
- **`savings.deposits`** — `m_savings_account (deposit_type_enum 200 FD / 300 RD)`
  joined `m_deposit_account_term_and_preclosure` (17: `savings_account_id,
  min_deposit_term, max_deposit_term, deposit_amount, maturity_amount,
  maturity_date`) and `m_deposit_account_recurring_detail` (8:
  `savings_account_id, mandatory_recommended_deposit_amount, is_mandatory`) and
  `m_mandatory_savings_schedule` (807). Filters: `savings_account_id`, `client_id`,
  `deposit_type_enum`. Maturity / recurring-schedule reporting.

### 6.5 GROUP / CENTER *(conditional)*
- **`group.groups`** — `m_group` (5): `id, account_no, display_name, status_enum,
  office_id, staff_id, parent_id, level_id, hierarchy` + `m_group_client`
  (`group_id, client_id`). Entity `kind: group, id_field: group_id`, label
  `display_name`. Resolver `group_by_name` (produces `group_id`); terminal
  membership. Enable per-deployment.

### 6.6 TAX *(deferred; thin)*
- **`tax.withholding`** — `m_tax_component` (62: `id, name, percentage,
  start_date`), `m_tax_group` (38), `m_savings_account_transaction_tax_details`
  (`tax_component_id, amount`). Withholding-tax-by-transaction. Low volume.

---

## 7. Activation gate for deferred domains (mandatory, not hidden)

Authoring the §6 datasets makes a deferred domain executable, which per
`docs/product/capability-coverage/09-g-deferred-domains.md` requires a governance
gate — this issue records it so completeness is never blocked silently:
1. flip `knowledge/domains/<domain>.yaml` from `deferred` → `approved`;
2. flip the `knowledge/data-scope/areas/*.yaml` status from `deferred` → in-use;
3. **PII sign-off per output field** (loan/account/GL rows carry identity and
   financial PII);
4. ≥1 runnable capability YAML per shape.
Until a domain passes this gate, its questions return `Unsupported(deferred_domain)`
— honestly, never fabricated (012 line 614). The gate is a scheduled step, not a
reason to leave the dataset unspecified.

## 8. Documentation contradiction to fix

`docs/product/reporting-data-scope/06-5-explicitly-out-of-scope-permanent.md:5-14`
lists Loan / GL / Tax / custom-datatables under *"permanent … will never be
built"*, contradicting every machine-readable source (`status: deferred`, not
`rejected`; milestone map loan → v1.0; active issue 008). The authoritative
`out-of-scope.yaml` lists only arbitrary SQL, schema search, doc/address/
user-audit, and writes. **Correct §6-5 to `deferred (scheduled)`.**

---

## 9. Gap matrix (single source of truth)

| Domain / surface | Rows | Area status | Dataset today | Verdict |
| --- | --- | --- | --- | --- |
| Organization (offices, staff) | 8 / 12 | included | offices, office_summary | ✅ covered |
| Client identity | 75 | included | identity (+resolver) | ✅ core; wire office resolver |
| Client grouped counts | — | included | portfolio_counts | ⚠️ **STALE** — wire/retire |
| Savings accounts | 189 | included | accounts | ⚠️ add id filters (product_id/office via join/`in`) |
| Savings transactions | 6425 | included | transactions (resolver) | ⚠️ **STALE resolver** — wire |
| Savings products | 15 | included | products (resolver) | ⚠️ **STALE** — wire |
| Savings charges | 290 | secondary | account_charges, charge_definitions | ⚠️ probe **STALE**; add id filters |
| Savings deposits (FD/RD) | 17+8 | included | — | ❌ **missing (in-scope)** — §6.4 |
| Loan (accounts/tx/schedule/arrears) | 65/3721/2167/48 | deferred | — | ❌ spec §6.1 (issue 008) |
| Loan products / charges | 10 / 236 | deferred | — | ❌ spec §6.1 |
| Accounting / GL | 21846 / 70 | deferred | — | ❌ spec §6.2 (high value) |
| Payments / transfers | 473 / 272 | none | — | ❌ spec §6.3 (add data area) |
| Group / center | 5 / 5 | conditional | — | ❌ spec §6.5 |
| Tax | 62 / 38 | deferred | — | ❌ spec §6.6 (thin) |
| Share | 0 | none | — | ⛔ empty — do not build |

---

## 10. Remediation roadmap

**P0 — in-scope, no domain approval needed.**
1. Close format defects D1 (id-filter enforcement), D2 (delete junk), D3 (two
   catalog lints: unconsumed shape fails, unwired resolver warns).
2. Add DB-verified stable-id + `IN` filters to the live savings shapes (§4).
3. Wire or retire the 5 stale datasets / 5 dead resolvers (§3).
4. Replace the fixture `account_activity` contract with the general two-stage
   activity path (§5); surface the deposit sub-domain (§6.4).

**P1 — LOAN** (§6.1) through the §7 gate (issue 008).
**P2 — ACCOUNTING/GL** (§6.2) + **PAYMENTS/TRANSFERS** (§6.3, needs a data area).
**P3 — GROUP** (§6.5), **TAX** (§6.6). Skip share.
**Cross-cutting:** fix doc §6-5 (§8).

---

## 11. Definition of done

1. `validate_dataset` enforces id-column filters (D1); the two catalog lints
   (D3) are in CI and green; no vestigial junk remains (D2).
2. Every dataset shape has a live consuming capability or a recorded retirement;
   no dead resolver/probe or orphan fragment remains.
3. Live savings shapes are filterable by `office_id`/`product_id`/stable-id +
   bounded `IN`; narrowing by id (not name) is expressible on the executed path.
4. Every domain in §9 marked ❌ has an **authored dataset** (in-scope) or a
   **complete spec + recorded activation-gate status** (deferred) — nothing left
   "incomplete because of MVP".
5. 012 scenario A1 (client → account resolution) runs end-to-end from a sentence.
6. Doc §6-5 no longer mislabels deferred domains as permanent.
7. Deferred domains have a written activation order (loan → GL → payments →
   group/tax) each referencing the §7 gate, so `Unsupported` is a shrinking,
   scheduled set.
8. No 012 security/PII/office-scope/budget invariant weakened.

## Links
- `docs/issues/resolved/012-agentic-workflow-runtime-and-framework-completion.md`
- `docs/issues/resolved/011-dataset-filter-and-shape-coverage-gaps.md`
- `docs/issues/active/008-loan-domain-analyst-capabilities.md`
- `docs/product/capability-coverage/09-g-deferred-domains.md` (activation gate)
- `docs/product/reporting-data-scope/06-5-explicitly-out-of-scope-permanent.md` (**fix**)
- `crates/chat/src/knowledge/dataset/{model.rs,validate.rs}` (format contract; D1 loophole)
- `knowledge/data-scope/reporting-scope.yaml`, `knowledge/datasets/**`, `knowledge/domains/**`
