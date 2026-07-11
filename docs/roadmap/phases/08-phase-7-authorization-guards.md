# Implementation Steps: Phase 7: Authorization Guards

Source: `docs-old/implementation-steps.md`

## Phase 7: Authorization Guards

Goal: enforce API key scopes before report execution.

Guard checks:

1. Selected capability is allowed by API key.
2. Requested office filter is inside `allowed_office_ids`.
3. PII fields are removed or masked if `can_view_pii=false`.
4. Async job result access belongs to the same API key.
5. Query parameters cannot bypass scopes.

This phase depends on Phase 6 because all report/chat endpoints must receive a validated `ClientContext`.

Failure examples:

```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "forbidden",
    "message": "This API key is not allowed to run the selected capability."
  }
}
```

```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "forbidden",
    "message": "Requested office is outside this API key scope."
  }
}
```

Current status:

```text
DONE

API key authentication produces a ClientContext.
Authorization helpers in crates/chat/src/policy/authorization.rs are wired through chat::chat::planner::evaluate_policy and gate chat::chat::executor::execute_plan.
Office filtering is enforced inside approved SQL via office_id = ANY($3::bigint[]) bound from policy_decision.office_ids.
Still pending: PII response masking templates beyond MVP fields (tracked under Phase 16).
```
