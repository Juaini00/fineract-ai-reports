# Issue 007 — Bundle 13: Drill-down Preparation (W-H + F5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep the ground prepared for a future conversational drill-down feature *without building any of it*. Concretely: (1) mark the two dead `ContextReference` variants as reserved so a cleanup sweep does not delete them; (2) make the `PayloadSource` audit-source enum extensible so a future `prior_job` source is not a contract break, proven by a deserialisation test; (3) record in issue 007 that drill-down is an out-of-scope follow-up, name it, and record the requirement that a future gateway must not silently accept a `context_reference` it cannot honour.

**Architecture:** Documentation comments + one attribute + one variant on an existing enum + one unit test + issue-doc edits. No behaviour change, no new runtime path, no new subsystem. Drill-down itself is explicitly **out of scope for issue 007**.

**Tech Stack:** Rust edition 2024, Cargo workspace, serde, schemars. Existing dependencies only.

**No authoritative spec:** this bundle is preparation-only; the roadmap row (Bundle 13) is its contract. Source requirements: issue 007 §W-H (lines 571–636) and §F5 (lines 1262–1290).

## Current state (verified 2026-07-27)

Audited against the working tree — several premises in the 007 text (dated 2026-07-24) have drifted:

- **`ContextReference` lives at `crates/chat/src/assistant/understanding/intent.rs:191–199`**, four variants `None` / `PreviousJob` / `PendingClarification` / `SessionTopic`, deriving `Serialize, Deserialize, JsonSchema, Default` with `#[serde(rename_all = "snake_case")]`. Confirmed.
- **`PreviousJob` and `SessionTopic` are still never produced and never consumed.** A whole-crate grep (`grep -rn "PreviousJob\|SessionTopic" crates/`) returns *only* the two declaration lines in `intent.rs`. F5's audit holds exactly.
- **`PendingClarification` is the only variant ever constructed**, at `execution/runtime/clarification.rs:157` and `:293`. `context_reference` is otherwise copied through (`clarification.rs:274`) or set to `None` (`runtime/transition.rs:55`, `llm/router.rs:91`/`:193`, `presentation/builder.rs:392`). There is no `match` on the value anywhere, so even the written variant is never branched on. Confirmed.
- **DRIFT — the "W-C2 PayloadSource enum (from Bundle 12)" already exists and does NOT depend on Bundle 12.** The roadmap lists Bundle 13 as depending on Bundle 12 (W-C) to *introduce* `PayloadSource`. It is already in the tree at `crates/chat/src/assistant/understanding/extraction/mod.rs:78–84`, variants `UserText` / `LlmClaim` / `CatalogDefault`, `#[serde(rename_all = "snake_case")]`, re-exported through `assistant/mod.rs:72`. It is **not** `#[non_exhaustive]` and has **no** catch-all variant, so today an unknown source string fails deserialisation. This bundle therefore does not need Bundle 12 to land first — it operates on the enum that already exists.
- **No `gateway/`, `resolver.rs`, or `decider.rs` exists** (W-C is unstarted). So the "gateway must not silently accept a `context_reference`" item is a *recorded requirement for future work*, not a guard to build now — there is no gateway to guard, and nothing honours any non-`None` reference today.
- **`issue 008` (loan domain) already exists.** The next free issue number for the drill-down follow-up is **009**.
- Test wiring: `extraction/mod.rs:13–14` declares `#[cfg(test)] mod tests;` → `extraction/tests.rs` (`use super::*;`). New deserialisation test goes there. These are pure unit tests (no DB).

**Consequence for scope:** nothing in this bundle carries a prior result set forward; it only prevents future contract breaks and stops a dead-code sweep from deleting reserved surface. All four decisions in §W-H are already resolved in the issue text (re-execute over cache; follow-up is a resolver concern; `prior_job` = new `PayloadSource` variant only; **out of scope, deferred**). This plan does not reopen them.

## Global Constraints

- Keep exactly `crates/app`, `crates/core`, `crates/chat`. No new crate, dependency, migration, `knowledge/**/*.yaml`, or `queries/**/*.sql`.
- No behaviour change. Do not add a runtime branch on `context_reference`; do not build drill-down, a resolver, a gateway, or a prior-job lookup.
- Do not delete `ContextReference::PreviousJob` or `::SessionTopic`.
- Preserve every public signature, serde representation of existing variants, HTTP route, JSON envelope, and SSE event. Adding a new `PayloadSource` variant must not rename or reorder the existing three.
- English-only doc/copy. Sanitized errors unchanged.
- **Do not include commit steps.** A task is done when its listed checks exit `0` and `git diff --check` is clean. The user commits manually.

---

## Task 1: Record a green baseline

**Files:** Read only.

- [ ] **Step 1: Verify formatting and full-workspace compilation**

Run:
```bash
cargo fmt --check
cargo check
```
Expected: both exit `0`.

- [ ] **Step 2: Run the unit tests this bundle touches, plus the contract guards for `context_reference`**

Run:
```bash
cargo test -p chat --lib assistant::understanding::extraction
cargo test -p chat --lib assistant::understanding::intent
```
Expected: each passes (or reports "0 tests" for `intent` if it has none — that is fine, it must not error). If any is red, record the exact command and error before editing — do not start on an unexplained red baseline.

- [ ] **Step 3: Re-confirm the two variants are still dead (guards the reason for Task 2)**

Run:
```bash
grep -rn "PreviousJob\|SessionTopic" crates/
```
Expected: exactly two lines, both in `crates/chat/src/assistant/understanding/intent.rs`. If any *other* line appears, a variant has gained a live user — STOP and re-scope; the "reserved / never produced" premise no longer holds.

---

## Task 2: Mark `PreviousJob` and `SessionTopic` as reserved

`ContextReference::PreviousJob` and `::SessionTopic` are declared intent with no behaviour. Without a comment, a dead-code sweep (or a `#[warn(dead_code)]`-style cleanup) would legitimately delete them. Add a doc comment naming the follow-up issue so their intent survives.

**Files:**
- Modify: `crates/chat/src/assistant/understanding/intent.rs`

- [ ] **Step 1: Add the reserved-variant doc comment**

Replace the enum (currently `intent.rs:191–199`):
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContextReference {
    #[default]
    None,
    PreviousJob,
    PendingClarification,
    SessionTopic,
}
```
with:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContextReference {
    #[default]
    None,
    /// RESERVED for the drill-down follow-up (issue 009), not issue 007.
    /// Never produced or consumed today: no code carries a prior result set or
    /// capability forward, and nothing branches on this value. Do not delete —
    /// removing it would silently narrow the extractor's accepted contract, and
    /// the follow-up depends on it. See issue 007 §W-H / §F5.
    // ponytail: reserved surface, kept deliberately dead until issue 009 starts.
    PreviousJob,
    PendingClarification,
    /// RESERVED for the drill-down follow-up (issue 009), not issue 007.
    /// Never produced or consumed today. Do not delete — see `PreviousJob` above
    /// and issue 007 §W-H / §F5.
    // ponytail: reserved surface, kept deliberately dead until issue 009 starts.
    SessionTopic,
}
```

- [ ] **Step 2: Confirm no behaviour or formatting drift**

Run:
```bash
cargo fmt --check
cargo check -p chat
git diff --check
```
Expected: all exit `0`. The diff is comment-only; no variant added, removed, renamed, or reordered.

---

## Task 3: Make `PayloadSource` extensible (TDD: test unknown value first)

Goal: a future `prior_job` audit source (§W-H decision 3) must be addable without breaking (a) downstream exhaustive matches and (b) deserialisation of already-stored `chat_jobs.state_json` audit payloads written by a newer producer. Two mechanical changes: `#[non_exhaustive]` (protects the re-exported type against exhaustive matching in the `app` crate) and a `#[serde(other)]` catch-all variant (so an unknown source string deserialises to `Unknown` instead of erroring). Grep confirmed there is **no** `match` on `PayloadSource` anywhere in the crate today — only construction — so no existing match arm needs updating.

**Files:**
- Modify: `crates/chat/src/assistant/understanding/extraction/mod.rs`
- Modify: `crates/chat/src/assistant/understanding/extraction/tests.rs`

- [ ] **Step 1 (RED): Add the deserialisation test asserting an unknown source is accepted safely**

Append to `crates/chat/src/assistant/understanding/extraction/tests.rs`:
```rust
#[test]
fn payload_source_unknown_variant_deserialises_safely() {
    // An audit source string this build does not know (e.g. a future `prior_job`
    // written by a newer producer) must deserialise rather than error, so reading
    // an older build's state_json never fails on forward-compatible data.
    let parsed: PayloadSource =
        serde_json::from_str("\"prior_job\"").expect("unknown source must deserialise");
    assert_eq!(parsed, PayloadSource::Unknown);

    // A whole candidate carrying the unknown source also round-trips through serde.
    let candidate: PayloadCandidate = serde_json::from_value(serde_json::json!({
        "field": "limit",
        "value": 10,
        "source": "some_future_source",
        "trust": "trusted"
    }))
    .expect("candidate with unknown source must deserialise");
    assert_eq!(candidate.source, PayloadSource::Unknown);

    // The three known sources still map to their exact snake_case tags.
    assert_eq!(
        serde_json::from_str::<PayloadSource>("\"user_text\"").unwrap(),
        PayloadSource::UserText
    );
    assert_eq!(
        serde_json::from_str::<PayloadSource>("\"llm_claim\"").unwrap(),
        PayloadSource::LlmClaim
    );
    assert_eq!(
        serde_json::from_str::<PayloadSource>("\"catalog_default\"").unwrap(),
        PayloadSource::CatalogDefault
    );
}
```

Run:
```bash
cargo test -p chat --lib assistant::understanding::extraction::tests::payload_source_unknown_variant_deserialises_safely
```
Expected: **compile error / test failure** — `PayloadSource::Unknown` does not exist yet, and `"prior_job"` currently fails deserialisation with an "unknown variant" error. This is the RED state.

- [ ] **Step 2 (GREEN): Make the enum non-exhaustive with a catch-all variant**

Replace the enum (currently `extraction/mod.rs:78–84`):
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PayloadSource {
    UserText,
    LlmClaim,
    CatalogDefault,
}
```
with:
```rust
/// Provenance of a resolved payload field, recorded for the issue-006 audit trail.
///
/// `#[non_exhaustive]` + the `Unknown` catch-all keep this forward-compatible: the
/// drill-down follow-up (issue 009, §W-H decision 3) will add a `PriorJob` variant
/// without a contract break, and an unknown source string from a newer producer
/// deserialises to `Unknown` instead of failing. Do not reorder or rename the
/// known variants — their snake_case tags are the stored audit contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PayloadSource {
    UserText,
    LlmClaim,
    CatalogDefault,
    /// Any source tag this build does not recognise. Forward-compatibility only —
    /// never construct this deliberately; producers emit a specific known source.
    #[serde(other)]
    Unknown,
}
```

Run:
```bash
cargo test -p chat --lib assistant::understanding::extraction::tests::payload_source_unknown_variant_deserialises_safely
```
Expected: passes (GREEN).

- [ ] **Step 3: Confirm the whole crate still builds and nothing else broke**

Run:
```bash
cargo fmt --check
cargo check -p chat
cargo test -p chat --lib assistant::understanding::extraction
git diff --check
```
Expected: all exit `0`. `cargo check` must not surface a "non-exhaustive patterns" error anywhere — confirming no in-crate `match` on `PayloadSource` needed a wildcard arm (as the pre-audit predicted). If one does appear, add `_ => ...` / `PayloadSource::Unknown => ...` at that single site to restore green; do not change its behaviour.

---

## Task 4: Record the follow-up in issue 007 and stub issue 009

Satisfies W-H acceptance "the issue states plainly that drill-down is a follow-up, and names it" and the item "record that a follow-up issue should be created but is not part of 007", plus the recorded requirement that a future gateway must not silently accept a `context_reference` it cannot honour.

**Files:**
- Modify: `docs/issues/active/007-analyst-grade-knowledge-and-request-mapping.md`
- Create: `docs/issues/active/009-conversational-drill-down.md`

- [ ] **Step 1: Name the follow-up and record the gateway requirement in 007 §W-H**

In `007-...md`, at the end of the §W-H "Acceptance" list (currently lines 629–634, just before the `---` on line 636), append these bullets:
```markdown
- **Drill-down is a follow-up, not part of issue 007.** It is tracked as
  `docs/issues/active/009-conversational-drill-down.md`. It depends on W-C2
  (resolver precedence) landing first and is greenfield (F5): nothing carries a
  prior result set or capability forward today.
- **Recorded requirement for the future gateway (not built here):** when
  drill-down is implemented, the gateway must not silently accept a
  `context_reference` it cannot honour. Today `ContextReference::PreviousJob`
  deserialises and is copied through with no code branching on it (F5); the
  drill-down work must either honour a non-`None` reference or normalise it to
  `None`, never accept-and-ignore. This bundle only preserves the surface
  (`intent.rs` reserved-variant comments) and the extensible `PayloadSource`
  enum; it adds no runtime guard because no gateway exists yet and no reference
  is honoured today.
```
Preserve English-only copy and the surrounding heading structure.

- [ ] **Step 2: Create the drill-down follow-up issue stub (009)**

Write `docs/issues/active/009-conversational-drill-down.md`:
```markdown
# Issue 009 — Conversational drill-down (multi-turn follow-up)

**Status:** Not started. Deferred out of issue 007 by W-H decision 4.
**Depends on:** issue 007 W-C2 (resolver precedence) landing first.

## Why this is a separate issue

Issue 007's goal is that a complex analyst question is answered *in one turn*.
Drill-down ("dari list itu mana yang paling besar?", "yang di kantor Jakarta
saja") is a different product behaviour and is greenfield — 007 §F5 confirmed
against the code that nothing carries a prior result set or capability forward:
`ContextReference::PreviousJob` / `SessionTopic` are declared but never produced
or consumed, and no code branches on `context_reference`.

## Ground already prepared (by issue 007 Bundle 13)

- `ContextReference::PreviousJob` and `SessionTopic` are kept, commented as
  reserved for this issue (`assistant/understanding/intent.rs`).
- `PayloadSource` (`assistant/understanding/extraction/mod.rs`) is
  `#[non_exhaustive]` with a `#[serde(other)] Unknown` catch-all, so adding a
  `PriorJob` audit source is not a contract break and forward data deserialises.

## Must contain (from 007 §W-H)

- **Re-execute, do not cache** (decision 1): re-run the prior `capability_id`
  with its resolved parameters plus the new constraint, keyed on the business
  date so a rollover cannot serve a stale set. No second source of truth for
  "as of when".
- **Mechanics** (decision 2): a follow-up is the same capability with an extra
  bound parameter (office / age filter) or an ordering + limit. It is a resolver
  (W-C2) precedence concern, not a new subsystem. Requires a capability whose
  parameter set admits the filter — a W-A (catalog) input, not a runtime one.
- **Audit lineage** (decision 3): the refining job records the prior `job_id`,
  each inherited parameter with `source = prior_job` (new `PayloadSource`
  variant), and the new constraint with its own source. Recommendation: a new
  `PayloadSource` variant only — `execution.authorized` already carries resolved
  parameters.
- **Gateway contract:** must not silently accept a `context_reference` it cannot
  honour — honour it or normalise to `None`.
- Open question carried from 007: "When may a terminal job accept a further
  message?" — answer this before designing precedence.

## Invariants (inherited from 007, non-negotiable)

Approved-SQL only; office scope bound inside SQL via `office_ids = ANY($n)`,
never Rust post-filter; SQL only in repositories; PII field-level gating;
"today" = Fineract tenant business date; sanitized errors; PostgreSQL durable /
Redis live-only; same-job clarification via `POST /chat/jobs/{job_id}/responses`;
three crates; English-only copy.
```

- [ ] **Step 3: Confirm docs are well-formed and nothing else changed**

Run:
```bash
git diff --check
grep -rn "PreviousJob\|SessionTopic" crates/
```
Expected: `git diff --check` exits `0`; the grep still returns exactly the two `intent.rs` declaration lines (Task 2 added comments, not new usages).

---

## Task 5: Full-workspace green + final verification

**Files:** Read only.

- [ ] **Step 1: Whole-workspace build, format, and the touched unit tests**

Run:
```bash
cargo fmt --check
cargo check
cargo test -p chat --lib assistant::understanding::extraction
```
Expected: all exit `0`; the extraction test module includes
`payload_source_unknown_variant_deserialises_safely` passing.

- [ ] **Step 2: Confirm the diff is preparation-only**

Run:
```bash
git diff --stat
git diff --check
```
Expected: changed files are exactly
`crates/chat/src/assistant/understanding/intent.rs`,
`crates/chat/src/assistant/understanding/extraction/mod.rs`,
`crates/chat/src/assistant/understanding/extraction/tests.rs`,
`docs/issues/active/007-analyst-grade-knowledge-and-request-mapping.md`, and a
new `docs/issues/active/009-conversational-drill-down.md`. No runtime branch on
`context_reference` was added; no new crate, dependency, migration, or
knowledge/queries YAML was touched.

---

## Out of scope (do not build in this bundle)

- Any drill-down behaviour: prior-job lookup, resolver precedence for inherited
  parameters, a `PriorJob` `PayloadSource` value being *produced*, or carrying a
  result set forward. That is issue 009.
- Any runtime guard/normalisation of `context_reference`. Recorded as a
  requirement for the future gateway (Task 4); not implemented because no gateway
  exists and no non-`None` reference is honoured today. Adding a guard now would
  be code for a behaviour with zero effect.
- Touching W-C (`gateway/`, `resolver.rs`, `decider.rs`) — unstarted, Bundle 12.
