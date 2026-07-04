# Capability Coverage Matrix

Single source of truth for **what user questions this service commits to answering, what runs today, what is on the roadmap, and what will never be built**. All other docs link here rather than restate scope.

Scope is not "what's implemented today". Scope is **the full agreed reporting-data surface** — every reasonable admin decision-support question over the in-scope Fineract data areas (savings, client, organization, and group/center foundations). Some capabilities are enabled at runtime today; the rest are `planned` and land on a milestone; deferred domains (loan, accounting, tax, audit, custom-datatables) are in-scope for the product vision but not yet activated. If a real admin question doesn't fit anywhere in this matrix, the matrix is wrong — file a row.

Update this file first when adding, deferring, or removing a capability. The runtime YAML in `knowledge/capabilities/**/*.yaml` is authoritative for what actually executes; this matrix is authoritative for what we *say* about scope.

## Status legend

| Value | Meaning |
| --- | --- |
| `implemented` | A capability YAML with runtime status `approved_mvp` exists in `knowledge/capabilities/` and is executable end-to-end. Cell links the capability id. Doc-facing term: **enabled capability**. |
| `planned` | On the roadmap. No approved capability YAML yet. Classifier semantically matches user intent and the job ends `planned_unimplemented` (see mapping below). Target milestone in parentheses. |
| `deferred` | The whole data area or domain is deferred (loan, accounting, tax, custom-datatables, audit-users-operations). In-scope for the product commitment but not yet activated; requires domain-level approval, not just a code change. |
| `out_of_scope` | Will never be built even when asked (writes, arbitrary SQL, raw account numbers, cross-tenant reads, schema exploration). Reason must be documented. |
| `—` | Combination is not meaningful (e.g. a snapshot has no monthly breakdown). |

## Column variants

Every row is expressed against the following query shapes. A cell describes the state of *this row × this shape*.

| Column | Meaning |
| --- | --- |
| Snapshot | "Right now" answer with no date bounds. |
| Aggregate total | Single number over a bounded date range. |
| Aggregate by day | One row per day in the range. |
| Aggregate by week | One row per ISO week. |
| Aggregate by month | One row per calendar month. |
| Aggregate by N-day bucket | Custom fixed-width bucket, parameter `bucket_days`. |
| Aggregate by quarter | One row per calendar quarter (roadmap; alias of `bucket=quarter`). |
| Top-N transactions | Ranked list across the range. |
| Top-N per month / week / day | Ranked list per bucket. |
| Individual row list | Paginated detail rows. |
| Composite | Multiple metrics in one response (planner returns `Vec<ExecutionPlan>`). |
| Comparative | Period-over-period delta. |
| Ranking (top-N entities) | Top-N offices / products / staff over a metric. |

Cell values use the status legend above. When a cell is `implemented`, the parenthesised id maps to a capability YAML in `knowledge/capabilities/`. When `planned`, the parenthesised id is the working name and may not yet exist — reference it via `<planned: id>` until the YAML lands.

---

## A. Savings — balance & lifecycle

Admin decisions supported here: portfolio health snapshots (who has how much where), account inventory (active vs closed, product mix), and lifecycle churn (openings and closures in a period). These answer "what does the book look like today" and "how did it change".

| Row | Snapshot | Aggregate total | By month | By week | By day | By quarter | Top-N | Ranking (offices/products/staff) | Comparative |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Portfolio balance | `implemented` (`savings_balance_summary`) | — | — | — | — | — | — | `planned` (v0.3) | `planned` (v0.3) |
| Balance per office | `planned` (v0.2) | — | — | — | — | — | — | `planned` (v0.3) | `planned` (v0.3) |
| Balance per product | `planned` (v0.2) | — | — | — | — | — | — | `planned` (v0.3) | `planned` (v0.3) |
| Account count active vs closed | `planned` (v0.2) | — | — | — | — | — | — | `planned` (v0.3) | `planned` (v0.3) |
| Accounts opened in period | — | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | — | `planned` (v0.4) | `planned` (v0.4) |
| Accounts closed in period | — | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | — | `planned` (v0.4) | `planned` (v0.4) |
| Dormant accounts | `planned` (v0.3) | — | — | — | — | — | `planned` (v0.4) | `planned` (v0.4) | — |
| Accounts by product | `planned` (v0.2) | — | — | — | — | — | — | `planned` (v0.3) | — |
| Accounts by savings officer / staff | `planned` (v0.3) | — | — | — | — | — | — | `planned` (v0.4) | — |

## B. Savings — transactions

Admin decisions supported here: cashflow direction and volume, throughput of the branch network, exceptional individual movements, and reversal risk. These are the questions asked most often on operating dashboards.

| Row | Aggregate total | By month | By week | By day | By N-day bucket | Top-N transactions | Top-N per month | Top-N per week | Individual list | Composite | Ranking (offices/products/staff) | Comparative |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Deposit | `implemented` (`savings_deposit_total`) | `implemented` (`savings_deposit_monthly_breakdown`) | `planned` (v0.2 — `<planned: savings_deposit_breakdown> bucket=week`) | `planned` (v0.2 — `bucket=day`) | `planned` (v0.3 — `bucket=N_days`) | `implemented` (`savings_deposit_top_n`) | `implemented` (`savings_deposit_monthly_top_n`) | `planned` (v0.2) | `planned` (v0.4 — `<planned: savings_activity_list>`) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) |
| Withdrawal | `implemented` (`savings_withdrawal_total`) | `implemented` (`savings_withdrawal_monthly_breakdown`) | `planned` (v0.2) | `planned` (v0.2) | `planned` (v0.3) | `implemented` (`savings_withdrawal_top_n`) | `implemented` (`savings_withdrawal_monthly_top_n`) | `planned` (v0.2) | `planned` (v0.4) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) |
| Deposit + withdrawal side by side | `planned` (v0.3 — composite planner) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.4) | — | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.3) |
| Net movement (deposit − withdrawal) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | — | — | — | — | — | `planned` (v0.4) | `planned` (v0.3) |
| Transaction count | `planned` (v0.2) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | — | — | — | — | — | `planned` (v0.4) | `planned` (v0.3) |
| Reversed transaction count | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.4) | — | — | — | — | `planned` (v0.4) | `planned` (v0.4) |
| Individual transaction list | — | — | — | — | — | — | — | — | `planned` (v0.4 — `<planned: savings_activity_list>`; new `list` output_mode + row-level PII gate) | — | — | — |
| Top-N per office / product | — | — | — | — | — | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.4) | — | — | `planned` (v0.3) | — |
| Top-N per custom bucket (bucket_days) | — | — | — | — | — | `planned` (v0.3) | — | — | — | — | — | — |

## C. Savings — interest & fees

Admin decisions supported here: revenue side (interest expense and charge income), collection health (outstanding charges), and encumbrance visibility (holds and their history). These are financial-controller questions.

| Row | Snapshot | Aggregate total | By bucket (day/week/month/N_days) | Top-N | Individual list | Ranking (offices/products) | Comparative |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Interest posting total per period | — | `planned` (v0.3) | `planned` (v0.3 — `<planned: savings_interest_posting_breakdown>`) | `planned` (v0.4) | `planned` (v0.4) | `planned` (v0.4) | `planned` (v0.4) |
| Interest posting per account (top-N) | — | — | — | `planned` (v0.4) | `planned` (v0.4) | — | — |
| Charge assessed total | — | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | — | `planned` (v0.4) | `planned` (v0.4) |
| Charge paid total | — | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | — | `planned` (v0.4) | `planned` (v0.4) |
| Charge waived total | — | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | — | `planned` (v0.4) | — |
| Charge outstanding (per client, per office) | `planned` (v0.2 — `<planned: savings_charge_outstanding_summary>`) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.4) | `planned` (v0.4) | `planned` (v0.4) |
| Hold amount active (per client, per office) | `planned` (v0.3 — `<planned: savings_hold_balance_summary>`) | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.4) | `planned` (v0.4) | `planned` (v0.4) | — |
| Hold release history | — | `planned` (v0.4) | `planned` (v0.4) | — | `planned` (v0.4) | — | — |

## D. Client foundation

Admin decisions supported here: portfolio composition by demographic, onboarding trend, staff/office attribution, and outreach gaps. These are relationship-manager questions.

| Row | Snapshot | Aggregate by bucket | Top-N | Ranking (offices/staff) | Comparative |
| --- | --- | --- | --- | --- | --- |
| Client count active / closed / pending | `planned` (v0.2 — `<planned: client_status_summary>`) | `planned` (v0.3) | — | `planned` (v0.3) | `planned` (v0.3) |
| Client onboarding trend | — | `planned` (v0.3 — `<planned: client_onboarding_breakdown>` — day/week/month/N_days) | — | `planned` (v0.4) | `planned` (v0.4) |
| Clients by office / staff | `planned` (v0.2) | — | — | `planned` (v0.3) | — |
| Client demographics (age band, gender, employment) | `planned` (v0.2 — `<planned: client_demographics_summary>`; aggregate only, PII-gated for row-level) | `planned` (v0.3) | — | `planned` (v0.4) | `planned` (v0.4) |
| Clients with no active account | `planned` (v0.3) | — | — | `planned` (v0.4) | — |
| Clients with multiple accounts | `planned` (v0.3) | — | `planned` (v0.4) | `planned` (v0.4) | — |

## E. Organization foundation

Admin decisions supported here: branch inventory, hierarchy for consolidation reads, staff attribution, cross-branch performance, and productivity by office / staff. Foundation lookups are prerequisites for meaningful office-level filtering, and organization-level rankings are the top-of-funnel questions in management review meetings.

| Row | Snapshot | Aggregate by bucket | Top-N | Individual list | Ranking (offices/staff) | Comparative |
| --- | --- | --- | --- | --- | --- | --- |
| Office directory | `implemented` (`organization_office_summary`) | — | — | `planned` (v0.2 — `<planned: office_list>`; flat list scoped to caller) | — | — |
| Office hierarchy tree | `planned` (v0.2 — `<planned: office_hierarchy>`) | — | — | `planned` (v0.2) | — | — |
| Staff directory (aggregate; PII-gated for row-level) | `planned` (v0.3 — `<planned: staff_directory>`) | — | — | `planned` (v0.4 — row-level PII gate) | — | — |
| Staff assignment history summary | `planned` (v0.3) | `planned` (v0.4) | — | `planned` (v0.4) | — | — |
| Staff count per office | `planned` (v0.3) | — | — | — | `planned` (v0.4) | — |
| Office activity summary (portfolio + transactions rolled up) | `planned` (v0.3 — `<planned: office_performance_summary>`) | `planned` (v0.3) | — | — | `planned` (v0.3) | `planned` (v0.4) |
| Office ranking by portfolio balance | — | — | `planned` (v0.3) | — | `planned` (v0.3) | `planned` (v0.4) |
| Office ranking by deposit volume | — | — | `planned` (v0.3) | — | `planned` (v0.3) | `planned` (v0.4) |
| Office ranking by client onboarding | — | — | `planned` (v0.4) | — | `planned` (v0.4) | `planned` (v0.4) |
| Staff ranking by loan officer portfolio (loan-domain gated) | — | — | `deferred` | — | `deferred` | `deferred` |

## F. Group / center foundation (conditional — enabled per deployment)

Admin decisions supported here: for Grameen-style group-lending deployments, group inventory, membership, and group-owned portfolios. This section is only active when the tenant enables `group_center_foundation` and the API key holds a group-scoped capability.

| Row | Snapshot | Aggregate | Ranking |
| --- | --- | --- | --- |
| Group directory | `planned` (v0.3, conditional) | — | — |
| Group membership counts | `planned` (v0.3, conditional) | — | `planned` (v0.4, conditional) |
| Group-owned savings portfolio | `planned` (v0.3, conditional) | `planned` (v0.4, conditional) | `planned` (v0.4, conditional) |
| Group activity summary | `planned` (v0.4, conditional) | `planned` (v0.4, conditional) | `planned` (v0.4, conditional) |

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

## H. Cross-cutting composite

Admin decisions supported here: real user turns bundle metrics. "Show me the biggest deposit, biggest withdrawal, biggest charge, and biggest hold this month" is one question, not four turns.

| Row | Aggregate | By bucket | Top-N | Ranking | Comparative |
| --- | --- | --- | --- | --- | --- |
| Multi-metric one request (deposit + withdrawal + charge + hold) | `planned` (v0.3 — planner returns `Vec<ExecutionPlan>`; see `docs/ai-reporting-design.md` §18.2) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.4) |
| Multi-domain one request (savings + group + office) | `planned` (v0.4) | `planned` (v0.4) | — | `planned` (v0.4) | `planned` (v0.4) |
| Comparative period-over-period (this period vs previous) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.4) | — |
| Ranking top-N offices / staff / products across a metric | — | — | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) |

## Rows always out of scope regardless of column

- Arbitrary SQL or schema exploration exposed to end users.
- Raw account numbers, external ids, payment references, tokens, secrets, or any field marked `secret_never_expose` — even with `can_view_pii=true`.
- Write operations (INSERT/UPDATE/DELETE/DDL/COPY) against Fineract.
- Cross-tenant reads (offices outside the API key `allowed_office_ids`) — enforced *inside* SQL, not as Rust post-filter.
- Reproducing raw AI planner output, prompts, or internal command JSON in user responses.
- Model training or fine-tuning over Fineract data.
- Bulk table export or CDC feeds.
- Individual staff app-user account, credential, session, or audit records — separately governed at the Fineract layer.

## Milestone map

| Milestone | Focus | Representative rows moving to `implemented` |
| --- | --- | --- |
| **v0.1 (current)** | Savings deposits + withdrawals aggregate/breakdown/top-N, portfolio balance snapshot | Nine `implemented` cells across Category A + B. |
| **v0.2** | Bucket-parametric breakdowns (week/day), foundation snapshots, first frontier activation | `savings_deposit_breakdown`, `savings_withdrawal_breakdown` (bucket=day/week), `savings_charge_outstanding_summary`, `client_status_summary`, `client_demographics_summary`, `office_directory`, `office_hierarchy`, balance per office / per product. |
| **v0.3** | Composite planner, comparative, ranking, per-bucket top-N, activity breakdown, hold snapshot | Composite `ExecutionPlanBatch`, comparative period-over-period, `savings_activity_breakdown`, `savings_hold_balance_summary`, dormant accounts, office performance, client onboarding breakdown. |
| **v0.4** | Individual-list output_mode, quarter bucket, cross-domain composites, interest & charge detail | `savings_activity_list` with `list` output_mode + row-level PII gate, quarterly and N-day buckets, staff and group activity, interest posting detail. |
| **v1.0** | Full savings + client + organization + group scope operational; one deferred domain (loan) begins activation | Group/center foundation rows go from `conditional planned` to `conditional implemented`; loan domain enters `frontier`. |
| **Backlog** | Loan, accounting/GL, tax, audit, custom-datatables activation | Category G becomes empty as domains one-by-one move out. |

## How outcomes map at runtime

The classifier and planner emit one of four terminal outcomes. The coverage-matrix status drives the mapping.

| Matrix status | Classifier outcome | Job terminal status | User-facing template |
| --- | --- | --- | --- |
| `implemented` | `Matched` (single) or `CompositeMatched` (batch) | `completed` | Report renders normally per output_mode. |
| `planned` | `PlannedUnimplemented` | `planned_unimplemented` | Sanitised: "This report is planned but not yet available in this release. Expected in {target_milestone}." No SQL runs. |
| `deferred` | `Unsupported` with reason `deferred_domain` | `unsupported` | Sanitised: "That data area is not yet enabled." |
| `out_of_scope` | `Unsupported` with reason `hard_reject` | `unsupported` | Sanitised: "That request is not supported." |
| — (nonsense combination) | `ClarificationRequired` | `awaiting_clarification` | Structured clarification prompt. |

`PlannedUnimplemented` is the fourth outcome; the runtime today has `Matched | ClarificationRequired | Unsupported`. See `docs/ai-reporting-design.md` §18.3 for the design.

## Adding a row or column

Adding a capability is a documentation-and-catalog change, not just code. Follow the three-phase guide in [`docs/knowledge-catalog.md` §14 Adding A Capability](./knowledge-catalog.md#14-adding-a-capability):

- **Phase A — Design:** flip a `planned` cell or add a new row here; write down expected inputs, expected outputs, PII contract.
- **Phase B — Implement:** author the capability YAML, SQL, query metadata, retrieval enrichment; run `POST /catalog/validate`; run `POST /vector-index/rebuild`.
- **Phase C — Verify:** contract-test fixture entry, scenario doc, integration test.

Adding a **column** (a new query shape) requires the same three phases applied to each row that gains an implementation, plus a new `output_mode` entry in `docs/ai-reporting-design.md` §6 if the shape needs one.

## Contract test policy

Every row in the matrix must have at least one contract-test entry in `crates/chat/tests/fixtures/prompts.yaml` — one prompt per row × column cell that is `implemented`, plus one prompt per row that is `planned` (asserting `planned_unimplemented`), plus one prompt per `deferred` row (asserting `unsupported/deferred_domain`).

> Follow-up implementation task (not a doc task): the fixtures file `crates/chat/tests/fixtures/prompts.yaml` does not exist yet. Track its creation as a Rust-side milestone.

## Cross-references

- Runtime capabilities: `knowledge/capabilities/**/*.yaml`
- Approved SQL: `queries/**/*.sql`
- PII behavior per capability: [`docs/reporting-pii-policy.md`](./reporting-pii-policy.md)
- Data area status: [`docs/reporting-data-scope.md`](./reporting-data-scope.md)
- Architecture and outcomes: [`docs/ai-reporting-design.md`](./ai-reporting-design.md)
- Capability contract details: [`docs/reporting-capabilities.md`](./reporting-capabilities.md)
- Non-goals: [`docs/reporting-capabilities.md` §12](./reporting-capabilities.md)
