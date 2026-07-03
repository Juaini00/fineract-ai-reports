---
type: Trace
title: Trace 05 — Deferred domain → unsupported (sanitized)
description: Loan reporting is deferred in MVP. Hard reject with a sanitized template — no internal detail leaks.
tags: [trace, unsupported, loan, deferred]
---

# User prompt

> **EN:** "How much loan did we disburse last month?"
> **ID:** "Berapa total pencairan pinjaman bulan lalu?"

# 1 · Classifier

Vector match on **loan** domain (candidates: `loan_disbursement_total`, etc.).

```json
{
  "intent": "aggregate_over_period",
  "candidate_capabilities": [],
  "candidate_domains": ["loan"],
  "extracted_params": { "from_date": "2026-06-01", "to_date": "2026-06-30" }
}
```

**No approved capability** exists in the `loan` domain — see [domains/loan](../domains/loan.md) (status `deferred`).

# 2 · Planner

- No `approved_mvp` capability match.
- Domain lookup: `loan.status = deferred` → route to [policies/unsupported-requests → hard_reject](../policies/unsupported-requests.md).

```json
{
  "decision": "unsupported",
  "reason": "deferred_domain",
  "template_key": "deferred_domain"
}
```

# 3 · Response (from [responses/unsupported](../responses/unsupported.md))

```
This data area is documented but not enabled for MVP reporting yet.
```

- `chat_jobs.status = failed_unsupported`
- **No SQL executes.** Fineract loan tables are never queried; no data leaks.

# What is deliberately NOT in the response

- The internal reason code (`deferred_domain`) — that's an implementation detail.
- The list of loan tables that would have been consulted (`m_loan`, `m_loan_transaction`, …) — that would leak schema.
- The names of the candidate capabilities the LLM considered — leaking those would help enumerate the deferred backlog.
- Any prompt, stack trace, or SQL fragment.

See [policies/query-safety](../policies/query-safety.md): sanitized errors only.

# Variant · Out-of-scope request

If the user had asked "give me the raw `m_loan` rows" or "run `SELECT * FROM m_loan`":

- Classification wouldn't match any capability.
- Policy `unsupported_requests.hard_reject` catches "arbitrary SQL" / "request out-of-scope tables".
- Response template: `unsafe_request` → "This request cannot be processed because it violates reporting safety rules."

Same sanitized shape. Same audit trail (`chat_job_events`). Same zero data disclosure.
