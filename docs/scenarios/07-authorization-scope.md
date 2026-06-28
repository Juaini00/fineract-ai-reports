# 07 — Authorization Scope

**Phase covered:** Phase 7 (helpers) + Phase 13 (`evaluate_policy` gate) + Phase 14 (SQL office-id bind).
**Precondition:** Two API keys with different scopes.

## Test status

✅ Passed on 2026-06-28 rerun.

- Narrow key top-N request ended as `waiting_for_user_input` with `classification.outcome=clarification_required`, showing out-of-scope top-N was not directly executed.
- Narrow key total request completed with `policy_decision.office_ids=[1]` and `result_json.query_id=savings.deposit_total`.
- Full key total request completed with `policy_decision.office_ids=[1,2,3]`.
- Cross-key job read returned HTTP 404.
- No ❌ failure observed. Full and narrow totals were equal in local data (`280.000000`), so the "larger total" comparison was not a useful data assertion in this run.

Create a second API key (`API_KEY_NARROW`) using `02-auth-api-keys.md` with:

```json
{
  "name": "narrow-client",
  "owner": "Bob",
  "allowed_office_ids": [1],
  "allowed_capabilities": ["savings_deposit_total"],
  "can_view_pii": false
}
```

## A. Capability gate

Use `API_KEY_NARROW` for a top-N question that needs `savings_deposit_top_n`:

```bash
curl -X POST {{BASE_URL}}/chat/jobs \
  -H "Authorization: Bearer {{API_KEY_NARROW}}" \
  -d '{ "message": "Show the largest deposits today" }'
```

### Expected job end-state
- `classification.candidates` is empty for `savings_deposit_top_n` because the SQL filter `source_id = ANY($allowed)` excludes it.
- Either clarifies to `savings_deposit_total` (if confidence is high enough) or fails as `unsupported` (`source: "vector_no_match"`).
- `policy_decision.status = "allowed"` only when the picked capability is in scope; otherwise `denied` with `reason: "capability_not_allowed"`.

## B. Office scope in approved SQL

Run the happy-path total job with `API_KEY_NARROW`. The executor binds `policy_decision.office_ids = [1]` to the `$3::bigint[]` placeholder.

```sql
WHERE ... AND t.office_id = ANY($3::bigint[])
```

### Verify
- `result_json.rows` totals are computed ONLY over office 1, even if rows exist in offices 2 and 3.
- There is no Rust post-filter — office scope lives inside the SQL itself.

### Compare with full-scope key
Same question with `{{API_KEY}}` (offices `[1, 2, 3]`) should return a larger total only when offices 2 or 3 have matching rows for the requested date window. If local data has matching rows only in office 1, verify the scope through `state_json.policy_decision.office_ids` instead.

## C. PII flag (placeholder)

`can_view_pii: false` is persisted and queryable. PII-aware response masking is **deferred** to Phase 16 expansion — current templates do not yet expose PII fields, so this is a latent guarantee, not an observed mask.

## D. Job ownership

```bash
curl {{BASE_URL}}/chat/jobs/{{JOB_ID}} -H "Authorization: Bearer {{OTHER_API_KEY}}"
```

### Expected
- HTTP 404 — `JobRepository::get_for_client` filters by `api_key_id`. Jobs are not visible across keys, even with a valid key.

## Failure modes

| Trigger | Expected |
| --- | --- |
| API key with empty `allowed_capabilities` | Classification short-circuits to `unsupported` (`source: "no_allowed_capabilities"`) — no Voyage call, no Fineract query |
| Office id in request outside scope | `evaluate_policy` returns `denied` with `reason: "office_outside_scope"`; executor refuses to run |
| `policy_decision.status = "denied"` | Executor `bail!`s; job ends `failed` with `code: "execution_failed"` |
