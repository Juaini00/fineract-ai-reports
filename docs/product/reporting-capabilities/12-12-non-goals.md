# Reporting Capabilities: 12. Non-Goals

Source: `docs-old/reporting-capabilities.md`

## 12. Non-Goals

The service will never build the following, even when explicitly asked. These are permanent rejections, not deferrals.

- Arbitrary AI-generated SQL execution.
- Schema exploration endpoints (e.g. list all tables, describe columns) exposed to end users.
- Write operations of any kind against Fineract — no INSERT, UPDATE, DELETE, MERGE, DDL, or copy-out.
- Raw account numbers (`m_savings_account.account_no`), external ids, payment references (`ref_no`, `payment_details`), or any field marked `secret_never_expose` in the PII policy — even with `can_view_pii=true`.
- Cross-tenant reads. Office scope broader than the API key's `allowed_office_ids` is always rejected inside SQL bindings.
- Reproducing raw AI planner output, prompts, or internal command JSON in end-user responses.
- Model training or fine-tuning over Fineract data.
- Exports of full Fineract tables regardless of filter (bulk pull is a data-warehouse concern, not a chat concern).
- Streaming diffs of Fineract data or change-data-capture feeds.
- Answering questions about individual staff app-user accounts, credentials, session tokens, or audit records for staff — these are covered by the deferred `audit-users-operations` data area and are separately access-gated at the Fineract layer.

If a user request maps here, the classifier emits `Unsupported` with reason `hard_reject`. The response is a fixed sanitized template. No candidate SQL is produced, no fallback capability is tried.
