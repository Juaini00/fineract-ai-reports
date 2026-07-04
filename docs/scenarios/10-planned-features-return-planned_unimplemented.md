# Planned Features Return `planned_unimplemented`

**Phase covered:** Decision policy fourth outcome — see `docs/ai-reporting-design.md` §18.3.

**Precondition:**

- `API_KEY` exists with all savings capabilities granted.
- `POST /vector-index/rebuild` has been run so the current capability YAML is indexed.
- The `PlannedUnimplemented` classifier outcome is enabled (not yet in `master`; scenario is a contract for the change).

## Intent

Prove the fourth terminal outcome. A reasonable, on-roadmap ask ("show me the weekly deposit breakdown this month") must:

1. Match semantically in retrieval.
2. Resolve to a coverage-matrix cell whose status is `planned`.
3. End the job in a distinct `planned_unimplemented` terminal state.
4. Return a sanitized, fixed template — never SQL, never partial data, never a fallback to the closest `implemented` capability.

If the classifier silently substitutes `savings_deposit_monthly_breakdown` instead, this scenario fails — that is the exact hallucination we are guarding against.

## Request A — English, weekly breakdown

```bash
curl -sS -X POST "$BASE_URL/chat/messages" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "session_id": "'"$SESSION_ID"'",
    "content": "Show me the weekly deposit breakdown for this month, per week."
  }'
```

## Request B — Bahasa Indonesia, custom bucket

```bash
curl -sS -X POST "$BASE_URL/chat/messages" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "session_id": "'"$SESSION_ID"'",
    "content": "Tampilkan setoran per 10 hari untuk bulan ini."
  }'
```

## Request C — Composite multi-metric

```bash
curl -sS -X POST "$BASE_URL/chat/messages" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "session_id": "'"$SESSION_ID"'",
    "content": "Show me the biggest deposit, biggest withdrawal, biggest charge, and biggest hold this month in one report."
  }'
```

## Expected response (HTTP 202 followed by SSE terminal event)

Envelope:

```json
{
  "success": true,
  "data": {
    "job_id": "…",
    "status": "queued"
  },
  "error": null
}
```

Then via `GET /chat/jobs/{id}/stream` the terminal SSE event:

```json
{
  "type": "job.completed_terminal",
  "job_id": "…",
  "status": "planned_unimplemented",
  "outcome": {
    "kind": "planned_unimplemented",
    "matched_matrix_cell": {
      "category": "Savings deposit",
      "shape": "Aggregate by week",
      "target_milestone": "v0.2"
    },
    "message": "This report is planned but not yet available in this release. Expected in v0.2."
  }
}
```

For Request C the `matched_matrix_cell` is `"Composite multi-metric"` × `"Composite (multiple metrics one request)"` with target `v0.3`, because at least one leaf metric (charge, hold) is not yet `implemented` and the composite planner (§18.2) is not yet shipped. Per §18.2, one `planned` leaf demotes the whole batch to `planned_unimplemented`.

## Side effects

- **Postgres `chat_jobs`.** Row exists with `status='planned_unimplemented'`, `state_json.classification.outcome='planned_unimplemented'`, `state_json.classification.matched_matrix_cell` populated. No `execution.rows` field.
- **Postgres `chat_job_events`.** One row with `event_type='planned_unimplemented'` carrying the matched matrix cell.
- **Postgres `chat_messages`.** Assistant reply is the fixed template only — no metric numbers, no capability id, no SQL fragment.
- **Redis `chat_job:{id}:live_state`.** Set to `planned_unimplemented` with 1h TTL. Distinct from both `completed` and `unsupported`.
- **Query log / Fineract read replica.** Zero rows read. No prepared statement issued against Fineract for this job id. This is the strongest side-effect assertion in the scenario — if any SQL runs, the outcome is wrong.

## Failure modes

| Trigger | Expected response |
| --- | --- |
| Classifier substitutes `savings_deposit_monthly_breakdown` because monthly is retrieval-adjacent to weekly | **BUG.** Terminal status should be `planned_unimplemented`, not `completed`. Retrieval score threshold or intent-shape disambiguation is misconfigured. |
| Classifier drops to `unsupported` with reason `vector_no_match` | **BUG.** The example set on the planned matrix cell has no synonyms; add bilingual examples so retrieval hits the `planned` sentinel. |
| Executor runs any SQL | **BUG.** The `PlannedUnimplemented` outcome must short-circuit before executor. See §18.3 assignment rule. |
| Response includes the target milestone as an absolute date | **BUG.** Template must reference the milestone name (`v0.2`), not a calendar promise. |
| Composite request Request C returns a partial answer for the two `implemented` leaves | **BUG.** Per §18.2, a single `planned` leaf demotes the whole batch. Partial answers are never acceptable — they train users to trust invalid composites. |
| `PlannedUnimplemented` outcome is not yet in code | **Expected today.** The scenario is a contract. Until §18.3 ships, these requests will return `unsupported`. Track under coverage-matrix rows tagged `planned` — closing this scenario is a gate for the §18.3 feature.

## Data leak checks

Independent of correctness, verify no data ever leaves the boundary:

- Assistant message body must not contain: a capability id, a SQL fragment, a table name, a row count, any numeric metric, any client/office id.
- SSE terminal event `outcome.message` is only the fixed template plus milestone name.
- `chat_job_events` payload for `planned_unimplemented` must not include any user PII from the request; only the matrix cell reference.

## Cross-references

- Coverage matrix: `docs/capability-coverage-matrix.md`
- Decision policy: `docs/ai-reporting-design.md` §8 and §18.3
- Non-goals (for the neighboring `unsupported` outcome): `docs/reporting-capabilities.md` §12
