---
type: Policy
title: PII Policy
description: Sensitivity classes and rules for when identity/reference fields may be returned by a capability.
resource: ../../knowledge/policies/pii.yaml
tags: [policy, pii, security]
---

# Rules

Default behavior: `omit`. `can_view_pii=true` on the API key is necessary but not sufficient — the selected capability must **also** explicitly allow every PII field it returns.

Sensitivity classes: `public_business`, `sensitive_business_identifier`, `pii`, `security_sensitive`, `secret_never_expose`, `free_text_sensitive`. Fields classified `secret_never_expose` (`password_hash`, `command_json`, raw account numbers, external ids, payment refs) must never be fetched, returned, logged, or sent to the AI.

Prefer aggregate reporting. Do not select fields you will then omit.

# Enforcement

Wired into `chat::policy::authorization::evaluate_policy`, called before `chat::chat::executor::execute_plan`. Full spec: [docs/reporting-pii-policy](../../docs/product/pii-policy/index.md).
