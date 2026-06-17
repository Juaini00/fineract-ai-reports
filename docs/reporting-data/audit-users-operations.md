# Reporting Data: Audit, Users, And Operations

This document contains the detailed table and field scope for Reporting Data Scope section `4.5 Audit, Users, And Operations`.

Status: deferred, documented for future activation.

## 1. Scope

Audit, Users, And Operations reporting is deferred except basic created/approved user references when needed by approved business reports.

Reason:

- Operational audit can be useful but should not be mixed into business reporting before core data definitions are stable.
- User tables contain PII and secrets.
- Command source tables can contain raw command JSON and request results.
- Audit fields can reveal internal operations and should only be exposed through explicit operational capabilities.

Verified Fineract table family:

- `m_appuser`.
- `m_appuser_role`.
- `m_role`.
- `m_permission`.
- `m_role_permission`.
- `m_portfolio_command_source`.
- `request_audit_table`.

Advanced/specialized operation tables observed but not detailed here:

- `m_appuser_previous_password`.
- `m_command`.
- `m_savings_account_amount_hold_audit`.
- `m_savings_account_receivable_settlement_audit`.
- `m_savings_account_settlement_mode_audit`.

Verified from:

- Fineract changelog: `fineract-provider/src/main/resources/db/changelog/tenant/parts/0001_initial_schema.xml` plus later command/audit migrations.
- Fineract domain models: `AppUser`, `CommandSource`.
- Local database `information_schema.columns` on `fineract_default`.

Activation rule:

- Include operational audit reporting only after explicit approval.
- Never expose password, token, temporary password, raw command JSON, raw command result, or request secrets.
- User identity display must be treated as PII.

## 2. `m_appuser`

Purpose:

- Canonical Fineract application user table.
- Referenced by many `*_userid`, `created_by`, and `last_modified_by` fields across Fineract tables.

### 2.1 Safe/Conditional User Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary app user id. Use for joins. |
| `is_deleted` | `boolean` | no | Deleted flag. Filter deleted users by default. |
| `office_id` | `bigint` | yes | User office id. Join to `m_office.id`. |
| `staff_id` | `bigint` | yes | Staff id. Join to `m_staff.id`. |
| `username` | `character varying` | no | Username. PII/internal identifier. Conditional display only. |
| `firstname` | `character varying` | no | First name. PII. |
| `lastname` | `character varying` | no | Last name. PII. |
| `email` | `character varying` | no | Email address. PII; exclude by default. |
| `firsttime_login_remaining` | `boolean` | no | Login state. Operational/security reporting only. |
| `nonexpired` | `boolean` | no | Account non-expired flag. Operational/security reporting only. |
| `nonlocked` | `boolean` | no | Account non-locked flag. Operational/security reporting only. |
| `nonexpired_credentials` | `boolean` | no | Credential non-expired flag. Operational/security reporting only. |
| `enabled` | `boolean` | no | Enabled flag. Useful for user administration reporting. |
| `last_time_password_updated` | `date` | no | Password update date. Security reporting only. |
| `password_never_expires` | `boolean` | no | Security setting. Security reporting only. |
| `cannot_change_password` | `boolean` | yes | Security setting. Security reporting only. |
| `password_reset_required` | `boolean` | no | Security setting. Security reporting only. |
| `failed_login_attempts` | `integer` | no | Security/audit metric. Sensitive. |
| `is_login_retries_enabled` | `boolean` | no | Security setting. |
| `temporary_password_expiry_time` | `timestamp with time zone` | yes | Temporary password expiry. Security reporting only. |
| `is_password_reset_enabled` | `boolean` | no | Security setting. |

### 2.2 Secret Columns Excluded Always

| Column | Type | Nullable | Rule |
| --- | --- | --- | --- |
| `password` | `character varying` | no | Never expose. Secret credential hash/value. |
| `temporary_password` | `character varying` | yes | Never expose. Secret temporary credential. |

Relationship rules:

- `m_appuser.office_id -> m_office.id`.
- `m_appuser.staff_id -> m_staff.id`.
- `m_appuser_role.appuser_id -> m_appuser.id`.
- Many domain audit columns reference `m_appuser.id`, such as `approvedon_userid`, `closedon_userid`, `created_by`, and `last_modified_by`.

Reporting rules:

- Use `id` as canonical user key.
- Filter `is_deleted = false` for active user lookups unless deleted users are explicitly requested.
- Do not expose `email`, `firstname`, `lastname`, or `username` unless operational/user reporting capability is approved.
- Never expose `password` or `temporary_password`.

## 3. Roles And Permissions

### 3.1 `m_appuser_role`

Purpose:

- Many-to-many user-role mapping table.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `appuser_id` | `bigint` | no | App user id. Join to `m_appuser.id`. |
| `role_id` | `bigint` | no | Role id. Join to `m_role.id`. |

### 3.2 `m_role`

Purpose:

- Role definition table.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary role id. |
| `name` | `character varying` | no | Role name. Sensitive/internal authorization info. |
| `description` | `character varying` | no | Role description. Sensitive/internal. |
| `is_disabled` | `boolean` | no | Disabled flag. |

### 3.3 `m_permission`

Purpose:

- Permission definition table.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary permission id. |
| `grouping` | `character varying` | yes | Permission grouping. |
| `code` | `character varying` | no | Permission code. Sensitive/internal authorization info. |
| `entity_name` | `character varying` | yes | Entity name. |
| `action_name` | `character varying` | yes | Action name. |
| `can_maker_checker` | `boolean` | no | Whether permission supports maker-checker. |

### 3.4 `m_role_permission`

Purpose:

- Many-to-many role-permission mapping table.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `role_id` | `bigint` | no | Role id. Join to `m_role.id`. |
| `permission_id` | `bigint` | no | Permission id. Join to `m_permission.id`. |

Reporting rules for roles/permissions:

- Exclude from MVP reporting.
- Include only for explicit user administration/security audit capabilities.
- Treat role names, permission codes, and permission mappings as sensitive internal authorization data.

## 4. `m_portfolio_command_source`

Purpose:

- Command source/maker-checker table.
- Stores action/entity, resource ids, maker/checker, raw command JSON, status, idempotency key, result, and client IP.

### 4.1 Safer Command Metadata Columns

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary command id. |
| `action_name` | `character varying` | no | Command action name. |
| `entity_name` | `character varying` | no | Command entity name. |
| `office_id` | `bigint` | yes | Office id. Join to `m_office.id`. |
| `group_id` | `bigint` | yes | Group id. Join to `m_group.id`. |
| `client_id` | `bigint` | yes | Client id. Join to `m_client.id`. |
| `loan_id` | `bigint` | yes | Loan id. Join to `m_loan.id`. |
| `savings_account_id` | `bigint` | yes | Savings account id. Join to `m_savings_account.id`. |
| `resource_id` | `bigint` | yes | Resource id. Meaning depends on entity/action. |
| `subresource_id` | `bigint` | yes | Subresource id. Meaning depends on entity/action. |
| `maker_id` | `bigint` | no | Maker user id. Join to `m_appuser.id`. |
| `made_on_date` | `timestamp without time zone` | yes | Legacy made timestamp. |
| `checker_id` | `bigint` | yes | Checker user id. Join to `m_appuser.id`. |
| `checked_on_date` | `timestamp without time zone` | yes | Legacy checked timestamp. |
| `status` | `smallint` | no | Command status enum. Needs mapping. |
| `product_id` | `bigint` | yes | Product id. Meaning depends on entity/action. |
| `transaction_id` | `character varying` | yes | Transaction id/reference. Sensitive/internal. |
| `made_on_date_utc` | `timestamp with time zone` | no | Made timestamp. |
| `checked_on_date_utc` | `timestamp with time zone` | yes | Checked timestamp. |
| `job_name` | `character varying` | yes | Job name, if command came from job. |
| `result_status_code` | `integer` | yes | Result status code. |
| `is_sanitized` | `boolean` | no | Whether command JSON/result was sanitized. |
| `client_ip` | `character varying` | yes | Client IP. Sensitive; security audit only. |

### 4.2 High-Risk Command Columns

| Column | Type | Nullable | Rule |
| --- | --- | --- | --- |
| `api_get_url` | `character varying` | no | Do not expose by default. May contain internal route/query context. |
| `command_as_json` | `text` | no | Never expose raw. May contain PII/secrets. |
| `idempotency_key` | `character varying` | no | Do not expose. Internal request idempotency key. |
| `resource_external_id` | `character varying` | yes | Sensitive/internal identifier. |
| `subresource_external_id` | `character varying` | yes | Sensitive/internal identifier. |
| `result` | `text` | yes | Never expose raw. May contain internal result/PII/errors. |
| `loan_external_id` | `character varying` | yes | Sensitive/internal identifier. |
| `creditbureau_id` | `bigint` | yes | Credit bureau scope only. |
| `organisation_creditbureau_id` | `bigint` | yes | Credit bureau scope only. |

Relationship rules:

- `m_portfolio_command_source.maker_id -> m_appuser.id`.
- `m_portfolio_command_source.checker_id -> m_appuser.id`.
- Optional resource links depend on entity/action and must be capability-specific.

Reporting rules:

- Include only for explicit maker-checker/operations reporting.
- Never return raw `command_as_json` or `result`.
- Do not expose `idempotency_key`.
- Treat `client_ip` as sensitive security data.
- Use `is_sanitized` if determining whether command data is safe for internal debugging, but still do not expose raw payloads to API clients.

## 5. `request_audit_table`

Purpose:

- Request audit table containing request/user/client-related fields.
- Contains high-risk PII and credentials.

Columns verified in local database:

| Column | Type | Nullable | Rule |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary request audit id. |
| `lastname` | `character varying` | no | PII. Exclude by default. |
| `username` | `character varying` | no | PII/internal identifier. Exclude by default. |
| `mobile_number` | `character varying` | yes | PII. Exclude. |
| `firstname` | `character varying` | no | PII. Exclude by default. |
| `authentication_token` | `character varying` | yes | Never expose. Secret/token. |
| `password` | `character varying` | no | Never expose. Secret. |
| `email` | `character varying` | no | PII. Exclude. |
| `client_id` | `bigint` | no | Client id. Join to `m_client.id`. |
| `created_date` | `date` | no | Request audit date. |
| `account_number` | `character varying` | no | Sensitive business identifier. Exclude by default. |

Reporting rules:

- Exclude from MVP and from most future reporting.
- Never expose `authentication_token` or `password`.
- Treat this table as high-risk. Any use requires explicit security review.

## 6. References From Business Tables To Users

Many scoped tables contain user/audit columns. Examples:

- `m_client.activatedon_userid`.
- `m_client.closedon_userid`.
- `m_savings_account.approvedon_userid`.
- `m_savings_account.activatedon_userid`.
- `m_savings_account.closedon_userid`.
- `m_savings_account_transaction.created_by`.
- `m_loan.approvedon_userid`.
- `m_loan.disbursedon_userid`.
- `m_loan.closedon_userid`.

Reporting rules:

- These references may be used internally for audit traceability.
- Do not join and display app user names by default.
- If displayed, user identity fields require an explicit operational/audit capability.

## 7. Required Before Promoting Audit/Operations To Approved Scope

Before enabling operational audit reporting capabilities, document:

- Which operations are allowed: user listing, maker-checker activity, command status, failed command analysis, security audit, or workflow audit.
- Which user identity fields can be displayed.
- Whether role/permission data can be exposed.
- Command status enum mapping.
- Maker/checker status semantics.
- Whether `client_ip` can be exposed.
- Strict prohibition on raw command JSON, raw result, passwords, tokens, idempotency keys, and temporary passwords.
- Office authorization behavior for user and command reports.

## 8. Initial Activation Candidate

When Audit/Users/Operations is promoted from deferred to approved scope, start narrowly:

- Maker/checker command counts by date, office, entity, action, and status.
- User activity counts by user id, not by name/email.
- Active app user counts by office.

Candidate tables for first activation:

- `m_portfolio_command_source` with sanitized fields only.
- `m_appuser` with `id`, `office_id`, `staff_id`, `enabled`, `is_deleted` only.
- `m_role` only if user administration reporting is approved.

Keep out of first activation:

- `request_audit_table`.
- `m_appuser.password`.
- `m_appuser.temporary_password`.
- `m_portfolio_command_source.command_as_json`.
- `m_portfolio_command_source.result`.
- `m_portfolio_command_source.idempotency_key`.
- Role-permission details unless security audit is approved.
