---
type: Response
title: Reporting Response Templates
description: Success-path templates for report output modes plus field labels and formatting rules.
resource: ../../knowledge/responses/reporting.yaml
tags: [responses]
---

# Templates

- `total` → "Total {metric_label} from {from_date} to {to_date} is {value}."
- `top_n` → "Here are the top {limit} transactions from {from_date} to {to_date}."
- `completed` → "Report completed."
- `empty_result` → "No data was found for the requested parameters."

# Rules

Use only fields the selected capability's output contract declares. Never expose PII unless explicitly allowed. Never mention SQL, prompts, stack traces, or policy internals. Dates in `YYYY-MM-DD`. Preserve database decimal precision.
