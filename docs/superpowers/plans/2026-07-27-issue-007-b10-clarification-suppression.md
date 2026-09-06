# Issue 007 — Bundle 10: Clarification Suppression Guarantee — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:test-driven-development for every task. Steps use checkbox (`- [ ]`) syntax. **Do not include commit steps** — the user commits manually.

**Goal:** Prove and durably guard both directions of the clarification contract:
a fully-defaulted capability never asks (W-E); a defaultless required parameter
(`client_name_lookup.search`) always asks on the confident routing path (W-O F8) —
returning a `collect_fields` clarification instead of a confident empty table or a
generic error.

**Architecture:** One policy-derived helper (`defaultless_missing_fields`) is the
single source of truth for both halves. F8 = narrow the extractor + call that
helper as a pre-execution gate above the canonical/legacy split. W-E = a validator
rule plus a catalog-wide planning test that fail if a future capability
reintroduces a `from_date`/`to_date` clarification.

**Tech Stack:** Rust edition 2024, Cargo workspace, axum, sqlx, PostgreSQL, Redis.
Existing dependencies only.

**Authoritative spec:** `docs/superpowers/specs/2026-07-27-issue-007-b10-clarification-suppression-design.md`.

## Global Constraints

- Approved-SQL only; office scope bound inside SQL via `office_ids = ANY($n::bigint[])`; no Rust post-filter. This bundle adds no SQL.
- No `sqlx` in handlers/services/`assistant/**`. No new crate, dependency, migration, or knowledge/queries YAML surface. **No new YAML field** — the invariant derives from existing `parameter_policies`.
- Sanitized errors: the ask exposes stable input-registry field metadata only, never a raw `bail!` string, SQL, or prompt.
- Same-job continuation: the ask sets `is_missing_execution_parameters = true` so `run_with_router` routes the reply back to the same capability/job (`POST /chat/jobs/{job_id}/responses`). No new job.
- "today" = `EvaluationContext.business_today`; wall clock only for audit. English-only copy.
- Do not touch `output_mode`, presentation, F1–F7, or loan capabilities.

---

## Task 1: Record a green baseline

**Files:** Read only.

- [ ] **Step 1: Confirm the tree compiles and the relevant suites are green**

Run:
```bash
cargo fmt --check
cargo check -p chat
cargo test -p chat --lib clarification
cargo test -p chat --lib execution::runtime::tests
cargo test -p chat --test catalog_validation
```
Expected: all exit `0`. If any is red or blocked (e.g. no database for
`--test catalog_validation` if it needs one — it does not), record the exact
command and error before changing code.

- [ ] **Step 2: Pin the two facts this bundle depends on**

Run:
```bash
grep -n "windows(2)" crates/chat/src/assistant/understanding/extraction/token.rs
grep -nE "required: true" knowledge/queries/client/name_lookup.yaml
```
Expected: the adjacency `windows(2)` loop is still present in `token.rs`
(`search` extractor not yet narrowed); `search` is `required: true` with no
`default` in the client lookup query. If either has already been changed,
re-audit before proceeding.

---

## Task 2: Add the policy-derived `defaultless_missing_fields` helper (TDD)

The single source of truth for both halves. Lives beside the existing
`ClarificationPlanner` so it reuses `required_inputs`, `input_satisfied`,
`field_for`, and `limit_default`.

**Files:**
- Modify: `crates/chat/src/assistant/context/clarification_planner.rs`

- [ ] **Step 1: Write the failing unit tests**

Add to the `#[cfg(test)] mod tests` block in
`crates/chat/src/assistant/context/clarification_planner.rs` (the existing
`catalog(...)` helper builds capabilities with `from_date`/`to_date` query params
and, for `*top*` ids, a `top_n`; extend it minimally as below):

```rust
    #[test]
    fn defaulted_capability_has_no_defaultless_fields() {
        // A capability whose from_date/to_date carry policy defaults must expose
        // nothing to ask for, even with empty facts (W-E: never ask).
        let mut c = catalog(vec![("total", None, None)]);
        c.capabilities[0].parameter_policies = vec![
            date_policy("from_date"),
            date_policy("to_date"),
        ];
        let fields =
            defaultless_missing_fields(&c, "total", &ClarificationFacts::default());
        assert!(fields.is_empty(), "defaulted params must not be asked: {fields:?}");
    }

    #[test]
    fn defaultless_required_param_is_asked_when_fact_absent() {
        // A required param with no default and no fact must be asked (F8).
        let mut c = catalog(vec![("lookup", None, None)]);
        c.queries[0].parameters = vec![param("search")];
        c.parameter_inputs.push(input(
            "search",
            vec!["search"],
            ClarificationFieldType::Text,
        ));
        c.capabilities[0].parameter_policies = vec![required_no_default("search")];
        let fields =
            defaultless_missing_fields(&c, "lookup", &ClarificationFacts::default());
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].key, "search");
    }

    #[test]
    fn present_fact_satisfies_defaultless_required_param() {
        let mut c = catalog(vec![("lookup", None, None)]);
        c.queries[0].parameters = vec![param("search")];
        c.parameter_inputs.push(input(
            "search",
            vec!["search"],
            ClarificationFieldType::Text,
        ));
        c.capabilities[0].parameter_policies = vec![required_no_default("search")];
        let facts = ClarificationFacts {
            values: [(
                ConstraintField::PersonName,
                TypedFactValue::PersonName("Tony".into()),
            )]
            .into_iter()
            .collect(),
        };
        assert!(defaultless_missing_fields(&c, "lookup", &facts).is_empty());
    }
```

Add these two test-only builders next to the existing `param`/`input` helpers in
the same `mod tests`:

```rust
    fn date_policy(name: &str) -> crate::knowledge::catalog::parameter_policy::ParameterPolicy {
        use crate::knowledge::catalog::parameter_policy::{DefaultExpr, ParameterPolicy, ParameterType};
        ParameterPolicy {
            name: name.into(),
            kind: ParameterType::Date,
            required: false,
            default: Some(DefaultExpr::BusinessToday),
            fill_when_missing: true,
            user_may_override: true,
            hard_cap: None,
        }
    }
    fn required_no_default(name: &str) -> crate::knowledge::catalog::parameter_policy::ParameterPolicy {
        use crate::knowledge::catalog::parameter_policy::{ParameterPolicy, ParameterType};
        ParameterPolicy {
            name: name.into(),
            kind: ParameterType::String,
            required: true,
            default: None,
            fill_when_missing: false,
            user_may_override: true,
            hard_cap: None,
        }
    }
```

Run:
```bash
cargo test -p chat --lib clarification_planner::tests::defaulted_capability_has_no_defaultless_fields
```
Expected: **fails to compile** (`defaultless_missing_fields` does not exist yet).
That is the RED state.

- [ ] **Step 2: Implement the helper**

Add to `crates/chat/src/assistant/context/clarification_planner.rs` (import
`ParameterPolicy` at the top: extend the existing
`use crate::knowledge::...` block with
`catalog::parameter_policy::ParameterPolicy`):

```rust
/// Required user inputs for `capability_id` that carry no policy default and are
/// not yet satisfied by `facts`. These are the only parameters the confident
/// routing path may ask for: parameters with a policy default are filled
/// silently (W-E), so they must never appear here — otherwise the E2 date
/// clarification returns. Today the set is `{ search }` for `client_name_lookup`
/// and empty for every other approved capability.
/// ponytail: `search` is the only member today; the loop stays general so a new
/// defaultless required parameter is covered with no further code change.
pub fn defaultless_missing_fields(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
    facts: &ClarificationFacts,
) -> Vec<ClarificationField> {
    let Some(capability) = catalog
        .capabilities
        .iter()
        .find(|c| c.id == capability_id && c.status == "approved_mvp")
    else {
        return Vec::new();
    };
    let Some(query) = catalog
        .queries
        .iter()
        .find(|q| q.id == capability.query_id)
    else {
        return Vec::new();
    };
    let inputs = required_inputs(query, &catalog.parameter_inputs);
    let defaults = limit_default(capability, query, &inputs);
    inputs
        .into_iter()
        .filter(|input| {
            // Keep only inputs whose every backing query parameter lacks a
            // policy default (and is not a limit/top_n handled by default_limit).
            input.parameters.iter().all(|name| {
                !parameter_has_default(&capability.parameter_policies, name)
                    && !(matches!(name.as_str(), "limit" | "top_n")
                        && capability.defaults.default_limit.is_some())
            })
        })
        .filter(|input| !input_satisfied(input, facts, &defaults))
        .map(|input| field_for(input, facts, capability))
        .collect()
}

fn parameter_has_default(policies: &[ParameterPolicy], name: &str) -> bool {
    policies
        .iter()
        .any(|policy| policy.name == name && !policy.required && policy.default.is_some())
}
```

Run:
```bash
cargo test -p chat --lib clarification_planner::tests
cargo fmt --check
```
Expected: the three new tests pass; existing planner tests still pass; `fmt` clean.

---

## Task 3: Narrow the person-name extractor (TDD)

Delete the adjacency scavenger so `search` is genuinely seen as missing.

**Files:**
- Modify: `crates/chat/src/assistant/understanding/extraction/token.rs`
- Modify: `crates/chat/src/assistant/understanding/extraction/tests.rs`

- [ ] **Step 1: Add the guard test (RED)**

Add to `crates/chat/src/assistant/understanding/extraction/tests.rs`:

```rust
    #[test]
    fn adjacency_to_client_does_not_scavenge_a_filler_word() {
        // "Search client by display name." must NOT yield a person name — the
        // token after `client` is a preposition, not a name (F8 root cause).
        let extraction = extract_message_facts("Search client by display name.");
        assert!(
            !extraction
                .entities
                .iter()
                .any(|e| e.entity_type == AssistantEntityType::PersonName),
            "no person name should be invented from adjacency: {:?}",
            extraction.entities
        );
    }
```

Run:
```bash
cargo test -p chat --lib extraction::tests::adjacency_to_client_does_not_scavenge_a_filler_word
```
Expected: **fails** — today the extractor returns `by` as a person name.

- [ ] **Step 2: Delete the adjacency loop**

In `crates/chat/src/assistant/understanding/extraction/token.rs`, remove the
second `windows(2)` loop so only the explicit `named`/`name` rule remains:

```rust
pub(super) fn extract_person_name(message: &str) -> Option<String> {
    let parts = message
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    for pair in parts.windows(2) {
        if matches!(pair[0].to_ascii_lowercase().as_str(), "named" | "name")
            && pair[1].chars().any(char::is_alphabetic)
        {
            return Some(pair[1].to_string());
        }
    }
    None
}
```

Run:
```bash
cargo test -p chat --lib extraction::tests
cargo fmt --check
```
Expected: the new test passes; `extracts_trusted_person_name` ("find client named
Tony") still passes (it uses the surviving `named` rule); `fmt` clean.

---

## Task 4: Gate required inputs before the canonical/legacy split (TDD)

Ask for the defaultless set on the confident routing path in **both** canonical
and legacy modes, above the branch that today errors in canonical mode.

**Files:**
- Modify: `crates/chat/src/assistant/execution/runtime/execution.rs`
- Modify: `crates/chat/src/assistant/execution/runtime/tests.rs`

- [ ] **Step 1: Write the two failing runtime tests (RED)**

Add to `crates/chat/src/assistant/execution/runtime/tests.rs`, reusing the
existing `runtime_test_catalog()`, `pending_context`, and `empty_memory`
harness (no DB; `fineract_pool = None`):

```rust
    #[tokio::test]
    async fn defaultless_required_search_asks_and_runs_nothing() {
        // F8: client_name_lookup selected, no person named → collect_fields(search),
        // and no execution with a scavenged term.
        let mut context = pending_context(false, 1, "client_name_lookup");
        context.active_domain = Some("client".into());
        context
            .pending_clarification
            .as_mut()
            .unwrap()
            .source_intent
            .as_mut()
            .unwrap()
            .domain = AssistantDomain::Client;
        let catalog = Arc::new(runtime_test_catalog());
        let client = PrincipalContext {
            user_id: Uuid::nil(),
            role: "admin".into(),
            office_ids: vec![1],
            capability_ids: vec!["client_name_lookup".into()],
            can_view_pii: true,
            legacy_api_key_id: None,
        };
        let message = "look up a client please";
        let result = AssistantGraphRuntime::run_with_router(
            empty_memory(),
            context,
            None,
            None,
            None,
            None,
            Some(&catalog),
            Some(&client),
            None,
            RuntimeUserInput {
                message: message.into(),
                source_message: message.into(),
                selected_option_id: Some("client_name_lookup".into()),
                clarification_id: None,
                clarification_revision: None,
                constraint_patch: Default::default(),
            },
        )
        .await;

        assert_eq!(
            result.memory.terminal_state,
            Some(TerminalState::WaitingForUserInput)
        );
        let payload = result
            .pending_clarification
            .as_ref()
            .and_then(|p| p.as_ref())
            .expect("must ask for the missing search parameter");
        assert!(
            payload.fields.iter().any(|f| f.key == "search"),
            "collect_fields must carry `search`: {:?}",
            payload.fields
        );
        // No execution ran with an invented term.
        assert!(result.memory.selected_tool.is_none());
        assert_eq!(result.memory.tool_params, json!({}));
    }

    #[tokio::test]
    async fn fully_defaulted_capability_completes_without_asking() {
        // W-E: every parameter has a default → no clarification, terminal is not
        // WaitingForUserInput. With no fineract pool the graph completes at
        // `execution_not_configured`; the guarantee under test is "no ask".
        let mut context = pending_context(false, 1, "organization_office_activity_ranking");
        context.active_domain = Some("organization".into());
        context
            .pending_clarification
            .as_mut()
            .unwrap()
            .source_intent
            .as_mut()
            .unwrap()
            .domain = AssistantDomain::Organization;
        let catalog = Arc::new(runtime_test_catalog());
        let client = PrincipalContext {
            user_id: Uuid::nil(),
            role: "admin".into(),
            office_ids: vec![1],
            capability_ids: vec!["organization_office_activity_ranking".into()],
            can_view_pii: true,
            legacy_api_key_id: None,
        };
        let message = "Rank offices by savings transaction volume";
        let result = AssistantGraphRuntime::run_with_router(
            empty_memory(),
            context,
            None,
            None,
            None,
            None,
            Some(&catalog),
            Some(&client),
            None,
            RuntimeUserInput {
                message: message.into(),
                source_message: message.into(),
                selected_option_id: Some("organization_office_activity_ranking".into()),
                clarification_id: None,
                clarification_revision: None,
                constraint_patch: Default::default(),
            },
        )
        .await;

        assert_ne!(
            result.memory.terminal_state,
            Some(TerminalState::WaitingForUserInput),
            "a fully-defaulted capability must not ask"
        );
        assert!(
            result
                .pending_clarification
                .as_ref()
                .and_then(|p| p.as_ref())
                .is_none(),
            "no clarification payload may be attached"
        );
    }
```

Run:
```bash
cargo test -p chat --lib execution::runtime::tests::defaultless_required_search_asks_and_runs_nothing
```
Expected: **fails** — today, without the gate, the legacy path invents nothing
(after Task 3) but there is no `search` fact, so it re-clarifies only via the
`Ok(None)` bail-recovery; assert the *gate* result shape. Before the gate exists
the payload fields / terminal assertions do not both hold. This is RED.

- [ ] **Step 2: Insert the pre-execution gate**

In `crates/chat/src/assistant/execution/runtime/execution.rs`, inside
`execute_selected_capability`, **after** the `let (Some(catalog), Some(client)) …`
guard and the `intent.is_none()` guard (i.e. immediately before the
`current_user_message_metadata … temporal_error` block at the current `:39`),
insert:

```rust
    // Ask for required inputs that carry no policy default before touching the
    // planner. Parameters with a default are filled silently (W-E); only the
    // defaultless set (today: `search`) may be asked. This runs above the
    // canonical/legacy split so both modes ask identically — the root-cause fix
    // for F8, where canonical mode otherwise returns a generic error.
    let clarification_facts = super::clarification_facts_from_intent(intent.as_ref());
    let missing_fields = crate::assistant::context::clarification_planner::defaultless_missing_fields(
        catalog,
        &capability_id,
        &clarification_facts,
    );
    if !missing_fields.is_empty() {
        let payload = ClarificationPayload {
            version: crate::assistant::clarification::CLARIFICATION_VERSION_1,
            id: uuid::Uuid::new_v4(),
            revision: 0,
            kind: crate::assistant::clarification::ClarificationKind::CollectFields,
            question: "What details should I use for this report?".into(),
            options: Vec::new(),
            fields: missing_fields,
            attempt: active_payload.map_or(0, |p| p.attempt.saturating_add(1)),
            source_intent: intent
                .as_ref()
                .map(|intent| source_intent_snapshot(intent, &intent.reason)),
            allow_free_text: false,
            is_missing_execution_parameters: true,
        };
        return graph_result(
            memory,
            TerminalState::WaitingForUserInput,
            "missing_execution_parameters",
            ResponseBuilder::clarification(payload.clone()),
            recent_message_count,
            Some(Some(payload)),
            execution_transitions(
                TerminalState::WaitingForUserInput,
                "missing_execution_parameters",
            ),
        );
    }
```

`intent` is already bound (`let intent = memory.intent.clone();`) above this point.

- [ ] **Step 3: Add the intent→facts adapter**

`defaultless_missing_fields` needs `ClarificationFacts`. Add a small adapter to
`crates/chat/src/assistant/execution/runtime/mod.rs` (it already imports
`ClarificationFacts`, `ConstraintField`, `TypedFactValue`, `AssistantIntent`):

```rust
/// Minimal fact projection for the pre-execution required-input gate. Only the
/// fields that back a defaultless required parameter matter today (person name
/// for `search`); dates/limits are handled by policy defaults, not asked.
pub(super) fn clarification_facts_from_intent(
    intent: Option<&AssistantIntent>,
) -> ClarificationFacts {
    let mut values = std::collections::BTreeMap::new();
    if let Some(intent) = intent
        && let Some(entity) = intent
            .entities
            .iter()
            .find(|e| e.entity_type == crate::assistant::AssistantEntityType::PersonName)
        && !entity.value.trim().is_empty()
    {
        values.insert(
            ConstraintField::PersonName,
            TypedFactValue::PersonName(entity.value.trim().to_string()),
        );
    }
    ClarificationFacts { values }
}
```

Add `AssistantEntityType` to the `use crate::assistant::{…}` list in `mod.rs` if
not already present (verify with `grep -n AssistantEntityType crates/chat/src/assistant/execution/runtime/mod.rs`).

Run:
```bash
cargo test -p chat --lib execution::runtime::tests
cargo fmt --check
cargo check -p chat
```
Expected: both new runtime tests pass; the existing runtime tests
(`selected_option_with_conflicting_message_reclarifies_and_increments_attempt`,
`source_month_survives_selection_and_limit_falls_back_to_default`, etc.) stay
green; `fmt` and `check` exit `0`.

---

## Task 5: Validator rule — reject reintroduction of the E2 date clarification (TDD)

**Files:**
- Modify: `crates/chat/src/knowledge/catalog/validator.rs`

- [ ] **Step 1: Add the failing unit test (RED)**

Add to the `#[cfg(test)] mod tests` block in
`crates/chat/src/knowledge/catalog/validator.rs`:

```rust
    #[test]
    fn rejects_required_date_parameter_without_a_policy_default() {
        use crate::knowledge::catalog::parameter_policy::{ParameterType, ParameterPolicy};
        let mut catalog = minimal_valid_catalog();
        // Make from_date required in the query with NO covering policy default.
        let query = &mut catalog.queries[0];
        query.parameters.push(QueryParameter {
            name: "from_date".into(),
            kind: "date".into(),
            required: true,
            source: None,
        });
        catalog.capabilities[0].parameter_policies = vec![ParameterPolicy {
            name: "from_date".into(),
            kind: ParameterType::Date,
            required: true,
            default: None,
            fill_when_missing: false,
            user_may_override: true,
            hard_cap: None,
        }];
        let err = KnowledgeValidator::validate(&catalog).unwrap_err();
        assert!(
            err.to_string().contains("from_date")
                && err.to_string().contains("default"),
            "validator must reject a required date param with no default: {err}"
        );
    }
```

Reuse or add a `minimal_valid_catalog()` builder in the test module modelled on
the loader path already used by `rejects_invalid_status` — if none exists,
construct the smallest catalog that passes `validate()` today (one approved
capability + its query with `from_date`/`to_date` defaulted and an
`output_field`), so that only the mutation under test trips the new rule.

Run:
```bash
cargo test -p chat --lib validator::tests::rejects_required_date_parameter_without_a_policy_default
```
Expected: **fails** — the rule does not exist yet.

- [ ] **Step 2: Implement `validate_clarification_contract`**

In `crates/chat/src/knowledge/catalog/validator.rs`, add the function and call it
from `KnowledgeValidator::validate` (inside the per-capability loop, next to the
existing `validate_capability_parameter_contract` call):

```rust
/// A capability may never require a `from_date`/`to_date` (date_range) user
/// parameter without a covering policy default — that is the exact shape W-E
/// removed (E2). Rejecting it at load time makes reintroduction a build failure,
/// not a silent regression discovered by an analyst.
fn validate_clarification_contract(
    capability: &CapabilityKnowledge,
    query: &QueryKnowledge,
) -> Result<()> {
    for parameter in query.parameters.iter().filter(|p| {
        p.required
            && p.source.as_deref() != Some("authorized_scope")
            && matches!(p.name.as_str(), "from_date" | "to_date")
    }) {
        let covered = capability.parameter_policies.iter().any(|policy| {
            policy.name == parameter.name && !policy.required && policy.default.is_some()
        });
        if !covered {
            bail!(
                "capability {} requires {} with no policy default; this reintroduces \
                 the date clarification W-E removed — add `required: false` with a \
                 `default` expression",
                capability.id,
                parameter.name
            );
        }
    }
    Ok(())
}
```

Wire the call (find the existing `validate_capability_parameter_contract(...)?`
call site and add the new one immediately after it):

```rust
        validate_clarification_contract(capability, query)?;
```

Run:
```bash
cargo test -p chat --lib validator::tests
cargo fmt --check
```
Expected: the new test passes; all existing validator tests pass. If a real
capability trips the new rule, that is a genuine catalog bug — stop and confirm
the offending YAML with the user before weakening the rule.

---

## Task 6: Catalog-wide durable guard for W-E (E1 + E3)

Prove over the **whole real catalog** that every non-clarifying capability plans
to completion without asking.

**Files:**
- Modify: `crates/chat/tests/catalog_validation.rs`

- [ ] **Step 1: Add the catalog-wide planning test**

Add to `crates/chat/tests/catalog_validation.rs` (it already loads the real
catalog via `KnowledgeLoader`; mirror the existing load helper used by
`client_name_lookup_policy_requires_capability_and_marks_pii_visibility`):

```rust
#[test]
fn every_fully_defaulted_capability_plans_without_asking() {
    use chat::assistant::context::clarification_planner::defaultless_missing_fields;
    use chat::assistant::plan_selected_capability_verified;
    use chat::assistant::{AssistantIntent, ClarificationFacts};
    use chat::knowledge::catalog::parameter_policy::EvaluationContext;

    let catalog = load_real_catalog(); // existing helper in this test file
    let ctx = EvaluationContext {
        business_today: chrono::NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        wall_today: chrono::NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        authorized_office_ids: vec![1],
    };
    for capability in catalog
        .capabilities
        .iter()
        .filter(|c| c.status == "approved_mvp")
    {
        // "Non-clarifying" = the policy-derived must-ask set is empty.
        if !defaultless_missing_fields(&catalog, &capability.id, &ClarificationFacts::default())
            .is_empty()
        {
            continue; // e.g. client_name_lookup (search) — covered by Task 4.
        }
        let intent = AssistantIntent {
            reason: capability.id.clone(),
            ..bare_intent(&capability.domain)
        };
        let plan = plan_selected_capability_verified(
            &catalog,
            &capability.id,
            &intent,
            None,
            Some(&ctx),
        )
        .unwrap_or_else(|e| {
            panic!(
                "fully-defaulted capability {} must plan without asking: {e}",
                capability.id
            )
        });
        let query = catalog
            .queries
            .iter()
            .find(|q| q.id == capability.query_id)
            .unwrap();
        for parameter in query
            .parameters
            .iter()
            .filter(|p| p.source.as_deref() != Some("authorized_scope"))
        {
            assert!(
                plan.params.get(&parameter.name).is_some(),
                "capability {} left parameter {} unfilled — it would ask",
                capability.id,
                parameter.name
            );
        }
    }
}
```

Add a `bare_intent(domain: &str) -> AssistantIntent` helper in this test file that
returns an `AssistantIntent` with `DataLookup`/`Aggregate` intent, the domain
parsed from `capability.domain`, empty entities and default constraints — modelled
on the intent literals already used in `catalog_validation.rs`. If the file lacks
a `load_real_catalog()` helper, add one that mirrors `runtime_test_catalog()` from
`runtime/tests.rs` (load `../..` `knowledge` + `queries`, then `validate`).

Run:
```bash
cargo test -p chat --test catalog_validation every_fully_defaulted_capability_plans_without_asking
```
Expected: passes. If it fails for a specific capability, that capability has a
required parameter with no default and no code-default — either it is genuinely
clarifying (and its must-ask set should be non-empty, so the `continue` skips it)
or it is a catalog bug. Do not weaken the assertion; fix the catalog or the
must-ask derivation.

- [ ] **Step 2: Update the existing end-to-end witness doc**

In `crates/chat/tests/chat_jobs.rs`, update the doc comment on
`date_parameters_with_a_policy_default_are_auto_filled_without_asking` (currently
at `:241-245`) to state that the durable, catalog-wide guarantee now lives in
`catalog_validation.rs::every_fully_defaulted_capability_plans_without_asking`,
and this test remains the single end-to-end witness through the HTTP + canonical
stack. Text-only change; no assertion change.

Run:
```bash
cargo fmt --check
cargo check -p chat --tests
```
Expected: exit `0`.

---

## Task 7: Full-bundle validation

**Files:** Read only.

- [ ] **Step 1: Run the whole contract**

Run:
```bash
cargo fmt --check
cargo check -p chat
cargo test -p chat --lib clarification_planner::tests
cargo test -p chat --lib extraction::tests
cargo test -p chat --lib execution::runtime::tests
cargo test -p chat --lib validator::tests
cargo test -p chat --test catalog_validation
git diff --check
```
Expected: all exit `0`; `git diff --check` clean.

- [ ] **Step 2: Confirm no invariant regressed**

Run:
```bash
grep -rnE 'sqlx::|query(_as)?!' crates/chat/src/assistant/context/clarification_planner.rs crates/chat/src/assistant/execution/runtime/execution.rs || echo "clean: no SQL added"
grep -rn "never_ask" knowledge crates/chat/src || echo "clean: no new YAML field"
git status --porcelain knowledge queries migrations
```
Expected: `clean: no SQL added`; `clean: no new YAML field`; the last command
prints nothing (no knowledge/queries/migrations changes).

- [ ] **Step 3: Run the end-to-end witness when a database is available**

Run (needs read-only Fineract DB + Redis, as today):
```bash
cargo test -p chat date_parameters_with_a_policy_default_are_auto_filled_without_asking
```
Expected: passes. If no DB is configured in this environment, record that it was
skipped for the reviewer rather than marking it green.

---

## Completion gate

Done only when: Tasks 2–6 are checked and each was independently green; the F8
positive and W-E negative runtime tests both pass in one `cargo test` run; the
validator rejects a required defaultless date parameter; the catalog-wide guard
passes over every approved capability; no SQL, dependency, migration, or YAML
surface changed; no `never_ask` field was introduced; and no commit step was
performed.
