# Unified Agentic Clarification Contract Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Build a versioned, job-scoped clarification workflow that bundles predictable option-specific inputs, accepts one typed submission, and preserves verified provenance and legacy clients.

**Architecture:** Add a catalog-owned parameter input registry and a pure clarification planner. Persist private active clarification in job memory, expose a safe discriminated `ClarificationView` through the existing structured response, validate typed submissions with clarification id/revision compare-and-set, and merge accepted answers/defaults into canonical facts before guarded execution.

**Tech Stack:** Rust, Serde/Schemars, Axum, validator, SQLx/PostgreSQL, YAML knowledge catalog, Tokio, existing assistant graph and canonical state.

---

## Preconditions and constraints

- Read `docs/superpowers/specs/2026-07-22-unified-agentic-clarification-contract-design.md` before implementation.
- Preserve the current continuation behavior: `message` remains authoritative, `option_id` is a discriminator, meaningful Others text reroutes or fills missing parameters appropriately, conflicts re-clarify, and unresolved attempts are bounded.
- Coordinate canonical fact changes with `docs/issues/active/003-verified-payload-extraction.md`; do not add a second fact model.
- Work in a dedicated git worktree. The current main workspace contains unrelated changes.
- Keep exactly three crates and route → service → repository → database layering.
- Keep English-only product copy.
- Do not change auth, approved SQL, office-scope, or PII semantics.
- Use TDD. Do not update a failing expectation to match production behavior unless the design explicitly changed that behavior.
- Commit after every task. Stage only files listed by that task.

## Task 0: Create an isolated worktree and establish the baseline

**Files:**
- Read: `docs/current/status.md`
- Read: `docs/current/active-context.md`
- Read: `docs/issues/active/005-unified-agentic-clarification-contract.md`
- Read: `docs/superpowers/specs/2026-07-22-unified-agentic-clarification-contract-design.md`
- Read: `docs/current/chat-client-integration.md` (same-job clarification section)

**Step 1: Create the worktree**

Use the Superpowers worktree skill:

```bash
/skill:using-git-worktrees
```

Create a feature branch such as:

```bash
git worktree add ../ai_report-clarification-contract -b feat/unified-clarification-contract
cd ../ai_report-clarification-contract
```

Expected: clean worktree on the commit containing issue `005`, the new design
spec, and this plan. If the 2026-07-19 continuation work is not committed yet,
land or cherry-pick it first; do not reimplement it inside this feature.

**Step 2: Verify the baseline is clean**

Run:

```bash
git status --short
cargo fmt --check
cargo test -p chat --lib
cargo test -p chat --test assistant_contracts
```

Expected: clean status and all commands pass. If baseline tests fail, stop and
record the exact failure before feature work.

**Step 3: Record the baseline commit**

Run:

```bash
git rev-parse --short HEAD
```

Expected: one commit id recorded in the execution notes. No feature commit is
needed for this task.

---

## Task 1: Add the public and private clarification contracts

**Files:**
- Modify: `crates/chat/src/assistant/context/clarification.rs`
- Modify: `crates/chat/src/assistant/presentation/response.rs`
- Modify: `crates/chat/src/assistant/presentation/builder.rs`
- Modify: `crates/chat/src/assistant/presentation/renderer.rs`
- Modify: `crates/chat/src/assistant/presentation/contracts.rs`
- Modify: `crates/chat/src/assistant/mod.rs`
- Modify: every `AssistantResponse { ... }` constructor reported by the compiler
- Test: `crates/chat/tests/assistant_contracts.rs`
- Test: `crates/chat/src/assistant/presentation/response.rs`

### Step 1: Write failing serialization and invariant tests

Add tests that require:

```rust
#[test]
fn clarification_contract_round_trips_select_option_with_conditional_fields() {
    let view = ClarificationView {
        version: 1,
        id: Uuid::nil(),
        revision: 1,
        kind: ClarificationKind::SelectOption,
        question: "Which report would you like?".into(),
        fields: vec![date_range_field()],
        options: vec![ClarificationChoice {
            id: "savings_deposit_top_n".into(),
            label: "Top Savings Deposits".into(),
            description: Some("Largest deposit transactions.".into()),
            help: None,
            fields: vec![limit_field()],
        }],
        allow_free_text: false,
    };

    let value = serde_json::to_value(&view).unwrap();
    assert_eq!(value["kind"], "select_option");
    assert_eq!(value["fields"][0]["type"], "date_range");
    let decoded: ClarificationView = serde_json::from_value(value).unwrap();
    assert_eq!(decoded, view);
}

#[test]
fn old_assistant_response_without_clarification_still_deserializes() {
    let value = serde_json::json!({
        "response_type": "summary",
        "title": null,
        "message": "ok",
        "sections": [],
        "table": null,
        "cards": [],
        "options": [],
        "warnings": [],
        "actions": [],
        "evidence_refs": [],
        "rendered_markdown": null
    });
    let response: AssistantResponse = serde_json::from_value(value).unwrap();
    assert!(response.clarification.is_none());
}
```

Also test the shape validator:

```rust
assert!(ClarificationView::validate_shape(&select_option_without_options()).is_err());
assert!(ClarificationView::validate_shape(&collect_fields_without_fields()).is_err());
assert!(ClarificationView::validate_shape(&free_text_with_fields()).is_err());
```

### Step 2: Run tests and verify failure

Run:

```bash
cargo test -p chat --test assistant_contracts clarification_contract -- --nocapture
```

Expected: FAIL because `ClarificationView`, kinds, fields, and
`AssistantResponse.clarification` do not exist.

### Step 3: Implement the minimum version-1 contracts

Add these types in `context/clarification.rs` with Serde/Schemars derives and
`snake_case` enums:

```rust
pub const CLARIFICATION_CONTRACT_VERSION: u16 = 1;

pub enum ClarificationKind {
    SelectOption,
    CollectFields,
    FreeText,
}

pub enum ClarificationFieldType {
    DateRange,
    Integer,
    Text,
}

#[derive(Default)]
pub struct ClarificationValidation {
    pub min_integer: Option<i64>,
    pub max_integer: Option<i64>,
    pub max_length: Option<u32>,
    pub max_range_days: Option<u32>,
}

pub struct ClarificationFieldError {
    pub code: String,
    pub message: String,
}

pub struct ClarificationField {
    pub key: String,
    #[serde(rename = "type")]
    pub field_type: ClarificationFieldType,
    pub label: String,
    pub required: bool,
    #[serde(default)]
    pub value: Option<serde_json::Value>,
    #[serde(default)]
    pub default_value: Option<serde_json::Value>,
    #[serde(default)]
    pub help_text: Option<String>,
    #[serde(default)]
    pub validation: ClarificationValidation,
    #[serde(default)]
    pub errors: Vec<ClarificationFieldError>,
}

pub struct ClarificationHelp {
    pub details: String,
    pub example: Option<String>,
    #[serde(default)]
    pub requirements: Vec<String>,
}

pub struct ClarificationChoice {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub help: Option<ClarificationHelp>,
    #[serde(default)]
    pub fields: Vec<ClarificationField>,
}

pub struct ClarificationView {
    pub version: u16,
    pub id: Uuid,
    pub revision: u32,
    pub kind: ClarificationKind,
    pub question: String,
    #[serde(default)]
    pub fields: Vec<ClarificationField>,
    #[serde(default)]
    pub options: Vec<ClarificationChoice>,
    #[serde(default)]
    pub allow_free_text: bool,
}
```

Add `ClarificationView::validate_shape`. Do not add version-2 field types.

Evolve private `ClarificationPayload` additively with defaults that preserve old
persisted JSON:

```rust
#[serde(default = "clarification_version")]
pub version: u16,
#[serde(default = "Uuid::new_v4")]
pub id: Uuid,
#[serde(default = "clarification_revision")]
pub revision: u32,
#[serde(default)]
pub kind: ClarificationKind,
#[serde(default)]
pub fields: Vec<ClarificationField>,
```

If `Uuid::new_v4` as a serde default makes old-state replay nondeterministic,
use `Option<Uuid>` for legacy decode and normalize once when loading. Do not
invent a deterministic id from user text.

Add `clarification: Option<ClarificationView>` to `AssistantResponse` with
`#[serde(default)]`. Set `None` in every non-clarification constructor.

### Step 4: Make ResponseBuilder produce the public projection

Add a pure conversion from private payload to public view. It must strip
`source_intent`, attempts, and internal flags. Populate deprecated top-level
`options` only from public report choices.

Replace prose-only clarification builders:

- `missing_parameter` must accept/build `collect_fields` rather than return an
  implicit message-only shape.
- `free_form_other_prompt` must include a `free_text` view.
- the no-router skeleton response must use a valid clarification view or become
  a non-clarification error; prefer the existing fail-closed error path.

Update Markdown rendering to render public `clarification` fields/options. Keep
legacy rendering fallback for old persisted responses.

### Step 5: Export Schemars roots

Add public contract names/schemas for:

```text
ClarificationView
ClarificationField
ClarificationChoice
```

Update `assistant_contracts_schemas_cover_boundary_roots` accordingly.

### Step 6: Run focused tests

Run:

```bash
cargo test -p chat --test assistant_contracts
cargo test -p chat assistant::presentation --lib
cargo check -p chat
```

Expected: PASS. Confirm old response JSON test passes.

### Step 7: Commit

```bash
git add crates/chat/src/assistant
         crates/chat/tests/assistant_contracts.rs
git commit -m "feat(chat): add versioned clarification contracts"
```

---

## Task 2: Add catalog-owned parameter input metadata and validation

**Files:**
- Create: `knowledge/parameters/date_range.yaml`
- Create: `knowledge/parameters/limit.yaml`
- Create: `knowledge/parameters/search.yaml`
- Modify: `crates/chat/src/knowledge/model.rs`
- Modify: `crates/chat/src/knowledge/catalog/loader.rs`
- Modify: `crates/chat/src/knowledge/catalog/validator.rs`
- Modify: test `KnowledgeCatalog { ... }` constructors found by `rg 'KnowledgeCatalog \{' crates/chat`
- Test: `crates/chat/tests/catalog_validation.rs`
- Test: `crates/chat/tests/catalog_endpoint.rs` if the endpoint exposes catalog counts/shapes

### Step 1: Write failing catalog tests

Add tests for:

```rust
#[test]
fn approved_required_parameters_have_exact_input_contract_coverage() {
    let catalog = load_catalog();
    KnowledgeValidator::validate(&catalog).unwrap();
    for capability in &catalog.capabilities {
        if capability.status != "approved_mvp" { continue; }
        assert_required_inputs_covered_once(&catalog, capability);
    }
}

#[test]
fn rejects_capability_required_parameters_that_do_not_match_query() {
    let mut catalog = load_catalog();
    let capability = catalog.capabilities.iter_mut()
        .find(|c| c.id == "savings_deposit_total").unwrap();
    capability.required_parameters.pop();
    let error = KnowledgeValidator::validate(&catalog).unwrap_err();
    assert!(error.to_string().contains("required parameters"));
}

#[test]
fn rejects_default_limit_above_capability_maximum() {
    let mut catalog = load_catalog();
    let capability = catalog.capabilities.iter_mut()
        .find(|c| c.id == "client_random_sample").unwrap();
    capability.defaults.default_limit = Some(500);
    capability.guards.max_limit = Some(50);
    assert!(KnowledgeValidator::validate(&catalog).is_err());
}
```

Also test that `date_range` covers exactly `from_date` and `to_date`, `limit`
covers `limit`, and `search` covers `search`.

### Step 2: Run tests and verify failure

```bash
cargo test -p chat --test catalog_validation required_parameters -- --nocapture
```

Expected: FAIL because parameter input metadata and typed defaults/guards are not
loaded or validated.

### Step 3: Add typed catalog models

Add:

```rust
pub struct ParameterInputKnowledge {
    pub id: String,
    pub parameters: Vec<String>,
    #[serde(rename = "type")]
    pub field_type: ClarificationFieldType,
    pub label: String,
    pub help_text: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub validation: ClarificationValidation,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CapabilityDefaults {
    pub default_limit: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CapabilityGuards {
    pub max_limit: Option<i64>,
    pub max_date_range_days: Option<u32>,
}
```

Add `parameter_inputs: Vec<ParameterInputKnowledge>` to `KnowledgeCatalog` and
load `knowledge/parameters`. Add typed `defaults`/`guards` to
`CapabilityKnowledge`. Preserve currently used guard/default YAML fields with
serde defaults or flattening if strict deserialization would discard needed
metadata.

Update all manual `KnowledgeCatalog` constructors with
`parameter_inputs: Vec::new()` or a focused fixture helper. Do not weaken tests
by loading production catalog where a small unit catalog is clearer.

### Step 4: Create the three version-1 registry YAML files

Use the exact field keys and bounds from the design. Do not add optional filter
fields yet.

`date_range.yaml`:

```yaml
id: date_range
parameters: [from_date, to_date]
type: date_range
label: Report period
help_text: Select inclusive start and end dates.
required: true
validation: {}
```

`limit.yaml`:

```yaml
id: limit
parameters: [limit]
type: integer
label: Number of records
help_text: Maximum number of records to return.
required: true
validation:
  min_integer: 1
```

`search.yaml`:

```yaml
id: search
parameters: [search]
type: text
label: Client name
help_text: Enter all or part of the client name.
required: true
validation:
  max_length: 200
```

### Step 5: Implement catalog validation

For every approved capability:

1. Find its query.
2. Compute required user parameters as query parameters with `required=true`
   and `source != authorized_scope`.
3. Require set equality with `capability.required_parameters`.
4. Require each required user parameter to be covered exactly once by registry
   entries.
5. Reject registry overlaps and unknown parameters.
6. Validate default limit against required/accepted limit and max limit.
7. Validate positive max values and compatible field/query types.
8. Ensure authorization-derived parameters are not covered by UI contracts.

Map types:

```text
date_range -> two date parameters named from_date/to_date
integer    -> one integer parameter
text       -> one string parameter
```

Do not validate only the prose `checks` array; implement actual Rust checks.

### Step 6: Run focused and full catalog tests

```bash
cargo test -p chat --test catalog_validation
cargo test -p chat --test catalog_endpoint
cargo test -p chat knowledge::catalog --lib
```

Expected: PASS for the current complete approved catalog.

### Step 7: Commit

```bash
git add knowledge/parameters crates/chat/src/knowledge crates/chat/tests
git commit -m "feat(catalog): define clarification input contracts"
```

---

## Task 3: Implement the pure clarification planner

**Files:**
- Create: `crates/chat/src/assistant/context/clarification_planner.rs`
- Modify: `crates/chat/src/assistant/context/mod.rs`
- Modify: `crates/chat/src/assistant/mod.rs`
- Test: `crates/chat/src/assistant/context/clarification_planner.rs` (`#[cfg(test)]` module)

### Step 1: Write failing planner tests

Cover these exact cases with small in-memory catalogs:

```rust
#[test]
fn one_capability_missing_only_dates_collects_one_date_range_field() {
    let result = plan(&["savings_deposit_total"], no_facts());
    assert_eq!(result.payload.kind, ClarificationKind::CollectFields);
    assert_eq!(keys(&result.payload.fields), ["date_range"]);
    assert!(result.payload.options.is_empty());
}

#[test]
fn known_date_range_is_not_asked_again() {
    let result = plan(&["savings_deposit_total", "savings_deposit_top_n"], july_facts());
    assert!(!all_field_keys(&result.payload).contains(&"date_range"));
}

#[test]
fn shared_dates_and_top_only_limit_are_minimized() {
    let result = plan(&["savings_deposit_total", "savings_deposit_top_n"], no_facts());
    assert_eq!(keys(&result.payload.fields), ["date_range"]);
    assert_eq!(option_keys(&result.payload, "savings_deposit_total"), []);
    assert_eq!(option_keys(&result.payload, "savings_deposit_top_n"), ["limit"]);
}

#[test]
fn complete_single_capability_returns_execute_not_clarify() {
    assert!(plan(&["savings_deposit_top_n"], complete_top_facts()).is_complete());
}

#[test]
fn capability_default_removes_limit_and_records_default() {
    let result = plan(&["client_random_sample"], no_facts());
    assert!(result.is_complete());
    assert_eq!(result.approved_defaults["limit"], serde_json::json!(5));
}
```

Also test partial date prefill and incompatible max-date bounds not being lifted
as shared fields.

### Step 2: Run tests and verify failure

```bash
cargo test -p chat clarification_planner --lib -- --nocapture
```

Expected: FAIL because the module and planner do not exist.

### Step 3: Implement planner inputs and result

Use a small adapter rather than coupling the pure planner directly to database
repositories:

```rust
#[derive(Debug, Clone, Default)]
pub struct ClarificationFacts {
    pub values: BTreeMap<ConstraintField, TypedFactValue>,
}

pub enum ClarificationPlanResult {
    Complete {
        capability_id: String,
        approved_defaults: ConstraintPatch,
    },
    Clarify {
        payload: ClarificationPayload,
        approved_defaults: ConstraintPatch,
    },
}

pub struct ClarificationPlanner<'a> {
    catalog: &'a KnowledgeCatalog,
}
```

Planner constructor/function accepts an injected id for deterministic unit tests
or creates `Uuid::new_v4()` at the runtime boundary. Keep the pure calculations
id-independent.

### Step 4: Implement required-input resolution

For each candidate:

- use catalog validator-approved required user parameters;
- group them by parameter input knowledge;
- map facts to field values;
- apply capability bounds;
- apply only typed catalog defaults;
- preserve partial date values;
- create option help from display name, description, first safe example, and
  required field labels.

Do not consult raw prompt keywords or LLM output for field types/bounds.

### Step 5: Implement shared-field minimization

Compare complete field contracts. Lift a field only when every non-Others
candidate contains an equal field. Keep stable catalog/candidate order. Append
Others once after real options.

### Step 6: Run planner tests

```bash
cargo test -p chat clarification_planner --lib
cargo check -p chat
```

Expected: PASS.

### Step 7: Commit

```bash
git add crates/chat/src/assistant/context/clarification_planner.rs \
        crates/chat/src/assistant/context/mod.rs \
        crates/chat/src/assistant/mod.rs
git commit -m "feat(chat): plan minimal clarification fields"
```

---

## Task 4: Make pending clarification authoritative in job memory

**Files:**
- Create: `migrations/20260722120000_add_job_scoped_clarification.sql`
- Modify: `crates/chat/src/assistant/state/memory.rs`
- Modify: `crates/chat/src/job/repository/assistant_memory.rs`
- Modify: `crates/chat/src/job/service/run.rs`
- Modify: `crates/chat/src/assistant/context/builder.rs`
- Modify: `crates/chat/src/conversation/repository/assistant_memory.rs` only for temporary compatibility projection
- Modify: JobMemory fixtures/constructors reported by the compiler
- Test: `crates/chat/tests/assistant_repositories.rs`
- Test: `crates/chat/tests/assistant_context_window.rs`

### Step 1: Write failing repository round-trip test

```rust
#[tokio::test]
async fn job_memory_round_trips_pending_clarification() {
    let mut memory = repo.create(job_id, user_id, "receive_message").await.unwrap();
    memory.pending_clarification = Some(test_payload(Uuid::new_v4(), 1));
    let saved = repo.save(&memory, memory.revision).await.unwrap();
    let loaded = repo.get(job_id, user_id).await.unwrap().unwrap();
    assert_eq!(loaded.pending_clarification, saved.pending_clarification);
}
```

Add a context test showing the pending payload explicitly supplied for Job A is
used even when session projection contains Job B's payload.

### Step 2: Run tests and verify failure

```bash
cargo test -p chat --test assistant_repositories job_memory_round_trips_pending -- --nocapture
cargo test -p chat --test assistant_context_window job_scoped -- --nocapture
```

Expected: FAIL because job memory has no pending field/column and context reads
only session pending.

### Step 3: Add the migration

```sql
ALTER TABLE assistant_job_memory
    ADD COLUMN pending_clarification_json JSONB NULL;
```

Do not copy current session pending into every job: it is not safely attributable
to a job. Existing waiting jobs use the legacy session fallback during rollout.

### Step 4: Persist the new JobMemory field

Add:

```rust
#[serde(default)]
pub pending_clarification: Option<ClarificationPayload>,
```

Update every JobMemory SQL `SELECT`, `RETURNING`, `UPDATE`, row field, and
conversion. Preserve the optimistic memory revision guard.

### Step 5: Change context construction ownership

Load job memory before context in `run_graph_skeleton`. Change context building
to accept the authoritative job pending payload:

```rust
context_builder
    .build(session_id, client, memory.pending_clarification.clone())
    .await?
```

During compatibility rollout, fallback to session pending only when job pending
is `None` and the response is legacy/waiting. Mark the fallback clearly for
removal. Never overwrite a non-`None` job payload with session state.

When a graph result produces `pending_clarification`, apply it to
`result.memory.pending_clarification` before saving. Keep the session write as a
projection only.

### Step 6: Add parallel-job isolation test

Create two jobs in one session, persist distinct payload ids, build each context,
and assert each sees its own payload. The test must not depend on frontend input
disabling.

### Step 7: Run migration/repository/context tests

```bash
cargo test -p chat --test assistant_repositories
cargo test -p chat --test assistant_context_window
cargo test -p chat --lib
```

Expected: PASS.

### Step 8: Commit

```bash
git add migrations/20260722120000_add_job_scoped_clarification.sql \
        crates/chat/src/assistant/state/memory.rs \
        crates/chat/src/job/repository/assistant_memory.rs \
        crates/chat/src/job/service/run.rs \
        crates/chat/src/assistant/context/builder.rs \
        crates/chat/src/conversation/repository/assistant_memory.rs \
        crates/chat/tests
git commit -m "feat(chat): scope pending clarification to jobs"
```

---

## Task 5: Add typed response DTO validation and stale-response CAS

**Files:**
- Modify: `crates/core/src/api/error.rs`
- Modify: `crates/chat/src/api/dto/job.rs`
- Modify: `crates/chat/src/api/handlers/job.rs`
- Modify: `crates/chat/src/job/model.rs`
- Create: `crates/chat/src/job/service/clarification_response.rs`
- Modify: `crates/chat/src/job/service/mod.rs`
- Modify: `crates/chat/src/job/repository/mod.rs`
- Test: `crates/chat/src/job/service/clarification_response.rs`
- Test: `crates/chat/tests/chat_sessions.rs` or a new focused API integration test

### Step 1: Write failing DTO/validator tests

Cover:

- structured request with id/revision/option/answers;
- legacy non-empty message;
- missing revision;
- mixed structured fields without id;
- unknown answer key;
- reversed/too-large date range;
- zero/over-max limit;
- missing required answer;
- Others without meaningful message.

Example:

```rust
#[test]
fn validates_structured_top_deposit_submission() {
    let request = request(json!({
        "clarification_id": Uuid::nil(),
        "clarification_revision": 1,
        "option_id": "savings_deposit_top_n",
        "answers": {
            "date_range": {"from":"2026-07-01","to":"2026-07-31"},
            "limit": 10
        }
    }));
    let validated = validate_submission(&request, &active_payload(), &principal()).unwrap();
    assert_eq!(validated.constraint_patch[&ConstraintField::LimitValue], TypedFactValue::Integer(10));
}
```

### Step 2: Run and verify failure

```bash
cargo test -p chat clarification_response --lib -- --nocapture
```

Expected: FAIL because DTO fields and validation module do not exist.

### Step 3: Evolve the request DTO additively

Implement:

```rust
#[derive(Debug, Deserialize, Validate)]
pub struct RespondToChatJobRequest {
    #[serde(default)]
    #[validate(length(max = 1000))]
    pub message: Option<String>,
    #[serde(default)]
    #[validate(length(max = 200))]
    pub option_id: Option<String>,
    #[serde(default)]
    pub clarification_id: Option<Uuid>,
    #[serde(default)]
    pub clarification_revision: Option<u32>,
    #[serde(default)]
    pub answers: BTreeMap<String, serde_json::Value>,
}
```

Cross-field validation belongs in `clarification_response.rs`, not in the
handler. Trim optional strings once.

### Step 4: Implement safe domain errors

Add coded constructors in `ApiError`:

```rust
pub fn bad_request_with_code(
    code: &'static str,
    message: impl Into<String>,
    details: Value,
) -> Self;

pub fn conflict_with_code(
    code: &'static str,
    message: impl Into<String>,
    details: Option<Value>,
) -> Self;
```

Create a typed `RespondToJobError` with variants for not found, validation,
stale, not active, and internal. Implement explicit handler mapping; never map
validation/stale errors to `500`.

### Step 5: Implement structured field validation and patch mapping

Use active public/private field definitions. Date parsing uses `%Y-%m-%d`,
inclusive range length, and capability max. Integer uses i64 and configured
bounds. Text trims and validates max length.

Map:

```text
date_range.from -> ConstraintField::FromDate / TypedFactValue::Date
date_range.to   -> ConstraintField::ToDate / TypedFactValue::Date
limit           -> LimitMode::TopN + LimitValue::Integer
search          -> PersonName::String (through the existing canonical field used by name lookup)
```

If `search` can semantically represent more than a person name, stop and align
issue `003` before encoding it; do not force a misleading canonical field.

Generate safe `display_message` from validated labels/values. Preserve actual
free text as `source_message`; do not manufacture prose for semantic routing.

### Step 6: Add repository compare-and-set outcome

Change repository response input to include expected id/revision and validated
metadata. Return an enum such as:

```rust
pub enum PersistResponseOutcome {
    Inserted(ChatMessage),
    NotFound,
    NotActive,
    Stale,
}
```

Inside the existing transaction, lock the job and assistant job-memory row. For
structured mode require:

```sql
pending_clarification_json->>'id' = $expected_id
AND (pending_clarification_json->>'revision')::integer = $expected_revision
```

Perform the match while the row is locked. Insert `answers` and
`constraint_patch` into message/checkpoint metadata. Do not clear pending state
in this transaction; the queued runtime consumes/replaces it, while job status
prevents a second accepted submission.

### Step 7: Add stale and concurrent submission integration tests

Assert:

- wrong revision returns `409 clarification_stale`;
- two concurrent posts yield one `201` and one `409`/not-active;
- invalid field returns `400 clarification_validation_error` and inserts no
  message;
- foreign/archived job remains sanitized `404`;
- legacy body still returns `201`.

### Step 8: Run focused tests

```bash
cargo test -p chat clarification_response --lib
cargo test -p chat --test chat_sessions clarification -- --nocapture
cargo check
```

Expected: PASS.

### Step 9: Commit

```bash
git add crates/core/src/api/error.rs \
        crates/chat/src/api/dto/job.rs \
        crates/chat/src/api/handlers/job.rs \
        crates/chat/src/job \
        crates/chat/tests
git commit -m "feat(chat): validate typed clarification responses"
```

---

## Task 6: Merge structured answers and approved defaults into canonical facts

**Files:**
- Modify: `crates/chat/src/assistant/execution/runtime/mod.rs`
- Modify: `crates/chat/src/assistant/execution/runtime/planning.rs`
- Modify: `crates/chat/src/assistant/context/canonical_state/facts.rs`
- Modify: `crates/chat/src/assistant/execution/tool/parameters.rs`
- Modify: `crates/chat/src/job/service/mod.rs`
- Test: `crates/chat/src/assistant/context/canonical_state/tests.rs`
- Test: `crates/chat/src/assistant/execution/runtime/tests.rs`
- Test: `crates/chat/src/assistant/execution/tool/tests.rs`

### Step 1: Write failing canonical provenance tests

```rust
#[tokio::test]
async fn structured_answers_become_clarification_observations() {
    let result = run_with_patch(date_limit_patch()).await;
    let observations = repo.list_observations(result.memory.job_id).await.unwrap();
    assert!(observations.iter().any(|o|
        o.source_kind == FactSourceKind::Clarification &&
        o.field_path == ConstraintField::FromDate));
    assert!(observations.iter().any(|o|
        o.source_kind == FactSourceKind::Clarification &&
        o.field_path == ConstraintField::LimitValue));
}

#[test]
fn approved_default_becomes_lower_precedence_observation() {
    let observations = approved_default_observations(...);
    assert!(observations.iter().all(|o| o.source_kind == FactSourceKind::ApprovedDefault));
}
```

Add a test proving an explicit clarification limit overrides an approved default.

### Step 2: Run and verify failure

```bash
cargo test -p chat canonical_state structured_answers --lib -- --nocapture
cargo test -p chat approved_default --lib -- --nocapture
```

Expected: FAIL because `RuntimeUserInput` has no patch and production does not
create approved-default observations.

### Step 3: Carry the validated patch into runtime

Extend:

```rust
pub struct RuntimeUserInput {
    pub message: String,
    pub source_message: String,
    pub selected_option_id: Option<String>,
    pub clarification_id: Option<Uuid>,
    pub clarification_revision: Option<u32>,
    pub constraint_patch: ConstraintPatch,
}
```

Default conversions use an empty patch. `JobService::respond` passes the
validated patch loaded from the current request/message metadata.

### Step 4: Add approved-default observation helper

Add a helper parallel to `observations_from_patch` that sets
`FactSourceKind::ApprovedDefault`, a stable source id containing capability and
catalog version, and validates through existing contracts. Do not call the
existing clarification helper and mutate the kind afterward.

### Step 5: Merge structured and deterministic facts correctly

In authoritative non-initial planning:

1. append approved-default observations that are still missing;
2. append structured clarification patch observations;
3. extract free-text facts only for fields absent from the structured patch;
4. derive effective constraints;
5. normalize parameters and persist the planner snapshot.

Avoid writing duplicate observations on retry/replay. Use source id and existing
conflicting-replay protection.

### Step 6: Remove the unconditional global required-limit default

Delete `DEFAULT_REPORT_LIMIT`/`default_required_parameter` as execution policy.
Parameter normalization must fail on a missing required field unless effective
constraints contain a verified fact or approved-default observation.

Update tests that relied on the global default. Add explicit catalog defaults to
capabilities only where product behavior approves them; otherwise planner asks
`limit`.

### Step 7: Run focused tests

```bash
cargo test -p chat canonical_state --lib
cargo test -p chat execution::tool --lib
cargo test -p chat execution::runtime --lib
```

Expected: PASS; no required execution field lacks provenance.

### Step 8: Commit

```bash
git add crates/chat/src/assistant crates/chat/src/job/service/mod.rs knowledge/capabilities
git commit -m "feat(chat): preserve clarification answer provenance"
```

---

## Task 7: Integrate clarification planning into retrieval and continuation

**Files:**
- Modify: `crates/chat/src/assistant/execution/runtime/semantic.rs`
- Modify: `crates/chat/src/assistant/execution/runtime/clarification.rs`
- Modify: `crates/chat/src/assistant/execution/runtime/execution.rs`
- Modify: `crates/chat/src/assistant/execution/runtime/transition.rs`
- Modify: `crates/chat/src/assistant/presentation/builder.rs`
- Test: `crates/chat/src/assistant/execution/runtime/tests.rs`
- Test: `crates/chat/tests/chat_no_loop.rs`
- Test: `crates/chat/tests/chat_full_flow.rs`

### Step 1: Write failing one-submit runtime tests

Add deterministic tests for:

1. ambiguous total/top deposit candidates with no facts produce shared
   `date_range` and top-only `limit`;
2. known month removes `date_range`;
3. selected top + structured date/limit executes without another clarification;
4. known capability missing date produces `collect_fields`, not capability
   options;
5. missing limit with no approved default asks `limit`;
6. meaningful Others retains current same-job reroute semantics;
7. invalid/stale option remains bounded and never executes.

Example assertion:

```rust
let view = result.memory.structured_response.unwrap().clarification.unwrap();
assert_eq!(view.kind, ClarificationKind::SelectOption);
assert_eq!(field_keys(&view.fields), ["date_range"]);
assert_eq!(choice_field_keys(&view, "savings_deposit_top_n"), ["limit"]);
```

### Step 2: Run and verify failure

```bash
cargo test -p chat execution::runtime::tests::ambiguous_candidates_bundle_fields --lib -- --nocapture
```

Expected: FAIL because runtime still calls flat `clarification_payload_for` and
discovers fields after selection.

### Step 3: Replace flat ambiguity payload construction

In reranker `Clarify` and invalid-`Select` branches:

- adapt current verified intent/effective facts to `ClarificationFacts`;
- call `ClarificationPlanner` with authorized evidence alternatives;
- persist returned private payload in `GraphRuntimeResult`/JobMemory;
- build public response through the single clarification builder.

Keep retrieval candidate selection and authorization checks unchanged.

### Step 4: Replace error-string missing-parameter clarification

In `execute_selected_capability`, do not expose `error.to_string()` as the user
question and do not create one capability plus Others manually. On a missing
parameter, call the planner for the selected capability. It returns
`collect_fields` with typed missing inputs.

Temporal validation errors update the relevant field errors on the same
clarification id and increment revision. Stable codes include:

```text
invalid_date
range_reversed
range_too_large
required
out_of_range
```

### Step 5: Consume structured submissions before semantic resolution

When id/revision/patch were already validated:

- match selected option directly against active payload;
- do not call embeddings/LLM for the control selection;
- merge patch and re-plan selected capability;
- execute when complete.

Legacy requests continue through exact/embedding/LLM resolution and the
2026-07-19 Others/conflict behavior.

### Step 6: Keep attempts and revisions separate

- field validation increments revision, not unresolved attempt;
- unresolved semantic reply increments both attempt and revision;
- new candidate set creates a new id/revision 1;
- bounded recovery still uses `MAX_CLARIFICATION_ATTEMPTS`.

Add audit values for id/revision/kind without source-intent leakage.

### Step 7: Run runtime and loop tests

```bash
cargo test -p chat execution::runtime --lib
cargo test -p chat --test chat_no_loop
cargo test -p chat --test chat_full_flow clarification -- --nocapture
```

Expected: PASS. Confirm no test accepts a second predictable clarification.

### Step 8: Commit

```bash
git add crates/chat/src/assistant/execution/runtime \
        crates/chat/src/assistant/presentation/builder.rs \
        crates/chat/tests/chat_no_loop.rs \
        crates/chat/tests/chat_full_flow.rs
git commit -m "feat(chat): bundle clarification inputs before selection"
```

---

## Task 8: Verify durable response, SSE, message, and audit projections

**Files:**
- Modify: `crates/chat/src/job/service/run.rs`
- Modify: `crates/chat/src/job/service/events.rs`
- Modify: `crates/chat/src/api/handlers/job.rs` only if serialization differs
- Modify: `crates/chat/src/assistant/presentation/renderer.rs`
- Modify: audit decision/checkpoint payload construction in the existing job/runtime files
- Test: `crates/chat/tests/chat_full_flow.rs`
- Test: `crates/chat/tests/assistant_answer_quality.rs`
- Test: `crates/chat/tests/assistant_repositories.rs`

### Step 1: Write failing projection-equivalence test

Create one waiting clarification and assert:

```rust
assert_eq!(
    get_job["result_json"]["structured_response"]["clarification"],
    latest_sse_update["payload"]["structured_response"]["clarification"]
);
assert_eq!(
    assistant_message["metadata_json"]["assistant_response"]["clarification"],
    get_job["result_json"]["structured_response"]["clarification"]
);
```

Also assert audit output includes safe `clarification_id`, `revision`, `kind`,
option ids, and field keys but excludes `source_intent`, prompt text, SQL, and
principal scope.

### Step 2: Run and verify failure

```bash
cargo test -p chat --test chat_full_flow clarification_projection -- --nocapture
```

Expected: FAIL if any path drops or reshapes the public object.

### Step 3: Reuse one serialized AssistantResponse everywhere

`run_graph_skeleton` already owns the saved `AssistantResponse`. Ensure job
result, assistant message metadata, and emitted update all serialize that same
value. Delete any clarification-specific payload reconstruction.

Keep SSE event names `status`/`update`. Do not add replay/id behavior.

### Step 4: Update Markdown fallback

Render:

- question;
- option labels/descriptions;
- shared and conditional field labels;
- help text only when safe;
- validation messages.

Markdown must not imply that local help/actions are server options. Avoid
rendering internal ids unless no safe label exists.

### Step 5: Add safe audit summaries

Persist only ids/revision/kind/offered keys/resolution outcome/provenance ids.
Use existing audit/checkpoint infrastructure; do not create a second audit
system.

### Step 6: Run projection and response-quality tests

```bash
cargo test -p chat --test chat_full_flow clarification_projection -- --nocapture
cargo test -p chat --test assistant_answer_quality
cargo test -p chat --test assistant_repositories
```

Expected: PASS.

### Step 7: Commit

```bash
git add crates/chat/src/job \
        crates/chat/src/api/handlers/job.rs \
        crates/chat/src/assistant/presentation/renderer.rs \
        crates/chat/tests
git commit -m "feat(chat): unify clarification delivery projections"
```

---

## Task 9: Add the clarification dataset and coverage harness

**Files:**
- Create: `crates/chat/tests/clarification_eval.rs`
- Create: `crates/chat/tests/fixtures/clarification/*.yaml`
- Modify: `crates/chat/Cargo.toml` only if a test-only dependency is genuinely missing (serde_yaml already exists; prefer no change)

### Step 1: Write the fixture loader and failing coverage test

Define the fixture contract from the design and tests that require named
buckets, not an exact fixture count:

```rust
#[test]
fn fixtures_cover_required_behavior_buckets() {
    let fixtures = load_fixtures();
    for bucket in [
        "direct_execution",
        "collect_one",
        "collect_multiple",
        "options_no_fields",
        "options_shared_fields",
        "options_conditional_fields",
        "known_values",
        "partial_values",
        "approved_default",
        "others",
        "invalid_value",
        "stale_revision",
        "parallel_job_isolation",
    ] {
        assert!(fixtures.iter().any(|f| f.bucket == bucket), "missing {bucket}");
    }
}
```

### Step 2: Run and verify failure

```bash
cargo test -p chat --test clarification_eval fixtures_cover -- --nocapture
```

Expected: FAIL because fixture files/buckets are missing.

### Step 3: Add focused fixtures

Create at least one scenario for each named bucket. Add positive and
missing/invalid coverage for every approved required user input contract:

```text
date_range
limit
search
```

Use English prompts only. Do not duplicate retrieval evaluation: planner
fixtures provide candidate capability ids directly.

### Step 4: Execute planner expectations from fixtures

For each fixture:

- load production catalog;
- construct typed known facts;
- call pure planner;
- compare kind, options, shared fields, conditional fields, must-not-ask, and
  direct execution;
- produce failure output containing fixture id and actual structured plan.

Add a second fixture mode for submission validation where useful.

### Step 5: Add catalog-growth coverage

Compute approved required user input keys and assert each has fixture coverage.
Do not assert there are exactly 29 capabilities or exactly N fixtures.

### Step 6: Run the harness

```bash
cargo test -p chat --test clarification_eval -- --nocapture
```

Expected: PASS with per-bucket summary.

### Step 7: Commit

```bash
git add crates/chat/tests/clarification_eval.rs \
        crates/chat/tests/fixtures/clarification
git commit -m "test(chat): add clarification behavior corpus"
```

---

## Task 10: Complete API integration, compatibility, and recovery tests

**Files:**
- Modify: `crates/chat/tests/chat_full_flow.rs`
- Modify: `crates/chat/tests/chat_no_loop.rs`
- Modify: `crates/chat/tests/assistant_answer_quality.rs`
- Modify: `crates/chat/tests/chat_sessions.rs`
- Modify: `crates/chat/tests/common/mod.rs`
- Create: `crates/chat/tests/clarification_api.rs` if existing files become unfocused

### Step 1: Add failing end-to-end one-submit test

Scenario:

1. Create an ambiguous deposit job.
2. Assert waiting response has `select_option` with typed fields.
3. POST selected top option plus date range and limit in one request.
4. Fetch the same job.
5. Assert it does not ask date/limit again.
6. If the test DB has executable rows, assert completed; otherwise assert it
   reached execution/expected configured terminal behavior rather than another
   predictable clarification.

### Step 2: Add recovery and isolation tests

Cover:

- `GET job` restores the exact id/revision after a simulated page refresh;
- Redis-disabled flow still restores it;
- duplicate SSE payload does not change id/revision;
- two same-session jobs have isolated active state;
- historical assistant clarification is read-only by status/revision contract;
- archived session returns sanitized `404`;
- stale and concurrent responses never execute twice.

### Step 3: Add legacy compatibility tests

Retain/add:

- message-only date reply;
- option id plus authoritative message;
- meaningful Others reroute;
- boilerplate Others free-text prompt;
- semantic paraphrase resolver;
- bounded invalid option recovery.

### Step 4: Run focused integration tests

```bash
cargo test -p chat --test clarification_api -- --nocapture
cargo test -p chat --test chat_no_loop
cargo test -p chat --test chat_full_flow clarification -- --nocapture
cargo test -p chat --test chat_sessions clarification -- --nocapture
```

If `clarification_api.rs` was not created, omit its command.

Expected: PASS.

### Step 5: Commit

```bash
git add crates/chat/tests
git commit -m "test(chat): verify structured clarification lifecycle"
```

---

## Task 11: Update client/API/architecture documentation

**Files:**
- Modify: `docs/current/chat-client-integration.md`
- Modify: `docs/architecture/chat-data-model/10-9-clarification-flow-state.md`
- Modify: `docs/api/README.md`
- Modify: `doc-knowledge/responses/clarification.md`
- Modify: `knowledge/responses/clarification.yaml` only for safe copy/rules, not protocol authority
- Modify: `docs/current/status.md`
- Modify: `docs/issues/active/005-unified-agentic-clarification-contract.md` with implementation status only after verification

### Step 1: Write exact TypeScript contracts

Document discriminated types matching Rust serialization:

```ts
type ClarificationKind = "select_option" | "collect_fields" | "free_text";
type ClarificationFieldType = "date_range" | "integer" | "text";

type ClarificationSubmission = {
  clarification_id?: string;
  clarification_revision?: number;
  option_id?: string;
  answers?: Record<string, unknown>;
  message?: string;
};
```

Include exhaustive render switch, local step calculation, unknown-type fallback,
final submission, and `GET job` recovery. Make clear that `201` still returns the
inserted `ChatMessage`.

### Step 2: Document endpoint semantics

Update POST jobs, GET job, stream, responses, messages, and audit examples.
Document `400 clarification_validation_error`, `409 clarification_stale`, and
`409 clarification_not_active` without exposing resource existence across auth
boundaries.

### Step 3: Update architecture state ownership

Replace session-authoritative examples with job-scoped active clarification.
Describe session memory as context/projection only. Include lifecycle and
id/revision CAS.

### Step 4: Update response knowledge copy

Keep response templates business-facing. Do not encode field types/validation
only in template strings; the typed catalog registry is authoritative.

### Step 5: Update issue/status truthfully

Only after all required tests pass:

- add an implementation/resolution note to issue `005`;
- move it to `docs/issues/resolved/` only if every acceptance gate is met;
- update `docs/issues/README.md` link if moved;
- update current status with verified behavior and any explicitly deferred
  compatibility cleanup.

### Step 6: Verify documentation paths and formatting

```bash
git diff --check
rg 'pending clarification' docs/current docs/architecture docs/api
```

Expected: no stale statement claims session pending is authoritative.

### Step 7: Commit

```bash
git add docs doc-knowledge knowledge/responses/clarification.yaml
git commit -m "docs: publish structured clarification client contract"
```

---

## Task 12: Run final verification and adversarial review

**Files:**
- Modify only files required by failures found in this task

### Step 1: Run formatting and static checks

```bash
cargo fmt --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: all PASS.

### Step 2: Run focused new suites

```bash
cargo test -p chat clarification --lib -- --nocapture
cargo test -p chat --test assistant_contracts
cargo test -p chat --test catalog_validation
cargo test -p chat --test clarification_eval -- --nocapture
cargo test -p chat --test chat_no_loop
```

Expected: all PASS.

### Step 3: Run the full chat suite

```bash
cargo test -p chat
```

Expected: PASS. If DB-backed tests require unavailable infrastructure, record the
exact skipped/failed commands and run them in the documented integration
environment before merging; do not claim completion without them.

### Step 4: Run migration validation

Using the documented local test database:

```bash
sqlx migrate run --database-url "$APP_DATABASE_URL"
sqlx migrate info --database-url "$APP_DATABASE_URL"
```

Expected: the job-scoped clarification migration is applied exactly once.

### Step 5: Perform an adversarial diff review

Use the Superpowers requesting-code-review skill. Review specifically for:

- source intent or principal data leaking into public response;
- stale/duplicate race permitting double execution;
- session pending still acting as authority;
- query required parameters missing catalog input coverage;
- global defaults surviving without provenance;
- structured answers being stringified/re-extracted;
- unauthorized office/capability values entering patches;
- implicit prose-only clarification builders;
- legacy behavior regressions;
- dataset being loaded into runtime LLM context.

### Step 6: Run final diff checks

```bash
git diff --check HEAD~1..HEAD
git status --short
```

Expected: no whitespace errors and only deliberate files present.

### Step 7: Commit any review fixes

```bash
git add <review-fix-files>
git commit -m "fix(chat): address clarification contract review"
```

Skip this commit if no fixes were needed.

### Step 8: Prepare merge handoff

Use `/skill:finishing-a-development-branch`. Report actual command outputs,
remaining compatibility deprecations, and any skipped live/DB checks.
