# Issue 007 — Bundle 7: Bilingual Retrieval Regression Implementation Plan

> **REQUIRED SUB-SKILL:** Use `superpowers:test-driven-development` per task: add the assertion, run it red, then make the smallest test-suite-only change and run it green.

**Goal:** Turn every row in the implemented analyst inventory into deterministic Indonesian and English retrieval evidence, while keeping product gaps explicit for Bundle 8 rather than weakening retrieval thresholds.

**Architecture:** Extend the existing real-catalog integration target, `crates/chat/tests/retrieval_scoring.rs`, with a compact in-code inventory transcription. Each covered row evaluates the deterministic catalog fallback with the target capability's declared request shape, then requires the target to be rank one and to clear the unchanged `classification.min_floor` and `classification.min_gap`. Partial and missing rows are separate, explicit known-gap assertions: partial rows remain in the Bundle 8 ledger without pretending their absent fields are retrievable, and missing loan rows prove their reserved id is absent. A deliberately out-of-catalog request uses a capability-restricted plan and the standard reranker fallback to prove `Unsupported` has no offered options.

**Tech Stack:** Rust 2024, existing `chat` retrieval engine/reranker/catalog loader, existing YAML catalog; no production code, new dependencies, migrations, SQL, or YAML changes.

**Status:** implemented 2026-07-28 as the D1 regression/ledger bundle. W-D2 remains pending Bundle 8: 28 covered-phrase scoring gaps, including English E1 pending charges, must be fixed in catalog metadata without changing the floor or gap.

**Inputs audited before execution:**
- `docs/product/analyst-question-inventory.md` (29 covered, 2 partial, 5 missing rows)
- `crates/chat/tests/retrieval_scoring.rs` and `crates/chat/src/assistant/retrieval/{engine,reranker}.rs`
- `knowledge/policies/classification.yaml` (unchanged `min_floor: 0.40`, `min_gap: 0.05`)

## Constraints

- Keep approved SQL, in-SQL office scope, PII policy, audit semantics, English-only product copy, and exactly three crates untouched.
- Exercise the loaded approved catalog only; do not encode fixture-specific production behavior or lower any retrieval/classification threshold.
- The fixture is a direct transcription of analyst-inventory phrasing and expected capability ids. It is test data, not a second catalog.
- Partial/missing are assertions about known coverage gaps, not ignored/xfail passing cases. Bundle 8 consumes their documented ledger.

## Task 1: Baseline and fixture design

**Files:** read-only, then `crates/chat/tests/retrieval_scoring.rs`.

- [x] Confirmed B3/B4 inputs exist and catalog has 30 approved capabilities / 30 approved queries.
- [x] Confirmed deterministic scoring is `catalog_fallback` plus declared request-shape boost; the active policy floor/gap remain `0.40` / `0.05`.
- [x] Chose an in-code fixture (not a second YAML): 72 bilingual phrasing rows are short, directly reviewed beside the retrieval API, and need no runtime loader surface.

## Task 2: RED — add covered-row rank/threshold assertions

**Files:**
- Modify: `crates/chat/tests/retrieval_scoring.rs`

- [x] Add the covered inventory fixture and evaluator that loads the real catalog and derives each target's declared request shape.
- [x] Add rank-one, floor, and classification-gap checks for both phrases of every covered row.
- [x] Run `cargo test -p chat --test retrieval_scoring bilingual_covered_inventory_rows_rank_first_and_clear_policy_thresholds -- --nocapture`; record its initial expected failures as the Bundle 8 scoring ledger.

## Task 3: GREEN — preserve the observed regression contract

**Files:**
- Modify: `crates/chat/tests/retrieval_scoring.rs`

- [x] Record every covered phrase whose real catalog candidate is not rank one or does not clear the existing policy floor/gap in the fixture’s explicit scoring-gap list, including observed top id/score relation.
- [x] Assert non-gap covered phrases satisfy rank-one/floor/gap and scoring-gap phrases retain their exact remediation target/observed condition, so catalog improvements deliberately fail this list and force Bundle 8 to update it.
- [x] Re-run the targeted covered suite green without changing production scorer/policy/catalog data.

## Task 4: Known gaps and unsupported contract

**Files:**
- Modify: `crates/chat/tests/retrieval_scoring.rs`

- [x] Assert all four bilingual partial phrases retrieve their existing target as a candidate but are not mislabelled rank-one support.
- [x] Drive all ten bilingual loan phrases through restricted retrieval and assert each reserved capability id remains absent, produces `Unsupported`, and offers no alternatives.
- [x] Add the deliberately out-of-catalog request check: restricted retrieval is empty, reranking returns `Unsupported`, and its alternatives/options list is empty.
- [x] Add the E1 assertion: Indonesian pending charges passes; the audited English phrase is an explicit scoring-ledger row for Bundle 8, so it cannot be silently treated as supported.

## Task 5: Verify and document the result

**Files:**
- Modify: `docs/product/analyst-question-inventory.md`
- Modify: `docs/superpowers/README.md`
- Modify: `docs/superpowers/plans/2026-07-27-issue-007-program-roadmap.md`
- Modify: `docs/current/status.md`
- Modify: `docs/issues/active/007-analyst-grade-knowledge-and-request-mapping.md`
- Modify: this plan

- [x] Run Bundle 7’s focused target and the relevant catalog/retrieval/chat workspace checks.
- [x] Record the exact scoring-gap ledger as Bundle 8 input, separately from the inventory’s semantic partial/missing rows.
- [x] Mark Bundle 7 implemented in the plan, docs ledger, roadmap, status, and Issue 007.
- [x] Run formatting, workspace compilation, and diff validation; document DB/live checks that cleanly skip because their environment is unavailable.

## Acceptance checklist

- [x] Every 36 inventory question has Indonesian and English fixture coverage (72 rows): 58 covered, 4 partial, 10 missing.
- [x] Covered cases enforce the active rank/floor/gap contract or are exhaustively named as real current-catalog scoring gaps for Bundle 8; thresholds are not weakened.
- [x] Partial and missing rows are explicit, actionable Bundle 8 inputs rather than false rank-one passes.
- [ ] E1 maps to `savings_pending_charges_clients` at rank one in both languages — **blocked on Bundle 8**: English currently ties/wrong-ranks behind `client_top_n_by_deposit_volume`.
- [x] A deliberately out-of-catalog request is `Unsupported` with an empty offered-options list.
- [x] No production retrieval, security, data-scope, SQL, PII, or response-copy behavior changes.
