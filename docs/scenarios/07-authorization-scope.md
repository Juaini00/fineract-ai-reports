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

## C. PII flag

`can_view_pii: false` is persisted and queryable, **but it does not bind in the chat pipeline.** Chat is admin-only and runs an admin projection (`project_admin_principal`) that forces `can_view_pii = true` on every job, so `pii`-class columns are not hidden by `response_builder::is_hidden` regardless of the key's flag. The masking logic itself works and is unit-tested — it is simply never reached with `can_view_pii = false` on the chat path. Treat the per-key flag as advisory for chat; it would bind only on a non-admin path or if scope were attached to the bearer/user identity instead.

## D. Job ownership

```bash
curl {{BASE_URL}}/chat/jobs/{{JOB_ID}} -H "Authorization: Bearer {{OTHER_SESSION_JWT}}"
```

### Expected
- HTTP 404 — jobs and sessions are scoped by `user_id` from the Bearer session, **not** by `api_key_id`. Jobs are not visible across users. (The older `JobRepository::get_for_client` / per-`api_key_id` filter no longer exists.)

## Failure modes

| Trigger | Expected |
| --- | --- |
| API key with empty `allowed_capabilities` | **No effect in chat** — `project_admin_principal` overwrites `capability_ids` with every `approved_mvp` capability, so classification proceeds normally. The short-circuit applies only where a non-admin projection is used |
| Office id in request outside scope | `evaluate_policy` returns `denied` with `reason: "office_outside_scope"`; executor refuses to run |
| `policy_decision.status = "denied"` | Executor `bail!`s; job ends `failed` with `code: "execution_failed"` |
