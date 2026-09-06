# Issue 007 Bundle 8 Catalog Retrieval Remediation Implementation Plan

> **REQUIRED SUB-SKILL:** Use `superpowers:test-driven-development` for each behavioural change.

**Goal:** Close Bundle 7's bounded catalog retrieval ledger without weakening classification thresholds, and make both savings-charge inventory gaps truthful and executable.

**Architecture:** Catalog text remains the primary remediation surface. The fallback scorer now normalizes lexical coverage rather than saturating raw hit counts, while `classification.min_floor/min_gap` remain unchanged. One new approved capability handles the separately-filtered strict-overdue workflow; existing metadata handles the levied-total field.

**Tech Stack:** Rust 2024 workspace, existing YAML catalog loader/validator, approved PostgreSQL SQL.

---

### Task 1: Turn B7's ledger into red acceptance tests

**Files:**
- Modify: `crates/chat/tests/retrieval_scoring.rs`
- Modify: `crates/chat/tests/catalog_validation.rs`

1. Move G1/G2 to the covered bilingual fixture; retarget G2 to `savings_strictly_overdue_charges_clients`.
2. Remove the 28 expected-gap allowance so each covered phrase requires rank one, floor `0.40`, and gap `0.05`.
3. Add a catalog test requiring the strict-overdue capability, query, `as_of_date`, and `days_overdue` contract.
4. Run the two focused tests and record the expected failures before changing catalog data.

### Task 2: Add the minimal strict-overdue approved capability

**Files:**
- Create: `queries/savings/strictly_overdue_charges_clients.sql`
- Create: `knowledge/queries/savings/strictly_overdue_charges_clients.yaml`
- Create: `knowledge/capabilities/savings/strictly_overdue_charges_clients.yaml`

1. Copy the approved pending-charge output ordering and PII classifications.
2. Bind office scope with `ANY($1::bigint[])`; use `$2::date` only as the strict `charge_due_date <` threshold and `$3` for the bound limit.
3. Reuse `savings.charge_amount_outstanding`; declare truthful English and Indonesian examples/intents, no unverified enum label.
4. Run the new catalog test and `cargo test -p chat --test catalog_validation`.

### Task 3: Normalize lexical coverage, enrich target metadata, and resolve duplicate vocabulary

**Files:**
- Modify: target YAMLs named by Bundle 7's 28 rows under `knowledge/capabilities/{savings,client,organization}/`

1. Replace raw lexical hit accumulation in `catalog_fallback` with bounded request-term coverage so broad three-term overlap cannot erase the rank/gap signal.
2. Add concise terminology for unpaid/pending/overdue charges, deposit vs withdrawal, monthly breakdown vs monthly top value, lifecycle activations, account-count vs balance rankings, office hierarchy/activity/dormancy/opening/savings/staff, and Indonesian equivalents.
3. Make the lifecycle-distribution and organization-population distinction explicit in the two client-count capabilities; do not change their queries or request shapes.
4. Do not add phrase-specific production branches; catalog examples remain stable capability vocabulary.
5. Run the focused retrieval suite until all 72 phrases pass at the unchanged thresholds.

### Task 4: Document the audited result

**Files:**
- Modify: `docs/product/analyst-question-inventory.md`
- Modify: `docs/current/status.md`
- Modify: `docs/issues/active/007-analyst-grade-knowledge-and-request-mapping.md`
- Modify: `docs/superpowers/README.md`
- Modify: `docs/superpowers/plans/2026-07-27-issue-007-program-roadmap.md`
- Modify: this plan

1. Record G1 as covered by the existing field and G2 as covered by the new strict-overdue capability.
2. Preserve the historical 28-row ledger, annotate its remediation and completion rather than deleting evidence.
3. Mark B8 implemented and point the B9 gate at the final 31-capability catalog.

### Task 5: Verify and commit

Run:
```bash
cargo fmt --check
cargo check -p chat
cargo test -p chat --test retrieval_scoring
cargo test -p chat --test catalog_validation
cargo test -p chat
cargo test -p core
cargo clippy --workspace --all-targets -- -D warnings
git diff --check
```

Run the runtime catalog check only when a compatible Fineract database is configured; otherwise record the exact skip. Commit only Bundle 8 files and never add `1necho` or `docs/product/landing-page-invideo-prompt.txt`.

**Status:** implementation complete; verification, review, and the isolated Bundle 8 commit remain pending.
