# Issue 007 Bundle 2 — Safety Pre-Catalog (W-O F1 / F2 / F7) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:executing-plans` or
> `superpowers:subagent-driven-development` to work task-by-task. Steps use checkbox
> (`- [ ]`) syntax. **No commit steps — the user commits manually.**

**Goal:** Make three declared-but-unhonoured contracts true: F1 (PII label → single
authority + typed gate), F2 (`hard_cap` actually applied), F7 (verify the landed 409 fix
and add the one missing test).

**Architecture:** F1 is a query-YAML reconciliation + a `String → enum` on
`QueryOutputField.sensitivity` with an exhaustive `is_hidden` match. F2 is one private
`clamp_hard_caps` helper called from the two param builders. F7 is verify-only plus one
integration test. No new crate, dependency, migration, SQL, or new capability YAML.

**Tech Stack:** Rust edition 2024, Cargo workspace, axum, sqlx, PostgreSQL, Redis,
serde/serde_yaml. Existing dependencies only.

**Authoritative spec:** `docs/superpowers/specs/2026-07-27-issue-007-b2-safety-pii-hardcap-status-design.md`.

## Global Constraints

- Exactly `crates/app`, `crates/core`, `crates/chat`. No new crate, dependency,
  migration, or new `knowledge`/`queries` YAML file (this bundle edits/deletes existing
  YAML only).
- Approved-SQL only; office scope bound inside SQL via `office_ids = ANY($n::bigint[])`,
  never Rust post-filter; `sqlx` only in repositories.
- PII gating stays field-level; sanitized errors (no SQL/prompt/stack in logs or
  responses); PostgreSQL durable truth, Redis live-SSE only; same-job clarification via
  `POST /chat/jobs/{job_id}/responses`; English-only copy.
- Do **not** add `deny_unknown_fields` to `CapabilityKnowledge`.
- A task is done when its listed checks exit `0` and `git diff --check` is clean.

---

## Task 1: Green baseline

**Files:** read only.

- [ ] **Step 1: fmt + full-workspace check**

Run:
```bash
cargo fmt --check
cargo check
```
Expected: both exit `0`.

- [ ] **Step 2: record the F2 unit baseline and F1 renderer baseline**

Run:
```bash
cargo test -p chat --lib assistant::execution::tool::tests::defaults_unbounded_limit_when_policy_declares_it
cargo test -p chat --lib assistant::presentation::builder::tests::hides_pii_columns_and_values_when_policy_disallows_pii
```
Expected: both pass. The first passing while asserting `i64::MAX` under `hard_cap:
Some(100)` is the bug this plan flips — note it now.

- [ ] **Step 3: confirm F7 is already fixed (verify-only, no edit)**

Run:
```bash
grep -n "if structured" crates/chat/src/job/service/mod.rs || echo "GUARD GONE (expected)"
grep -n "PersistResponseOutcome::NotActive => RespondToChatJobOutcome::NotActive" crates/chat/src/job/service/mod.rs
grep -n "clarification_not_active" crates/chat/src/api/handlers/job.rs
grep -n "async fn follow_up_message_stays_on_the_same_job" crates/chat/tests/chat_jobs.rs
```
Expected: `GUARD GONE (expected)`; the `NotActive => NotActive` arm present at
`mod.rs:~298`; `clarification_not_active` present in the handler; the existing test
present. If the guard is NOT gone, stop and re-scope — the issue says do not fix F7 twice.

---

## Task 2 (F1): reconcile `client_id` sensitivity to `public_business`

**Files (edit each, `pii` → `public_business` on the `client_id` output field only):**
- `knowledge/queries/client/client_list_recent.yaml`
- `knowledge/queries/client/top_n_by_savings_balance.yaml`
- `knowledge/queries/client/client_random_sample.yaml`
- `knowledge/queries/client/top_n_by_savings_account_count.yaml`
- `knowledge/queries/client/top_n_by_deposit_volume.yaml`

- [ ] **Step 1: verify the 5 target files and confirm `client_display_name` stays `pii`**

Run:
```bash
for f in client_list_recent top_n_by_savings_balance client_random_sample \
         top_n_by_savings_account_count top_n_by_deposit_volume; do
  echo "-- $f"; grep -A1 "name: client_id" "knowledge/queries/client/$f.yaml"
done
```
Expected: each prints `sensitivity: pii` under `name: client_id`.

- [ ] **Step 2: flip each `client_id` field to `public_business`**

In each of the 5 files, the two lines
```yaml
  - name: client_id
    sensitivity: pii
```
become
```yaml
  - name: client_id
    sensitivity: public_business
```
Do **not** touch `client_display_name` (stays `pii`).

- [ ] **Step 3: verify catalog now agrees with itself**

Run:
```bash
grep -rn -A1 "name: client_id" knowledge/queries/ | grep sensitivity | sort | uniq -c
grep -rn -A1 "name: client_display_name" knowledge/queries/ | grep sensitivity | sort | uniq -c
```
Expected: `client_id` shows `public_business` on all 12 occurrences and zero `pii`;
`client_display_name` shows `pii` on all 11.

---

## Task 3 (F1): delete the inert capability-level `pii:` and `output_fields:` blocks

These blocks are dropped by serde today (`CapabilityKnowledge` declares no such fields).
Deleting them removes a false review surface. `request_shape.pii:` is a different,
modelled field — leave it.

**Files:** all capability YAMLs that carry a `pii:` or `output_fields:` block
(30 with `pii:`; enumerate with the Step 1 grep).

- [ ] **Step 1: enumerate the affected files**

Run:
```bash
grep -rln "returns_pii" knowledge/capabilities/
echo "--- output_fields blocks ---"
grep -rln "^output_fields:" knowledge/capabilities/
```
Expected: ~30 files for the first list. Treat these as the edit set.

- [ ] **Step 2: in each file, delete the top-level `pii:` block and any top-level
      `output_fields:` block**

Remove the whole block. Example — in
`knowledge/capabilities/savings/deposit_top_n.yaml` delete:
```yaml
pii:
  returns_pii: conditional
  allowed_fields_when_can_view_pii:
  - client_id
  - client_display_name
  omitted_when_cannot_view_pii:
  - client_id
  - client_display_name
  never_return:
  - account_no
  - external_id
  - ref_no
  - payment_detail_id
```
and:
```yaml
output_fields:
  public:
  - transaction_id
  - transaction_date
  - amount
  - currency_code
  - office_id
  - office_name
  - product_id
  - product_name
  pii_conditional:
  - client_id
  - client_display_name
```
Leave `request_shape:` (including its `pii: conditional_client_identity` sub-key),
`defaults:`, `guards:`, `parameters:`, and `checks:` unchanged. (The `checks:` prose is
also inert; per open decision it is left in place unless the user asks to strip it.)

- [ ] **Step 3: verify the blocks are gone and nothing else moved**

Run:
```bash
grep -rn "returns_pii\|allowed_fields_when_can_view_pii\|omitted_when_cannot_view_pii\|never_return" knowledge/capabilities/ | wc -l
grep -rn "^output_fields:" knowledge/capabilities/ | wc -l
grep -rn "^  pii: conditional" knowledge/capabilities/ | wc -l
```
Expected: first two commands print `0`; the third (the modelled `request_shape.pii`,
indented under `request_shape:`) is unchanged (>0). Adjust the third grep to your files'
indentation if needed — the point is `request_shape.pii` survives.

---

## Task 4 (F1): parse `sensitivity` into a typed enum

**Files:**
- `crates/chat/src/knowledge/model.rs`

- [ ] **Step 1 (RED intent): add the enum and switch the struct field**

In `crates/chat/src/knowledge/model.rs`, above `QueryOutputField`, add:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    PublicBusiness,
    Pii,
}

impl Sensitivity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PublicBusiness => "public_business",
            Self::Pii => "pii",
        }
    }
}
```
Change the struct field:
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct QueryOutputField {
    pub name: String,

    #[serde(rename = "type")]
    pub kind: String,

    pub sensitivity: Sensitivity,
}
```
(`Deserialize` is already imported in this module.)

- [ ] **Step 2: build — expect the three known break sites**

Run:
```bash
cargo check -p chat 2>&1 | grep -E "sensitivity|Sensitivity" | head
```
Expected: compile errors at `builder.rs` (`== "pii"` and the two test constructors),
`management/knowledge.rs:117` (`.clone()` type mismatch), and `validator.rs:299`
(`&field.sensitivity` no longer `&str`). Tasks 5–7 fix each.

---

## Task 5 (F1): exhaustive `is_hidden` + renderer test

**Files:**
- `crates/chat/src/assistant/presentation/builder.rs`

- [ ] **Step 1: replace the string compare with an exhaustive match**

Change `is_hidden` (`builder.rs:331-333`) to:
```rust
fn is_hidden(field: &QueryOutputField, can_view_pii: bool) -> bool {
    match field.sensitivity {
        Sensitivity::Pii => !can_view_pii,
        Sensitivity::PublicBusiness => false,
    }
}
```
Add `Sensitivity` to the `use crate::knowledge::model::{...}` import at the top of the
file (alongside `KnowledgeCatalog, QueryOutputField`).

- [ ] **Step 2: fix the two test constructors and add a public-field case**

In the `#[cfg(test)] mod tests` `catalog()` builder, change the two
`sensitivity: "public_business".into()` / `"pii".into()` to
`sensitivity: Sensitivity::PublicBusiness` / `sensitivity: Sensitivity::Pii`.

Add a test after `hides_pii_columns_and_values_when_policy_disallows_pii`:
```rust
#[test]
fn public_business_columns_are_never_hidden_even_without_pii_access() {
    let field = QueryOutputField {
        name: "amount".into(),
        kind: "decimal".into(),
        sensitivity: Sensitivity::PublicBusiness,
    };
    assert!(!is_hidden(&field, false));
    assert!(!is_hidden(&field, true));
}
```

- [ ] **Step 3: run the renderer tests**

Run:
```bash
cargo test -p chat --lib assistant::presentation::builder::tests
```
Expected: all pass, including the new case and the existing
`hides_pii_columns_and_values_when_policy_disallows_pii`.

---

## Task 6 (F1): fix the management DTO reader

**Files:**
- `crates/chat/src/management/knowledge.rs`

- [ ] **Step 1: convert enum → String for the DTO**

At `management/knowledge.rs:117`, change:
```rust
                    sensitivity: field.sensitivity.clone(),
```
to:
```rust
                    sensitivity: field.sensitivity.as_str().to_string(),
```
(The DTO `OutputFieldResponse.sensitivity` stays `String` — no API shape change.)

- [ ] **Step 2: build**

Run:
```bash
cargo check -p chat 2>&1 | grep "management/knowledge" || echo "management OK"
```
Expected: `management OK`.

---

## Task 7 (F1): drop the dead string allowlist in the validator

The two-variant enum now enforces at load that a query output field only carries a
sensitivity the renderer acts on, so the `SENSITIVITY_CLASSES` string check for query
fields is dead.

**Files:**
- `crates/chat/src/knowledge/catalog/validator.rs`

- [ ] **Step 1: remove the dead validation call and const**

Delete the block at `validator.rs:296-301`:
```rust
                validate_status(
                    "query output sensitivity",
                    &format!("{}.{}", query.id, field.name),
                    &field.sensitivity,
                    SENSITIVITY_CLASSES,
                )?;
```
Keep the surrounding empty-name check (`:291-294`). Then delete the now-unused
`SENSITIVITY_CLASSES` const at `:38-45`.

- [ ] **Step 2: build (catch any other `SENSITIVITY_CLASSES` / `validate_status` user)**

Run:
```bash
cargo check -p chat 2>&1 | grep -E "SENSITIVITY_CLASSES|validate_status|never used" || echo "validator OK"
```
Expected: `validator OK`. If `validate_status` is now unused elsewhere, that is a
separate symbol — do not delete it unless the compiler flags it as dead; leave a
`#[allow(dead_code)]`-free tree only if it is genuinely unused.

- [ ] **Step 3: prove bad sensitivity fails load, real catalog loads clean**

Run:
```bash
cargo test -p chat --test catalog_validation
```
Expected: passes. If no test asserts a bad value fails deserialization, add one to
`crates/chat/tests/catalog_validation.rs` that feeds a query YAML fragment with
`sensitivity: secret_never_expose` and asserts catalog load returns an error (serde
rejects the unknown enum variant).

---

## Task 8 (F2): add `clamp_hard_caps` and wire both builders

**Files:**
- `crates/chat/src/assistant/execution/tool/parameters.rs`

- [ ] **Step 1: add the helper**

In `parameters.rs`, add near the other private helpers:
```rust
/// Clamp any scalar integer parameter to its policy `hard_cap`. `hard_cap` is
/// validated to appear only on integer/integer_array types (see
/// `parameter_policy::validate_policies`); array params have no scalar ceiling,
/// so only i64 values are clamped. Applied at the one boundary where the resolved
/// value and its policy are both in hand.
fn clamp_hard_caps(params: &mut serde_json::Map<String, Value>, policies: &[ParameterPolicy]) {
    for policy in policies {
        let Some(cap) = policy.hard_cap else { continue };
        let Some(requested) = params.get(&policy.name).and_then(Value::as_i64) else {
            continue;
        };
        if requested > cap {
            tracing::warn!(
                target: "assistant::hard_cap_clamp",
                parameter = %policy.name,
                requested,
                applied = cap,
                "row-limit clamped to catalog hard_cap"
            );
            params.insert(policy.name.clone(), json!(cap));
        }
    }
}
```

- [ ] **Step 2: call it from `params_from_verified`**

At the end of `params_from_verified`, replace:
```rust
    Ok(Value::Object(params))
}
```
with:
```rust
    clamp_hard_caps(&mut params, policies);
    Ok(Value::Object(params))
}
```

- [ ] **Step 3: call it from `normalize_effective_parameters`**

`capability` is already bound at the top of `normalize_effective_parameters`. Replace its
trailing:
```rust
    Ok(Value::Object(params))
}
```
with:
```rust
    clamp_hard_caps(&mut params, &capability.parameter_policies);
    Ok(Value::Object(params))
}
```

- [ ] **Step 4: fix the false comment**

Replace the comment at `parameters.rs:296-302` in `resolved_to_value`:
```rust
        // Unbounded: no user-supplied cap. The runtime clamps to i64::MAX so
        // callers that require an integer parameter (e.g. `LIMIT $n`) still
        // bind successfully; catalog `hard_cap` is enforced elsewhere.
        // ponytail: i64::MAX sentinel, upgrade to LIMIT-omitting SQL if a real
        // "no limit" query appears.
        ResolvedValue::Unbounded => json!(i64::MAX),
```
with:
```rust
        // Unbounded: no user-supplied cap. i64::MAX is the pre-clamp sentinel so a
        // required integer parameter (e.g. `LIMIT $n`) still binds; the parameter's
        // catalog `hard_cap`, if declared, is applied by `clamp_hard_caps` in the
        // same builder before this value is bound.
        // ponytail: i64::MAX sentinel, upgrade to LIMIT-omitting SQL if a real
        // "no limit" query appears.
        ResolvedValue::Unbounded => json!(i64::MAX),
```

- [ ] **Step 5: build**

Run:
```bash
cargo check -p chat
```
Expected: exit `0`.

---

## Task 9 (F2): unit tests for the clamp

**Files:**
- `crates/chat/src/assistant/execution/tool/tests.rs`

- [ ] **Step 1: flip the existing bug-encoding assertion**

In `defaults_unbounded_limit_when_policy_declares_it` (`tests.rs:359-400`), the policy
declares `hard_cap: Some(100)` and default `Unbounded`. Change the final assertion:
```rust
    assert_eq!(params["limit"], i64::MAX);
```
to:
```rust
    // Unbounded resolves to the i64::MAX sentinel, then clamp_hard_caps applies the
    // declared hard_cap.
    assert_eq!(params["limit"], 100);
```
Rename the test to `unbounded_limit_is_clamped_to_hard_cap` for accuracy.

- [ ] **Step 2: add a user-over-cap clamp case and a within-cap passthrough case**

Add after that test (reuse the file's `parameter(...)` / `intent_with_quantity(...)`
helpers; a quantity of `Some(n)` supplies a user limit `n`):
```rust
#[test]
fn user_supplied_limit_over_hard_cap_is_clamped() {
    use crate::knowledge::catalog::parameter_policy::{
        EvaluationContext, ParameterPolicy, ParameterType,
    };
    let query = QueryKnowledge {
        id: "test.query".into(),
        database: "fineract".into(),
        sql_file: "test.sql".into(),
        data_areas: Vec::new(),
        tables: Vec::new(),
        metrics: Vec::new(),
        parameters: vec![parameter("limit", true)],
        output_fields: Vec::new(),
    };
    let policies = vec![ParameterPolicy {
        name: "limit".into(),
        kind: ParameterType::Integer,
        required: false,
        default: None,
        fill_when_missing: true,
        user_may_override: true,
        hard_cap: Some(100),
    }];
    let today = chrono::NaiveDate::from_ymd_opt(2026, 7, 24).unwrap();
    let ctx = EvaluationContext {
        business_today: today,
        wall_today: today,
        authorized_office_ids: Vec::new(),
    };

    let over = params_from_verified(
        &query,
        &intent_with_quantity(Some(5000)),
        None,
        &policies,
        Some(&ctx),
    )
    .unwrap();
    assert_eq!(over["limit"], 100, "over-cap user limit must clamp to hard_cap");

    let within = params_from_verified(
        &query,
        &intent_with_quantity(Some(25)),
        None,
        &policies,
        Some(&ctx),
    )
    .unwrap();
    assert_eq!(within["limit"], 25, "within-cap user limit must pass through");
}

#[test]
fn limit_without_hard_cap_is_not_clamped() {
    use crate::knowledge::catalog::parameter_policy::{
        EvaluationContext, ParameterPolicy, ParameterType,
    };
    let query = QueryKnowledge {
        id: "test.query".into(),
        database: "fineract".into(),
        sql_file: "test.sql".into(),
        data_areas: Vec::new(),
        tables: Vec::new(),
        metrics: Vec::new(),
        parameters: vec![parameter("limit", true)],
        output_fields: Vec::new(),
    };
    let policies = vec![ParameterPolicy {
        name: "limit".into(),
        kind: ParameterType::Integer,
        required: false,
        default: None,
        fill_when_missing: true,
        user_may_override: true,
        hard_cap: None,
    }];
    let today = chrono::NaiveDate::from_ymd_opt(2026, 7, 24).unwrap();
    let ctx = EvaluationContext {
        business_today: today,
        wall_today: today,
        authorized_office_ids: Vec::new(),
    };

    let params = params_from_verified(
        &query,
        &intent_with_quantity(Some(5000)),
        None,
        &policies,
        Some(&ctx),
    )
    .unwrap();
    // No hard_cap here: the global backstop for uncapped capabilities is Bundle 6.
    assert_eq!(params["limit"], 5000);
}
```
If `intent_with_quantity` in this file does not accept `Some(n)` as a user limit, inspect
its definition and use the file's existing idiom for supplying a `Quantity::Limit { value }`;
the assertions above are what matters.

- [ ] **Step 3: run the F2 unit tests**

Run:
```bash
cargo test -p chat --lib assistant::execution::tool::tests
```
Expected: all pass, including the renamed and two new tests.

---

## Task 10 (F7): add the missing same-job continuation test

**Files:**
- `crates/chat/tests/chat_jobs.rs`

- [ ] **Step 1: add the test**

After `follow_up_message_stays_on_the_same_job`, add (mirrors its tolerant style; uses
the helpers already in the file: `spawn_app`, `login_admin`, `create_job`,
`wait_for_terminal`, `post_json_bearer`, `get_bearer`):
```rust
/// `client_name_lookup` declares `search` as `required: true` with no default. A
/// follow-up on the SAME job must stay on that job — the /responses route is reachable
/// (200/201/400/409), never a 404 that would push the client to spawn a replacement job
/// (the clarification contract, CLAUDE.md). Tolerant to whether turn 1 asks or completes,
/// so this guards the F7 contract without depending on F8's turn-1-ask behavior.
#[tokio::test(flavor = "multi_thread")]
async fn required_parameter_without_default_asks_and_answer_continues_same_job() {
    let app = spawn_app().await;
    let token = app.login_admin().await;

    let job = create_job(&app, &token, "Find the client named Ada").await;
    let job_id = job["job_id"].as_str().unwrap().to_string();
    let _ = wait_for_terminal(&app, &token, &job).await;

    let resp = app
        .post_json_bearer(
            &format!("/chat/jobs/{job_id}/responses"),
            &token,
            &json!({ "message": "Ada Lovelace" }),
        )
        .await;
    assert!(
        matches!(resp.status().as_u16(), 200 | 201 | 400 | 409),
        "responses route must be reachable on the same job, got {}",
        resp.status()
    );

    let got = app.get_bearer(&format!("/chat/jobs/{job_id}"), &token).await;
    assert_eq!(got.status(), 200);
    let got_json: Value = got.json().await.unwrap();
    assert_eq!(got_json["data"]["id"], job_id);
}
```

- [ ] **Step 2: run both F7 tests**

Run:
```bash
cargo test -p chat --test chat_jobs follow_up_message_stays_on_the_same_job
cargo test -p chat --test chat_jobs required_parameter_without_default_asks_and_answer_continues_same_job
```
Expected: both pass. If the harness has no database and these are skipped/blocked, record
the exact command and skip reason — do not claim green without the run.

---

## Task 11: full verification

**Files:** read only.

- [ ] **Step 1: fmt + whole-workspace check + chat tests**

Run:
```bash
cargo fmt --check
cargo check
cargo test -p chat
```
Expected: `fmt --check` and `check` exit `0`; `cargo test -p chat` green (or, for
DB-gated integration tests, the same skip reason as the pre-existing baseline — no NEW
red). `git diff --check` clean.

- [ ] **Step 2: confirm the catalog still loads with the edited YAML**

Run:
```bash
cargo test -p chat --test catalog_validation
```
Expected: passes — the `client_id` relabel and the block deletions leave a valid catalog,
and the `Sensitivity` enum accepts every query output field in `knowledge/queries/**`.
