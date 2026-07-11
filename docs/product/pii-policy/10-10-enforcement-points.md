# Reporting PII Policy: 10. Enforcement Points

Source: `docs-old/reporting-pii-policy.md`

## 10. Enforcement Points

PII policy must be enforced in these layers:

- Capability registry: declares allowed output fields and PII class per field.
- Policy guard: checks `can_view_pii`, `allowed_capabilities`, and output contract before execution.
- Query layer: selects only allowed columns.
- Response shaping: masks or omits fields according to capability contract.
- Error handling: never includes raw SQL, raw parser details, secrets, prompts, or internal payloads.
- Tracing/logging: never logs PII payloads or secrets; log ids/counts/status instead.

Preferred implementation rule:

- Do not fetch fields that will be omitted. Select only the fields allowed by the capability and caller policy.
