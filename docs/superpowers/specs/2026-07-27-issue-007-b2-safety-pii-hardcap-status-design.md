# Issue 007 Bundle 2 — Safety Pre-Catalog (W-O F1 / F2 / F7) Design

**Goal:** Make three declared-but-unhonoured contracts true, so the catalog is a
trustworthy review surface *before* Bundles 4/8 add capabilities on top of it:

- **F1 — PII gate:** one authority for the sensitivity label; the renderer's gate
  is a parsed enum (unhandled value = compile/load error), not a loose string match.
- **F2 — `hard_cap`:** the declared per-parameter row ceiling is actually applied
  at the one boundary where the resolved value and its policy are both in hand.
- **F7 — 409 vs 404:** verify the already-landed fix and add the one missing test.

**Authoritative issue:** `docs/issues/active/007-analyst-grade-knowledge-and-request-mapping.md`
§W-O F1 (lines 1067–1149), F2 (1151–1192), F7 (1319–1388).
**Program roadmap:** `docs/superpowers/plans/2026-07-27-issue-007-program-roadmap.md` (Bundle 2).

## Current state (verified 2026-07-27)

Audited against the working tree. **Trust the code below over the 2026-07-24 issue text.**

### F1 — PII gating
- `crates/chat/src/assistant/presentation/builder.rs:331-333`: `is_hidden` is
  `!can_view_pii && field.sensitivity == "pii"` — an exact **string** compare.
  Called from `filtered_row` (`:308`, drops value), `table_column` (`:327`, sets
  `hidden`) and the `pii_hidden` warning (`:43`). **Confirmed as issue describes.**
- `crates/chat/src/knowledge/model.rs:286-293`: `QueryOutputField.sensitivity: String`.
  Only other readers: `management/knowledge.rs:117` (`.clone()` into a DTO `String`)
  and `validator.rs:299` (`validate_status` against `SENSITIVITY_CLASSES`).
- `crates/chat/src/knowledge/catalog/validator.rs:38-45,296-301`: `SENSITIVITY_CLASSES`
  is a **6-value** string allowlist used *only* at `:300` for query output fields.
- **Query YAML sensitivity values, verified by grep:** exactly two are ever used —
  `public_business` (161 fields) and `pii` (17 fields). `pii_conditional` as a
  *sensitivity value* appears nowhere. The audit's `pii_conditional` claim is
  **refuted** (matches issue's own correction). The other four allowlist values
  (`sensitive_business_identifier`, `security_sensitive`, `secret_never_expose`,
  `free_text_sensitive`) appear **only** under `knowledge/schema/**`, which loads as
  untyped `GenericKnowledge` — never as a `QueryOutputField`. They are inert.
- **`client_id` is inconsistent, confirmed:** `public_business` on 6 query YAMLs
  (`savings/pending_charges_clients`, `savings/activity_list`, `savings/deposit_top_n`,
  `savings/deposit_monthly_top_n`, `savings/withdrawal_top_n`,
  `savings/withdrawal_monthly_top_n`) and `pii` on 5 (`client/client_list_recent`,
  `client/top_n_by_savings_balance`, `client/client_random_sample`,
  `client/top_n_by_savings_account_count`, `client/top_n_by_deposit_volume`).
  `client/name_lookup` is `public_business`. **`client_display_name` is `pii` on all
  11 files — already consistent.**
- **Capability-level blocks are dropped at load, confirmed:** `CapabilityKnowledge`
  (`model.rs:206-248`) declares no `pii:`/`output_fields:`/`checks:` fields and carries
  no `deny_unknown_fields`, so serde silently drops them. 30 capability YAMLs still
  carry a `pii:` block (`returns_pii` / `allowed_fields_when_can_view_pii` /
  `omitted_when_cannot_view_pii` / `never_return`) and several carry an `output_fields:`
  block; both are inert. `grep -rn returns_pii crates/` → 0 hits.
- **Gate is unreachable on the chat path today:** `project_admin_principal` forces
  `can_view_pii = true` on every create/respond, so `is_hidden` returns `false` for
  every field. **No live over-disclosure.** F1 corrects the *mechanism* for a future
  non-admin principal; it changes **zero** current behavior.

### F2 — `hard_cap`
- The false comment exists verbatim at
  `crates/chat/src/assistant/execution/tool/parameters.rs:296-302` ("catalog `hard_cap`
  is enforced elsewhere"). **It is false — no site clamps against it.**
- `hard_cap` is parsed (`loader.rs:179,187`), stored (`parameter_policy.rs:46`), and
  type-checked (`parameter_policy.rs:161-169`, rejects `hard_cap` on a non-integer type).
  No comparison of a resolved value against it exists anywhere.
- **19 capability YAMLs declare a `hard_cap`** (verified by grep), all on the scalar
  `limit` parameter; 11 of 30 declare none.
- `params_from_verified` (`parameters.rs:216-273`) and `normalize_effective_parameters`
  (`:59-95`) are the **only** two builders that produce the bound params JSON. Both
  have the `ParameterPolicy` list in scope: `params_from_verified` via its `policies:
  &[ParameterPolicy]` argument (fed from `capability.parameter_policies` at
  `planning.rs:57-63`), and `normalize_effective_parameters` via
  `executable_capability(...).parameter_policies`. This is the single boundary the
  issue's "clamp where value and policy are both in hand" points at.
- **Drift the plan must handle:** `crates/chat/src/assistant/execution/tool/tests.rs:359-400`
  (`defaults_unbounded_limit_when_policy_declares_it`) currently declares
  `hard_cap: Some(100)` and *asserts* `params["limit"] == i64::MAX`. That assertion
  encodes the bug; enforcing the cap **must** change it to `100`.

### F7 — 409 vs 404
- **Already fixed on this branch.** `crates/chat/src/job/service/mod.rs:291-301`: the
  `if structured` guard is **gone**; both inactive submission styles return
  `PersistResponseOutcome::NotActive => RespondToChatJobOutcome::NotActive`; the
  explanatory comment is at `:294-297`.
- `crates/chat/src/api/handlers/job.rs:208-214`: `NotFound → 404 "chat job not found"`,
  `NotActive → 409 code=clarification_not_active`. Correct.
- `crates/chat/tests/chat_jobs.rs:196` — `follow_up_message_stays_on_the_same_job`
  exists (the rename described in the issue landed). The second test,
  `required_parameter_without_default_asks_and_answer_continues_same_job`, **does not
  exist yet** — the only remaining F7 work.

## Constraints

Every cross-cutting invariant applies unchanged: approved-SQL only; office scope bound
**inside** SQL via `office_ids = ANY($n::bigint[])`, never Rust post-filter; `sqlx`
only in repositories; PII gating field-level; "today" = Fineract tenant business date
(wall clock for audit only); sanitized errors; PostgreSQL durable truth, Redis live-SSE
only; same-job clarification via `POST /chat/jobs/{job_id}/responses`; exactly three
crates; **no new dependencies, migrations, or new `knowledge`/`queries` YAML surface**
(this bundle adds no capability — it only edits existing YAML labels and deletes inert
blocks); English-only copy.

## Design

### F1 — one authority, a typed gate, a load-time rule

**Single authority = the query-level `output_fields[].sensitivity`.** The
capability-level blocks add no information the query does not already carry and would
require a per-file coherence check to stay true; they are deleted, not wired.

1. **Reconcile `client_id` → `public_business` everywhere.** Edit the 5 `client/*`
   query YAMLs that say `pii` to `public_business`. A client id is a non-identifying
   business key; the identifying field is `client_display_name`, which stays `pii` on
   all 11 files. This makes the catalog agree with itself. **No runtime effect today**
   (admin projection), so this is safe to land pre-catalog.

2. **Delete the inert capability-level `pii:` and `output_fields:` blocks** from all
   capability YAMLs. They are already dropped by serde; deleting them removes a false
   review surface. Leave `request_shape.pii:` (a different, modelled field) untouched.

3. **Parse `sensitivity` into a two-variant enum.** In `model.rs`:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
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
   `QueryOutputField.sensitivity` becomes `Sensitivity`. Because only these two values
   are the ones the renderer acts on, serde now **rejects at catalog load** any query
   output field carrying a value the renderer does not honour — this *is* the validator
   rule the issue asks for, enforced structurally. (The four schema-only classes live
   in untyped `GenericKnowledge` and are unaffected.)

4. **Make `is_hidden` an exhaustive match** so a future variant is a compile error:
   ```rust
   fn is_hidden(field: &QueryOutputField, can_view_pii: bool) -> bool {
       match field.sensitivity {
           Sensitivity::Pii => !can_view_pii,
           Sensitivity::PublicBusiness => false,
       }
   }
   ```

5. **Downstream readers:** `management/knowledge.rs:117` uses
   `field.sensitivity.as_str().to_string()` (DTO field stays `String`). In
   `validator.rs`, delete the now-dead `validate_status(... SENSITIVITY_CLASSES)` call
   for query output fields and the `SENSITIVITY_CLASSES` const (its only use); the enum
   subsumes it. The `builder.rs` unit test constructs `Sensitivity::PublicBusiness` /
   `Sensitivity::Pii` instead of strings.

**Not done (deliberate):** `CapabilityKnowledge` does **not** gain `deny_unknown_fields`
— `CapabilityDefaults` and other partial structs already rely on serde dropping
unmodelled keys (e.g. `defaults.exclude_reversed`), so denying unknowns would break load.
Deletion of the inert blocks is the fix; hardening the struct is out of scope.

### F2 — clamp the declared `hard_cap` at the builder boundary

Add one private helper in `parameters.rs` and call it from **both** param builders — the
root-cause point every bound-params path routes through:

```rust
/// Clamp any scalar integer parameter to its policy `hard_cap`. hard_cap is
/// validated to only appear on integer/integer_array types (parameter_policy.rs);
/// array params have no scalar ceiling to apply, so only i64 values are clamped.
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

- `params_from_verified`: `clamp_hard_caps(&mut params, policies);` before
  `Ok(Value::Object(params))`.
- `normalize_effective_parameters`: `clamp_hard_caps(&mut params, &capability.parameter_policies);`
  before its `Ok(...)`.

**Audit-visibility** is the `tracing::warn!` (structured `requested`/`applied`, no SQL or
prompt text) **plus** the already-persisted clamped value in the plan snapshot's
`normalized_parameters`. No new event type, no migration, no new plumbing.

This makes `limit: unbounded` bind `min(i64::MAX, hard_cap)` = the cap instead of
`9223372036854775807`, and clamps an over-large *user-supplied* limit identically,
regardless of whether the value came from the user, a policy default, or
`DEFAULT_REPORT_LIMIT`.

**Fix the false comment** at `parameters.rs:296-302` so it states the truth: the
`Unbounded → i64::MAX` sentinel is the *pre-clamp* value; the parameter's `hard_cap`,
if declared, is applied by `clamp_hard_caps` in the same builder before the value is
bound.

**Explicitly OUT OF SCOPE (belongs to Bundle 6 / W-I):**
- A configured **global backstop** for the 11 capabilities that declare no `hard_cap`.
- A user-facing **truncation warning** in the response.
- Loading `timeout_ms` / `cost_class` (F3).

This bundle only makes the **declared** cap real. An uncapped capability stays uncapped
until Bundle 6 adds the backstop.

### F7 — verify, then add the one missing test

Code is already correct (see Current state). No production edit. Add
`required_parameter_without_default_asks_and_answer_continues_same_job` to
`crates/chat/tests/chat_jobs.rs`, modelled on the tolerant style of
`follow_up_message_stays_on_the_same_job`: drive `client_name_lookup` (its `search`
parameter is `required: true` with no default), send a follow-up on the **same** job,
and assert the `/responses` route is reachable (status ∈ {200,201,400,409}, **never 404
/ 401 / 500**) and the job under that id is unchanged. The tolerant status set keeps the
test green whether turn 1 asks (still-clarifying) or completes, so it guards the F7
same-job contract without depending on F8's turn-1-ask behavior (a separate bundle).

## Testing strategy

- **F1 unit (`builder.rs` tests):** the existing
  `hides_pii_columns_and_values_when_policy_disallows_pii` continues to pass with the
  enum; add a case that `Sensitivity::PublicBusiness` is never hidden even when
  `can_view_pii = false`. Compile-time guard: the exhaustive `match` in `is_hidden`.
- **F1 load-time:** a `catalog_validation` assertion (or existing catalog load test)
  proves a bad sensitivity string fails deserialization; the real catalog loads clean.
- **F2 unit (`parameters.rs`/`tool/tests.rs`):** rewrite
  `defaults_unbounded_limit_when_policy_declares_it` to assert `params["limit"] == 100`
  (the declared cap) instead of `i64::MAX`; add a case that a user-supplied
  over-cap limit is clamped and a within-cap limit is untouched; add a case that a
  parameter with **no** `hard_cap` is left exactly as-is (proves backstop is out of scope).
- **F7:** the two `chat_jobs.rs` tests (existing + new). These need a database; if the
  harness has none, record the skip explicitly.
- **Whole workspace:** `cargo fmt --check` and `cargo test -p chat` green.

## Out of scope

- Global row backstop, truncation warning, `timeout_ms`/`cost_class` loading (Bundle 6).
- F8 turn-1 "required-no-default asks" behavior (Bundle 10 / W-E).
- Any new capability, query, SQL, migration, or dependency.
- `deny_unknown_fields` on `CapabilityKnowledge`.
- The inert capability `checks:` prose (already dropped by serde) beyond removing the
  `pii:`/`output_fields:` blocks — see open decision.

## Open decisions for spec review

1. **`client_id` → `public_business` (a).** Confirm a client id is non-identifying
   business data in your PII policy, and only `client_display_name` needs the `pii`
   gate. If a client id must stay gated, we instead flip the 6 savings YAMLs to `pii`
   (the opposite reconciliation) — this changes which columns a future non-admin sees.
2. **Delete vs keep capability `pii:`/`output_fields:` blocks (b).** Recommended: delete
   (they are inert and misleading). Confirm no external tool reads these YAML blocks
   out-of-band. Also confirm whether to additionally strip the now-orphaned pii-related
   `checks:` entries (also inert) for coherence, or leave the whole `checks:` block.
3. **`hard_cap` audit annotation shape (d).** Recommended: a structured `tracing::warn!`
   plus the clamped value already in the plan snapshot. Confirm that satisfies "audit-
   visible"; a durable `chat_job_audit_events` row would need new plumbing/migration and
   is deferred to Bundle 6 if required.
4. **New F7 test scope (e).** Recommended: tolerant same-job/non-404 assertion that does
   not depend on F8. Confirm you do not want the stricter "turn 1 must ask" assertion
   here (that pulls F8/Bundle 10 forward).
