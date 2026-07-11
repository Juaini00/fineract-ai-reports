# Knowledge Catalog: 4. Recommended Directory Structure

Source: `docs-old/knowledge-catalog.md`

## 4. Recommended Directory Structure

```text
knowledge/
  data-scope/
    reporting-scope.yaml
    areas/
      organization-foundation.yaml
      client-foundation.yaml
      group-center-foundation.yaml
      savings-core.yaml
      savings-transactions.yaml
      savings-charges-fees.yaml
      deferred.yaml
      out-of-scope.yaml
  domains/
    organization.yaml
    client.yaml
    savings.yaml
  schema/
    fineract/
      organization.yaml
      client.yaml
      savings.yaml
      enums/
        savings_transaction_type.yaml
        savings_account_status.yaml
        client_status.yaml
      joins/
        office_scope.yaml
        client_savings_account.yaml
        group_savings_account.yaml
        savings_transaction_account.yaml
      columns/
        sensitivity.yaml
        excluded.yaml
  metrics/
    savings/
      deposit_amount.yaml
      deposit_count.yaml
      withdrawal_amount.yaml
      account_balance.yaml
  capabilities/
    savings/
      deposit_total.yaml
      deposit_top_n.yaml
  queries/
    savings/
      deposit_total.yaml
      deposit_top_n.yaml
  policies/
    pii.yaml
    query_safety.yaml
    office_scope.yaml
    execution_limits.yaml
    unsupported_requests.yaml
  responses/
    reporting.yaml
    clarification.yaml
    unsupported.yaml

queries/
  savings/
    deposit_total.sql
    deposit_top_n.sql
```

Directory rules:

- `knowledge/data-scope/` mirrors `docs/reporting-data-scope.md`.
- `knowledge/domains/` describes business language and user intent.
- `knowledge/schema/` describes allowed tables, columns, enums, joins, and sensitivity.
- `knowledge/metrics/` describes reusable business calculations.
- `knowledge/capabilities/` declares executable reporting abilities.
- `knowledge/queries/` maps capabilities to approved SQL files.
- `knowledge/policies/` defines cross-cutting enforcement rules.
- `knowledge/responses/` defines safe output and clarification templates.
- `queries/` contains reviewed SQL only.
