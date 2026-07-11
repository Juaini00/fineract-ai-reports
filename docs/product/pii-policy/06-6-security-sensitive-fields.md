# Reporting PII Policy: 6. Security Sensitive Fields

Source: `docs-old/reporting-pii-policy.md`

## 6. Security Sensitive Fields

These require explicit operational/security capabilities and are excluded from every currently implemented and planned business reporting capability.

Examples:

- `m_role.name`.
- `m_permission.code`.
- Role-permission mappings.
- `m_portfolio_command_source.client_ip`.
- `m_appuser.failed_login_attempts`.
- `m_appuser.nonlocked`.
- `m_appuser.password_reset_required`.
- Maker/checker user names.

Default rule:

- Aggregate by user id only if operational reporting is approved.
- Do not display usernames, role names, permission codes, or IP addresses in any currently implemented or planned response.
