# Golden dataset

`assistant_scenarios.jsonl` is one JSON object per line with these exact fields:

- `prompt`: user message.
- `expected_intent`: snake_case assistant intent.
- `expected_domain`: expected reporting domain or `unknown`.
- `expected_entities`: array of expected entity hints.
- `expected_constraints`: object with expected constraint hints.
- `expected_response_type`: snake_case response shape expected by the assistant.
