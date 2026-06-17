# Reporting Data: Organization Foundation

This document contains the detailed table and field scope for Reporting Data Scope section `3.1 Organization Foundation`.

Status: completed for initial review.

## 1. Scope

Organization Foundation is included in the MVP foundation.

Purpose:

- Provide organizational hierarchy and branch/office filters.
- Support authorization by allowed office ids.
- Anchor all client and account reporting to office-level access control.

High-level data concepts:

- Office.
- Parent office.
- Office hierarchy.
- Office opening date.
- Staff assigned to an office.
- Active/inactive staff.

Verified Fineract table family:

- `m_office`.
- `m_staff`.
- `m_staff_assignment_history`, later if center/staff assignment history is needed.

Verified from:

- Fineract changelog: `fineract-provider/src/main/resources/db/changelog/tenant/parts/0001_initial_schema.xml`.
- Fineract domain models: `Office`, `Staff`, `StaffAssignmentHistory`.
- Local database `information_schema.columns` on `fineract_default`.

## 2. `m_office`

Purpose:

- Canonical organization/branch table.
- Used for office filtering, office rollups, and API-key office authorization.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary office id. Use for joins and authorization. |
| `parent_id` | `bigint` | yes | Parent office id. Use for hierarchy traversal. |
| `hierarchy` | `character varying` | yes | Materialized office hierarchy path. Useful for parent-office rollups after hierarchy semantics are approved. |
| `external_id` | `character varying` | yes | External office reference. Treat as business identifier. |
| `name` | `character varying` | no | Office/branch display name. |
| `opening_date` | `date` | no | Office opening date. Useful for filtering offices active by date. |

Primary relationship rules:

- `m_office.parent_id -> m_office.id` for parent/child office hierarchy.
- `m_client.office_id -> m_office.id` for client office ownership.
- `m_staff.office_id -> m_office.id` for staff office assignment.
- `m_savings_account_transaction.office_id -> m_office.id` for transaction office context.

Reporting rules:

- Use `id` as the canonical authorization key for `allowed_office_ids`.
- Use `name` for display.
- Use `hierarchy` only after defining whether parent office access includes child offices.
- Do not infer office status from missing close fields; this table has no explicit `closed_on` or `is_active` column in the verified schema.

PII/sensitivity:

- `m_office` does not contain direct client PII.
- `external_id` may still be sensitive as an internal business identifier and should not be exposed unless needed.

## 3. `m_staff`

Purpose:

- Staff/officer table.
- Used for assigned officer dimensions and operational reporting.
- Can be joined to clients and savings accounts through staff/officer fields.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary staff id. Use for joins. |
| `is_loan_officer` | `boolean` | no | Indicates officer role flag. Name is loan-specific but often used for field/loan officer semantics. |
| `office_id` | `bigint` | yes | Staff office assignment. Join to `m_office.id`. |
| `firstname` | `character varying` | yes | Staff first name. Sensitive personal data. |
| `lastname` | `character varying` | yes | Staff last name. Sensitive personal data. |
| `display_name` | `character varying` | no | Staff display name. Sensitive personal data. |
| `mobile_no` | `character varying` | yes | Staff mobile number. PII; exclude by default. |
| `external_id` | `character varying` | yes | External staff identifier. Sensitive/internal. |
| `organisational_role_enum` | `smallint` | yes | Organization role enum. Needs enum mapping before business use. |
| `organisational_role_parent_staff_id` | `bigint` | yes | Manager/parent staff reference. Join to `m_staff.id`. |
| `is_active` | `boolean` | no | Staff active flag. Use when reporting current staff. |
| `joining_date` | `date` | yes | Staff joining date. |
| `image_id` | `bigint` | yes | Image reference. Exclude from reporting. |
| `email_address` | `character varying` | yes | Staff email address. PII; exclude by default. |

Primary relationship rules:

- `m_staff.office_id -> m_office.id`.
- `m_staff.organisational_role_parent_staff_id -> m_staff.id` for staff hierarchy.
- `m_client.staff_id -> m_staff.id` for assigned client staff.
- `m_savings_account.field_officer_id -> m_staff.id` for savings field officer, if populated.
- `m_savings_officer_assignment_history.field_officer_id -> m_staff.id` will be reviewed in the savings detail section.

Reporting rules:

- Use `id` as canonical staff key.
- Use `display_name` for staff display only when staff identity is allowed in the capability.
- Use `is_active = true` for current-staff reports unless the capability explicitly includes inactive staff.
- Do not use `organisational_role_enum` until enum mapping is documented.
- Do not expose `mobile_no`, `email_address`, or `image_id` in MVP reporting.

PII/sensitivity:

- Staff names, mobile numbers, email addresses, and external ids are sensitive.
- Default output should prefer aggregate staff ids/counts or masked names unless the capability allows staff identity display.

## 4. `m_staff_assignment_history`

Purpose:

- Historical assignment of staff to centers.
- This is not the primary staff-office assignment table.
- Use only when group/center reporting is enabled.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary assignment history id. |
| `centre_id` | `bigint` | yes | Center/group id. Join to `m_group.id` when center/group scope is enabled. |
| `staff_id` | `bigint` | no | Staff id. Join to `m_staff.id`. |
| `start_date` | `date` | no | Assignment start date. |
| `end_date` | `date` | yes | Assignment end date. `NULL` means current assignment in Fineract domain model. |
| `createdby_id` | `bigint` | yes | Audit user id. Exclude from MVP unless audit scope is enabled. |
| `created_date` | `timestamp without time zone` | yes | Audit timestamp. Exclude from MVP unless audit scope is enabled. |
| `lastmodified_date` | `timestamp without time zone` | yes | Audit timestamp. Exclude from MVP unless audit scope is enabled. |
| `lastmodifiedby_id` | `bigint` | yes | Audit user id. Exclude from MVP unless audit scope is enabled. |

Primary relationship rules:

- `m_staff_assignment_history.staff_id -> m_staff.id`.
- `m_staff_assignment_history.centre_id -> m_group.id`.

Reporting rules:

- Do not use this table for simple staff office reporting; use `m_staff.office_id` instead.
- Use only for center/group staff assignment history.
- Current assignment means `end_date IS NULL` according to the Fineract `StaffAssignmentHistory.isCurrentRecord()` domain method.
- Keep this table optional until group/center scope is confirmed.

PII/sensitivity:

- This table does not directly contain staff names or client PII.
- It links to `m_staff`, so downstream output can become sensitive if staff identity fields are selected.

## 5. MVP Inclusion Decision

Included immediately:

- `m_office.id`.
- `m_office.parent_id`.
- `m_office.hierarchy`.
- `m_office.external_id`.
- `m_office.name`.
- `m_office.opening_date`.
- `m_staff.id`.
- `m_staff.office_id`.
- `m_staff.display_name`, only when staff display is explicitly allowed.
- `m_staff.is_loan_officer`.
- `m_staff.is_active`.
- `m_staff.joining_date`.

Conditionally included:

- `m_staff.firstname`.
- `m_staff.lastname`.
- `m_staff.external_id`.
- `m_staff.organisational_role_enum`, after enum mapping.
- `m_staff.organisational_role_parent_staff_id`, after staff hierarchy reporting is approved.
- `m_staff_assignment_history.*`, only if group/center reporting is enabled.

Excluded from MVP output:

- `m_staff.mobile_no`.
- `m_staff.email_address`.
- `m_staff.image_id`.
- `m_staff_assignment_history.createdby_id`.
- `m_staff_assignment_history.created_date`.
- `m_staff_assignment_history.lastmodified_date`.
- `m_staff_assignment_history.lastmodifiedby_id`.
