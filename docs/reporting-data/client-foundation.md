# Reporting Data: Client Foundation

This document contains the detailed table and field scope for Reporting Data Scope section `3.2 Client Foundation`.

Status: completed for initial review.

## 1. Scope

Client Foundation is included in the MVP foundation.

Purpose:

- Identify customers connected to savings and future loan reports.
- Provide client lifecycle/status filters.
- Support office-scoped reporting.
- Support PII-aware output masking.

High-level data concepts:

- Client identity.
- Client account number.
- Client external id.
- Client status.
- Client office.
- Assigned staff.
- Client activation/submission/closure lifecycle.
- Client type/classification.
- Client legal form.
- Basic contact fields, subject to PII rules.

Verified Fineract table family:

- `m_client`.
- `m_client_identifier`, later and only if identity document reporting is approved.
- `m_client_address`, later and only if address/location reporting is approved.
- `m_client_non_person`, later if entity/business clients are in scope.
- `m_client_transfer_details`, later if client transfer reporting is needed.

Verified from:

- Fineract changelog: `fineract-provider/src/main/resources/db/changelog/tenant/parts/0001_initial_schema.xml` plus later audit migrations.
- Fineract domain models: `Client`, `ClientIdentifier`, `ClientAddress`, `ClientNonPerson`, `ClientTransferDetails`.
- Local database `information_schema.columns` on `fineract_default`.

PII note:

- Client names, phone numbers, email addresses, identifiers, dates of birth, and addresses must be treated as PII or sensitive client data.
- Default behavior should be aggregate reporting or masked output unless `can_view_pii=true`.

## 2. `m_client`

Purpose:

- Canonical client/customer table.
- Primary owner dimension for savings and future loan reports.
- Primary source for client office, assigned staff, lifecycle status, and PII-aware identity display.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary client id. Use for joins. |
| `account_no` | `character varying` | no | Client account number. Sensitive business identifier. |
| `external_id` | `character varying` | yes | External client identifier. Sensitive business identifier. |
| `status_enum` | `integer` | no | Client status enum. Needs enum mapping before client lifecycle reports. |
| `sub_status` | `integer` | yes | Client sub-status code value id. Needs code-value mapping. |
| `activation_date` | `date` | yes | Client activation date. |
| `office_joining_date` | `date` | yes | Date client joined office. |
| `office_id` | `bigint` | no | Client office id. Join to `m_office.id`; required for office authorization. |
| `transfer_to_office_id` | `bigint` | yes | Pending/target transfer office id. Join to `m_office.id`. |
| `staff_id` | `bigint` | yes | Assigned staff id. Join to `m_staff.id`. |
| `firstname` | `character varying` | yes | Client first name. PII. |
| `middlename` | `character varying` | yes | Client middle name. PII. |
| `lastname` | `character varying` | yes | Client last name. PII. |
| `fullname` | `character varying` | yes | Client full legal/name field. PII. |
| `display_name` | `character varying` | no | Client display name. PII. |
| `mobile_no` | `character varying` | yes | Client mobile number. PII; exclude by default. |
| `is_staff` | `boolean` | no | Indicates client is also staff. Sensitive; include only when needed. |
| `gender_cv_id` | `integer` | yes | Gender code value id. Sensitive demographic field. Needs code-value mapping. |
| `date_of_birth` | `date` | yes | Date of birth. PII; exclude by default. |
| `image_id` | `bigint` | yes | Image reference. Exclude from reporting. |
| `closure_reason_cv_id` | `integer` | yes | Closure reason code value id. Needs code-value mapping. |
| `closedon_date` | `date` | yes | Client closure date. |
| `updated_by` | `bigint` | yes | Legacy/update user id. Exclude from MVP reporting. |
| `updated_on` | `date` | yes | Legacy/update date. Exclude from MVP reporting. |
| `submittedon_date` | `date` | yes | Client submission date. |
| `activatedon_userid` | `bigint` | yes | Activation user id. Audit field; exclude from MVP unless operational audit scope is enabled. |
| `closedon_userid` | `bigint` | yes | Closure user id. Audit field; exclude from MVP unless operational audit scope is enabled. |
| `default_savings_product` | `bigint` | yes | Default savings product id. Join to `m_savings_product.id` after savings scope mapping. |
| `default_savings_account` | `bigint` | yes | Default savings account id. Join to `m_savings_account.id` after savings scope mapping. |
| `client_type_cv_id` | `integer` | yes | Client type code value id. Needs code-value mapping. |
| `client_classification_cv_id` | `integer` | yes | Client classification code value id. Needs code-value mapping. |
| `reject_reason_cv_id` | `integer` | yes | Rejection reason code value id. Needs code-value mapping. |
| `rejectedon_date` | `date` | yes | Client rejection date. |
| `rejectedon_userid` | `bigint` | yes | Rejection user id. Audit field; exclude from MVP unless operational audit scope is enabled. |
| `withdraw_reason_cv_id` | `integer` | yes | Withdrawal reason code value id. Needs code-value mapping. |
| `withdrawn_on_date` | `date` | yes | Client withdrawal date. |
| `withdraw_on_userid` | `bigint` | yes | Withdrawal user id. Audit field; exclude from MVP unless operational audit scope is enabled. |
| `reactivated_on_date` | `date` | yes | Client reactivation date. |
| `reactivated_on_userid` | `bigint` | yes | Reactivation user id. Audit field; exclude from MVP unless operational audit scope is enabled. |
| `legal_form_enum` | `integer` | yes | Legal form enum. Needs enum mapping before use. |
| `reopened_on_date` | `date` | yes | Client reopened date. |
| `reopened_by_userid` | `bigint` | yes | Reopen user id. Audit field; exclude from MVP unless operational audit scope is enabled. |
| `email_address` | `character varying` | yes | Client email address. PII; exclude by default. |
| `proposed_transfer_date` | `date` | yes | Proposed transfer date. Use only when transfer reporting is approved. |
| `created_on_utc` | `timestamp with time zone` | no | Audit creation timestamp. Useful for technical audit, not default business reporting. |
| `created_by` | `bigint` | no | Audit creator user id. Exclude from MVP unless operational audit scope is enabled. |
| `last_modified_by` | `bigint` | no | Audit updater user id. Exclude from MVP unless operational audit scope is enabled. |
| `last_modified_on_utc` | `timestamp with time zone` | no | Audit update timestamp. Exclude from MVP unless operational audit scope is enabled. |

Primary relationship rules:

- `m_client.office_id -> m_office.id` for office ownership and authorization.
- `m_client.transfer_to_office_id -> m_office.id` for pending/target transfer office.
- `m_client.staff_id -> m_staff.id` for assigned staff.
- `m_client.default_savings_product -> m_savings_product.id` after savings product mapping.
- `m_client.default_savings_account -> m_savings_account.id` after savings account mapping.
- `m_savings_account.client_id -> m_client.id` for client-owned savings accounts.
- `m_group_client.client_id -> m_client.id` for group membership, if group/center scope is enabled.

Reporting rules:

- Use `id` as canonical client key.
- Use `office_id` as mandatory authorization dimension.
- Use `status_enum` only after documenting Fineract client status enum values.
- Use `display_name` only when `can_view_pii=true` or the capability explicitly allows client identity display.
- Prefer aggregate reporting by office/status/product instead of returning client-level identity rows.
- Do not use `gender_cv_id`, `date_of_birth`, `mobile_no`, or `email_address` in MVP reporting output.
- Do not use audit user fields unless operational audit scope is enabled.

PII/sensitivity:

- Client names, account numbers, external ids, mobile numbers, emails, birth dates, gender, and identifiers are sensitive.
- If `can_view_pii=false`, omit or mask client identity fields.
- Even when `can_view_pii=true`, capabilities should select only the minimum necessary identity fields.

## 3. `m_client_identifier`

Purpose:

- Stores client identity documents/identifiers.
- High-risk PII table.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary identifier id. |
| `client_id` | `bigint` | no | Client id. Join to `m_client.id`. |
| `document_type_id` | `integer` | no | Document type code value id. Needs code-value mapping. |
| `document_key` | `character varying` | no | Identity document value/key. High-risk PII. |
| `status` | `integer` | no | Identifier status enum. Needs enum mapping. |
| `active` | `integer` | yes | Active marker used by Fineract uniqueness constraint. Needs enum/status mapping. |
| `description` | `character varying` | yes | Identifier description. Sensitive free text. |
| `created_by` | `bigint` | no | Audit creator user id. Exclude from MVP. |
| `last_modified_by` | `bigint` | no | Audit updater user id. Exclude from MVP. |
| `created_date` | `timestamp without time zone` | yes | Legacy audit timestamp. Exclude from MVP. |
| `lastmodified_date` | `timestamp without time zone` | yes | Legacy audit timestamp. Exclude from MVP. |
| `created_on_utc` | `timestamp with time zone` | no | Audit creation timestamp. Exclude from MVP. |
| `last_modified_on_utc` | `timestamp with time zone` | no | Audit update timestamp. Exclude from MVP. |

Scope rule:

- Excluded from MVP reporting output.
- Include later only for explicitly approved identity-document capabilities.
- Never expose `document_key` unless a capability has strong authorization and masking rules.

## 4. `m_client_address`

Purpose:

- Links clients to address records.
- High-risk PII/location area.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary client-address link id. |
| `client_id` | `bigint` | no | Client id. Join to `m_client.id`. |
| `address_id` | `bigint` | no | Address id. Address table must be mapped before use. |
| `address_type_id` | `integer` | no | Address type code value id. Needs code-value mapping. |
| `is_active` | `boolean` | no | Active address flag. |

Scope rule:

- Excluded from MVP reporting output.
- Include later only if location/address reporting is explicitly approved.
- Do not join to address detail tables until address PII policy is defined.

## 5. `m_client_non_person`

Purpose:

- Stores extra data for non-person/entity clients.
- Useful only if business/entity client reporting is required.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary non-person row id. |
| `client_id` | `bigint` | no | Client id. Join to `m_client.id`. Unique in domain model. |
| `constitution_cv_id` | `integer` | no | Constitution code value id. Needs code-value mapping. |
| `incorp_no` | `character varying` | yes | Incorporation number. Sensitive business identifier. |
| `incorp_validity_till` | `date` | yes | Incorporation validity date. |
| `main_business_line_cv_id` | `integer` | yes | Main business line code value id. Needs code-value mapping. |
| `remarks` | `character varying` | yes | Free text remarks. Sensitive; exclude by default. |

Scope rule:

- Conditionally included only if entity/business-client reporting is enabled.
- Exclude `incorp_no` and `remarks` from MVP output.

## 6. `m_client_transfer_details`

Purpose:

- Stores client office transfer events/proposals.
- Useful for transfer lifecycle reporting, not needed for basic savings reports.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Primary transfer detail id. |
| `client_id` | `bigint` | no | Client id. Join to `m_client.id`. |
| `from_office_id` | `bigint` | no | Source office id. Join to `m_office.id`. |
| `to_office_id` | `bigint` | no | Target office id. Join to `m_office.id`. |
| `proposed_transfer_date` | `date` | yes | Proposed transfer date. |
| `transfer_type` | `smallint` | no | Transfer type enum. Needs enum mapping. |
| `submitted_on` | `date` | no | Transfer submission date. |
| `submitted_by` | `bigint` | no | Submitting user id. Exclude from MVP unless operational audit scope is enabled. |

Scope rule:

- Excluded from MVP reporting output.
- Include later only for client transfer reporting.

## 7. MVP Inclusion Decision

Included immediately:

- `m_client.id`.
- `m_client.office_id`.
- `m_client.staff_id`.
- `m_client.status_enum`, after enum mapping is documented.
- `m_client.activation_date`.
- `m_client.office_joining_date`.
- `m_client.submittedon_date`.
- `m_client.closedon_date`.
- `m_client.rejectedon_date`.
- `m_client.withdrawn_on_date`.
- `m_client.reactivated_on_date`.
- `m_client.default_savings_product`.
- `m_client.default_savings_account`.
- `m_client.client_type_cv_id`, after code-value mapping is approved.
- `m_client.client_classification_cv_id`, after code-value mapping is approved.
- `m_client.legal_form_enum`, after enum mapping is documented.

Conditionally included:

- `m_client.account_no`, only for client-level output and subject to PII/business identifier policy.
- `m_client.external_id`, only for explicitly approved internal/reference use.
- `m_client.display_name`, only when `can_view_pii=true` or masked output is acceptable.
- `m_client.firstname`.
- `m_client.middlename`.
- `m_client.lastname`.
- `m_client.fullname`.
- `m_client.transfer_to_office_id`, only for transfer-aware capabilities.
- `m_client.proposed_transfer_date`, only for transfer-aware capabilities.
- `m_client_non_person.*`, only for entity/business-client reporting.

Excluded from MVP output:

- `m_client.mobile_no`.
- `m_client.email_address`.
- `m_client.date_of_birth`.
- `m_client.gender_cv_id`.
- `m_client.image_id`.
- `m_client_identifier.*`.
- `m_client_address.*`.
- `m_client_transfer_details.*`.
- All `*_userid`, `created_by`, `last_modified_by`, `created_on_utc`, and `last_modified_on_utc` audit fields unless operational audit scope is enabled.
