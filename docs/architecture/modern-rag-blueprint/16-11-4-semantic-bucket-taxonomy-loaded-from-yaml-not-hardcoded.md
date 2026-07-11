# Modern RAG Architecture Blueprint: 11.4 Semantic bucket taxonomy (loaded from YAML, not hardcoded)

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

## 11.4 Semantic bucket taxonomy (loaded from YAML, not hardcoded)

Config: `knowledge/domain/savings-transaction-types.yaml`

``` yaml
id: savings_transaction_type_enum
source_doc: Apache Fineract m_savings_account_transaction.transaction_type_enum
buckets:
  deposits:
    enums: [1]
    label: "Deposits"
  withdrawals:
    enums: [2]
    label: "Withdrawals"
  charges_paid:
    enums: [4, 5, 17]
    label: "Charges paid"
  interest_and_dividends:
    enums: [3, 8]
    label: "Interest and dividends"
  holds:
    enums: [20, 21]
    label: "Holds"
fallback:
  id: other
  label: "Other activity"
```

- Executor SQL keeps returning `transaction_type_enum` int + a
  best-effort label; the formatter does the bucketing at render time
  by consulting this YAML.
- New Fineract enum value → add to YAML, no code change, no redeploy.
- `other` is the fallback for enums not listed. It is NOT a place we
  dump things we know but couldn't be bothered to label.
