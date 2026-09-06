# LLM Extraction Gateway Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the three-layer extraction gateway (LLM report → deterministic
resolver → clarification decider) with per-parameter YAML policy and a
first-class `BusinessDateProvider`, so capabilities can silently default
`from_date`/`to_date`/`limit` and use tenant business date for "today".

**Architecture:** Layer 1 is a schema-constrained LLM call producing
`LlmGatewayExtraction`. Layer 2 evaluates a whitelisted default-expression
DSL over `(BusinessDateProvider, caller_scope, user text, LLM hint)`. Layer 3
only asks a clarification question when a `required: true` parameter is
unfilled after Layer 2 or the classifier gap is too small.

**Tech Stack:** Rust, `sqlx`/PostgreSQL (chat DB + Fineract read-replica),
`schemars`/`serde` for LLM schema, `axum` runtime, existing `TracedLlmClient`,
existing YAML catalog under `knowledge/`.

## Global Constraints

- Branch: `feature/llm-extraction-gateway` (new branch off current work).
- Workspace stays at three crates: `app`, `core`, `chat`. No new crates.
- Layering: `route → service → repository → database`; no `sqlx` in
  handlers/services. All new SQL lives in repositories.
- HTTP envelope `{ success, data, error }` preserved; errors sanitized.
- Schema changes only via new `migrations/*.sql`. Startup never creates schema.
- English-only user-visible copy.
- Every capability YAML under `knowledge/capabilities/` must load cleanly at
  startup after this change; `KnowledgeValidator` fails fast otherwise.
- `BusinessDateProvider` **never** replaces `Utc::now()` for audit / trace /
  outbox timestamps.
- Default-expression DSL is a fixed whitelist (see spec §5.2). No new tokens
  outside the spec table without an amendment.
- Every task ends with `cargo fmt`, `cargo check -p chat`, and its own test
  green before commit. `cargo clippy --workspace --all-targets` must not
  regress (pre-commit hook enforces).
- Design reference: `docs/superpowers/specs/2026-07-24-llm-extraction-gateway-design.md`.

## File Structure

**New files:**

- `crates/chat/src/assistant/temporal/business_date.rs` — `BusinessDate`,
  `BusinessDateSource`, `BusinessDateError`, `BusinessDateProvider` trait,
  `FineractBusinessDateProvider` impl, `StaticBusinessDateProvider` test double.
- `crates/chat/src/assistant/temporal/mod.rs` — module export (may already
  exist; extend or create).
- `crates/chat/src/knowledge/catalog/parameter_policy.rs` — the new
  per-parameter policy model (`ParameterPolicy`, `DefaultExpr`, parser).
- `crates/chat/src/knowledge/catalog/parameter_policy_tests.rs` — unit tests
  for the DSL parser + validator.
- `crates/chat/src/assistant/understanding/gateway/mod.rs` — Layer 1 module.
- `crates/chat/src/assistant/understanding/gateway/schema.rs` —
  `LlmGatewayExtraction` type + `schemars`-derived JSON schema.
- `crates/chat/src/assistant/understanding/gateway/client.rs` — the LLM call
  that returns a validated `LlmGatewayExtraction`.
- `crates/chat/src/assistant/understanding/resolver.rs` — Layer 2 resolver.
- `crates/chat/src/assistant/understanding/decider.rs` — Layer 3 decider.
- `crates/chat/tests/business_date_provider.rs` — integration test for the
  provider fallback + audit event emission.
- `crates/chat/tests/extraction_gateway_scenarios.rs` — the seven worked
  examples from spec §7 as end-to-end scenario tests.
- `docs/current/extraction-gateway.md` — user-facing note describing the
  new resolver semantics for the frontend / integrators.

**Modified files:**

- `crates/chat/src/knowledge/catalog/loader.rs` — parse the new `parameters:`
  block; migrate old lists on the fly (see Phase 3).
- `crates/chat/src/knowledge/catalog/validator.rs` — enforce policy rules
  (spec §5.1).
- `crates/chat/src/knowledge/model.rs` — extend `CapabilityKnowledge` with
  `parameter_policies: Vec<ParameterPolicy>`.
- `crates/chat/src/assistant/understanding/mod.rs` — expose `gateway`,
  `resolver`, `decider` submodules; deprecate direct callers of the old
  extractor.
- `crates/chat/src/assistant/understanding/clarification_resolver.rs` —
  consume the new decider result; drop hard-coded `missing_parameters` list.
- `crates/chat/src/assistant/execution/runtime/mod.rs` — feed
  `BusinessDateProvider` into the graph runtime context.
- `crates/chat/src/job/service/mod.rs` — inject `BusinessDateProvider` when
  spawning graph runs.
- `crates/chat/src/api/mod.rs` — wire the provider at `ChatAppState::new`.
- Every `knowledge/capabilities/**/*.yaml` — replace `required_parameters` /
  `optional_parameters` / `clarification.missing_parameters` with the new
  `parameters:` block (Phase 3).
- `crates/chat/src/audit/*` — new sanitized event kinds
  `business_date.fallback_used`, `llm_gateway.candidate_dropped`,
  `llm_gateway.entity_dropped` (reuse existing outbox path).

---

## Phase 0 — Baseline and branch prep

### Task 0.1: Create branch and confirm baseline

- [ ] **Step 1: Check current tree is clean or committed**

Run: `git status --porcelain`
Expected: empty output, or only intentional in-progress work committed.

- [ ] **Step 2: Create branch**

```bash
git switch -c feature/llm-extraction-gateway
```

- [ ] **Step 3: Run baseline checks**

```bash
cargo fmt --check
cargo check -p chat
cargo test -p chat --lib
```

Expected: green. If any pre-existing failure, note it in the plan
tracking file — do not "fix" it by changing new expectations later.

- [ ] **Step 4: Record baseline SHA**

```bash
git rev-parse --short HEAD
```

Save the SHA in your notes. Every commit in this plan lands on top of this.

---

## Phase 1 — BusinessDateProvider

### Task 1.1: Explore Fineract business-date source

**Files:**
- Read: `crates/chat/src/knowledge/embedding/`, `crates/core/src/config/`,
  Fineract migration/schema references under `docs/reporting-data/`.

- [ ] **Step 1: Identify the source column/table**

Search Fineract schema docs for how business date is stored. Candidates:
`c_configuration.name = 'business_date'`, or a per-tenant field on
`m_office`, or a Fineract endpoint accessible via SQL. Read
`docs/reporting-data-scope.md` and any file under `docs/reporting-data/` that
mentions "business date" / "cob" / "close of business".

- [ ] **Step 2: Write a probe SQL**

Draft one read-only SQL against the Fineract replica that returns the current
business date for the tenant, e.g.:

```sql
SELECT value FROM c_configuration WHERE name = 'business-date-enabled-for-transactions' LIMIT 1;
```

Adjust based on what you found in Step 1. The exact query lands in Task 1.3.

- [ ] **Step 3: Record findings**

Append your findings (table, column, SQL, fallback behavior) to
`docs/current/extraction-gateway.md` (create if absent) under a
"Business date source" section.

- [ ] **Step 4: Commit findings**

```bash
git add docs/current/extraction-gateway.md
git commit -m "docs: record Fineract business-date source"
```

### Task 1.2: BusinessDateProvider trait + types

**Files:**
- Create: `crates/chat/src/assistant/temporal/business_date.rs`
- Modify: `crates/chat/src/assistant/temporal/mod.rs` (create if missing)
- Test: colocated in `business_date.rs` `#[cfg(test)] mod tests`.

**Interfaces:**
- Produces:
  ```rust
  pub struct BusinessDate { pub date: NaiveDate, pub source: BusinessDateSource, pub resolved_at: DateTime<Utc> }
  pub enum BusinessDateSource { Fineract, WallClockFallback }
  pub enum BusinessDateError { Query(anyhow::Error), MissingConfiguration, Timeout }
  #[async_trait] pub trait BusinessDateProvider: Send + Sync { async fn today(&self) -> Result<BusinessDate, BusinessDateError>; }
  pub struct StaticBusinessDateProvider { pub value: NaiveDate, pub source: BusinessDateSource }
  ```

- [ ] **Step 1: Write failing test for StaticBusinessDateProvider**

```rust
#[tokio::test]
async fn static_provider_returns_configured_date() {
    let provider = StaticBusinessDateProvider {
        value: NaiveDate::from_ymd_opt(2026, 7, 24).unwrap(),
        source: BusinessDateSource::Fineract,
    };
    let result = provider.today().await.unwrap();
    assert_eq!(result.date, NaiveDate::from_ymd_opt(2026, 7, 24).unwrap());
    assert!(matches!(result.source, BusinessDateSource::Fineract));
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p chat --lib business_date::tests::static_provider_returns_configured_date
```

Expected: FAIL — module does not exist.

- [ ] **Step 3: Write the module**

Create `crates/chat/src/assistant/temporal/business_date.rs` with the types
in the interfaces block and the `StaticBusinessDateProvider` implementation.
Add `pub mod business_date;` to `assistant/temporal/mod.rs`.

- [ ] **Step 4: Run test to verify pass**

Same command as Step 2. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/chat/src/assistant/temporal/
git commit -m "feat(chat): add BusinessDateProvider trait and static test double"
```

### Task 1.3: FineractBusinessDateProvider

**Files:**
- Modify: `crates/chat/src/assistant/temporal/business_date.rs`
- Test: `crates/chat/tests/business_date_provider.rs`

**Interfaces:**
- Consumes: `sqlx::PgPool` (the Fineract read-replica pool from
  `AppState::pools::fineract`).
- Produces: `FineractBusinessDateProvider { pool }` implementing
  `BusinessDateProvider`.

- [ ] **Step 1: Write failing integration test**

```rust
// crates/chat/tests/business_date_provider.rs
mod common;
use common::spawn_app;
use chat::assistant::temporal::business_date::{BusinessDateProvider, FineractBusinessDateProvider};

#[tokio::test(flavor = "multi_thread")]
async fn fineract_provider_reads_configured_business_date() {
    let app = spawn_app().await;
    let provider = FineractBusinessDateProvider::new(app.fineract_pool.clone());
    let result = provider.today().await.unwrap();
    // Fineract test container seeds a known configuration.
    assert!(result.date.year() >= 2020);
    assert!(matches!(result.source, chat::assistant::temporal::business_date::BusinessDateSource::Fineract));
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p chat --test business_date_provider fineract_provider_reads_configured_business_date
```

Expected: FAIL — `FineractBusinessDateProvider` does not exist.

- [ ] **Step 3: Implement FineractBusinessDateProvider**

Add to `business_date.rs`:

```rust
pub struct FineractBusinessDateProvider { pool: sqlx::PgPool }

impl FineractBusinessDateProvider {
    pub fn new(pool: sqlx::PgPool) -> Self { Self { pool } }
}

#[async_trait::async_trait]
impl BusinessDateProvider for FineractBusinessDateProvider {
    async fn today(&self) -> Result<BusinessDate, BusinessDateError> {
        // Use the SQL identified in Task 1.1.
        let value: Option<chrono::NaiveDate> = sqlx::query_scalar(
            "SELECT value::date FROM c_configuration WHERE name = 'business_date' LIMIT 1"
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| BusinessDateError::Query(e.into()))?;
        match value {
            Some(date) => Ok(BusinessDate { date, source: BusinessDateSource::Fineract, resolved_at: chrono::Utc::now() }),
            None => Ok(BusinessDate {
                date: chrono::Utc::now().date_naive(),
                source: BusinessDateSource::WallClockFallback,
                resolved_at: chrono::Utc::now(),
            }),
        }
    }
}
```

Adjust the SQL to whatever Task 1.1 discovered.

- [ ] **Step 4: Run test to verify pass**

Same command as Step 2. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/chat/src/assistant/temporal/business_date.rs crates/chat/tests/business_date_provider.rs
git commit -m "feat(chat): add FineractBusinessDateProvider with wall-clock fallback"
```

### Task 1.4: Wire provider into ChatAppState

**Files:**
- Modify: `crates/chat/src/api/mod.rs`
- Modify: `crates/chat/src/job/service/mod.rs`

**Interfaces:**
- Produces: `ChatServices` (or a new sibling on `ChatAppState`) exposes an
  `Arc<dyn BusinessDateProvider>` reachable from graph runtime.

- [ ] **Step 1: Add field to ChatAppState**

Add `pub business_date: Arc<dyn BusinessDateProvider>` on `ChatAppState`.
Initialize inside `ChatAppState::new` as
`Arc::new(FineractBusinessDateProvider::new(core.pools.fineract.clone()))`.

- [ ] **Step 2: Thread into JobService**

Add matching field to `JobService`, populate from `ChatAppState::new` when
building `ChatServices`.

- [ ] **Step 3: Compile check**

```bash
cargo check -p chat
```

Expected: green.

- [ ] **Step 4: Commit**

```bash
git add crates/chat/src/api/mod.rs crates/chat/src/job/service/mod.rs
git commit -m "feat(chat): inject BusinessDateProvider into chat app state"
```

### Task 1.5: Emit `business_date.fallback_used` audit event

**Files:**
- Modify: `crates/chat/src/management/model.rs` — add
  `AuditEventType::BusinessDateFallback` and `AuditSummary::BusinessDateFallback`.
- Modify: `crates/chat/src/assistant/temporal/business_date.rs` — the
  fallback branch invokes a hook.
- Test: `crates/chat/tests/business_date_provider.rs` add case.

- [ ] **Step 1: Extend AuditEventType and AuditSummary**

Add variants `BusinessDateFallback` with rename `"business_date.fallback_used"`.
Add `AuditSummary::BusinessDateFallback { requested_at: DateTime<Utc> }` and
matching `PolicyResult`/serde rules if the summary needs a field.

- [ ] **Step 2: Write failing test that fallback emits audit**

Use `StaticBusinessDateProvider { source: WallClockFallback, .. }` composed
with a callback trait. Assert callback invoked exactly once when
`today()` is called through the runtime path (Task 1.6 wires the call site
through the enqueue helper).

- [ ] **Step 3: Introduce hook + emit**

Wrap the provider in a thin `AuditingBusinessDateProvider { inner, pool }`
in `business_date.rs` that, when `inner.today()` returns
`WallClockFallback`, opens a short tx and enqueues a
`ManagementAuditEvent { event_type: BusinessDateFallback, ... }` via
`crate::management::enqueue`.

- [ ] **Step 4: Run test, verify pass**

- [ ] **Step 5: Commit**

```bash
git add crates/chat/src/management/model.rs crates/chat/src/assistant/temporal/business_date.rs crates/chat/tests/business_date_provider.rs
git commit -m "feat(chat): audit business-date fallback via management outbox"
```

### Task 1.6: Wrap production provider with AuditingBusinessDateProvider

**Files:**
- Modify: `crates/chat/src/api/mod.rs`.

- [ ] **Step 1: Change ChatAppState::new to wrap the provider**

```rust
let inner = FineractBusinessDateProvider::new(core.pools.fineract.clone());
let business_date: Arc<dyn BusinessDateProvider> = Arc::new(
    AuditingBusinessDateProvider::new(inner, core.pools.app.clone())
);
```

- [ ] **Step 2: `cargo check -p chat`**

Expected: green.

- [ ] **Step 3: Commit**

```bash
git add crates/chat/src/api/mod.rs
git commit -m "feat(chat): use auditing wrapper for production business-date provider"
```

---

## Phase 2 — Capability parameter policy model + DSL

### Task 2.1: Introduce ParameterPolicy and DefaultExpr

**Files:**
- Create: `crates/chat/src/knowledge/catalog/parameter_policy.rs`
- Modify: `crates/chat/src/knowledge/catalog/mod.rs` — export new module.

**Interfaces:**
- Produces:
  ```rust
  pub enum ParameterType { Date, Integer, IntegerArray, String, Currency }
  pub enum DefaultExpr {
      BusinessToday,
      WallToday,
      BusinessTodayMinusDays(u16),
      BusinessTodayMinusMonths(u16),
      BusinessTodayMinusYears(u16),
      StartOfMonthBusinessToday,
      EndOfMonthBusinessToday,
      Unbounded,
      AuthorizedScope,
      LiteralInt(i64),
      LiteralDate(NaiveDate),
  }
  pub struct ParameterPolicy {
      pub name: String,
      pub kind: ParameterType,
      pub required: bool,
      pub default: Option<DefaultExpr>,
      pub fill_when_missing: bool,
      pub user_may_override: bool,
      pub hard_cap: Option<i64>,
  }
  ```

- [ ] **Step 1: Write failing unit test for DSL parser**

```rust
#[test]
fn parses_business_today_minus_1m() {
    assert_eq!(DefaultExpr::parse("business_today - 1m").unwrap(),
               DefaultExpr::BusinessTodayMinusMonths(1));
}
#[test]
fn rejects_unknown_expression() {
    assert!(DefaultExpr::parse("today() + 1w").is_err());
}
```

- [ ] **Step 2: Run tests to verify fail**

- [ ] **Step 3: Implement `DefaultExpr::parse` with the whitelist from spec §5.2**

Recognizers, in order: literal ISO date, literal integer, `business_today`,
`wall_today`, `unbounded`, `authorized_scope`, `start_of_month(business_today)`,
`end_of_month(business_today)`, `business_today - Nd|Nm|Ny`.

- [ ] **Step 4: Run tests, verify pass**

- [ ] **Step 5: Commit**

```bash
git add crates/chat/src/knowledge/catalog/parameter_policy.rs crates/chat/src/knowledge/catalog/mod.rs
git commit -m "feat(chat): add ParameterPolicy model and default-expression DSL parser"
```

### Task 2.2: Extend YAML loader to parse `parameters:` block

**Files:**
- Modify: `crates/chat/src/knowledge/catalog/loader.rs`
- Modify: `crates/chat/src/knowledge/model.rs` — add
  `parameter_policies: Vec<ParameterPolicy>` to `CapabilityKnowledge`.

- [ ] **Step 1: Write failing test that loads a capability with the new block**

Create a small fixture YAML at `crates/chat/tests/fixtures/knowledge/capability_with_policies.yaml`
containing the shape from spec §5.1. Test invokes `KnowledgeLoader::load_capability_from_str`
(add this helper if needed) and asserts `parameter_policies.len() == 3`.

- [ ] **Step 2: Verify fail**

- [ ] **Step 3: Add serde structs + parse into ParameterPolicy**

Introduce private `RawParameters { by_name: BTreeMap<String, RawParameterPolicy> }`
serde struct in loader.rs. Convert to `Vec<ParameterPolicy>` in the loader,
calling `DefaultExpr::parse` for defaults.

- [ ] **Step 4: Run test, verify pass**

- [ ] **Step 5: Commit**

```bash
git add crates/chat/src/knowledge/catalog/loader.rs crates/chat/src/knowledge/model.rs crates/chat/tests/fixtures/knowledge/capability_with_policies.yaml
git commit -m "feat(chat): parse per-parameter policy from capability YAML"
```

### Task 2.3: Enforce policy rules in validator

**Files:**
- Modify: `crates/chat/src/knowledge/catalog/validator.rs`

Rules (spec §5.1):

1. Every query-required parameter has either `required: true` or `default`.
2. `hard_cap` only on integer / integer_array types.
3. `office_ids.user_may_override == false`.
4. No two policies with duplicate names.

- [ ] **Step 1: Write failing test per rule**

Create four test cases each producing a policy set violating exactly one
rule and asserting the specific validator error variant.

- [ ] **Step 2: Verify fail**

- [ ] **Step 3: Implement checks**

- [ ] **Step 4: Verify pass**

- [ ] **Step 5: Commit**

```bash
git add crates/chat/src/knowledge/catalog/validator.rs
git commit -m "feat(chat): validate per-parameter policy rules at catalog load"
```

### Task 2.4: DefaultExpr evaluator

**Files:**
- Modify: `crates/chat/src/knowledge/catalog/parameter_policy.rs`.

**Interfaces:**
- Consumes: `EvaluationContext { business_today: NaiveDate, authorized_office_ids: Vec<i64> }`.
- Produces: `pub enum ResolvedValue { Date(NaiveDate), Integer(i64), IntegerArray(Vec<i64>), Unbounded }` +
  `impl DefaultExpr { pub fn evaluate(&self, ctx: &EvaluationContext) -> ResolvedValue }`.

- [ ] **Step 1: Write failing test for each expression variant**

- [ ] **Step 2: Verify fail**

- [ ] **Step 3: Implement `evaluate`**

Use `chrono` for month/year arithmetic. `start_of_month` uses
`NaiveDate::from_ymd_opt(y, m, 1)`. `end_of_month` uses
`chrono::Months::new(1)` then subtract one day.

- [ ] **Step 4: Verify pass**

- [ ] **Step 5: Commit**

```bash
git add crates/chat/src/knowledge/catalog/parameter_policy.rs
git commit -m "feat(chat): evaluate ParameterPolicy default expressions"
```

---

## Phase 3 — YAML migration of existing capabilities

### Task 3.1: Auto-migration script

**Files:**
- Create: `crates/chat/src/bin/migrate_capability_policies.rs` (throw-away
  binary, kept in-tree for reproducibility).

- [ ] **Step 1: Write script**

For every `knowledge/capabilities/**/*.yaml`:

1. Read as `serde_yaml::Value`.
2. Collect existing `required_parameters` + `optional_parameters`.
3. For each parameter in the underlying query (look up `query_id` in
   `knowledge/queries.yaml` or its shard), emit a policy entry:
   - if the parameter is a date and appears in `required_parameters`:
     `required: false, default: business_today, fill_when_missing: true`.
   - if the parameter is `limit`:
     `required: false, default: unbounded, hard_cap: <from `guards.max_limit`>`.
   - if the parameter is `office_ids`:
     `required: false, default: authorized_scope, user_may_override: false`.
   - otherwise leave `required: true` with no default.
4. Remove `required_parameters`, `optional_parameters`,
   `clarification.missing_parameters`.
5. Write back preserving key order.

- [ ] **Step 2: Run script (dry run flag)**

```bash
cargo run -p chat --bin migrate_capability_policies -- --dry-run
```

Manually diff-review three sample files. Note anything unusual for Task 3.2.

- [ ] **Step 3: Run script for real**

```bash
cargo run -p chat --bin migrate_capability_policies
```

- [ ] **Step 4: Commit generated changes**

```bash
git add knowledge/capabilities/
git commit -m "chore(knowledge): auto-migrate capabilities to per-parameter policy"
```

### Task 3.2: Manual audit pass

- [ ] **Step 1: For every migrated capability, sanity-check**

Open each file. Verify defaults match spec §7 examples where applicable
(loan_arrears_clients → date defaults; savings_fee_assignments → no date
parameters at all; office_hierarchy_tree → `limit.default: unbounded`).
Amend any incorrect default by hand.

- [ ] **Step 2: `cargo test -p chat --test catalog_validation`**

Expected: green (all capabilities load and validate).

- [ ] **Step 3: Commit corrections**

```bash
git add knowledge/capabilities/
git commit -m "chore(knowledge): manual corrections after policy migration"
```

---

## Phase 4 — Layer 1 (LLM Gateway)

### Task 4.1: Define `LlmGatewayExtraction` schema

**Files:**
- Create: `crates/chat/src/assistant/understanding/gateway/mod.rs`
- Create: `crates/chat/src/assistant/understanding/gateway/schema.rs`
- Modify: `crates/chat/src/assistant/understanding/mod.rs` — `pub mod gateway;`.

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
  pub struct LlmGatewayExtraction {
      pub intent_kind: IntentKind,
      pub domain: AssistantDomain,
      pub language: AssistantLanguage,
      pub entities: Vec<GatewayEntity>,
      pub temporal_hint: Option<TemporalHint>,
      pub quantity_hint: Option<QuantityHint>,
      pub candidates: Vec<GatewayCandidate>,
  }
  ```
  and matching sub-types with exact fields from spec §4.1.

- [ ] **Step 1: Write failing test for schema serde round-trip**

Fixture JSON at `crates/chat/tests/fixtures/gateway/extraction_sample.json`
matching spec §4.1 exactly. Test deserializes then re-serializes and asserts
the JSON is stable.

- [ ] **Step 2: Verify fail**

- [ ] **Step 3: Implement the types with `schemars` derives**

Reuse `AssistantDomain`, `AssistantLanguage` from `understanding/intent.rs`.
Introduce new `IntentKind` if the existing `AssistantIntentKind` cannot be
reused verbatim (prefer reuse).

- [ ] **Step 4: Verify pass**

- [ ] **Step 5: Commit**

```bash
git add crates/chat/src/assistant/understanding/gateway/ crates/chat/tests/fixtures/gateway/
git commit -m "feat(chat): define LlmGatewayExtraction schema"
```

### Task 4.2: Prompt construction

**Files:**
- Create: `crates/chat/src/assistant/understanding/gateway/prompt.rs`
- Modify: `crates/chat/src/assistant/understanding/gateway/mod.rs` —
  `pub mod prompt;`.

**Interfaces:**
- Produces: `pub fn build_gateway_prompt(user_message: &str, catalog_summary: &[CapabilitySummary], history_summary: Option<&str>) -> String`.

- [ ] **Step 1: Write failing test asserting prompt contains capability ids and NOT SQL**

- [ ] **Step 2: Verify fail**

- [ ] **Step 3: Implement**

Deterministically render Markdown prompt with sections:
- User message (verbatim).
- Recent turns (if any).
- Visible capabilities: `id`, `display_name`, `description`, `use_when`
  (derive from `examples` if `use_when` absent). Never include SQL,
  parameter internals, PII policy.
- Output-schema description referencing the JSON schema (schemars
  `schema_for!` output as JSON attached).

- [ ] **Step 4: Verify pass**

- [ ] **Step 5: Commit**

```bash
git add crates/chat/src/assistant/understanding/gateway/prompt.rs
git commit -m "feat(chat): build LLM gateway prompt with safe capability summaries"
```

### Task 4.3: Gateway client

**Files:**
- Create: `crates/chat/src/assistant/understanding/gateway/client.rs`

**Interfaces:**
- Consumes: `SharedLlmClient` (existing `TracedLlmClient` type),
  `KnowledgeCatalog`, `PrincipalContext`.
- Produces:
  ```rust
  pub struct GatewayClient { llm: SharedLlmClient }
  impl GatewayClient {
      pub async fn extract(&self, user_message: &str, history: Option<&str>, catalog: &KnowledgeCatalog, principal: &PrincipalContext) -> Result<LlmGatewayExtraction, GatewayError>;
  }
  pub enum GatewayError { SchemaInvalidAfterRetry, ProviderUnavailable, ProviderMalformed }
  ```

- [ ] **Step 1: Write failing test with a stub LLM returning canned JSON**

Add a mock impl of `LlmPlannerClient`/`SharedLlmClient` returning the same
fixture from Task 4.1. Assert `extract` returns the deserialized value.

- [ ] **Step 2: Verify fail**

- [ ] **Step 3: Implement**

1. Build prompt via `prompt::build_gateway_prompt` (filter catalog to
   visible capabilities based on the principal's `allowed_capabilities`).
2. Call `llm.complete_structured(prompt, schema_for!(LlmGatewayExtraction))`
   (add this helper on `SharedLlmClient` if not present; wrap the existing
   plain `complete` if the underlying client returns raw text).
3. On JSON-schema-invalid, retry once with the same prompt.
4. On second failure → `GatewayError::SchemaInvalidAfterRetry`.
5. Sanitize the returned struct: drop entities whose `value` is not
   substring of the user message; drop candidates whose id is not visible
   to the principal. Emit `llm_gateway.entity_dropped` /
   `llm_gateway.candidate_dropped` audit events (management outbox).

- [ ] **Step 4: Verify pass**

- [ ] **Step 5: Commit**

```bash
git add crates/chat/src/assistant/understanding/gateway/client.rs
git commit -m "feat(chat): implement LLM gateway extraction client with retry and sanitization"
```

---

## Phase 5 — Layer 2 (Deterministic Resolver)

### Task 5.1: ResolverRequest and ResolvedRequest types

**Files:**
- Create: `crates/chat/src/assistant/understanding/resolver.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct ResolverRequest<'a> {
      pub extraction: &'a LlmGatewayExtraction,
      pub capability: &'a CapabilityKnowledge,
      pub business_today: NaiveDate,
      pub authorized_office_ids: Vec<i64>,
      pub user_message: &'a str,
  }
  pub struct ResolvedRequest {
      pub capability_id: String,
      pub parameters: BTreeMap<String, ResolvedParameter>,
      pub unfilled_required: Vec<String>,
  }
  pub struct ResolvedParameter { pub value: ResolvedValue, pub source: PayloadSource }
  ```

- [ ] **Step 1: Write failing unit test for the "loan_arrears_clients" scenario**

Assert `resolve()` on a policy set with `as_of: default=business_today` +
`limit: default=unbounded` and an extraction with no user-typed dates
produces `parameters = { as_of: (business_today, catalog_default), limit: (Unbounded, catalog_default) }`
and `unfilled_required = []`.

- [ ] **Step 2: Verify fail**

- [ ] **Step 3: Implement `resolve()`**

Follow spec §5.4 order: user text → LLM hint → YAML default. For the
LLM-hint step use the mapping table in spec §5.4 (freshly added in
review). For type-check step, reject values that violate `hard_cap`.

- [ ] **Step 4: Verify pass**

- [ ] **Step 5: Commit**

```bash
git add crates/chat/src/assistant/understanding/resolver.rs
git commit -m "feat(chat): implement Layer-2 deterministic resolver"
```

### Task 5.2: LLM-hint mapping table tests

Add one unit test per row of the mapping table in spec §5.4.

- [ ] **Step 1: Write nine failing tests** (one per row, one for `range`, one for `none`).

- [ ] **Step 2: Verify fail**

- [ ] **Step 3: Fill any gaps in the mapping helper**

- [ ] **Step 4: Verify pass**

- [ ] **Step 5: Commit**

```bash
git add crates/chat/src/assistant/understanding/resolver.rs
git commit -m "test(chat): cover full temporal-hint mapping table"
```

---

## Phase 6 — Layer 3 (Clarification Decider)

### Task 6.1: Decider outcome type

**Files:**
- Create: `crates/chat/src/assistant/understanding/decider.rs`

**Interfaces:**
- Produces:
  ```rust
  pub enum DecisionOutcome {
      Execute { capability_id: String, parameters: BTreeMap<String, ResolvedParameter> },
      Clarify { question: String, options: Vec<ClarificationOption>, missing_fields: Vec<String> },
      Reject { code: String, message: String },
  }
  pub fn decide(extraction: &LlmGatewayExtraction, resolved: &ResolvedRequest, classification: DecideOutcome) -> DecisionOutcome;
  ```

- [ ] **Step 1: Write failing tests for each outcome**

Cases:
- `unfilled_required.is_empty()` and classification `Match` → `Execute`.
- `unfilled_required = ["from_date"]` → `Clarify`.
- classification `Clarify` regardless of parameters → `Clarify`.
- intent_kind `UnsafeRequest` → `Reject`.

- [ ] **Step 2: Verify fail**

- [ ] **Step 3: Implement**

- [ ] **Step 4: Verify pass**

- [ ] **Step 5: Commit**

```bash
git add crates/chat/src/assistant/understanding/decider.rs
git commit -m "feat(chat): implement Layer-3 clarification decider"
```

---

## Phase 7 — Runtime wiring

### Task 7.1: Route requests through gateway → resolver → decider

**Files:**
- Modify: `crates/chat/src/assistant/execution/runtime/mod.rs`
- Modify: `crates/chat/src/assistant/execution/runtime/execution.rs`
- Modify: `crates/chat/src/assistant/understanding/clarification_resolver.rs`

- [ ] **Step 1: Add a `BusinessDate` field to `CanonicalRuntimeContext`**

```rust
pub struct CanonicalRuntimeContext {
    // existing fields ...
    pub business_today: chrono::NaiveDate,
    pub business_date_source: BusinessDateSource,
}
```

- [ ] **Step 2: Populate it in `JobService::run_graph_skeleton`**

Call `self.business_date.today().await?` and put the result on the context.

- [ ] **Step 3: Insert gateway call into the entry step**

At the beginning of the current router step, invoke
`GatewayClient::extract`. On success, store the extraction on
`memory.state_json.llm_extraction`. On `SchemaInvalidAfterRetry`,
return the sanitized fail-terminal path.

- [ ] **Step 4: Resolve + decide**

For the top-scoring candidate, run `resolver::resolve` then `decider::decide`.
Feed the outcome into the existing execution/clarification branches.

- [ ] **Step 5: Cargo check + focused tests**

```bash
cargo check -p chat
cargo test -p chat --test extraction_gateway_scenarios --no-run
```

- [ ] **Step 6: Commit**

```bash
git add crates/chat/src/assistant/
git commit -m "feat(chat): wire gateway → resolver → decider into graph runtime"
```

### Task 7.2: Retire the legacy `DeterministicExtraction` primary path

**Files:**
- Modify: `crates/chat/src/assistant/understanding/extraction/mod.rs`

The deterministic extractors become validators, not primary source.

- [ ] **Step 1: Mark `resolve_temporal` / `extract_domain` / `extract_quantity` as pub(crate) verification helpers**

Update callers to the new gateway. Leave the algorithms in place for
verification of LLM output (used to sanity-check `entities` and
`temporal_hint.phrase_span`).

- [ ] **Step 2: `cargo check -p chat`**

- [ ] **Step 3: Commit**

```bash
git add crates/chat/src/assistant/understanding/extraction/
git commit -m "refactor(chat): demote deterministic extractors to verification helpers"
```

---

## Phase 8 — Scenario tests (spec §7)

### Task 8.1: End-to-end scenario harness

**Files:**
- Create: `crates/chat/tests/extraction_gateway_scenarios.rs`

For every row of spec §7:

- [ ] **Step 1: Write a test that seeds a stubbed LLM returning the expected extraction JSON**

Use `spawn_app` (existing test harness) with a stub LLM planner client. Post
the sample user message via `/chat/jobs`. Assert:

- The returned job either reaches `Completed` (no clarification) or produces
  a specific clarification payload for the ambiguous case.
- The stored `ResolvedRequest.parameters` matches the expected filling for
  the row.
- No `chat.clarification_requested` audit event is emitted for the six
  auto-execute rows.

- [ ] **Step 2: Verify fail**

- [ ] **Step 3: Iterate until all seven rows pass**

- [ ] **Step 4: Commit**

```bash
git add crates/chat/tests/extraction_gateway_scenarios.rs
git commit -m "test(chat): cover seven spec §7 scenarios end-to-end"
```

---

## Phase 9 — Docs and verification

### Task 9.1: User-facing doc

**Files:**
- Modify: `docs/current/extraction-gateway.md` (started in Task 1.1)

- [ ] **Step 1: Extend with sections**

Cover: how defaults are chosen, examples of "today" vs business date,
observation of `business_date.fallback_used` events, migration notes for
capability authors.

- [ ] **Step 2: Commit**

```bash
git add docs/current/extraction-gateway.md
git commit -m "docs: publish extraction-gateway integration guide"
```

### Task 9.2: Final workspace test run

- [ ] **Step 1:**

```bash
cargo fmt --check
cargo check --workspace
cargo test -p chat --lib
cargo test -p chat --test business_date_provider --test extraction_gateway_scenarios
```

Expected: all green.

- [ ] **Step 2: Update roadmap**

Append this phase to `docs/current/status.md` under a fresh dated entry.

- [ ] **Step 3: Commit**

```bash
git add docs/current/status.md
git commit -m "docs(status): record extraction-gateway shipping"
```

---

## Self-review checklist (author fills BEFORE handing to executor)

- [ ] Every spec §7 worked example has a corresponding assertion in
      `tests/extraction_gateway_scenarios.rs` (Task 8.1).
- [ ] Every DSL row in spec §5.2 has a passing case in
      Tasks 2.1 and 2.4 tests.
- [ ] Every spec §5.4 mapping-table row has a passing test in Task 5.2.
- [ ] `BusinessDateProvider` is threaded from `ChatAppState::new` all the
      way into the resolver via `CanonicalRuntimeContext` (Task 7.1).
- [ ] `business_date.fallback_used` audit event fires only from the
      auditing wrapper (Task 1.5) and never from timestamp code paths.
- [ ] Legacy `required_parameters` / `optional_parameters` /
      `clarification.missing_parameters` no longer appear in any YAML file
      after Task 3.1.
- [ ] No SQL, PII, prompt template, or provider raw text appears in any
      new response, audit event, or sanitized error.
- [ ] No implementation code was added outside the file paths listed in
      "File Structure".
