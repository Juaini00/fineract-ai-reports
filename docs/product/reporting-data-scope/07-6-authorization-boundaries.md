# Reporting Data Scope: 6. Authorization Boundaries

Source: `docs-old/reporting-data-scope.md`

## 6. Authorization Boundaries

Every reporting capability must be checked against API key scope.

Required authorization dimensions:

- Allowed capabilities.
- Allowed office ids.
- PII visibility through `can_view_pii`.

Office rules:

- Office filtering must apply to client/account/report queries.
- A caller should not be able to bypass `allowed_office_ids` through user-provided filters.
- Office hierarchy behavior must be defined before allowing parent-office rollups.

PII rules:

- Aggregates should be preferred by default.
- Client-level rows require `can_view_pii=true` if they include identifying fields.
- If `can_view_pii=false`, names and contact fields must be omitted or masked.
