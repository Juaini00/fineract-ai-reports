---
type: Trace
title: Trace 04 — Missing date range → clarify → resume
description: Same job across two HTTP calls. Shows how clarification resumes rather than spawning a new job.
tags: [trace, clarification, savings, withdrawal]
---

# Turn 1 · User prompt

> **EN:** "How much did we withdraw?"
> **ID:** "Berapa penarikan yang terjadi?"

# 1 · Classifier

```json
{
  "intent": "aggregate_over_period",
  "candidate_capabilities": ["savings_withdrawal_total"],
  "extracted_params": {},
  "output_mode_hint": "total",
  "requires_date_range": true
}
```

# 2 · Planner

Match on `savings_withdrawal_total`. `required_parameters = [from_date, to_date]` — **both missing**.

```json
{
  "decision": "clarify",
  "capability_id": "savings_withdrawal_total",
  "missing": ["from_date", "to_date"],
  "template_key": "missing_date_range"
}
```

# 3 · Response (job stays open)

```
Which date range should this report use?
```

- `chat_jobs.status = needs_clarification`
- `chat_jobs.state_json` retains the partial plan.
- SSE stream stays connected; `live_state != completed`.

# Turn 2 · User answers

```
POST /chat/jobs/{job_id}/responses
{
  "message": "From April 1 to April 30 2026."
}
```

- **New `chat_message` appended.** No new `chat_job`.
- Handler calls `JobService::respond(job_id, ...)`, which merges the extracted `from_date="2026-04-01"`, `to_date="2026-04-30"` into the saved plan.
- Pipeline re-enters at **policy** (step 5 of [request-flow](../architecture/request-flow.md)), then executor, then formatter.

# 4 · Final response

```
Total withdrawals from 2026-04-01 to 2026-04-30: IDR 1,847,300,000.00 across 5,412 transactions.
```

# Invariants exercised

- Clarification **continues the same job**. If the client had instead called `POST /chat/messages` again with a new prompt, that would be a new job — losing the accumulated plan and doubling the LLM/vector cost.
- All prior classification remains in `state_json`; only the newly extracted params merge in.
- If Turn 2 still doesn't answer both dates, planner emits `clarify` again with a more specific template (`missing_from_date` or `missing_to_date`).
