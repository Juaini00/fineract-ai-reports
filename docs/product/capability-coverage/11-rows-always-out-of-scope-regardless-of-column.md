# Capability Coverage Matrix: Rows always out of scope regardless of column

Source: `docs-old/capability-coverage-matrix.md`

## Rows always out of scope regardless of column

- Arbitrary SQL or schema exploration exposed to end users.
- Raw account numbers, external ids, payment references, tokens, secrets, or any field marked `secret_never_expose` — even with `can_view_pii=true`.
- Write operations (INSERT/UPDATE/DELETE/DDL/COPY) against Fineract.
- Cross-tenant reads (offices outside the API key `allowed_office_ids`) — enforced *inside* SQL, not as Rust post-filter.
- Reproducing raw AI planner output, prompts, or internal command JSON in user responses.
- Model training or fine-tuning over Fineract data.
- Bulk table export or CDC feeds.
- Individual staff app-user account, credential, session, or audit records — separately governed at the Fineract layer.
