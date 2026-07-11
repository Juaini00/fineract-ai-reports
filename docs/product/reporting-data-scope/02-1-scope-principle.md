# Reporting Data Scope: 1. Scope Principle

Source: `docs-old/reporting-data-scope.md`

## 1. Scope Principle

The service must not treat the full Fineract database as available reporting context.

Only explicitly approved data areas may be used.

Rules:

- Read from Fineract through `FINERACT_DATABASE_URL` only.
- Do not modify Fineract data or schema.
- Do not let AI generate or execute arbitrary SQL.
- Runtime queries must come from approved reporting capabilities.
- Each approved capability must declare its allowed tables, joins, filters, metrics, and PII behavior.
- If a user asks for data outside the approved scope, reject or ask for clarification.
- Every approved, conditional, deferred, and out-of-scope data area must have a matching machine-readable entry under `knowledge/data-scope/` before catalog validation is considered complete.
- Knowledge files must not introduce runtime access to tables, columns, joins, metrics, or response fields outside this scope.
