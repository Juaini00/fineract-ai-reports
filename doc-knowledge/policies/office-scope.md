---
type: Policy
title: Office Scope Policy
description: All Fineract reports must be constrained to the API key's authorized offices via a bound SQL parameter.
resource: ../../knowledge/policies/office_scope.yaml
tags: [policy, security, multi-tenant]
---

# Rules

- Fineract report queries must constrain office ids to `api_key.allowed_office_ids`.
- A user-provided `office_ids` must be a subset of `allowed_office_ids`; if omitted, all allowed offices are used.
- Transaction reports must constrain `m_savings_account_transaction.office_id` with `ANY($office_ids)`.
- Account owner office paths should be validated where practical.

# Enforcement

- Policy guard rejects any requested `office_ids` not in scope.
- Query metadata declares `parameters[office_ids].source: authorized_scope`.
- SQL binds `ANY($n::bigint[])` — never string interpolation, never Rust post-filter.

See [docs/reporting-capabilities](../../docs/reporting-capabilities.md#4-common-mvp-savings-joins).
