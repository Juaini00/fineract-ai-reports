# Reporting PII Policy: 12. Review Checklist For New Capabilities

Source: `docs-old/reporting-pii-policy.md`

## 12. Review Checklist For New Capabilities

Before adding a new capability, answer:

- Does the capability require row-level output?
- Which fields are PII, sensitive business identifiers, security sensitive, or free text?
- Can the report be answered as an aggregate instead?
- Does it require `can_view_pii=true`?
- Which fields must be masked or omitted when `can_view_pii=false`?
- Are any fields always excluded?
- Does the SQL select only allowed fields?
- Are logs and errors sanitized?
- Does the capability respect `allowed_office_ids`?
