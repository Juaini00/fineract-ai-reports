# Reporting PII Policy: 9. Per-Capability Application (currently implemented)

Source: `docs-old/reporting-pii-policy.md`

## 9. Per-Capability Application (currently implemented)

### 9.1 `savings_deposit_total`

PII behavior:

- Does not require `can_view_pii=true`.
- Must not return client names, account numbers, external ids, payment references, or app user fields.
- Returns aggregate metrics only.

### 9.2 `savings_deposit_top_n`

PII behavior:

- Without `can_view_pii`, return transaction amount/date/currency/office/product only.
- With `can_view_pii`, may return `client_id` and `client_display_name` only if the capability output contract includes them.
- Must still exclude account numbers, external ids, transaction references, payment references, and app user fields.
