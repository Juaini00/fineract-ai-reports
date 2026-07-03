---
type: Policy
title: Unsupported Requests Policy
description: Which user intents are hard-rejected vs. routed to clarification.
resource: ../../knowledge/policies/unsupported_requests.yaml
tags: [policy]
---

# Hard reject

Arbitrary SQL, full-schema exploration, write/update Fineract data, requests for secret fields, raw account numbers / external ids / payment refs / command JSON, out-of-scope tables, deferred domains in MVP, office scope outside `allowed_office_ids`. These map to [../responses/unsupported](../responses/unsupported.md).

# Clarify

Missing date range, ambiguous output mode, ambiguous domain, missing `limit` for `top_n`. Clarification continues the same chat job via `POST /chat/jobs/{job_id}/responses` — never a new job. Templates in [../responses/clarification](../responses/clarification.md).
