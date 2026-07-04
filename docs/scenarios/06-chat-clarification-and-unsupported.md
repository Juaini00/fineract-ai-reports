# 06 — Clarification + Unsupported

**Phase covered:** Phase 12–13 decision policy (`unsupported_threshold`, `clarify_score`, close-candidate margin).
**Precondition:** Same as `05`.

## Test status

✅ Passed on 2026-06-28 rerun after the off-domain override fix.

- ✅ A: `deposits this month` ended `waiting_for_user_input` with `classification.outcome=clarification_required`, `source=vector`, and 2 options.
- ✅ A.1: response `{ "message": "1" }` returned HTTP 201 and continued the same job to `completed` with `source=clarification_option`.
- ✅ A.2: `Show customer savings activity this week` includes `other_activity`; free-text response `all acticity for this week` ends `failed` with `source=clarification_other` instead of repeating clarification.
- ✅ B: write intent `create a new savings account` ended `failed`, `source=write_intent`, `error_json.code=unsupported_request`.
- ✅ C: API key with `allowed_capabilities=[]` can be created; the job ends `failed`, `source=no_allowed_capabilities`, `error_json.code=unsupported_request`.
- ✅ D: `banana` ended `failed`, `source=vector_no_match`, `error_json.code=unsupported_request`.
- ✅ E: loan/accounting/group-center now end `failed` with `source=off_domain_match`; tax ends `failed` with `source=vector_no_match`.

## A. Clarification — ambiguous deposit question

```bash
curl -X POST {{BASE_URL}}/chat/jobs \
  -H "Authorization: Bearer {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{ "session_id": "{{SESSION_ID}}", "message": "deposits this month" }'
```

### Expected job end-state
```json
{
  "status": "waiting_for_user_input",
  "current_step": "taking_decision",
  "state_json": {
    "classification": {
      "outcome": "clarification_required",
      "options": [
        { "label": "...", "capability": "savings_deposit_total" },
        { "label": "...", "capability": "savings_deposit_top_n" }
      ],
      "source": "vector"
    }
  }
}
```

### SSE
```text
event: update
data: {"kind":"clarification","step":"taking_decision","payload":{"options":[...]}}
```

Assistant message inserted with `metadata_json.type = "clarification"` and the options list.

## A.1 Respond to clarification

User picks option 1 (option text, capability id, or 1-based number all accepted):

```bash
curl -X POST {{BASE_URL}}/chat/jobs/{{JOB_ID}}/responses \
  -H "Authorization: Bearer {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{ "message": "1" }'
```

### Expected
- HTTP 201 with the inserted user message.
- Background pipeline re-runs `classify_clarification_response` → builds plan → executes. **Same `JOB_ID` continues** — no new job is created.
- Final SSE `update` event reaches `final` step with `status: completed`.

## A.2 Respond with other activity

```bash
curl -X POST {{BASE_URL}}/chat/jobs \
  -H "Authorization: Bearer {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{ "session_id": "{{SESSION_ID}}", "message": "Show customer savings activity this week" }'
```

### Expected clarification options
```json
[
  { "capability": "savings_deposit_top_n", "label": "Largest deposit this week" },
  { "capability": "savings_deposit_total", "label": "Total deposit this week" },
  { "capability": "savings_withdrawal_top_n", "label": "Largest withdrawal this week" },
  { "capability": "savings_withdrawal_total", "label": "Total withdrawal this week" },
  { "capability": "other_activity", "label": "Other activity this week" }
]
```

If the user responds with `all acticity for this week`, final job state is `failed` with `state_json.classification.source = "clarification_other"`. It must not ask the same clarification again.

Free text is accepted too: `Maybe the largest deposit is good choice` should select `savings_deposit_top_n`; users do not need to copy option labels exactly.

## B. Unsupported — write intent

```bash
curl -X POST {{BASE_URL}}/chat/jobs \
  -H "Authorization: Bearer {{API_KEY}}" \
  -d '{ "session_id": "{{SESSION_ID}}", "message": "create a new savings account" }'
```

### Expected job end-state
```json
{
  "status": "failed",
  "state_json": {
    "classification": {
      "outcome": "unsupported",
      "source": "write_intent",
      "candidates": []
    }
  },
  "error_json": {
    "code": "unsupported_request",
    "message": "No approved reporting capability matched this request."
  }
}
```

The write-intent guard rejects **before** spending any embedding tokens.

## C. Unsupported — no allowed capability

Repeat with an API key whose `allowed_capabilities` is `[]`. Classification short-circuits with `source: "no_allowed_capabilities"`.

✅ Rerun result: API key creation with `allowed_capabilities: []` succeeds. A deposit job with that key fails before retrieval with `classification.source = "no_allowed_capabilities"`.

## D. Unsupported — low confidence

A nonsense message such as `"banana"` → embedding runs, but top capability distance falls below `0.40` confidence → job `failed` with `source: "vector_no_match"`.

## E. Unsupported — deferred domain detection

After the knowledge expansion plus the **off-domain override** in `JobService::context_overrides_capability`, retrieval can recognize off-MVP intents and override an otherwise-matching savings capability to `unsupported`. The override fires when:

1. Top context candidate (data_area or domain) outranks the top capability candidate by `> 0.10` confidence margin, AND
2. Top context confidence is `>= 0.50`, AND
3. The matched source is non-executable per the catalog: status `deferred`, `deferred_group`, `rejected`, `rejected_group`, `out_of_scope`, or `candidate` (per `knowledge-catalog.md` §2.5 + group_center default rule).

When the override fires, `classification.source = "off_domain_match"`. When the capability already failed confidence (`< 0.40`), the source stays `vector_no_match` and no override is needed.

### Loan intent

✅ Rerun result: candidates included `loan (domain)` and `loans (data_area)`; job ended `failed` with `source=off_domain_match` and `error_json.code=unsupported_request`.

```bash
curl -X POST {{BASE_URL}}/chat/jobs \
  -H "Authorization: Bearer {{API_KEY}}" \
  -d '{ "session_id": "{{SESSION_ID}}", "message": "total loan disbursement this month" }'
```

Expected end-state:
```json
{
  "status": "failed",
  "state_json": {
    "classification": {
      "outcome": "unsupported",
      "source": "off_domain_match",
      "candidates": [
        { "capability": "savings_deposit_total", "source_type": "capability", "confidence": <n> },
        { "capability": "loan", "source_type": "domain", "confidence": <n> },
        { "capability": "loans", "source_type": "data_area", "confidence": <n> }
      ]
    }
  }
}
```

Reading the candidates: the savings capability matched mid-confidence (because of "total" + "month"), but the loan domain matched higher (because of "loan disbursement"). The override gate triggered and the decision flipped to `unsupported`.

### Accounting intent
`"show me journal entries today"` → expected `source = "off_domain_match"` because `accounting` domain (`status: deferred`) outranks any savings candidate.

✅ Rerun result: candidates included `accounting (domain)` and `accounting_gl (data_area)`; job ended `failed` with `source=off_domain_match` and `error_json.code=unsupported_request`.

### Tax intent
`"how much tax was collected this quarter"` → savings capability candidates are weak (confidence `< 0.40`), so `classify_from_candidates` returns `None` and the result is `unsupported` with `source = "vector_no_match"`. Override is not needed (and does not fire because `result.outcome` is already `unsupported`). Context candidates still show `tax (domain)` and `tax (data_area)`.

✅ Rerun result: job ended `failed`, `source=vector_no_match`, `error_json.code=unsupported_request`, with `tax (domain)` and `tax (data_area)` candidates.

### Group/center intent
`"show me groups with the most deposits this month"` → `group_center` domain (`status: candidate`) outranks savings capability and the override fires with `source = "off_domain_match"`. Phrasings dominated by "deposits" (e.g. `"group X total deposits this month"`) may still execute `savings.deposit_total` if the savings signal is strong enough to outrank `group_center`. ponytail: tolerable — savings total is the closest in-scope answer, and the user can clarify with a follow-up.

✅ Rerun result: candidates included `group_center_foundation (data_area)` and `group_center (domain)`; job ended `failed` with `source=off_domain_match` and `error_json.code=unsupported_request`.

### Verification via vector-index
After `POST /vector-index/rebuild`, `GET /vector-index/status` should show `document_count=72` for the current expanded catalog.

### Verification via job state
For loan / accounting cases, confirm `state_json.classification.source = "off_domain_match"`. For tax, confirm `state_json.classification.source = "vector_no_match"`. Both should have `status: "failed"` and `error_json.code = "unsupported_request"`.

## Failure modes / edge cases

| Trigger | Expected |
| --- | --- |
| Clarification respond with `"3"` when only 2 options | Re-clarifies (or unsupported), no execution |
| Respond to a completed/failed job | HTTP 409 or 404 depending on path |
| Respond with empty `message` | HTTP 400 validation error |
