# Reporting PII Policy: 11. AI Prompt Safety

Source: `docs-old/reporting-pii-policy.md`

## 11. AI Prompt Safety

The configured LLM provider must not receive:

- Raw PII unless the user is authorized and the prompt path explicitly requires it.
- Secrets or credentials under any condition.
- Raw command JSON/results.
- Payment references or account numbers unless explicitly approved and masked.

AI planning/formatting receives:

- Capability id.
- Sanitized parameters.
- Aggregate result values.
- Non-PII labels such as office/product/currency.
