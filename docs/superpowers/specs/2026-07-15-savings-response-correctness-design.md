# Savings Clarification Continuation and Response Correctness Design

## Goal

Correct savings clarification continuation so a selected capability remains
authoritative when the initial request lacks required parameters. Extend the
existing savings answer-quality suite to independently verify every HTTP
response value against the live read-only Fineract database.

## Scope

- Fix continuation through the existing job-response endpoint; do not create a
  new job for a clarification.
- Cover all seven approved savings capabilities:
  1. deposit total
  2. deposit top-N
  3. monthly deposits
  4. monthly deposit top-N
  5. balance summary
  6. withdrawal total
  7. withdrawal top-N
- Retain authorization office-scope checks and client-response no-leak checks.
- Change test coverage and the minimal runtime behavior required to satisfy it.

## Non-Goals

- No new savings capabilities, catalog schema, or API routes. The minimal
  `TestApp` change may expose its existing read-only Fineract pool to this test.
- No write access to Fineract.
- No Rust-side office filtering or relaxed authorization behavior.
- No changes to response-envelope, PII, or English-only policies.

## Clarification Continuation Contract

When capability selection succeeds but parameter extraction reports missing
required values, persist the selected capability as continuation context on the
same job. `POST /chat/jobs/{job_id}/responses` must merge the supplied answer
with the existing request context, then resume planning/execution using that
persisted capability.

The resumed request must not depend on reclassification from the short
clarification answer. A response such as a date, quantity, or month therefore
cannot cause routing to a different capability or fail merely because it lacks
the original savings wording. Existing policy evaluation remains before query
execution, using the authenticated client's capability and office scopes.

If the job has no valid persisted selected capability, retain current fail-safe
behavior rather than guessing from clarification text.

## HTTP Correctness Test Design

Extend `crates/chat/tests/savings_answer_quality.rs`, reusing
`crates/chat/tests/common/mod.rs` for app setup and authenticated HTTP requests;
the minimal `TestApp` pool exposure provides live Fineract access. Each scenario
must exercise the public HTTP flow:

1. Create or use an authorized client and submit the savings request.
2. For parameterized capabilities, assert that the job requests clarification,
   submit the missing value to the same job, and await completion.
3. Read the completed HTTP response envelope.
4. Run an independent, read-only SQL assertion against Fineract for the same
   capability, parameters, and authorized office ids.
5. Compare every reported value and ordering-relevant field to the independent
   result, not to the application query or formatter output.

The seven scenario assertions must validate:

- totals: exact aggregate amount and applicable currency/period metadata;
- top-N results: exact rows, aggregate values, deterministic order, and limit;
- monthly results: exact month buckets and amounts in returned order;
- balance summary: each reported summary value and its applicable scope;
- clarification cases: selected capability identity before and after response,
  plus values from the resumed query.

Independent assertions may share fixture and connection helpers, but must use
separately written read-only SQL in the test rather than call repository,
planner, executor, catalog-query, or response-rendering code under test.

## Security Assertions

For every capability scenario, preserve or add assertions that:

- rows and aggregates are restricted by the authorized office ids in SQL;
- a narrower-scope client cannot obtain out-of-scope data;
- HTTP output contains no raw SQL, database errors, stack traces, prompts,
  unapproved office data, or internal execution details;
- failures remain sanitized envelope errors.

## Acceptance Criteria

- All seven capabilities have HTTP-level response-value comparisons against
  independently queried live read-only Fineract data.
- At least one missing-parameter path proves that the same selected savings
  capability executes after clarification without reclassification.
- Top-N, monthly, and balance-summary assertions cover their full returned
  data, not only a headline value.
- Office-scope and no-leak assertions pass alongside value comparisons.
- The targeted `savings_answer_quality` test suite passes against the required
  local runtime dependencies.

## Validation Command

```bash
cargo test -p chat --test savings_answer_quality
```
