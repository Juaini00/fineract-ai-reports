# Issue 007 — Bundle 6: Query Budget & Timeout Design (W-I + F3)

**Goal:** Put a real ceiling and a real clock under approved-SQL execution.
Today `limit: unbounded` binds `i64::MAX`, a declared `hard_cap` is decorative,
`timeout_ms` in every query YAML is discarded, and `QUERY_DEFAULT_TIMEOUT_MS`
has no reader. This bundle makes the row cap real (declared cap **and** a
configured global backstop), warns when a result is truncated, and enforces a
per-query wall-clock budget that fails cleanly with a sanitized error and an
audit event.

**Authoritative issue:** `docs/issues/active/007-analyst-grade-knowledge-and-request-mapping.md`
§W-I (lines 638-725) and §F3 (lines 1194-1228).
**Program roadmap:** `docs/superpowers/plans/2026-07-27-issue-007-program-roadmap.md` (Bundle 6).

---

## Current state (verified 2026-07-27)

Read against the working tree, not the issue text. Findings, with drift called out:

1. **`hard_cap` is still only parsed, never enforced — Bundle 2 is NOT in the tree yet.**
   `loader.rs:179` reads `hard_cap` into `ParameterPolicy::hard_cap`;
   `parameter_policy.rs:161` validates it only appears on integer params. There
   is **no** enforcement anywhere. The stale comment at
   `execution/tool/parameters.rs:297-303` still claims "catalog `hard_cap` is
   enforced elsewhere" — it is not. No Bundle 2 spec/plan exists under
   `docs/superpowers/{specs,plans}/`.
   **Drift from the bundle brief:** the brief says "Bundle 2 already enforces the
   DECLARED hard_cap; add only the global backstop." It does not. Therefore this
   bundle delivers the *unified* clamp (declared cap **or** backstop) in one
   place — the root-cause fix — rather than layering a backstop on top of a
   clamp that does not exist. If Bundle 2 later lands its own clamp, it should
   reuse the seam this bundle creates (`effective_row_cap`), not add a second.

2. **`unbounded` → `i64::MAX`.** `parameters.rs:302` (`ResolvedValue::Unbounded => json!(i64::MAX)`).
   Confirmed. Kept as-is (the sentinel is fine); the executor now clamps it.

3. **`timeout_ms` and `cost_class` are discarded by the loader.** `QueryKnowledge`
   (`knowledge/model.rs:250-270`) has fields `id, database, sql_file, data_areas,
   tables, metrics, parameters, output_fields` and nothing else; no
   `deny_unknown_fields`, so `timeout_ms`/`cost_class` are dropped silently.
   Confirmed. **All 30** query YAMLs declare both (`timeout_ms` 3000-8000,
   `cost_class` low/medium/high); **all 30** capability YAMLs also declare
   `cost_class`. **19 of 30** capabilities declare a `hard_cap`, so **11** need
   the global backstop (matches the issue's F2 correction).

4. **YAML drift — `activity_list.yaml` nests `timeout_ms` under `guards:`.**
   `knowledge/queries/savings/activity_list.yaml:85` puts `timeout_ms: 3000`
   *inside* the `guards:` block, unlike the other 29 queries which declare it at
   the top level. Adding a top-level `timeout_ms` field would leave this one file
   silently at the default. Must be normalized to top-level.

5. **No query-level protection exists at all.** No `statement_timeout` on either
   pool, no `tokio::time::timeout`, no axum timeout layer. `QueryConfig.default_timeout_ms`
   (`core/src/config/mod.rs:54-56, 215`, default 3000) has **zero read sites**
   outside the test fixture (`tests/common/mod.rs:425`). The only live timeouts
   are LLM/embedding HTTP client timeouts — they bound calls to the model
   provider, not Postgres. Confirmed.

6. **`execution_limits.yaml` numbers are non-binding.** Loaded as
   `GenericKnowledge`; used only for policy-ID validation (`validator.rs`) and
   retrieval text (`retrieval.rs`). Its `query_timeout_ms`/`max_rows` values are
   never read. Confirmed.

7. **Execution seam.** Approved SQL runs in `crates/chat/src/execution/repository.rs::execute_plan(pool, catalog, plan, policy)` — the sole SQL layer,
   already relocated out of `assistant/**`. `limit`/`top_n` bind via
   `integer_param` at line 45. Results become
   `{ query_id, row_count, rows }` (repository.rs:84-88) → `tool_result_from_execution`
   (`tool/mod.rs:63`) → `ResponseBuilder::from_tool_result` (`presentation/builder.rs:19`),
   which is where the `pii_hidden` warning is already produced (builder.rs:41-49).

8. **Config is not threaded into the runtime.** `execute_plan`'s callers
   (`assistant/execution/runtime/execution.rs:234`) receive no config.
   `CanonicalRuntimeContext` (`runtime/mod.rs:76-88`) is the per-turn context
   already carrying `business_today` into the exact execution function; it is the
   natural carrier for the two execution-limit numbers. `JobService` builds it in
   `job/service/run.rs:70-82` and would receive `QueryConfig` at construction
   (`job/service/mod.rs:82`, call site `api/mod.rs:86`).

9. **Failure/audit path.** `execute_plan` returning `Err` maps to
   `TerminalState::FailedOperational` reason `"execution_failed"`
   (`execution.rs:257-275`); the durable failure writes a `ChatJobFailed`
   management audit event with `SanitizedError { code: Unknown }`
   (`job/repository/mod.rs:698-715`) — already sanitized, no SQL. The internal
   `memory.warnings` string is not part of the client response contract.

---

## Constraints (inherited, must not be violated)

- Approved-SQL only; no AI-generated SQL. Office scope stays bound **inside** SQL
  via `office_ids = ANY($n::bigint[])`; never a Rust post-filter. The clamp only
  changes the value bound to the existing `limit`/`top_n` parameter — it does not
  edit any `.sql` file.
- `sqlx` only in `crates/chat/src/execution/repository.rs`. The timeout/cap logic
  lives there; the runtime/orchestration layer only passes numbers in.
- Sanitized errors — no SQL/prompt/stack text ever reaches the client. Timeout
  message is a fixed English string.
- PostgreSQL durable truth; Redis live-SSE only. No change here.
- Exactly 3 crates. **No new dependency, no migration.** The only YAML surface
  change is deleting a dead key (`cost_class`) and normalizing one misplaced key
  (`timeout_ms` in `activity_list.yaml`) — no capability is added.
- English-only copy.

---

## Design

### A. Load `timeout_ms` into `QueryKnowledge`; delete `cost_class` from YAML

Add one field to `QueryKnowledge`:

```rust
#[serde(default)]
pub timeout_ms: Option<u64>,
```

`serde` populates it from the top-level `timeout_ms:` in each query YAML; absent →
`None` → default applies. Normalize `activity_list.yaml` so its `timeout_ms` is
top-level (Finding 4).

`cost_class` has **no consumer and no consumer planned in the savings/client
scope of 007** (grep-confirmed: zero Rust reads, no roadmap bundle depends on it).
Per the invariant "the YAML must not declare what the loader ignores" and YAGNI,
**delete `cost_class` from all 30 query YAMLs and all 30 capability YAMLs** rather
than add a dead Rust field. (Open decision O1 — a broad, purely-mechanical YAML
diff; confirm at spec review.)

`execution_limits.yaml` stays as a descriptive policy doc (it still participates
in policy-ID validation and retrieval as opaque `GenericKnowledge`, so the loader
does not silently drop named siblings from it the way `QueryKnowledge` did). Its
numeric knobs remain advisory and are **not** wired — the single source of truth
for the budget is now query-YAML `timeout_ms` + the config backstop. (Open
decision O2 — keep-as-descriptive vs delete the numeric block; recommend keep.)

### B. Configured global backstop

Add one field to `QueryConfig` (`core/src/config/mod.rs`):

```rust
pub global_max_rows: i64,
```

populated from `QUERY_GLOBAL_MAX_ROWS`, default `"50000"` (tens of thousands, well
above the measured scale of ~204 charge rows / 299 overdue instalments per the
appendix; generous but finite). This is the ceiling for any capability whose
`limit`/`top_n` parameter has no declared `hard_cap`.

### C. One execution-limits carrier, threaded through the existing context

Introduce a small `Copy` struct in `execution/repository.rs`:

```rust
#[derive(Debug, Clone, Copy)]
pub struct ExecutionLimits {
    pub default_timeout_ms: u64,
    pub global_max_rows: i64,
}

impl Default for ExecutionLimits {
    // Fallback for the canonical-absent (legacy/test) execution path only.
    // ponytail: matches the config defaults; real requests carry the configured values.
    fn default() -> Self {
        Self { default_timeout_ms: 3000, global_max_rows: 50000 }
    }
}
```

Carry it on `CanonicalRuntimeContext` (`execution_limits: ExecutionLimits`) — the
same per-turn object that already carries `business_today` into
`execute_selected_capability`. `JobService` stores the `QueryConfig` and sets the
field when it builds the context in `run.rs`. `execute_selected_capability` reads
`canonical.map(|c| c.execution_limits).unwrap_or_default()` and passes it to
`execute_plan`. This is the smallest correct diff: no new parameter is threaded
through `run_with_router` and the three `execute_selected_capability` call sites.

### D. Row cap + truncation, in `execute_plan`

Single seam — a pure, unit-testable function:

```rust
/// The row ceiling for this capability's limit/top_n parameter:
/// its declared hard_cap if any, else the configured global backstop.
pub(crate) fn effective_row_cap(declared_hard_cap: Option<i64>, global_max_rows: i64) -> i64 {
    declared_hard_cap.unwrap_or(global_max_rows)
}
```

In `execute_plan`, before the bind loop:
- Look up the capability (`catalog.capabilities.iter().find(|c| c.id == plan.capability)`)
  and its `limit`/`top_n` `ParameterPolicy.hard_cap`.
- `let cap = effective_row_cap(declared_hard_cap, limits.global_max_rows);`
- Identify the limit parameter (name ∈ {`limit`, `top_n`}) if present.

When binding that integer parameter, bind `fetch_limit + 1` where
`fetch_limit = min(requested_value, cap)` (requested comes from `plan.params`;
`unbounded`/`i64::MAX` collapses to `cap`). Fetching one extra row is how
truncation is detected with **no SQL change** (issue decision 6).

After `fetch_all`:
- `let truncated = rows.len() as i64 > fetch_limit;`
- If truncated, drop the surplus row(s) so the result holds exactly `fetch_limit`.
- Return `{ query_id, row_count, rows, truncated, shown }` where `shown = fetch_limit`
  when truncated (else `row_count`).

Capabilities whose query has **no** `limit`/`top_n` parameter are not clamped
(nothing to bind); at the measured data scale this is not an exposure. Recorded as
a deferral (issue decision 4) with the trigger: a no-limit capability whose result
grows past the backstop in production.

Update the stale comment at `parameters.rs:297-303`: the `i64::MAX` sentinel
stays, but the note now reads that the executor clamps it to the effective row cap
— making the comment true (issue acceptance bullet 3).

### E. Truncation warning, in the presentation builder

`ToolResult` (`tool/mod.rs:37-49`) gains `#[serde(default)] pub truncated: Option<u64>`
(= `shown` when truncated, else `None`). `tool_result_from_execution` reads
`truncated`/`shown` from the execution JSON. `ResponseBuilder::from_tool_result`
appends, alongside the existing `pii_hidden` logic:

```rust
if let Some(shown) = tool_result.truncated {
    warnings.push(ResponseWarning {
        code: "result_truncated".into(),
        message: format!(
            "Showing the first {shown} rows. More than {shown} rows match; \
             narrow your request (add a date range, office, or lower limit) to see the rest."
        ),
    });
}
```

Warning code is exactly `result_truncated` (issue acceptance bullet 5). English,
sanitized, no counts leaked beyond the shown ceiling.

### F. Per-query timeout, in `execute_plan`

`timeout = query.timeout_ms.unwrap_or(limits.default_timeout_ms)` (issue decision 3,
F3 fix — this is also `QUERY_DEFAULT_TIMEOUT_MS`'s first real reader).

Enforce server-side via `statement_timeout` inside a read transaction — the
correct mechanism (it actually cancels the Postgres query, unlike a client-side
`tokio` timeout which would leave the query running):

```rust
async fn fetch_all_with_timeout(
    pool: &PgPool,
    sql: &str,
    binder: impl FnOnce(sqlx::query::Query<'_, Postgres, PgArguments>) -> sqlx::query::Query<'_, Postgres, PgArguments>,
    timeout_ms: u64,
) -> Result<Vec<PgRow>, ExecError> { /* see plan */ }
```

Implementation: `pool.begin()`, `SET LOCAL statement_timeout = <timeout_ms>`
(the value is a trusted integer from config/YAML, never user input — safe to
format into the statement), run the bound query on the transaction, then rollback
(read-only). On error, classify by SQLSTATE:

```rust
fn is_statement_timeout(code: Option<&str>) -> bool {
    code == Some("57014") // query_canceled
}
```

A `57014` becomes a distinct clean error the runtime maps to
`TerminalState::FailedOperational` with reason `"execution_timed_out"`. **No
partial rows** are returned (issue decision 3 — a partial analyst answer is the
same failure class as under-reporting). The failure flows through the existing
sanitized `ChatJobFailed` audit event (`SanitizedError { code: Unknown }`, no SQL)
— satisfying "fails cleanly and emits an audit event, and leaks no SQL" without
new audit plumbing.

**Open decision O3:** upgrade the timeout to a dedicated audit code/event
(`NormalizedErrorCode::ExecutionTimeout` and/or an `execution.timed_out`
management event) for observability. Recommend deferring the dedicated event to
Bundle 11 (W-L, which owns observability + new events); B6 keeps the generic
sanitized failure event, which already meets the acceptance bar.

---

## Testing strategy

Pure unit tests (no DB), in `execution/repository.rs`:
- `effective_row_cap(Some(100), 50000) == 100`; `effective_row_cap(None, 50000) == 50000`.
- `is_statement_timeout(Some("57014"))` true; `Some("42P01")` / `None` false.

DB-gated integration tests (mirror the `savings_answer_quality.rs` harness, which
already hits `app.fineract`; skip cleanly when no Fineract DB):
- **hard_cap bites:** a capability with `hard_cap: 2` provably returns ≤ 2 rows
  against the populated charges table, and the response carries a
  `result_truncated` warning (issue acceptance bullets 1 + 5).
- **backstop bites:** a capability with no `hard_cap` and `global_max_rows: 2`
  returns ≤ 2 rows (issue acceptance bullet 2).
- **timeout mechanism (deterministic):** `fetch_all_with_timeout` running
  `SELECT pg_sleep(0.2)` with `timeout_ms = 1` returns the sanitized timeout
  error (SQLSTATE 57014), proving `SET LOCAL statement_timeout` + the classifier
  end-to-end without depending on any approved query being slow (issue acceptance
  bullet 4). A companion assertion confirms the error string carries no SQL text.

Regression guards that must stay green:
```
cargo test -p chat --test savings_answer_quality
cargo test -p chat --test catalog_validation
cargo test -p chat --test assistant_response
cargo test -p chat --test public_api_compat
```

Full-workspace gate after each task: `cargo fmt --check` and `cargo check` exit 0.

---

## Out of scope

- Streaming or API-layer pagination (issue decision 4 — deferred; trigger: a
  capability whose uncapped result exceeds the backstop in production).
- Clamping capabilities whose query has no `limit`/`top_n` parameter (deferred,
  same trigger).
- Rewriting any `.sql` file, adding capabilities, or altering office/PII behavior.
- Bundle 2's separately-scoped F1 (PII gate) and F7 (409 vs 404).
- A dedicated `execution.timed_out` audit event (open decision O3 → Bundle 11).
- `cost_class` as a live signal (deleted, not wired) and `execution_limits.yaml`
  numeric wiring (kept descriptive).
