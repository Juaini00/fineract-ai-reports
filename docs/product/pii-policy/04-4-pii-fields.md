# Reporting PII Policy: 4. PII Fields

Source: `docs-old/reporting-pii-policy.md`

## 4. PII Fields

These require both `can_view_pii=true` and explicit capability approval.

Client fields:

- `m_client.firstname`.
- `m_client.middlename`.
- `m_client.lastname`.
- `m_client.fullname`.
- `m_client.display_name`.
- `m_client.mobile_no`.
- `m_client.email_address`.
- `m_client.date_of_birth`.
- Client address fields from `m_client_address`, if later approved.
- Client identifier fields from `m_client_identifier`, if later approved.

Staff fields:

- `m_staff.firstname`.
- `m_staff.lastname`.
- `m_staff.mobile_no`.
- `m_staff.email_address`.

App user fields:

- `m_appuser.username`.
- `m_appuser.firstname`.
- `m_appuser.lastname`.
- `m_appuser.email`.

Custom datatable fields:

- Any person name, national id, mobile number, address, beneficiary, employer, salary, or financial personal field.
- Every custom datatable field must be classified before use.
