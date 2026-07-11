# Capability Coverage Matrix: G. Deferred domains

Source: `docs-old/capability-coverage-matrix.md`

## G. Deferred domains

These domains are inside the product commitment but not yet activated. Activation requires: (a) the domain's `knowledge/domains/<domain>.yaml` moved from `deferred` to `approved`, (b) the corresponding `knowledge/data-scope/areas/*.yaml` flipped from `deferred` to `in_use`, (c) at least one PII policy sign-off per output field, (d) at least one runnable capability YAML per shape shipped.

| Domain | Status | Activation requires |
| --- | --- | --- |
| Loan | `deferred` | Loan repayment schedule semantics, arrears / delinquency / overpayment / write-off rules, loan-charge tax linkage confirmed. Then Category A–D-shaped rows re-materialise under this domain. |
| Accounting / GL | `deferred` | Chart-of-accounts export, journal-entry reconciliation rules, product-to-GL mapping validated per tenant. |
| Tax | `deferred` | Per-transaction tax-detail semantics for savings and loans; tax-group evolution over time. |
| Audit, users, operations | `deferred` | Sensitivity classification signoff (roles, permissions, IPs, maker/checker); most fields are `security_sensitive` and stay out of chat answers regardless of activation. |
| Custom datatables | `deferred` | Per-installation column classification; no automatic exposure. Adds one row/column at a time. |

Until activated, every deferred-domain question maps to `Unsupported(deferred_domain)`. Even so, the coverage matrix is the *right* place to describe what those future domains will support — deleting them would signal we do not intend to build them, which is false.
