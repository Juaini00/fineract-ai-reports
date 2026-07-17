# Savings Clarification and HTTP Response Correctness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

## Goal

Preserve a previously selected savings capability while collecting its missing
parameters, then prove all seven savings HTTP answers match independently
queried, read-only Fineract data within the caller's office scope.

## Architecture

Continue an existing chat job's selected savings capability through clarification,
and independently verify rendered HTTP answers against live Fineract reads.

## Tech Stack

Rust, Axum, SQLx, PostgreSQL/Fineract, and the existing chat integration tests.

## Global Constraints

- Add no dependencies or routes; change no catalog files or authorization semantics.
- Use only live, read-only Fineract data through the exposed `TestApp.fineract: PgPool` field.
- Bind office scope in SQL; never post-filter office results in Rust.
- Do not commit unless the user asks.
- Continue the existing job through `POST /chat/jobs/{job_id}/responses`.
- Keep the selected capability authoritative for parameter-only replies; if no
  persisted selection exists, retain the current fail-safe behavior.
- Query `TestApp.fineract` from the test only. Do not call repository,
  planner, executor, catalog-query, or response-rendering code to form expected
  values.
- Bind `OFFICE_IDS` in every expected SQL query; do not filter query results in
  Rust. Compare full ordered arrays, not sets or headline values.

## Step 1: Expose the existing Fineract pool

**Files:** `crates/chat/tests/common/mod.rs:34-40,353-387`

1. Write the first failing test compile reference to `app.fineract` in
   `savings_answer_quality.rs`.
2. Add `pub fineract: PgPool` to `TestApp`.
3. Immediately after `DatabasePools::connect`, clone `pools.fineract` alongside
   `pools.app`; retain it in the returned `TestApp`.
4. Do not alter `DatabasePools`; its `fineract` field is already public.

**Check:** `cargo test -p chat --test savings_answer_quality` now compiles past
the test helper access, with the new correctness assertion failing until later
steps are implemented.

## Step 2: Preserve selection on missing execution parameters

**Files:** `crates/chat/src/assistant/runtime/mod.rs:633-705`

1. Add a runtime-focused failing test through the existing HTTP suite: submit a
   capability-selected savings prompt missing a required parameter, confirm
   `waiting_for_user_input` and `selected_capability`, submit only the missing
   date/quantity/month on the same job, and require completion with that exact
   capability.
2. In the `plan_selected_capability_verified` error branch, construct the
   missing-parameter clarification payload as today, but ensure it carries the
   selected capability as its continuation option/context rather than only
   retrieval evidence. Preserve source intent and prior extracted facts.
3. On resumed input, merge facts from the answer with the persisted source
   intent, set `memory.selected_capability` from the persisted value, and call
   `execute_selected_capability` directly. Do not route/reclassify a bare
   parameter reply.
4. Keep the existing policy guard after plan construction and before
   `execute_plan`; the continuation must still honor capability and office
   scopes.
5. Keep the existing failure behavior for a job lacking a valid persisted
   selected capability.

**Expected regression proof:** a short reply such as `5`, `2026-01-01 to
2026-12-31`, or a month executes the originally selected savings report, not a
newly classified report.

## Step 3: Build independent expected-result helpers

**Files:** `crates/chat/tests/savings_answer_quality.rs:1-407`

1. Introduce small test-local helpers returning `serde_json::Value` rows and
   aggregates from `app.fineract`; each helper accepts date bounds, top-N limit
   where applicable, and `OFFICE_IDS` as bound SQL parameters.
2. Write separate read-only SQL against Fineract transaction/account/product/
   office tables. Mirror business semantics, not application SQL text:
   - deposit and withdrawal totals: aggregate amount/count for the requested
     transaction type and inclusive period;
   - top-N: select every response field, deterministic amount-descending order
     plus stable tie-breakers, then bind `LIMIT`;
   - monthly breakdown: group by month, sum/count, ascending month order;
   - monthly top-N: rank per month with the same deterministic ranking, retain
     ranks through the requested limit, order month then rank;
   - balance summary: account count, total, average, and maximum balance for
     scoped active savings accounts.
3. Keep expected conversion local and explicit for decimal/date/null JSON
   representation so comparisons report field-specific failures rather than
   silently coercing values.
4. Add one test helper that extracts table rows/columns by response key and one
   that asserts exact row order and values against the independent result.

## Step 4: Replace shape-only matrix checks with value checks

**Files:** `crates/chat/tests/savings_answer_quality.rs:20-299`

For each existing `Case`, after HTTP completion and structural/no-leak checks,
run its independent query and assert:

1. `savings_deposit_total` and `savings_withdrawal_total`: exact aggregate
   amount, count, currency/period metadata, and selected capability.
2. `savings_deposit_top_n` and `savings_withdrawal_top_n`: exact returned rows,
   every reported field, deterministic order, and exact requested limit (or
   fewer only when Fineract has fewer scoped rows).
3. `savings_deposit_monthly_breakdown`: every month bucket, amount, count, and
   ascending returned order.
4. `savings_deposit_monthly_top_n`: every row for each month, rank-relevant
   order, amounts, and the per-month top-two limit.
5. `savings_balance_summary`: every summary value and its scoped-account basis.

Retain `assert_table_contract` as a schema guard, but make independent expected
results the correctness authority.

## Step 5: Add continuation, scope, and no-leak regressions

**Files:** `crates/chat/tests/savings_answer_quality.rs:34-100,336-354`

1. Replace the ambiguous-option-only continuation assertion with a selected
   capability that is missing a required parameter. Assert the selected ID
   before reply, submit only that parameter to the same `job_id`, then assert
   the same ID, completed answer, and independent values afterward.
2. For every matrix scenario, provision the scoped key as today and confirm
   expected SQL includes only `OFFICE_IDS`.
3. Add one narrower-office key case using a capability with available data;
   independently query that narrower scope and assert its rows/aggregates do
   not include out-of-scope offices and differ from the wider scoped result when
   fixture data permits.
4. Keep no-leak checks on every response and expand forbidden text only for
   stable internal markers observed in this suite; retain sanitized-envelope
   assertions for failures.

## Step 6: Validate incrementally and finish

1. Run after each focused change:

   ```bash
   cargo test -p chat --test savings_answer_quality
   ```

2. When it passes against local read-only Fineract and the app test database,
   run:

   ```bash
   cargo check
   ```

3. Run `cargo fmt` before the final targeted test if formatting changed.

## Acceptance Checklist

- [ ] Each of seven capability responses is checked against separately written,
  read-only, office-bound Fineract SQL.
- [ ] Totals and balance summaries compare all aggregate values.
- [ ] Top-N and monthly variants compare full rows and exact order.
- [ ] A parameter-only reply resumes the same selected capability on the same
  job without reclassification.
- [ ] Narrow scope cannot expose out-of-scope rows or aggregates.
- [ ] Responses and errors remain sanitized; no SQL or internal details leak.
- [ ] Targeted suite and workspace type-check pass.
