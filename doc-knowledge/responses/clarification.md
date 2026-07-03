---
type: Response
title: Clarification Templates
description: Questions asked when required parameters are missing or intent is ambiguous.
resource: ../../knowledge/responses/clarification.yaml
tags: [responses, clarification]
---

# Templates

| Missing / ambiguous | Prompt |
|---|---|
| `missing_date_range` | "Which date range should this report use?" |
| `missing_from_date` | "What start date should this report use?" |
| `missing_to_date` | "What end date should this report use?" |
| `missing_limit` | "How many top records should be returned?" |
| `missing_office_scope` | "Which authorized office scope should this report use?" |
| `ambiguous_output_mode` | "Do you want a combined total, transaction list, or period breakdown?" |
| `ambiguous_savings_deposit` | "Do you want total deposits or the largest deposit transactions?" |

# Job continuity

Clarification answers **must** continue the same job via `POST /chat/jobs/{job_id}/responses`. Do not spawn a new job. See [../policies/unsupported-requests](../policies/unsupported-requests.md).
