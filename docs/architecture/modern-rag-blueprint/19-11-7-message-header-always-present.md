# Modern RAG Architecture Blueprint: 11.7 Message header (always present)

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

## 11.7 Message header (always present)

First sentence of every non-empty reporting response, generated
deterministically from `answer_plan` (no LLM required):

> "Savings activity from **2026-05-05** to **2026-07-05**, across
> **1 office** (Head Office), in currencies **USD**, **AED**.
> Showing **10** transactions (truncated by `limit=10`)."

Fields:

- Range: from `coverage.requested_range`.
- Office count + names: from `coverage.offices_returned` joined against
  the rows' `office_name` (never a hardcoded lookup).
- Currency list: from `coverage.currencies_returned`, sorted
  alphabetically.
- Truncation note: only when `truncated == true`.
