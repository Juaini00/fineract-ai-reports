# Reporting Capabilities: 10. Implementation Notes

Source: `docs-old/reporting-capabilities.md`

## 10. Implementation Notes

The implementation uses static approved SQL bindings in Rust, with each executable capability mapped through catalog metadata to one reviewed query file and output contract.

Current static approved SQL bindings:

```text
savings_deposit_total -> queries/savings/deposit_total.sql
savings_deposit_top_n -> queries/savings/deposit_top_n.sql
savings_withdrawal_total -> queries/savings/withdrawal_total.sql
savings_withdrawal_top_n -> queries/savings/withdrawal_top_n.sql
savings_deposit_monthly_breakdown -> queries/savings/deposit_monthly_breakdown.sql
savings_deposit_monthly_top_n -> queries/savings/deposit_monthly_top_n.sql
savings_withdrawal_monthly_breakdown -> queries/savings/withdrawal_monthly_breakdown.sql
savings_withdrawal_monthly_top_n -> queries/savings/withdrawal_monthly_top_n.sql
savings_balance_summary -> queries/savings/balance_summary.sql
```

The classifier should only emit capability ids that exist in the registry. The policy guard should validate capability scope, parameters, office scope, PII behavior, and limits before SQL execution.

Note on numbering: earlier drafts of this document appended a "Classification thresholds" section that duplicated the §9 heading. That content has been folded into `docs/ai-reporting-design.md` §8 Decision Policy where thresholds are documented alongside the outcome mapping. Section numbers in this file are contiguous 1–12.
