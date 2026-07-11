# Reporting PII Policy: 2. Sensitivity Classes

Source: `docs-old/reporting-pii-policy.md`

## 2. Sensitivity Classes

| Class | Meaning | Default behavior |
| --- | --- | --- |
| `public_business` | Non-personal business dimension suitable for normal reports. | May be returned if capability allows it. |
| `sensitive_business_identifier` | Account numbers, external ids, references, and internal identifiers. | Exclude by default; return only through explicit capability approval. |
| `pii` | Personal identity or contact data. | Require `can_view_pii=true` and explicit capability approval. |
| `security_sensitive` | User roles, permissions, IPs, audit/security state. | Exclude by default; require explicit operational/security capability. |
| `secret_never_expose` | Passwords, tokens, temporary credentials, raw command JSON/results. | Never return, log, or send to AI. |
| `free_text_sensitive` | Free text that may contain PII or operational notes. | Exclude unless explicitly reviewed and approved. |
