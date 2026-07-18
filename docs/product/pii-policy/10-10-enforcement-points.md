# Reporting PII Policy: 10. Enforcement Points

Source: `docs-old/reporting-pii-policy.md`

## 10. Enforcement Points

PII policy must be enforced in these layers:

- Capability registry: declares allowed output fields and PII class per field.
- Policy guard: checks `can_view_pii`, `allowed_capabilities`, and output contract before execution.

> **Current implementation caveat (chat path).** Chat is admin-only and runs an admin projection (`chat::policy::authorization::project_admin_principal`) that grants every `approved_mvp` capability and forces `can_view_pii = true` before the policy guard runs. So on the chat path the guard always resolves to "PII allowed", and `response_builder::is_hidden` never masks `pii`-class columns — the per-API-key `can_view_pii` / `allowed_capabilities` values are advisory there, by design. The rules below describe the intended field-level policy and remain authoritative for capability design and for any non-admin path; they are not currently enforced against an admin chat caller. Making them binding would require attaching scope to the bearer/user identity rather than to the optional API key.
- Query layer: selects only allowed columns.
- Response shaping: masks or omits fields according to capability contract.
- Error handling: never includes raw SQL, raw parser details, secrets, prompts, or internal payloads.
- Tracing/logging: never logs PII payloads or secrets; log ids/counts/status instead.

Preferred implementation rule:

- Do not fetch fields that will be omitted. Select only the fields allowed by the capability and caller policy.
