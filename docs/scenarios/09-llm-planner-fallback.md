# 09 — LLM Planner Fallback

**Phase covered:** Phase 17 constrained planner fallback.
**Precondition:** `LLM_API_KEY` configured for an OpenAI-compatible provider, `API_KEY` allows `savings_deposit_total` and `savings_deposit_top_n`.

## Test status

✅ Passed on 2026-07-02 via local Postman-derived request flow.

## Request

```bash
curl -X POST {{BASE_URL}}/chat/jobs \
  -H "Authorization: Bearer {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{ "session_id": "{{SESSION_ID}}", "message": "Show customer savings activity this week" }'
```

## Expected

The job may either execute a selected approved capability or ask a clarification. The key invariant is safety:

```json
{
  "success": true,
  "data": {
    "status": "waiting_for_user_input",
    "state_json": {
      "classification": {
        "source": "llm_planner",
        "outcome": "clarification_required",
        "options": [
          { "capability": "savings_deposit_top_n" },
          { "capability": "savings_deposit_total" }
        ]
      },
      "execution_plan": null
    }
  }
}
```

If the LLM selects a capability instead, it must be one of the provided options, and execution still goes through Rust parameter extraction, policy checks, and static approved SQL bindings.

## Safety Rules

- The LLM receives approved clarification options, not raw SQL.
- The LLM cannot introduce a capability outside the provided options.
- The LLM cannot author SQL.
- If the LLM fails or returns invalid JSON, the deterministic clarification result is kept.
