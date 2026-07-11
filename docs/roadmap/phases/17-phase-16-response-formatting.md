# Implementation Steps: Phase 16: Response Formatting

Source: `docs-old/implementation-steps.md`

## Phase 16: Response Formatting

Goal: return user-friendly answers.

MVP response strategy:

```text
template first
LLM provider later
```

Example:

```text
The largest savings deposit today is IDR 25,000,000 from account SV-001.
```

If PII is not allowed:

```text
The largest savings deposit today is IDR 25,000,000 from account SV-****001.
```

Current status:

```text
PARTIALLY DONE

Implemented:
crates/chat/src/chat/formatter.rs
Successful report execution inserts an assistant chat_messages row with a simple English template response.
GET /chat/sessions/{session_id}/messages now shows user and assistant messages after successful execution.
Savings formatter returns empty-result messages for total/top-N/monthly report shapes and only prefixes amounts with a runtime `currency_code` when one is present in query output or request params.
Response formatting is now catalog-driven by `query_id`, `output_mode`, declared `output_fields`, and response field labels instead of hardcoded capability IDs.
PII/secret output fields are omitted by the generic formatter unless a future explicit PII-aware formatter is added.

Still pending:
LLM formatting fallback for complex responses
```
