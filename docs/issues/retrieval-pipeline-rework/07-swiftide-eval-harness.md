# 07 — Regression eval harness with fixture set

**Parent:** [Epic](./README.md) · **Priority:** P2 · **Effort:** M (~200 LoC + fixtures)

## Problem

Every fix in this epic (issues 01, 02, 03, 04) risks regressing queries that currently work. Existing tests cover unit-level scenarios of the matcher but nothing end-to-end at the intent-classification-and-retrieval boundary. Prompt tweaks are invisible until a real user hits a broken query.

## Proposed change

A fixture set + eval runner using `swiftide 0.32`'s evaluator components (or a hand-rolled equivalent if swiftide's is too heavy for this scale):

**Fixture format** — `crates/chat/tests/fixtures/retrieval_eval/*.yaml`:

```yaml
- id: id_ranking_savings_account
  message: "3 clients where have the most savings account for this year"
  language: en
  expected_decision: select
  expected_capability: client_top_n_by_savings_account_count

- id: id_random_client
  message: "coba berikan saya 5 client sembarang pada tahun ini"
  language: id
  expected_decision: select
  expected_capability: client_random_sample     # after issue 03

- id: id_out_of_scope_weather
  message: "what's the weather today"
  language: en
  expected_decision: unsupported
  expected_capability: null
```

Target: **20 fixtures** across ID/EN, covering select/clarify/unsupported paths, all 3 domains (client/organization/savings), and edge cases (empty message, PII request without permission, follow-up context reference).

**Runner** — `crates/chat/tests/retrieval_eval.rs`:

- Load fixtures.
- For each: construct `RuntimeContext`, invoke the assistant graph (LLM stubbed OR real if `EVAL_USE_REAL_LLM=1` env is set).
- Compute top-1 accuracy per language + per decision-type.
- Fail the test if accuracy drops below floor: **90% overall**, **85% per bucket**.

Optional CI integration: run stubbed eval on every PR; run real-LLM eval nightly against staging.

## Files

- `crates/chat/tests/fixtures/retrieval_eval/*.yaml` — 20 fixtures.
- `crates/chat/tests/retrieval_eval.rs` — runner.
- `crates/chat/src/assistant/eval.rs` (optional) — swiftide adapter if useful.
- `.github/workflows/eval.yml` (optional) — nightly job.

## Acceptance criteria

- 20 fixtures committed.
- Test passes at the accuracy floor on current code (baseline captured).
- Test fails if any issue 01-05 regresses accuracy below floor.
- Runtime under 30s for stubbed run; under 5 min for real-LLM run.

## Test plan

- Meta: run the eval before merging issues 01, 02, 03, 04 to establish baseline. Run again after each merge to measure impact.
- Fixture coverage review: at least 2 fixtures per capability domain, at least 4 clarify cases, at least 3 unsupported cases.

## Out of scope

- End-to-end SQL execution testing (that's the executor's concern, covered by `chat/tests/assistant_*_quality.rs`).
- Response formatting quality. Retrieval accuracy only.

## Dependencies

- Best after issue 06 (trace) — makes fixture failures easier to diagnose.
- Best after issue 03 (browse primitives) — otherwise browse-fixtures can only expect `unsupported`.
