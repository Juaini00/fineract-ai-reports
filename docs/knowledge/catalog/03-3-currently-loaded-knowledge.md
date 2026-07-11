# Knowledge Catalog: 3. Currently Loaded Knowledge

Source: `docs-old/knowledge-catalog.md`

## 3. Currently Loaded Knowledge

The knowledge below is currently authored and loaded at runtime. It covers organization, client, and savings foundations plus the deferred domain YAML placeholders. Additional knowledge lands as `planned` capabilities move to `implemented` — see the coverage matrix milestone map.

### 3.1 Data Scope Files (currently loaded)

Initial files:

- [x] `knowledge/data-scope/reporting-scope.yaml`
- [x] `knowledge/data-scope/areas/organization-foundation.yaml`
- [x] `knowledge/data-scope/areas/client-foundation.yaml`
- [x] `knowledge/data-scope/areas/group-center-foundation.yaml`
- [x] `knowledge/data-scope/areas/savings-core.yaml`
- [x] `knowledge/data-scope/areas/savings-transactions.yaml`
- [x] `knowledge/data-scope/areas/savings-charges-fees.yaml`
- [x] `knowledge/data-scope/areas/deferred.yaml`
- [x] `knowledge/data-scope/areas/out-of-scope.yaml`

These files mirror:

- `docs/reporting-data-scope.md`
- `docs/reporting-data/organization-foundation.md`
- `docs/reporting-data/client-foundation.md`
- `docs/reporting-data/group-center-foundation.md`
- `docs/reporting-data/savings-core.md`
- `docs/reporting-data/savings-transactions.md`
- `docs/reporting-data/savings-charges-fees.md`

Required area statuses (runtime YAML enum values kept for backward compatibility; `included_mvp_*` is legacy YAML terminology mirroring the `in_use` matrix status):

| Area | Machine-readable id | Status |
| --- | --- | --- |
| Organization Foundation | `organization_foundation` | `included_mvp_foundation` |
| Client Foundation | `client_foundation` | `included_mvp_foundation` |
| Group And Center Foundation | `group_center_foundation` | `conditional` |
| Savings Core | `savings_core` | `included_mvp_domain` |
| Savings Transactions | `savings_transactions` | `included_mvp_domain` |
| Savings Charges And Fees | `savings_charges_fees` | `secondary` |

Deferred areas must be represented explicitly:

- `loans`
- `accounting_gl`
- `tax`
- `custom_datatables`
- `audit_users_operations`

Out-of-scope areas must be represented explicitly:

- Arbitrary SQL exploration.
- Full Fineract schema search.
- Document/image/file reporting.
- Identity document reporting.
- Address-level reporting.
- Write/update/delete actions against Fineract.

### 3.2 Domain Files

Initial files:

- [x] `knowledge/domains/organization.yaml`
- [x] `knowledge/domains/client.yaml`
- [x] `knowledge/domains/savings.yaml`

Additional deferred/candidate domain context:

- [x] `knowledge/domains/group_center.yaml`
- [x] `knowledge/domains/loan.yaml`
- [x] `knowledge/domains/accounting.yaml`
- [x] `knowledge/domains/tax.yaml`

Purpose:

- `organization`: office hierarchy and office-scoped reporting.
- `client`: client ownership and PII-aware identity context.
- `savings`: savings accounts and savings transactions.

### 3.3 Schema Files

Initial files:

- [x] `knowledge/schema/fineract/organization.yaml`
- [x] `knowledge/schema/fineract/client.yaml`
- [x] `knowledge/schema/fineract/savings.yaml`
- [x] `knowledge/schema/fineract/enums/savings_transaction_type.yaml`
- [x] `knowledge/schema/fineract/enums/savings_account_status.yaml`
- [x] `knowledge/schema/fineract/enums/client_status.yaml`
- [x] `knowledge/schema/fineract/joins/office_scope.yaml`
- [x] `knowledge/schema/fineract/joins/client_savings_account.yaml`
- [x] `knowledge/schema/fineract/joins/group_savings_account.yaml`
- [x] `knowledge/schema/fineract/joins/savings_transaction_account.yaml`
- [x] `knowledge/schema/fineract/columns/sensitivity.yaml`
- [x] `knowledge/schema/fineract/columns/excluded.yaml`

Additional schema context:

- [x] `knowledge/schema/fineract/group_center.yaml`
- [x] `knowledge/schema/fineract/savings_charges_fees.yaml`

Initial table families:

- `m_office`
- `m_staff`, only basic office/staff context if needed
- `m_client`
- `m_group`, conditional
- `m_savings_account`
- `m_savings_product`
- `m_savings_account_transaction`
- `m_charge`, later for savings charge reporting

### 3.4 Metric Files

Initial files:

- [x] `knowledge/metrics/savings/deposit_amount.yaml`
- [x] `knowledge/metrics/savings/deposit_count.yaml`
- [x] `knowledge/metrics/savings/withdrawal_amount.yaml`
- [x] `knowledge/metrics/savings/account_balance.yaml`

### 3.5 Capability Files

Initial files:

- [x] `knowledge/capabilities/savings/deposit_total.yaml`
- [x] `knowledge/capabilities/savings/deposit_top_n.yaml`

Initial approved capabilities:

- `savings_deposit_total`
- `savings_deposit_top_n`

Next likely capabilities:

- `savings_deposit_monthly_breakdown`
- `savings_withdrawal_total`
- `savings_balance_summary`

### 3.6 Query Files

Initial query metadata:

- [x] `knowledge/queries/savings/deposit_total.yaml`
- [x] `knowledge/queries/savings/deposit_top_n.yaml`

Initial SQL files:

- [x] `queries/savings/deposit_total.sql`
- [x] `queries/savings/deposit_top_n.sql`

### 3.7 Policy Files

Initial files:

- [x] `knowledge/policies/pii.yaml`
- [x] `knowledge/policies/query_safety.yaml`
- [x] `knowledge/policies/office_scope.yaml`
- [x] `knowledge/policies/execution_limits.yaml`
- [x] `knowledge/policies/unsupported_requests.yaml`

These files may initially mirror existing docs:

- `docs/reporting-pii-policy.md`
- `docs/reporting-capabilities.md`
- `docs/reporting-data-scope.md`

### 3.8 Response Files

Initial files:

- [x] `knowledge/responses/reporting.yaml`
- [x] `knowledge/responses/clarification.yaml`
- [x] `knowledge/responses/unsupported.yaml`
