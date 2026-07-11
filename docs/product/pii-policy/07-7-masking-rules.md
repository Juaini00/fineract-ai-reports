# Reporting PII Policy: 7. Masking Rules

Source: `docs-old/reporting-pii-policy.md`

## 7. Masking Rules

Use omission by default. Masking is allowed only when the capability's output contract includes a masked field.

Suggested masking formats:

| Field type | Masking format |
| --- | --- |
| Person name | First character plus `***`, or stable label such as `Client #123`. |
| Mobile number | Last 2-4 digits only, for example `******1234`. |
| Email | First character and domain only, for example `a***@example.com`. |
| Account/reference number | Last 4 digits only, for example `****1234`. |
| External id | Omit unless explicitly approved; if masked, last 4 characters only. |
| Date of birth | Omit; age band only if an approved capability defines it. |
| Address | Omit; area/office-level aggregate only unless address reporting is approved. |

Do not invent masked values. If the source value is missing, return `null` or omit the field according to the output contract.
