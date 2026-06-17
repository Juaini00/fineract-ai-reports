# Reporting Data: Group And Center Foundation

This document contains the detailed table and field scope for Reporting Data Scope section `3.3 Group And Center Foundation`.

Status: completed for initial review, conditionally included.

## 1. Scope

Group And Center Foundation is conditionally included.

Purpose:

- Support installations that use group/center-based client organization.
- Allow savings reporting by group or center if the Fineract setup uses this model.
- Provide group/client membership context.
- Provide group hierarchy and group-level office/staff dimensions.

High-level data concepts:

- Group.
- Center.
- Group hierarchy.
- Group level.
- Client membership in group.
- Client role in group.
- Group staff assignment.

Verified Fineract table family:

- `m_group`.
- `m_group_client`.
- `m_group_level`.
- `m_group_roles`.
- `m_staff_assignment_history`, documented in `docs/reporting-data/organization-foundation.md`.

Verified from:

- Fineract changelog: `fineract-provider/src/main/resources/db/changelog/tenant/parts/0001_initial_schema.xml`.
- Fineract domain models: `Group`, `GroupLevel`, `GroupRole`, `StaffAssignmentHistory`.
- Local database `information_schema.columns` on `fineract_default`.

Scope rule:

- Include this area only if the local Fineract usage relies on groups/centers.
- If group/center data is not used in the deployment, keep this area as optional context and do not expose reporting capabilities for it.

## 2. `m_group`

Purpose:

- Canonical group/center table.
- Stores both groups and centers; distinction comes through `level_id` and `m_group_level.level_name`.
- Provides group-level office, staff, lifecycle, hierarchy, and account number context.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary group/center id. Use for joins. |
| `external_id` | `character varying` | yes | External group identifier. Sensitive business identifier. |
| `status_enum` | `integer` | no | Group status enum. Needs enum mapping before lifecycle reports. |
| `activation_date` | `date` | yes | Group activation date. |
| `office_id` | `bigint` | no | Group office id. Join to `m_office.id`; required for office authorization. |
| `staff_id` | `bigint` | yes | Assigned staff id. Join to `m_staff.id`. |
| `parent_id` | `bigint` | yes | Parent group/center id. Join to `m_group.id`. |
| `level_id` | `integer` | no | Group level id. Join to `m_group_level.id`. |
| `display_name` | `character varying` | no | Group/center display name. Sensitive business/customer grouping label. |
| `hierarchy` | `character varying` | yes | Materialized group hierarchy path. Use only after hierarchy semantics are defined. |
| `closure_reason_cv_id` | `integer` | yes | Closure reason code value id. Needs code-value mapping. |
| `closedon_date` | `date` | yes | Group closure date. |
| `activatedon_userid` | `bigint` | yes | Activation user id. Audit field; exclude from MVP unless operational audit scope is enabled. |
| `submittedon_date` | `date` | yes | Group submission date. |
| `submittedon_userid` | `bigint` | yes | Submission user id. Audit field; exclude from MVP unless operational audit scope is enabled. |
| `closedon_userid` | `bigint` | yes | Closure user id. Audit field; exclude from MVP unless operational audit scope is enabled. |
| `account_no` | `character varying` | no | Group account number. Sensitive business identifier. |

Primary relationship rules:

- `m_group.office_id -> m_office.id` for office ownership and authorization.
- `m_group.staff_id -> m_staff.id` for assigned staff.
- `m_group.parent_id -> m_group.id` for group/center hierarchy.
- `m_group.level_id -> m_group_level.id` for group vs center semantics.
- `m_group_client.group_id -> m_group.id` for client membership.
- `m_savings_account.group_id -> m_group.id` for group-owned savings accounts, after savings scope mapping.
- `m_staff_assignment_history.centre_id -> m_group.id` for center staff assignment history.

Reporting rules:

- Use `id` as canonical group/center key.
- Use `office_id` as the authorization dimension.
- Use `level_id` plus `m_group_level.level_name` to distinguish group vs center.
- Use `status_enum` only after documenting Fineract group status enum values.
- Use `hierarchy` only after defining whether parent group/center access includes children.
- Do not expose `account_no` or `external_id` by default.

PII/sensitivity:

- Group/center names can reveal customer grouping and should be treated as sensitive business data.
- Group account numbers and external ids are sensitive business identifiers.

## 3. `m_group_client`

Purpose:

- Many-to-many membership table linking clients to groups.
- Needed for group/center rollups and client membership filters.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `group_id` | `bigint` | no | Group id. Join to `m_group.id`. |
| `client_id` | `bigint` | no | Client id. Join to `m_client.id`. |

Primary relationship rules:

- `m_group_client.group_id -> m_group.id`.
- `m_group_client.client_id -> m_client.id`.

Reporting rules:

- Use only when group/center scope is enabled.
- This table has no lifecycle/status dates; membership interpretation should be validated against Fineract behavior before historical membership reports.
- For current aggregate reporting, treat rows as current membership unless a later source indicates otherwise.

PII/sensitivity:

- Membership links client identity to group identity, so output can become sensitive if client-level rows are returned.

## 4. `m_group_level`

Purpose:

- Defines group hierarchy levels such as `Center` and `Group`.
- Used to determine whether a row in `m_group` represents a group, center, or another configured level.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `integer` | no | Primary group level id. Join from `m_group.level_id`. |
| `parent_id` | `integer` | yes | Parent group level id. Join to `m_group_level.id`. |
| `super_parent` | `boolean` | no | Indicates top-level group hierarchy. |
| `level_name` | `character varying` | no | Level name, for example `Center` or `Group`. |
| `recursable` | `boolean` | no | Indicates whether the level can recurse. |
| `can_have_clients` | `boolean` | no | Indicates whether groups at this level can directly contain clients. |

Primary relationship rules:

- `m_group.level_id -> m_group_level.id`.
- `m_group_level.parent_id -> m_group_level.id` for level hierarchy.

Reporting rules:

- Use `level_name` to label group/center output.
- Use `can_have_clients` when validating whether client membership is expected at a level.
- Do not hard-code only `Center` and `Group` without checking local configured values.

PII/sensitivity:

- This table does not contain direct client PII.

## 5. `m_group_roles`

Purpose:

- Links clients to roles inside groups.
- Useful for role-based group membership reporting, if approved.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary group role id. |
| `client_id` | `bigint` | yes | Client id. Join to `m_client.id`. |
| `group_id` | `bigint` | yes | Group id. Join to `m_group.id`. |
| `role_cv_id` | `integer` | yes | Role code value id. Needs code-value mapping. |

Primary relationship rules:

- `m_group_roles.client_id -> m_client.id`.
- `m_group_roles.group_id -> m_group.id`.
- `m_group_roles.role_cv_id` maps to a code value and needs code-value mapping before display.

Reporting rules:

- Exclude from MVP unless group role reporting is explicitly needed.
- Do not use `role_cv_id` until code-value mapping is documented.

PII/sensitivity:

- Role rows link client identity to group role. Treat as sensitive if returning client-level rows.

## 6. `m_staff_assignment_history`

Purpose in this area:

- Historical assignment of staff to centers.
- Detailed columns are documented in `docs/reporting-data/organization-foundation.md`.

Relevant relationship rules:

- `m_staff_assignment_history.centre_id -> m_group.id`.
- `m_staff_assignment_history.staff_id -> m_staff.id`.

Reporting rules:

- Use only for center/group staff assignment history.
- Current assignment means `end_date IS NULL` according to the Fineract `StaffAssignmentHistory.isCurrentRecord()` domain method.
- Do not use this table for simple staff office reporting; use `m_staff.office_id` instead.

## 7. MVP Inclusion Decision

Included only if group/center model is active in the deployment:

- `m_group.id`.
- `m_group.office_id`.
- `m_group.staff_id`.
- `m_group.parent_id`.
- `m_group.level_id`.
- `m_group.display_name`, subject to sensitive business data rules.
- `m_group.hierarchy`.
- `m_group.status_enum`, after enum mapping is documented.
- `m_group.activation_date`.
- `m_group.closedon_date`.
- `m_group.submittedon_date`.
- `m_group_client.group_id`.
- `m_group_client.client_id`.
- `m_group_level.id`.
- `m_group_level.parent_id`.
- `m_group_level.level_name`.
- `m_group_level.can_have_clients`.

Conditionally included later:

- `m_group.external_id`.
- `m_group.account_no`.
- `m_group.closure_reason_cv_id`, after code-value mapping.
- `m_group_level.super_parent`.
- `m_group_level.recursable`.
- `m_group_roles.*`, only for group role reporting.
- `m_staff_assignment_history.*`, only for center staff assignment history.

Excluded from MVP output:

- `m_group.activatedon_userid`.
- `m_group.submittedon_userid`.
- `m_group.closedon_userid`.
- `m_group_roles.role_cv_id` until code-value mapping is documented.
