# Reporting PII Policy: 1. Core Rules

Source: `docs-old/reporting-pii-policy.md`

## 1. Core Rules

- Prefer aggregate reporting by default.
- Only return row-level identity fields when the selected capability explicitly allows them.
- `can_view_pii=true` is necessary but not sufficient: the capability must also allow the specific field.
- `can_view_pii=false` means client/user/staff identity fields must be omitted or masked.
- Some fields are never returned, even when `can_view_pii=true`.
- Logs, prompts, traces, errors, and audit events must not contain raw secrets or raw command payloads.
