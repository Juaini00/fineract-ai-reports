# Reporting Data Scope: 0. Status Legend And Area Index

Source: `docs-old/reporting-data-scope.md`

## 0. Status Legend And Area Index

Each of the 13 data areas below is tagged with a lifecycle status. Reviewers should update the tag on the area's subsection heading whenever the coverage matrix moves a related capability. Tags are the same across this doc, the runtime `knowledge/data-scope/areas/*.yaml`, and [`docs/capability-coverage-matrix.md`](../capability-coverage/index.md).

| Status | Meaning |
| --- | --- |
| `in_use` | At least one enabled (runtime `approved_mvp`) capability queries this area today. Tables here are validated as safe to widen incrementally. |
| `frontier` | Not queried today, but the next `planned` capability will use it. Activation criteria below must be met before flipping to `in_use`. |
| `conditional` | Queryable but only under a narrower feature flag (e.g. group/center scope only when the tenant enables group workflows). |
| `deferred` | Whole area is deferred. No capability may reference it. Requires domain-level approval, not just a code change, to activate. |
| `rejected` | Permanent out-of-scope. Even future roadmap items will not be built against these tables. |

Every in-scope area listed below is a commitment — capabilities that use it are either `implemented` today, `planned` on the roadmap, or `deferred` (activated at a domain-level approval). Area index with current tags (four areas `in_use` today; every other in-scope area has planned capabilities):

| # | Area | Status | Approved capabilities that touch it |
| --- | --- | --- | --- |
| 3.1 | Organization Foundation | `in_use` | All 9 approved capabilities (office join for scope) |
| 3.2 | Client Foundation | `in_use` | All top-N capabilities (client display join) |
| 3.3 | Group And Center Foundation | `conditional` | none today; enabled per-tenant |
| 3.4 | Savings Core | `in_use` | `savings_balance_summary` plus every deposit/withdrawal capability |
| 3.5 | Savings Transactions | `in_use` | All deposit/withdrawal capabilities |
| 3.6 | Savings Charges And Fees | `frontier` | none today; targets `savings_charge_outstanding_*` (v0.2 – v0.3) |
| 4.1 | Loans | `deferred` | — |
| 4.2 | Accounting And General Ledger | `deferred` | — |
| 4.3 | Tax | `deferred` | — |
| 4.4 | Custom Datatables | `deferred` | — |
| 4.5 | Audit, Users, And Operations | `deferred` | — |
| 5   | Explicitly Out Of Scope | `rejected` | — |

Activation criteria for `frontier` and `conditional` areas — what must be true before flipping to `in_use`:

- **Savings Charges And Fees (`frontier` → `in_use`).** Charge enum mapping (`m_charge.charge_type_enum`, `m_charge.charge_calculation_enum`) must be documented in a schema knowledge file. The `m_savings_account_charge.amount_outstanding_derived` semantics must be reviewed to confirm it is a running balance we can trust across time zones. Sensitivity class of the charge reference identifier must be assigned in `docs/reporting-pii-policy.md` (currently reserved as `sensitive_business_identifier`).
- **Group And Center Foundation (`conditional` remains `conditional`).** Only queryable when the API key `allowed_capabilities` set includes a group-scoped capability AND the tenant is configured for group workflows. The group office path (`m_group.office_id`) is validated separately from the client office path in the same query.
- **Any `deferred` area (`deferred` → `frontier`).** A capability YAML must be authored under `knowledge/capabilities/<domain>/`, the coverage matrix must gain an `implemented` entry (row flipped from `deferred`), the domain YAML under `knowledge/domains/` must be moved from deferred to approved, and a PII rule review under `docs/reporting-pii-policy.md` must sign off on every field the new capability will output.
