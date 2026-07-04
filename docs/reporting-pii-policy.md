# Reporting PII Policy

This document defines how the AI Reporting Service handles PII, sensitive business identifiers, and secrets in Fineract reporting responses.

The policy applies to every protected reporting endpoint, every chat/report response, and every reporting capability — currently implemented or planned. PII gating is orthogonal to capability status: a `planned` capability that will return PII must declare its PII contract before implementation begins.

## 1. Core Rules

- Prefer aggregate reporting by default.
- Only return row-level identity fields when the selected capability explicitly allows them.
- `can_view_pii=true` is necessary but not sufficient: the capability must also allow the specific field.
- `can_view_pii=false` means client/user/staff identity fields must be omitted or masked.
- Some fields are never returned, even when `can_view_pii=true`.
- Logs, prompts, traces, errors, and audit events must not contain raw secrets or raw command payloads.

## 2. Sensitivity Classes

| Class | Meaning | Default behavior |
| --- | --- | --- |
| `public_business` | Non-personal business dimension suitable for normal reports. | May be returned if capability allows it. |
| `sensitive_business_identifier` | Account numbers, external ids, references, and internal identifiers. | Exclude by default; return only through explicit capability approval. |
| `pii` | Personal identity or contact data. | Require `can_view_pii=true` and explicit capability approval. |
| `security_sensitive` | User roles, permissions, IPs, audit/security state. | Exclude by default; require explicit operational/security capability. |
| `secret_never_expose` | Passwords, tokens, temporary credentials, raw command JSON/results. | Never return, log, or send to AI. |
| `free_text_sensitive` | Free text that may contain PII or operational notes. | Exclude unless explicitly reviewed and approved. |

## 3. Always Excluded Fields

These must never be returned to API clients or sent to AI prompts.

Credential and token fields:

- `m_appuser.password`.
- `m_appuser.temporary_password`.
- `request_audit_table.password`.
- `request_audit_table.authentication_token`.

Raw command/request fields:

- `m_portfolio_command_source.command_as_json`.
- `m_portfolio_command_source.result`.
- `m_portfolio_command_source.idempotency_key`.

Sensitive payment/reference fields unless a future capability explicitly approves masked display:

- `m_payment_detail.account_number`.
- `m_payment_detail.check_number`.
- `m_payment_detail.receipt_number`.
- `m_payment_detail.bank_number`.
- `m_payment_detail.routing_code`.

Sensitive free-text fields excluded from every currently implemented and planned capability:

- `m_savings_account.reason_for_block`.
- `m_savings_account_transaction.reason_for_block`.

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

## 5. Sensitive Business Identifiers

These are not always personal data, but they can identify customers, accounts, transactions, or internal records. Exclude from default output.

Examples:

- `m_client.account_no`.
- `m_client.external_id`.
- `m_savings_account.account_no`.
- `m_savings_account.external_id`.
- `m_savings_account.iban`.
- `m_savings_account_transaction.external_id`.
- `m_savings_account_transaction.ref_no`.
- `m_group.account_no`.
- `m_group.external_id`.
- `m_office.external_id`.
- `m_staff.external_id`.
- Loan `account_no` and `external_id`, when loan scope is later approved.

Default rule:

- Do not return these fields in any currently implemented or planned capability without explicit sensitivity re-classification.
- Use internal numeric ids only where required for traceability and only if the capability declares them.

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

## 8. Behavior By API Key

### 8.1 `can_view_pii=false`

Allowed:

- Aggregate totals.
- Counts.
- Office/product/currency dimensions.
- Non-identifying numeric ids only if declared by the capability.
- Masked identity fields only if declared by the capability.

Not allowed:

- Client names.
- Staff names.
- App user names.
- Email addresses.
- Phone numbers.
- Account numbers.
- External ids.
- Payment references.
- Raw free text.

### 8.2 `can_view_pii=true`

Allowed only when declared by the selected capability:

- Client display names.
- Staff display names.
- App user display names.
- Selected row-level identifying fields.

Still not allowed:

- Passwords.
- Tokens.
- Temporary credentials.
- Raw command JSON.
- Raw command results.
- Idempotency keys.
- Unapproved payment references.
- Unapproved account/external ids.

## 9. Per-Capability Application (currently implemented)

### 9.1 `savings_deposit_total`

PII behavior:

- Does not require `can_view_pii=true`.
- Must not return client names, account numbers, external ids, payment references, or app user fields.
- Returns aggregate metrics only.

### 9.2 `savings_deposit_top_n`

PII behavior:

- Without `can_view_pii`, return transaction amount/date/currency/office/product only.
- With `can_view_pii`, may return `client_id` and `client_display_name` only if the capability output contract includes them.
- Must still exclude account numbers, external ids, transaction references, payment references, and app user fields.

## 10. Enforcement Points

PII policy must be enforced in these layers:

- Capability registry: declares allowed output fields and PII class per field.
- Policy guard: checks `can_view_pii`, `allowed_capabilities`, and output contract before execution.
- Query layer: selects only allowed columns.
- Response shaping: masks or omits fields according to capability contract.
- Error handling: never includes raw SQL, raw parser details, secrets, prompts, or internal payloads.
- Tracing/logging: never logs PII payloads or secrets; log ids/counts/status instead.

Preferred implementation rule:

- Do not fetch fields that will be omitted. Select only the fields allowed by the capability and caller policy.

## 11. AI Prompt Safety

The configured LLM provider must not receive:

- Raw PII unless the user is authorized and the prompt path explicitly requires it.
- Secrets or credentials under any condition.
- Raw command JSON/results.
- Payment references or account numbers unless explicitly approved and masked.

AI planning/formatting receives:

- Capability id.
- Sanitized parameters.
- Aggregate result values.
- Non-PII labels such as office/product/currency.

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

## 13. Reserved Sensitivity Classes

Section 2 defines six sensitivity classes. Only two — `public_business` and `pii` — are actively enforced by any currently implemented capability. The other four are **reserved**: documented in advance so that policy design is future-proof, and so that reviewers of new capabilities never have to invent a new class ad hoc.

Reserved classes are not dead code. Every reserved class has a concrete planned capability (see [`docs/capability-coverage-matrix.md`](./capability-coverage-matrix.md)) that will activate it. When those capabilities land, the class moves from "reserved" to "active" and the mask/omit rules already defined in §§3–8 apply without further policy changes.

| Class | Status | Guards which planned capability | What it protects |
| --- | --- | --- | --- |
| `sensitive_business_identifier` | reserved | `savings_activity_list`, `savings_charge_outstanding_breakdown`, `office_directory` when it starts including internal codes | Savings `account_no`, savings `external_id`, transaction `external_id`, product internal codes. Never shown even with `can_view_pii=true`; only aggregate references allowed. |
| `security_sensitive` | reserved | `audit-users-operations` domain (deferred) — password hashes, session tokens, API-key hashes | Never returned in any capability response regardless of PII flag. If a reviewer sees a column of this class in a proposed SELECT list, the capability is rejected. |
| `secret_never_expose` | reserved | Payment reference detail on `savings_activity_list`, hold reference detail on `savings_hold_balance` capabilities | `ref_no`, payment channel account numbers, encrypted payment payloads, third-party gateway ids. Hard exclude — no mask, no summary, no count-only. Present in Fineract, permanently absent from any AI response. |
| `free_text_sensitive` | reserved | `savings_activity_list` (transaction description), `client_demographics_summary` if it includes free-text address/employer notes | User-authored free-text fields that may contain PII, business intent, or slurs. Excluded by default; a future capability may summarize by fixed enum only. |

Rules that apply to reserved classes today, before any planned capability lands:

- The referential-integrity validator flags any capability YAML that declares an output field belonging to a reserved class, unless the capability also references the appropriate planned entry in `docs/capability-coverage-matrix.md`. This prevents accidental early exposure.
- The AI prompt safety rules in §11 already forbid the classifier or planner from suggesting fields it hasn't seen in an `approved_mvp` capability; reserved-class fields are therefore unreachable via prompt injection.
- `POST /catalog/validate` cross-checks the sensitivity class of every declared output field against the capability's status. `approved_mvp` capabilities may not declare reserved-class fields.

Migration path when activating a reserved class:

1. Add or flip the capability to `implemented` in the coverage matrix.
2. Update this section: move the class row into §§4–6 as appropriate, or add a new numbered subsection describing the exact fields now in scope.
3. Update the capability YAML with explicit output field declarations.
4. Update review checklist §12 questions if the class introduces a new gating flag.
