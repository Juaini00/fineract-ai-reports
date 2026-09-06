---
type: Response
title: Clarification Templates
description: Safe English business copy for clarification prompts.
resource: ../../knowledge/responses/clarification.yaml
tags: [responses, clarification]
---

# Clarification copy

The typed clarification contract and catalog input contracts are authoritative for kind, fields, options, validation, defaults, and whether a clarification is needed. This template file supplies only safe English-facing prompt copy; prose must not define client behavior or validation.

| Template | Prompt |
|---|---|
| `missing_date_range` | "Which date range should this report use?" |
| `missing_from_date` | "What start date should this report use?" |
| `missing_to_date` | "What end date should this report use?" |
| `missing_limit` | "How many top records should be returned?" |
| `missing_office_scope` | "Which authorized office scope should this report use?" |
| `ambiguous_output_mode` | "Do you want a combined total, transaction list, or period breakdown?" |
| `ambiguous_savings_deposit` | "Do you want total deposits or the largest deposit transactions?" |

# Continuity and safety

Answers continue the same job via `POST /chat/jobs/{job_id}/responses`; never create a job for a clarification answer. “Others” is a report/request escape only. Help/detail is local presentation, not an answer. Keep copy safe, business-facing, and English-only.
