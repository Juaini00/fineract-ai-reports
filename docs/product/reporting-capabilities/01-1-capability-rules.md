# Reporting Capabilities: 1. Capability Rules

Source: `docs-old/reporting-capabilities.md`

## 1. Capability Rules

Every reporting capability the service commits to must eventually declare:

- Capability id.
- Status.
- User intent it supports.
- Required API key scope.
- Allowed tables and joins.
- Required parameters.
- Optional parameters.
- Default filters.
- Output mode.
- Allowed output fields.
- PII behavior.
- Office authorization behavior.
- Approved query file path.

Runtime rules:

- A user request must map to one approved capability or be rejected/clarified.
- API key `allowed_capabilities` must contain the capability id.
- API key `allowed_office_ids` must be enforced on every Fineract query.
- Date ranges and limits must be validated before query execution.
- Reversed transactions must be excluded by default unless the capability explicitly analyzes reversals.
- Response output must use only declared fields.
- Raw SQL must come from approved query files, not from AI output.
