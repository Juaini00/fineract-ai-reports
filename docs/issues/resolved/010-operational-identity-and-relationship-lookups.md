# 010 — Operational identity and relationship lookups

Status: resolved
Severity: high
Area: knowledge | catalog | datasets | retrieval | SQL | authorization | PII | audit
Created: 2026-08-03
Resolved: 2026-08-03

Related: 008 (loan domain analyst capabilities), 009 (conversational drill-down)
Depends on: production-safe Dataset Model bridge and approved-SQL execution

## Problem

The reporting catalog is strong for aggregates, rankings, trends, and charge lists, but it does not yet cover several foundational operational lookups that begin from an exact business identifier or entity relationship.

Examples:

- Who owns savings account number `XXXXXXXX`?
- Which product is savings account `XXXXXXXX` using?
- What interest rate and overdraft terms apply to savings account `XXXXXXXX`?
- Which office/organization owns client Nour Hashem?
- Which savings accounts and products belong to that client?
- Who owns loan number `XXXXXXXX`?
- Which loan product and terms apply to loan number `XXXXXXXX`?

These are basic relationship traversals, not arbitrary analytics. Their absence forces valid operational questions into `Unsupported`, even though parts of the relationships already exist in schema knowledge and approved joins.

## Impact

- Operations users cannot resolve an account/loan number to its owner, office, product, status, or terms.
- Client identity questions cannot consistently traverse client → office → accounts → products.
- Existing aggregate reports cannot answer common account-servicing questions.
- Treating account/loan numbers as globally excluded prevents valid exact-match workflows, while exposing them freely would create enumeration and privacy risks.
- Loan identity lookups risk being implemented ad hoc before issue 008 resolves loan ownership and scope semantics.

## Current behavior

### Supported

- Exact client-name lookup through an approved client capability.
- Savings lookup by client + product + exact latest transaction amount, returning account ID and transaction count.
- Savings balance/activity reports and recent/pending/strict-overdue charges.
- Existing office summaries/rankings/hierarchy via approved capabilities.

### Missing

- Exact savings account-number lookup.
- Savings account → client/office/product/status/terms traversal.
- Account-specific interest-rate source and overdraft configuration.
- Generic client → office/accounts/products relationship lookup.
- Exact loan-number lookup and loan → borrower/office/product/terms traversal.
- Filter-only identifier sensitivity and masked identifier output contracts.

### Deferred

Loan runtime execution remains governed by issue 008. No loan dataset or query may be activated from this issue until issue 008 confirms office ownership paths, approved tables, identifier sensitivity, status semantics, and loan-product/terms contracts.

## Expected behavior

### Savings account identity

Given an exact savings account number, the system can return only approved fields such as:

- masked account number;
- internal `savings_account_id`;
- owner client ID/display name, subject to PII policy;
- owner office ID/name within authorized scope;
- savings product ID/name;
- account status;
- currency.

The raw account number is usable only as an exact filter and is never returned unmasked or logged.

### Savings account terms

For an exact account, the system can distinguish and disclose the source of terms:

- account-level interest rate, when actually stored on the account;
- product-level interest configuration, when inherited from product configuration;
- overdraft enabled on the product;
- overdraft enabled on the individual account;
- overdraft limit and related approved terms;
- whether overdraft is currently used, only if a reviewed balance semantic exists.

The answer must not collapse these different facts into one ambiguous `has_overdraft` or one unlabeled interest rate.

### Client relationships

For an exact/disambiguated client, the system can answer:

- owning office/organization;
- active savings-account count;
- approved list/summary of savings products used;
- approved account identity summary;
- pending/overdue savings-charge summary where declared.

Duplicate names trigger clarification using safe distinguishing fields; the system never selects the first name match silently.

### Loan identity

After issue 008 prerequisites are satisfied, an exact loan number can resolve:

- masked loan number/internal loan ID;
- borrower identity under PII policy;
- owner office through the approved client/group ownership path;
- loan product;
- status and approved terms;
- principal/outstanding/rate source only where semantics are explicitly approved.

## Implementation status (2026-08-03)

Delivered:

- contextual savings/loan identifier extraction and pre-LLM/persistence redaction;
- Redis-backed per-bearer identifier-attempt limiting before job creation;
- typed exact-identifier and masked-output Dataset Model validation;
- SQL-bound office scope and SQL-derived masking for `savings.accounts`;
- savings account `identity` and conservative `terms` shapes/capabilities;
- separate account/product interest and overdraft fields with no inferred effective value;
- exact client-name relationship lookup with safe duplicate clarification and same-job stable `client_id` continuation;
- selected client → office, active account count, masked account identity, and product relationships;
- existing organization summaries plus exact savings account → owner office traversal;
- exact loan-number redaction and sanitized deferred routing while executable loan lookup remains owned by issue 008.

Transferred to issue 008 (not a blocker for this issue's resolution):

- executable loan identity/terms lookup after client/group ownership, product, status, terms, PII, and scope semantics are approved.

Verification:

- static catalog validation and strict Rust lint pass;
- 287 chat unit tests, 16 catalog tests, 2 retrieval tests, and 2 dataset-equivalence tests pass;
- SQL is authored, parameterized, office-scoped, and runtime-validated at application startup when `CATALOG_VALIDATE_ON_STARTUP=true`;
- database-backed HTTP authorization tests remain environment-gated by the existing Voyage embedding startup requirement, not by Issue 010 behavior.

## Proposed fix

## Phase 1 — Identifier sensitivity contract

Extend schema/catalog sensitivity beyond a binary exposed/excluded model:

- `filter_only`: exact-match input allowed; projection forbidden;
- `masked_output`: only a deterministic masked representation may be returned;
- `public_business`: approved public business field;
- `pii`: policy-gated projection;
- `never_use`: cannot be filtered, projected, logged, or sent to an LLM.

Apply the contract to savings account numbers first. Loan numbers are defined but remain inactive until the loan scope gate passes.

Validation must reject:

- prefix, substring, range, fuzzy, or list-enumeration operators on filter-only identifiers;
- raw identifier projection;
- raw identifier logging/audit payloads;
- capability/dataset mappings that bypass masking;
- an identifier lookup without SQL-bound office scope.

Preserve leading zeros and normalize only documented formatting characters. Do not parse account/loan numbers as integers.

## Phase 2 — `savings.accounts` dataset

Create a reusable approved dataset rooted in `m_savings_account` and reviewed joins to client, office, and savings product.

Initial exact filters:

- account number (`filter_only`, equality only);
- internal savings account ID;
- client ID or disambiguated identity;
- product ID/name where already approved;
- status where enum semantics are confirmed.

Initial shapes:

1. `identity`
   - masked account number;
   - account ID;
   - client identity under PII policy;
   - office;
   - product;
   - account status/currency.

2. `terms`
   - approved account-level terms;
   - approved product-level terms;
   - explicit `*_source` fields;
   - account/product overdraft flags and limits as separate fields.

3. `balance_summary`
   - reviewed current/available balance semantics;
   - currency/status;
   - no inferred values.

4. `activity_summary`
   - non-reversed transaction count;
   - deterministic latest transaction date/amount;
   - optional approved date range.

5. `charge_summary`
   - pending/overdue counts and outstanding amount using the established savings-charge semantics.

Start with identity and terms; add summaries only after their legacy/reference semantics are measured and tested.

## Phase 3 — `client.clients` relationship dataset

Create approved client relationship shapes:

- `identity`;
- `office`;
- `account_summary`;
- `product_summary`;
- `charge_summary`.

Name lookup must use the existing clarification contract:

- zero matches → valid empty result;
- one match → execute;
- multiple matches → clarification with safe attributes;
- no silent first-row selection.

Office scope is enforced in SQL through authorized IDs. Parent office access does not imply descendant access unless policy explicitly expands it.

## Phase 4 — Organization traversal

Add declared organization-oriented relationship shapes without dynamic cross-dataset joins:

- office → clients summary;
- office → savings accounts summary;
- office → products summary;
- exact savings account → owner office.

Reuse subject datasets where possible. Do not place every office-grouped report into one organization dataset merely because office is an output dimension.

## Phase 5 — Loan identity activation gate

Coordinate with issue 008 before authoring `loan.accounts`.

Required decisions:

- client-owned versus group-owned loan office path;
- exact loan-number source column and sensitivity;
- loan product join;
- confirmed status enum semantics;
- account-level versus product-level interest terms;
- principal, outstanding, arrears, repayment, and charge semantics;
- client/group PII projection;
- read-only approved tables and timeout/row limits.

Only after these are documented and validated may this issue add loan identity/terms shapes. Loan reporting remains deferred until then.

## Security and privacy invariants

- Exact-match identifier lookup only; no prefix/fuzzy enumeration.
- Office scope bound inside approved SQL.
- Raw account/loan numbers never appear in application logs, traces, prompts, audit payloads, errors, or responses.
- Responses use masked identifiers only.
- Unauthorized scope returns the same empty/not-found behavior as a nonexistent identifier; do not reveal cross-office existence.
- Client/borrower identity remains PII-gated.
- Rate limiting applies to identifier lookup surfaces.
- Queries remain single-statement, SELECT-only, authored SQL/dataset fragments.
- No dynamic SQL, arbitrary joins, or user-supplied identifiers.

## Acceptance criteria

### Catalog and validation

- Identifier sensitivity classes are typed and validated.
- Filter-only identifiers permit equality only and cannot be projected.
- Masked output is generated by reviewed SQL or a deterministic approved formatter that never receives broader data than needed.
- Every lookup requires authorized office scope.
- Dataset sources/shapes pass static validation and live `PREPARE`.

### Savings account lookup

- Exact account number resolves identity, owner office, and product.
- Raw identifier is absent from logs/audit/response.
- Leading zeros are preserved.
- Unauthorized and nonexistent account numbers are indistinguishable externally.
- Duplicate/ambiguous non-identifier client lookups clarify safely.
- Interest/overdraft fields identify account versus product source and do not infer missing values.

### Client relationships

- Exact/disambiguated client resolves office.
- Account/product summaries remain within authorized offices.
- PII-disabled responses hide client identity without removing public business fields.

### Loan gate

- Before issue 008 prerequisites are complete, loan-number prompts return the sanitized deferred-domain response and create no execution plan.
- After activation, client-owned and group-owned office scope tests pass independently.

### Verification

Run focused tests only:

- catalog sensitivity/recipe rejection tests;
- exact identifier normalization/masking tests;
- office-scope integration tests;
- PII projection tests;
- savings identity/terms runtime parity tests;
- retrieval tests for representative English and Indonesian phrasings;
- deferred-loan routing tests;
- live `PREPARE`/`EXPLAIN` and HTTP/SSE smoke when Fineract is available.

## Non-goals

- Exposing full account/loan numbers.
- Wildcard identifier search or enumeration.
- Arbitrary relationship graph traversal.
- Cross-domain custom reports assembled by the LLM.
- Write operations, account maintenance, disbursement, repayment posting, or reversals.
- Loan analytics already owned by issue 008 unless directly required for identity/terms lookup.

## Implementation order

1. Identifier sensitivity and masking contract.
2. Savings account identity shape.
3. Savings account terms shape.
4. Client office/account/product relationship shapes.
5. Organization relationship projections.
6. Loan activation only after issue 008 gates pass.

## Links

- `docs/issues/active/008-loan-domain-analyst-capabilities.md`
- `docs/issues/active/009-conversational-drill-down.md`
- `docs/superpowers/specs/2026-07-31-dataset-model-design.md`
- `knowledge/policies/pii.yaml`
- `knowledge/policies/office_scope.yaml`
- `knowledge/schema/fineract/columns/excluded.yaml`
- `knowledge/datasets/savings/account_activity.yaml`
- `knowledge/datasets/savings/account_charges.yaml`
