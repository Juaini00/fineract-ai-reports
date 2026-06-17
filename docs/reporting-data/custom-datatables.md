# Reporting Data: Custom Datatables

This document contains the detailed table and field scope for Reporting Data Scope section `4.4 Custom Datatables`.

Status: deferred, documented for future review.

## 1. Scope

Custom datatable reporting is deferred.

Reason:

- Custom datatables vary by installation.
- They may contain PII, local business fields, free text, or poorly documented semantics.
- Table and column names may contain spaces and mixed case.
- Datatables are not safe to expose automatically just because they exist in Fineract.

Verified Fineract metadata table family:

- `x_registered_table`.
- `x_table_column_code_mappings`.
- `m_entity_datatable_check`.
- `m_code`.
- `m_code_value`.

Verified local registered custom datatables:

- `Client Identification Details`.
- `Client Pfm`.
- `Customer Additional Information`.
- `dfsd`.
- `Employment Details`.
- `Group Additional Details`.
- `Kyc_Fields_For_Entity_tmp`.
- `Loan Additional Information`.
- `loan_translation`.
- `ResidentialAddress`.
- `Saving Account Additional Info`.
- `Saving Additional Information`.
- `savings_product_extra_info`.

Verified from:

- Fineract changelog: `fineract-provider/src/main/resources/db/changelog/tenant/parts/0001_initial_schema.xml`.
- Local database `information_schema.columns` on `fineract_default`.
- Local `x_registered_table` rows on `fineract_default`.

Activation rule:

- Do not automatically expose custom datatables.
- Each custom datatable must be explicitly reviewed and approved by table name and column name before use.
- Each custom datatable capability must declare allowed joins, allowed filters, PII handling, and whether free-text fields can be returned.

## 2. Metadata Tables

### 2.1 `x_registered_table`

Purpose:

- Registers Fineract custom datatables and associates them with application tables.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `registered_table_name` | `character varying` | no | Registered custom table name. This can contain spaces. |
| `application_table_name` | `character varying` | no | Parent/core Fineract table name, for example `m_client` or `m_savings_account`. |
| `entity_subtype` | `character varying` | yes | Optional entity subtype, for example `Person`, `Entity`, or `Savings Product`. |
| `category` | `integer` | no | Datatable category enum/id. Needs mapping before business use. |

Reporting rules:

- Use this table only to discover candidate datatables.
- Do not treat registration as approval for reporting.
- Registered table names with spaces require SQL identifier quoting if used later.

### 2.2 `x_table_column_code_mappings`

Purpose:

- Maps custom datatable column aliases to Fineract codes.
- Useful for columns that store code value ids.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `column_alias_name` | `character varying` | no | Custom datatable column alias/name. |
| `code_id` | `integer` | no | Code id. Join to `m_code.id`. |

Relationship rules:

- `x_table_column_code_mappings.code_id -> m_code.id`.
- Code values are resolved through `m_code_value.code_id -> m_code.id`.

Reporting rules:

- Do not display raw code ids from custom datatable columns when code mapping exists.
- Code mapping still requires per-column approval.

### 2.3 `m_entity_datatable_check`

Purpose:

- Defines datatable checks/rules for application tables and products.

Columns verified in local database:

| Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- |
| `id` | `integer` | no | Primary check id. |
| `application_table_name` | `character varying` | no | Parent/core Fineract table name. |
| `x_registered_table_name` | `character varying` | no | Registered custom datatable name. |
| `status_enum` | `integer` | no | Status enum. Needs mapping. |
| `system_defined` | `boolean` | no | System-defined flag. |
| `product_id` | `bigint` | yes | Product id if check is product-specific. |

Reporting rules:

- Use only for datatable configuration review.
- Do not use as reporting data by default.

### 2.4 `m_code` And `m_code_value`

Purpose:

- Resolve code-backed custom datatable fields.

Relevant verified columns:

| Table | Column | Type | Nullable | Reporting use |
| --- | --- | --- | --- | --- |
| `m_code` | `id` | `integer` | no | Code id. |
| `m_code` | `code_name` | `character varying` | yes | Code name. |
| `m_code` | `is_system_defined` | `boolean` | no | System-defined flag. |
| `m_code_value` | `id` | `integer` | no | Code value id. |
| `m_code_value` | `code_id` | `integer` | no | Parent code id. |
| `m_code_value` | `code_value` | `character varying` | yes | Code value display. |
| `m_code_value` | `code_description` | `character varying` | yes | Code value description. |
| `m_code_value` | `order_position` | `integer` | no | Display order. |
| `m_code_value` | `code_score` | `integer` | yes | Code score. |
| `m_code_value` | `is_active` | `boolean` | no | Active flag. |
| `m_code_value` | `is_mandatory` | `boolean` | no | Mandatory flag. |

Reporting rules:

- Resolve code values only for explicitly approved custom datatable columns.
- Filter `m_code_value.is_active = true` for active code-value display unless historical values are needed.

## 3. Registered Custom Datatables In Local Database

The following table names and columns are verified from the local `fineract_default` database.

Important:

- These are documented as candidates only.
- None are approved for default reporting yet.
- Names with spaces or mixed case must be treated as exact SQL identifiers if queried.

### 3.1 `Client Identification Details`

Parent table: `m_client`.

Entity subtype: `Person`.

Columns:

| Column | Type | Nullable | Sensitivity |
| --- | --- | --- | --- |
| `client_id` | `bigint` | no | Join key to `m_client.id`. |
| `national_id` | `character varying` | yes | High-risk PII. |
| `created_at` | `timestamp without time zone` | yes | Audit/operational. |
| `updated_at` | `timestamp without time zone` | yes | Audit/operational. |

Decision: excluded until identity-document reporting is explicitly approved.

### 3.2 `Client Pfm`

Parent table: `m_client`.

Entity subtype: `Person`.

Columns:

| Column | Type | Nullable | Sensitivity |
| --- | --- | --- | --- |
| `client_id` | `bigint` | no | Join key to `m_client.id`. |
| `pfmRef` | `character varying` | yes | Local/business reference. Sensitive until defined. |
| `created_at` | `timestamp without time zone` | yes | Audit/operational. |
| `updated_at` | `timestamp without time zone` | yes | Audit/operational. |

Decision: excluded until local meaning of `pfmRef` is defined.

### 3.3 `Customer Additional Information`

Parent table: `m_client`.

Entity subtype: `Person`.

Columns:

| Column | Type | Nullable | Sensitivity |
| --- | --- | --- | --- |
| `client_id` | `bigint` | no | Join key to `m_client.id`. |
| `National ID` | `character varying` | no | High-risk PII. |
| `PEP` | `boolean` | yes | Sensitive compliance/KYC indicator. |
| `ADDRESS_TYPE_cd_Proof of Address` | `integer` | yes | Code-backed address proof field. Sensitive KYC data. |
| `created_at` | `timestamp without time zone` | yes | Audit/operational. |
| `updated_at` | `timestamp without time zone` | yes | Audit/operational. |

Decision: excluded until KYC/compliance reporting is explicitly approved.

### 3.4 `dfsd`

Parent table: `m_client`.

Entity subtype: `Person`.

Columns:

| Column | Type | Nullable | Sensitivity |
| --- | --- | --- | --- |
| `client_id` | `bigint` | no | Join key to `m_client.id`. |
| `dsfsd` | `boolean` | yes | Undefined local field. |
| `created_at` | `timestamp without time zone` | yes | Audit/operational. |
| `updated_at` | `timestamp without time zone` | yes | Audit/operational. |

Decision: excluded because semantics are undefined.

### 3.5 `Employment Details`

Parent table: `m_client`.

Entity subtype: `Person`.

Columns:

| Column | Type | Nullable | Sensitivity |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Row id. |
| `client_id` | `bigint` | no | Join key to `m_client.id`. |
| `Employment Status_cd_Employment_Status` | `integer` | yes | Code-backed employment status. Sensitive. |
| `Employer Name` | `text` | yes | PII/sensitive employment data. |
| `Annual Salary` | `numeric` | yes | Financial PII. |
| `created_at` | `timestamp without time zone` | yes | Audit/operational. |
| `updated_at` | `timestamp without time zone` | yes | Audit/operational. |

Decision: excluded until employment/affordability reporting is explicitly approved.

### 3.6 `Group Additional Details`

Parent table: `m_group`.

Entity subtype: none.

Columns:

| Column | Type | Nullable | Sensitivity |
| --- | --- | --- | --- |
| `group_id` | `bigint` | no | Join key to `m_group.id`. |
| `Primary Holder` | `character varying` | yes | Potential client/person identity. Sensitive. |
| `created_at` | `timestamp without time zone` | yes | Audit/operational. |
| `updated_at` | `timestamp without time zone` | yes | Audit/operational. |

Decision: excluded until group detail semantics are approved.

### 3.7 `Kyc_Fields_For_Entity_tmp`

Parent table: `m_client`.

Entity subtype: `Entity`.

Columns:

| Column | Type | Nullable | Sensitivity |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Row id. |
| `client_id` | `bigint` | no | Join key to `m_client.id`. |
| `test_ref` | `character varying` | yes | Undefined local KYC field. Sensitive until defined. |
| `created_at` | `timestamp without time zone` | yes | Audit/operational. |
| `updated_at` | `timestamp without time zone` | yes | Audit/operational. |

Decision: excluded until KYC/entity reporting is explicitly approved.

### 3.8 `Loan Additional Information`

Parent table: `m_loan`.

Entity subtype: none.

Columns:

| Column | Type | Nullable | Sensitivity |
| --- | --- | --- | --- |
| `loan_id` | `bigint` | no | Join key to `m_loan.id`. |
| `Nominee` | `character varying` | yes | Person identity/beneficiary. PII. |
| `Defaulter` | `boolean` | yes | Sensitive credit risk field. |
| `created_at` | `timestamp without time zone` | yes | Audit/operational. |
| `updated_at` | `timestamp without time zone` | yes | Audit/operational. |

Decision: excluded until loan scope and custom field semantics are approved.

### 3.9 `loan_translation`

Parent table: `m_product_loan`.

Entity subtype: none.

Columns:

| Column | Type | Nullable | Sensitivity |
| --- | --- | --- | --- |
| `id` | `bigint` | no | Row id. |
| `product_loan_id` | `bigint` | no | Join key to `m_product_loan.id`. |
| `keyname` | `text` | yes | Translation key. |
| `lang` | `text` | yes | Language code. |
| `transvalue` | `text` | yes | Translation value/free text. |
| `created_at` | `timestamp without time zone` | yes | Audit/operational. |
| `updated_at` | `timestamp without time zone` | yes | Audit/operational. |

Decision: excluded until loan product localization reporting is approved.

### 3.10 `ResidentialAddress`

Parent table: `m_client`.

Entity subtype: `Person`.

Columns:

| Column | Type | Nullable | Sensitivity |
| --- | --- | --- | --- |
| `client_id` | `bigint` | no | Join key to `m_client.id`. |
| `AddressLine1` | `text` | yes | Address PII. |
| `AddressLine2` | `text` | yes | Address PII. |
| `created_at` | `timestamp without time zone` | yes | Audit/operational. |
| `updated_at` | `timestamp without time zone` | yes | Audit/operational. |

Decision: excluded until address/location reporting is explicitly approved.

### 3.11 `Saving Account Additional Info`

Parent table: `m_savings_account`.

Entity subtype: none.

Columns:

| Column | Type | Nullable | Sensitivity |
| --- | --- | --- | --- |
| `savings_account_id` | `bigint` | no | Join key to `m_savings_account.id`. |
| `Primary Earner` | `boolean` | yes | Local household/economic role. Sensitive until defined. |
| `Nominee Name` | `character varying` | yes | Person identity/beneficiary. PII. |
| `created_at` | `timestamp without time zone` | yes | Audit/operational. |
| `updated_at` | `timestamp without time zone` | yes | Audit/operational. |

Decision: excluded until savings custom fields are explicitly approved.

### 3.12 `Saving Additional Information`

Parent table: `m_savings_account`.

Entity subtype: none.

Columns:

| Column | Type | Nullable | Sensitivity |
| --- | --- | --- | --- |
| `savings_account_id` | `bigint` | no | Join key to `m_savings_account.id`. |
| `Additional Info` | `text` | yes | Free text. Sensitive until reviewed. |
| `created_at` | `timestamp without time zone` | yes | Audit/operational. |
| `updated_at` | `timestamp without time zone` | yes | Audit/operational. |

Decision: excluded until savings custom fields are explicitly approved.

### 3.13 `savings_product_extra_info`

Parent table: `m_savings_product`.

Entity subtype: `Savings Product`.

Columns:

| Column | Type | Nullable | Sensitivity |
| --- | --- | --- | --- |
| `savings_product_id` | `bigint` | no | Join key to `m_savings_product.id`. |
| `minimumCustomerAge` | `integer` | yes | Product eligibility/configuration field. |
| `created_at` | `timestamp without time zone` | yes | Audit/operational. |
| `updated_at` | `timestamp without time zone` | yes | Audit/operational. |

Decision: candidate for later product configuration reporting, but excluded from MVP until approved.

## 4. Required Before Promoting Any Custom Datatable

Before any custom datatable can be used in approved reporting, document:

- Exact table name.
- Exact column names.
- Parent application table and join key.
- Whether table/column names need SQL quoting.
- Field meaning from business owner.
- PII/sensitivity classification per column.
- Whether free-text fields can be searched or returned.
- Code mappings through `x_table_column_code_mappings`, `m_code`, and `m_code_value`.
- Allowed filters.
- Allowed aggregates.
- Whether client-level output requires `can_view_pii=true`.
- Whether office authorization can be enforced through the parent table.

## 5. Initial Activation Candidate

Most custom datatables should remain excluded.

If one is promoted first, the safest candidate is:

- `savings_product_extra_info.minimumCustomerAge`.

Reason:

- It is product-level, not client-level.
- It does not directly contain PII.
- It can join through `savings_product_id -> m_savings_product.id`.

Still required before activation:

- Confirm business meaning of `minimumCustomerAge`.
- Confirm whether it should be part of product/reporting capability output.
