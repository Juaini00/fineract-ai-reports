# Issue 007 — Bundle 6: Query Budget & Timeout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:test-driven-development` per task (test first, watch it fail, implement, watch it pass). Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make the row cap real (declared `hard_cap` or a configured global
backstop), warn on truncation, and enforce a per-query `statement_timeout` that
fails cleanly — all in the single SQL repository seam.

**Architecture:** `timeout_ms` loaded into `QueryKnowledge`; `cost_class` deleted
from YAML; a `QueryConfig.global_max_rows` env-backed backstop; one `Copy`
`ExecutionLimits` carried on `CanonicalRuntimeContext` into
`execution/repository.rs::execute_plan`, which clamps the `limit`/`top_n` bind to
the effective cap, fetches `cap + 1` to detect truncation, and runs the query
under `SET LOCAL statement_timeout`. The truncation flag surfaces as a
`result_truncated` `ResponseWarning`.

**Tech Stack:** Rust edition 2024, axum, sqlx (Postgres), serde_yaml. Existing
dependencies only. No migration.

**Authoritative spec:** `docs/superpowers/specs/2026-07-27-issue-007-b6-query-budget-timeout-design.md`.

## Global Constraints

- Approved-SQL only; never edit a `.sql` file. Office scope stays bound inside SQL.
- `sqlx` only in `crates/chat/src/execution/repository.rs`.
- Sanitized errors (no SQL/prompt/stack to the client). PostgreSQL durable; Redis live-only.
- Exactly 3 crates. No new dependency, no migration. Only YAML change: delete
  `cost_class`, normalize one `timeout_ms`.
- English-only copy.
- **No commit steps.** A task is done when its checks exit `0` and `git diff --check` is clean.

## Open decisions to confirm before starting (from spec)

- **O1:** Delete `cost_class` from all 60 YAMLs (recommended) vs keep. This plan deletes.
- **O2:** Keep `execution_limits.yaml` as a descriptive doc (recommended, this plan) vs delete its numeric block.
- **O3:** Reuse the generic sanitized `ChatJobFailed` audit event for timeouts (this plan) vs a dedicated `execution.timed_out` event (defer to Bundle 11).

---

## Task 1: Green baseline

**Files:** read only.

- [ ] **Step 1: Confirm formatting + compilation**
```bash
cargo fmt --check
cargo check
```
Expected: both exit `0`.

- [ ] **Step 2: Confirm the guard tests are green (or DB-skipped)**
```bash
cargo test -p chat --test catalog_validation
cargo test -p chat --test assistant_response
cargo test -p chat --test public_api_compat
cargo test -p chat --test savings_answer_quality
```
Expected: pass, or skip cleanly if no Fineract DB. Record any unexplained red before proceeding.

---

## Task 2: Load `timeout_ms` into `QueryKnowledge`; normalize `activity_list.yaml`

**Files:**
- `crates/chat/src/knowledge/model.rs`
- `crates/chat/src/knowledge/catalog/loader.rs` (test only)
- `knowledge/queries/savings/activity_list.yaml`

- [ ] **Step 1: Test first — loader reads top-level `timeout_ms`**

In `crates/chat/src/knowledge/catalog/loader.rs`, extend the existing query-load
test module (near the `by_name["limit"].hard_cap` assertion at ~:261) with:
```rust
#[test]
fn load_query_reads_timeout_ms() {
    let yaml = r#"
id: savings.demo
database: fineract
sql_file: queries/savings/demo.sql
timeout_ms: 8000
parameters:
  - name: limit
    type: integer
    required: false
"#;
    let query: crate::knowledge::model::QueryKnowledge =
        serde_yaml::from_str(yaml).expect("query yaml parses");
    assert_eq!(query.timeout_ms, Some(8000));
}

#[test]
fn load_query_timeout_ms_absent_is_none() {
    let yaml = r#"
id: savings.demo
database: fineract
sql_file: queries/savings/demo.sql
"#;
    let query: crate::knowledge::model::QueryKnowledge =
        serde_yaml::from_str(yaml).expect("query yaml parses");
    assert_eq!(query.timeout_ms, None);
}
```
```bash
cargo test -p chat --lib knowledge::catalog::loader::tests::load_query_reads_timeout_ms
```
Expected: FAILS to compile (`no field timeout_ms`).

- [ ] **Step 2: Add the field to `QueryKnowledge`**

In `crates/chat/src/knowledge/model.rs`, inside `pub struct QueryKnowledge`, after
`output_fields`:
```rust
    #[serde(default)]
    pub output_fields: Vec<QueryOutputField>,

    /// Per-query Postgres statement-timeout budget in milliseconds. Absent →
    /// falls back to `QueryConfig.default_timeout_ms` at execution.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}
```
```bash
cargo test -p chat --lib knowledge::catalog::loader::tests::load_query_reads_timeout_ms knowledge::catalog::loader::tests::load_query_timeout_ms_absent_is_none
```
Expected: both PASS.

- [ ] **Step 3: Normalize `activity_list.yaml`**

`knowledge/queries/savings/activity_list.yaml` currently nests `timeout_ms: 3000`
under `guards:` (line ~85). Remove that nested line and add a top-level
`timeout_ms: 3000` immediately above the top-level `cost_class:` line, matching
the layout of the other 29 query YAMLs. Leave the rest of `guards:` intact.
```bash
grep -n "timeout_ms" knowledge/queries/savings/activity_list.yaml
```
Expected: exactly one hit, at top level (column 0), not indented under `guards:`.

- [ ] **Step 4: Compile gate**
```bash
cargo check -p chat
```
Expected: exit `0`.

---

## Task 3: Delete `cost_class` from all YAMLs (O1)

**Files:** all `knowledge/queries/**/*.yaml` and `knowledge/capabilities/**/*.yaml`
that declare `cost_class` (30 + 30).

- [ ] **Step 1: Confirm the surface before deleting**
```bash
grep -rln "cost_class" knowledge/queries knowledge/capabilities | wc -l   # expect 60
grep -rn  "cost_class" crates                                             # expect 0 (no Rust reader)
```
Expected: 60 files, 0 Rust references.

- [ ] **Step 2: Remove every top-level `cost_class:` line**
```bash
grep -rl "cost_class" knowledge/queries knowledge/capabilities \
  | xargs sed -i '' '/^cost_class:/d'
grep -rn "cost_class" knowledge/queries knowledge/capabilities   # expect no output
```
Expected: no remaining `cost_class` declarations. (If any file indented it, remove
that line too — re-run the grep until clean.)

- [ ] **Step 3: Catalog still loads and validates**
```bash
cargo test -p chat --test catalog_validation
```
Expected: PASS (catalog referential integrity unaffected by dropping an unused key).

---

## Task 4: Add the `global_max_rows` backstop to config

**Files:**
- `crates/core/src/config/mod.rs`
- `crates/chat/tests/common/mod.rs`

- [ ] **Step 1: Add the field and its env read**

In `crates/core/src/config/mod.rs`, extend `QueryConfig`:
```rust
#[derive(Clone, Debug)]
pub struct QueryConfig {
    pub default_timeout_ms: u64,
    /// Row ceiling applied when a capability's limit/top_n parameter declares no
    /// hard_cap. Prevents an uncapped analyst query from binding i64::MAX.
    pub global_max_rows: i64,
}
```
In the `QueryConfig { .. }` initializer (~:214):
```rust
            query: QueryConfig {
                default_timeout_ms: get_env_or("QUERY_DEFAULT_TIMEOUT_MS", "3000")
                    .parse()
                    .context("QUERY_DEFAULT_TIMEOUT_MS must be an integer")?,
                global_max_rows: get_env_or("QUERY_GLOBAL_MAX_ROWS", "50000")
                    .parse()
                    .context("QUERY_GLOBAL_MAX_ROWS must be an integer")?,
            },
```

- [ ] **Step 2: Fix the test fixture**

In `crates/chat/tests/common/mod.rs` (~:424):
```rust
        query: QueryConfig {
            default_timeout_ms: 3000,
            global_max_rows: 50000,
        },
```

- [ ] **Step 3: Compile gate**
```bash
cargo check
```
Expected: exit `0` (no other `QueryConfig { .. }` literal exists — grep to confirm).
```bash
grep -rn "QueryConfig {" crates
```
Expected: only the two sites above.

---

## Task 5: `ExecutionLimits` + `effective_row_cap` + `is_statement_timeout` (pure, tested)

**Files:** `crates/chat/src/execution/repository.rs`

- [ ] **Step 1: Test first — the two pure helpers**

Add to the `#[cfg(test)] mod tests` in `repository.rs`:
```rust
#[test]
fn effective_row_cap_prefers_declared_hard_cap() {
    assert_eq!(super::effective_row_cap(Some(100), 50000), 100);
}

#[test]
fn effective_row_cap_falls_back_to_backstop() {
    assert_eq!(super::effective_row_cap(None, 50000), 50000);
}

#[test]
fn statement_timeout_sqlstate_is_recognized() {
    assert!(super::is_statement_timeout(Some("57014")));
    assert!(!super::is_statement_timeout(Some("42P01")));
    assert!(!super::is_statement_timeout(None));
}
```
```bash
cargo test -p chat --lib execution::repository::tests::effective_row_cap_prefers_declared_hard_cap
```
Expected: FAILS to compile (functions absent).

- [ ] **Step 2: Add the struct and helpers**

At the top of `crates/chat/src/execution/repository.rs`, after the imports:
```rust
/// Execution ceilings resolved from config and carried into the SQL layer.
#[derive(Debug, Clone, Copy)]
pub struct ExecutionLimits {
    pub default_timeout_ms: u64,
    pub global_max_rows: i64,
}

impl Default for ExecutionLimits {
    // Fallback for the canonical-absent (legacy/test) execution path only; real
    // requests carry the configured values via CanonicalRuntimeContext.
    // ponytail: mirrors the QueryConfig env defaults.
    fn default() -> Self {
        Self { default_timeout_ms: 3000, global_max_rows: 50000 }
    }
}

/// Row ceiling for a capability's limit/top_n parameter: its declared hard_cap
/// if present, else the configured global backstop.
pub(crate) fn effective_row_cap(declared_hard_cap: Option<i64>, global_max_rows: i64) -> i64 {
    declared_hard_cap.unwrap_or(global_max_rows)
}

/// Postgres SQLSTATE 57014 = query_canceled (statement_timeout fired).
fn is_statement_timeout(code: Option<&str>) -> bool {
    code == Some("57014")
}
```
```bash
cargo test -p chat --lib execution::repository::tests::effective_row_cap_prefers_declared_hard_cap execution::repository::tests::effective_row_cap_falls_back_to_backstop execution::repository::tests::statement_timeout_sqlstate_is_recognized
```
Expected: all PASS.

---

## Task 6: Clamp the limit bind + emit truncation in `execute_plan`

**Files:** `crates/chat/src/execution/repository.rs`

- [ ] **Step 1: Change `execute_plan`'s signature to accept limits**

```rust
pub async fn execute_plan(
    pool: &PgPool,
    catalog: &KnowledgeCatalog,
    plan: &ExecutionPlan,
    policy: &PolicyDecision,
    limits: ExecutionLimits,
) -> Result<Value> {
```
(Call site updated in Task 8; the crate will not compile until then — that is expected mid-task.)

- [ ] **Step 2: Resolve the effective cap and fetch limit before the bind loop**

After the `let query = ...` / `let sql = ...` block and before
`let mut sql_query = ...`:
```rust
    // Resolve the row ceiling for this capability's limit/top_n parameter.
    let declared_hard_cap = catalog
        .capabilities
        .iter()
        .find(|capability| capability.id == plan.capability)
        .and_then(|capability| {
            capability
                .parameter_policies
                .iter()
                .find(|policy| matches!(policy.name.as_str(), "limit" | "top_n"))
                .and_then(|policy| policy.hard_cap)
        });
    let row_cap = effective_row_cap(declared_hard_cap, limits.global_max_rows);
    let limit_param = query
        .parameters
        .iter()
        .find(|parameter| matches!(parameter.name.as_str(), "limit" | "top_n"))
        .map(|parameter| parameter.name.clone());
    // fetch one extra row past the ceiling to detect truncation with no SQL change.
    let fetch_limit = limit_param.as_ref().map(|name| {
        let requested = plan
            .params
            .get(name)
            .and_then(Value::as_i64)
            .unwrap_or(row_cap);
        requested.min(row_cap)
    });
```

- [ ] **Step 3: Bind `fetch_limit + 1` for the limit parameter**

Replace the integer-bind arm so the limit/top_n parameter binds the fetch value:
```rust
    for parameter in &query.parameters {
        match parameter.kind.as_str() {
            "date" => sql_query = sql_query.bind(date_param(plan, parameter)?),
            "integer" => {
                let value = if Some(&parameter.name) == limit_param.as_ref() {
                    fetch_limit.map(|limit| limit.saturating_add(1))
                } else {
                    integer_param(plan, parameter)?
                };
                sql_query = sql_query.bind(value);
            }
            "string" => sql_query = sql_query.bind(string_param(plan, parameter)?),
            "array_bigint" => {
                sql_query = sql_query.bind(array_bigint_param(plan, policy, parameter)?)
            }
            other => bail!("unsupported query parameter {other}"),
        }
    }
```

- [ ] **Step 4: Detect truncation after fetch, trim surplus, report `shown`**

Replace `let rows = sql_query.fetch_all(pool).await?;` and the result assembly.
First keep the fetch (timeout wrapper added in Task 7); then:
```rust
    let (truncated, shown) = match fetch_limit {
        Some(limit) if rows.len() as i64 > limit => (true, limit),
        _ => (false, rows.len() as i64),
    };
    if truncated {
        rows.truncate(shown as usize); // requires `let mut rows`
    }
```
And extend the returned JSON:
```rust
    Ok(json!({
        "query_id": query.id,
        "row_count": result_rows.len(),
        "rows": result_rows,
        "truncated": truncated,
        "shown": shown,
    }))
```
(Change `let rows` to `let mut rows` at the fetch site.)

- [ ] **Step 5: Make the stale comment true in `parameters.rs`**

In `crates/chat/src/assistant/execution/tool/parameters.rs` (~:297-303) replace the
comment block on `ResolvedValue::Unbounded`:
```rust
        // Unbounded: no user-supplied cap. Bound as i64::MAX so callers that
        // require an integer parameter (e.g. `LIMIT $n`) still bind; the SQL
        // repository clamps this to the capability's effective row cap
        // (declared hard_cap or the configured global backstop) before binding.
        // ponytail: i64::MAX sentinel, upgrade to LIMIT-omitting SQL if a real
        // "no limit" query appears.
        ResolvedValue::Unbounded => json!(i64::MAX),
```

- [ ] **Step 6: Compile the module in isolation**
```bash
cargo check -p chat 2>&1 | grep -E "repository.rs|parameters.rs" || echo "module edits internally consistent"
```
Expected: remaining errors are only the `execute_plan` call-site arity mismatch (fixed in Task 8) and the `ToolResult` field (Task 8) — no errors local to `repository.rs`.

---

## Task 7: Enforce `statement_timeout` in `execute_plan`

**Files:** `crates/chat/src/execution/repository.rs`

- [ ] **Step 1: Add the timeout-scoped fetch helper**

Add imports at the top: `use sqlx::{Postgres, postgres::{PgArguments, PgRow}};`
(keep the existing `PgPool, Row` imports). Then:
```rust
/// Run a bound query inside a read transaction with a per-statement timeout.
/// On SQLSTATE 57014 (statement_timeout) returns a sanitized timeout error and
/// no partial rows.
async fn fetch_all_with_timeout<'q>(
    pool: &PgPool,
    query: sqlx::query::Query<'q, Postgres, PgArguments>,
    timeout_ms: u64,
) -> Result<Vec<PgRow>> {
    let mut tx = pool.begin().await?;
    // timeout_ms is a trusted integer from config/YAML, never user input.
    sqlx::query(AssertSqlSafe(format!("SET LOCAL statement_timeout = {timeout_ms}")).into_sql_str())
        .execute(&mut *tx)
        .await?;
    let outcome = query.fetch_all(&mut *tx).await;
    let _ = tx.rollback().await; // read-only; nothing to commit
    match outcome {
        Ok(rows) => Ok(rows),
        Err(error) => {
            let code = error.as_database_error().and_then(|db| db.code());
            if is_statement_timeout(code.as_deref()) {
                // Sanitized: no SQL, no parameters, no SQLSTATE leak to the client.
                bail!("execution_timed_out");
            }
            Err(error.into())
        }
    }
}
```

- [ ] **Step 2: Use it in `execute_plan`**

Compute the budget and replace the fetch:
```rust
    let timeout_ms = query.timeout_ms.unwrap_or(limits.default_timeout_ms);
    let mut rows = fetch_all_with_timeout(pool, sql_query, timeout_ms).await?;
```
(`sql_query` is moved into the helper; it is not used afterward.)

- [ ] **Step 3: Test first — DB-gated timeout mechanism**

Add a DB-gated test to `repository.rs` tests (guard with the same env check the
integration harness uses; skip when `FINERACT_DATABASE_URL` is unset):
```rust
#[tokio::test]
async fn statement_timeout_cancels_slow_query() {
    let Ok(url) = std::env::var("FINERACT_DATABASE_URL") else {
        eprintln!("skipping: FINERACT_DATABASE_URL unset");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.expect("connect fineract");
    let query = sqlx::query("SELECT pg_sleep(0.2)");
    let error = super::fetch_all_with_timeout(&pool, query, 1)
        .await
        .expect_err("1ms budget must trip on a 200ms sleep");
    let message = error.to_string();
    assert_eq!(message, "execution_timed_out");
    assert!(!message.contains("pg_sleep"), "error must not leak SQL");
}
```
```bash
FINERACT_DATABASE_URL=postgres://root:password@127.0.0.1:5432/fineract_default \
  cargo test -p chat --lib execution::repository::tests::statement_timeout_cancels_slow_query -- --nocapture
```
Expected: PASS (or clean skip if no DB). Confirms `SET LOCAL statement_timeout` +
57014 classification + sanitized message end-to-end.

- [ ] **Step 4: Module compile gate**
```bash
cargo check -p chat 2>&1 | grep "repository.rs" || echo "repository.rs clean"
```
Expected: no `repository.rs`-local errors.

---

## Task 8: Thread `ExecutionLimits` and surface `truncated` to the response

**Files:**
- `crates/chat/src/assistant/execution/runtime/mod.rs`
- `crates/chat/src/assistant/execution/runtime/execution.rs`
- `crates/chat/src/job/service/mod.rs`
- `crates/chat/src/job/service/run.rs`
- `crates/chat/src/api/mod.rs`
- `crates/chat/src/assistant/execution/tool/mod.rs`
- `crates/chat/src/assistant/presentation/builder.rs`

- [ ] **Step 1: Carry limits on `CanonicalRuntimeContext`**

In `runtime/mod.rs`, add to `pub struct CanonicalRuntimeContext`:
```rust
    pub business_date_source: BusinessDateSource,
    pub execution_limits: crate::execution::repository::ExecutionLimits,
}
```

- [ ] **Step 2: `execute_plan` call site reads the limits**

In `runtime/execution.rs`, at the `execute_plan(pool, catalog, &plan, &policy)`
call (~:234):
```rust
    let limits = canonical
        .map(|context| context.execution_limits)
        .unwrap_or_default();
    match execute_plan(pool, catalog, &plan, &policy, limits).await {
```
(`execute_plan` and `ExecutionLimits` are already imported via the runtime's `use`
tree; add `use crate::execution::repository::ExecutionLimits;` if the check reports it missing.)

- [ ] **Step 3: `JobService` stores `QueryConfig` and sets the field**

In `job/service/mod.rs`: import `QueryConfig` (`use app_core::config::{.. , QueryConfig};`),
add `query_config: QueryConfig` to `struct JobService`, add a `query_config: QueryConfig`
parameter to `JobService::new` (place it right after `chat_features`), and store it
in the `Self { .. }` literal.

In `job/service/run.rs`, in the `CanonicalRuntimeContext { .. }` literal (~:70):
```rust
            business_date_source: today.source,
            execution_limits: crate::execution::repository::ExecutionLimits {
                default_timeout_ms: self.query_config.default_timeout_ms,
                global_max_rows: self.query_config.global_max_rows,
            },
```

- [ ] **Step 4: Pass the config at construction**

In `crates/chat/src/api/mod.rs`, in the `JobService::new( .. )` call (~:86), add
`core.config.query.clone()` in the same position chosen in Step 3 (after
`core.config.chat_features.clone()`).

- [ ] **Step 5: `ToolResult` carries `truncated`; builder emits the warning**

In `assistant/execution/tool/mod.rs`, add to `pub struct ToolResult`:
```rust
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    /// When the row cap trimmed the result, the number of rows actually shown.
    #[serde(default)]
    pub truncated: Option<u64>,
}
```
and in `tool_result_from_execution`:
```rust
        evidence_refs: request.evidence_refs.clone(),
        truncated: execution_result
            .get("truncated")
            .and_then(Value::as_bool)
            .filter(|&t| t)
            .and(execution_result.get("shown").and_then(Value::as_u64)),
    }
```

In `assistant/presentation/builder.rs::from_tool_result`, change the `warnings`
binding from `let warnings = ...` to `let mut warnings = ...` (it currently ends in
`.collect()`), then after it:
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

- [ ] **Step 6: Fix every other `ToolResult { .. }` / `CanonicalRuntimeContext { .. }` literal**
```bash
grep -rn "ToolResult {" crates/chat/src crates/chat/tests
grep -rn "CanonicalRuntimeContext {" crates/chat/src crates/chat/tests
```
Add `truncated: None` / `execution_limits: Default::default()` to any literal the
grep finds (test fixtures included). `#[serde(default)]` covers deserialization
paths; struct literals must be updated explicitly.

- [ ] **Step 7: Full compile gate**
```bash
cargo fmt
cargo check
```
Expected: exit `0`.

---

## Task 9: DB-gated cap + truncation integration test

**Files:** `crates/chat/tests/savings_answer_quality.rs` (or a new
`crates/chat/tests/query_budget.rs` following the same harness in `tests/common`).

- [ ] **Step 1: Test — hard_cap bites and warns**

Using the shared harness (`common::spawn_app` / `app.fineract`), drive the
`savings_pending_charges_clients` capability against the populated charges table
with a fixture/override cap of `2` (via a fixture catalog whose `limit`
`ParameterPolicy.hard_cap = Some(2)`), assert:
- the returned table has at most 2 rows, and
- `response.warnings` contains one with `code == "result_truncated"`.

- [ ] **Step 2: Test — backstop bites**

With a capability whose `limit` policy has `hard_cap: None` and an
`ExecutionLimits { global_max_rows: 2, .. }`, assert the result is bounded to 2
rows. (Construct the limits directly when calling `execute_plan` in a
repository-level integration test, or set `QUERY_GLOBAL_MAX_ROWS=2` for a
process-level test.)

- [ ] **Step 3: Run**
```bash
FINERACT_DATABASE_URL=postgres://root:password@127.0.0.1:5432/fineract_default \
  cargo test -p chat --test savings_answer_quality
```
Expected: PASS, or clean skip without a DB. If the populated table has fewer than
3 matching rows, lower nothing — instead assert the cap is respected (≤ cap) and
that `result_truncated` appears only when `row_count > cap`; document the row
count observed.

---

## Task 10: Final verification

- [ ] **Step 1: Format + full workspace**
```bash
cargo fmt --check
cargo check
git diff --check
```
Expected: all exit `0`; no whitespace errors.

- [ ] **Step 2: Full guard suite**
```bash
cargo test -p chat --lib execution::repository
cargo test -p chat --lib knowledge::catalog::loader
cargo test -p chat --test catalog_validation
cargo test -p chat --test assistant_response
cargo test -p chat --test public_api_compat
cargo test -p chat --test savings_answer_quality
```
Expected: all pass (DB-gated ones skip cleanly without Fineract).

- [ ] **Step 3: Acceptance cross-check against the issue**
  - `hard_cap: N` capability returns ≤ N rows (Task 9 Step 1). ✔ bullet 1
  - no-`hard_cap` capability bounded by the backstop (Task 9 Step 2). ✔ bullet 2
  - `parameters.rs:299` comment is now true (Task 6 Step 5). ✔ bullet 3
  - `timeout_ms` loaded + applied; exceed fails cleanly with an audit event and no
    SQL leak (Task 2 + Task 7 + existing sanitized `ChatJobFailed`). ✔ bullet 4
  - truncated result carries `result_truncated` in `warnings` (Task 8 Step 5). ✔ bullet 5
