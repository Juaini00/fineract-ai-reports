# Dataset Coverage & Catalog Completeness — Implementation Plan (Issue 013)

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development or superpowers:executing-plans to implement task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Spec:** [`docs/superpowers/specs/2026-08-11-issue-013-dataset-coverage-design.md`](../specs/2026-08-11-issue-013-dataset-coverage-design.md)
**Issue (source of truth):** [`docs/issues/active/013-dataset-coverage-and-catalog-completeness.md`](../../issues/active/013-dataset-coverage-and-catalog-completeness.md)

**Goal:** Close the format loophole and the stale surface inside the approved
MVP scope (P0), then author the uncovered domains as scheduled, gated phases
(P1–P3), so `Unsupported` becomes a shrinking, recorded set — with no 012
invariant weakened.

**Tech stack:** Rust 2024, `crates/chat`; `serde_yaml` knowledge YAML under
`knowledge/`, SQL fragments under `queries/`. Tests: `cargo test -p chat`.

## Global constraints

- Workspace locked to `app`/`core`/`chat` — do not add a crate. (`CLAUDE.md`)
- No `sqlx` in handlers/services — repositories only. (`CLAUDE.md`)
- Every executable SQL character originates in a file or a declared `expr`; the
  LLM contributes only ids/values. (spec §Non-goals)
- Office scope enforced inside bound SQL via `office_ids` — never a post-fetch
  Rust filter. (`AGENTS.md` / 012)
- Schema changes only via `migrations/*.sql`; **do not modify Fineract schema**.
  This work touches Fineract only through the read-only replica in SELECTs.
- Every `§4`/`§6` column re-verified against `fineract_local_default` (via the
  `postgresql` MCP) before authoring. Row counts from `fineract_default`.
- Pre-commit: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` must pass.
- Ponytail: smallest correct change; reuse `filters_exempt`, the existing 011
  check, and existing shapes before authoring anything new.
- Follow the memory note **"Verify the path, not the artifact"**: a dataset is
  not done until its own example runs end-to-end, not merely because YAML loads
  and SQL PREPAREs.

---

## PHASE P0 — in-scope, no domain approval needed

### Task 1 — D1: id-filter enforcement in `validate_dataset`

Extend the 011 completeness rule (`validate.rs:186–206`) to stable-id `bigint`
columns, symmetric with the string rule.

**Files:** Modify `crates/chat/src/knowledge/dataset/validate.rs`.

- [ ] **Step 1 (RED):** add a unit test: a shape returning a `bigint` FK output
  (`office_id`) with no filter and not in `filters_exempt` must fail validation
  with the "declare a filter or list it under filters_exempt" message. Run
  `cargo test -p chat validate` → it FAILS.
- [ ] **Step 2 (GREEN):** in the shape loop, after the existing string check, add:
  a `bigint` output field whose name matches the id shape (`*_id` or the entity
  `id_field`) and is neither a filter slot nor in `filters_exempt` → `Err`. Reuse
  the existing message text. Run the test → PASSES.
- [ ] **Step 3:** run the full `validate.rs` suite + load all 10 current datasets.
  Any current dataset that now fails is either fixed in Task 3/4 or gets a
  justified `filters_exempt` entry (display-only id). Record each exemption.

**Validation:** `cargo test -p chat validate` green; all 10 datasets still load
(catalog load test green).

### Task 2 — D2: delete vestigial junk (YAML only)

**Files:** `knowledge/datasets/savings/{account_activity,account_charges,accounts}.yaml`.

- [ ] Remove dataset-level empty `order_by: []`, `output_fields: []`,
  `parameters: []` from the three datasets. No code change.

**Validation:** catalog load test green (serde defaults cover the absent keys).

### Task 3 — §3: wire the stale surface (decisions locked)

All five shapes are **wired** — each 013 §3 row names an in-scope question, so
none is retired. Criterion applied: a shape is retired only if **no named
in-scope question** consumes it; all five have one, so all five wire. Author the
small consuming capability where it does not exist yet (in-scope, not speculative
— the question is named in §3/§5).

**Files:** capability YAML under `knowledge/**`; `knowledge/datasets/**` for the
`probe:` attachments. No fragment is deleted (nothing is retired).

- [ ] `client.portfolio_counts/counts_by_client` → wire to a new
  `clients_with_account_counts` capability (per-client savings-account count).
- [ ] `organization.offices/office_candidates` → wire as an office-name `probe:`
  (office-name → `office_id` resolution; feeds the §4 `office_id` filters).
- [ ] `savings.products/products_by_client` → wire as a per-client product `probe:`.
- [ ] `savings.transactions/activity_rows` → wire as the activity source of the
  §5 general activity path (Task 5 consumes it).
- [ ] `savings.charge_definitions/charge_type_candidates` → wire as the A3
  charge-type `probe:` (author the A3 consuming capability if absent).
- [ ] Record each wiring (shape → consuming capability id) in the issue §3 table.

**Validation:** no orphan fragment remains (grep each `queries/**` fragment for a
consuming shape); Task 6's `unconsumed_shape` lint green after this.

### Task 4 — §4: DB-verified id + `IN` filters on live savings shapes

Re-verify columns via the `postgresql` MCP, then add filters.

**Files:** `knowledge/datasets/savings/{accounts,account_charges}.yaml` (+ their
fragments under `queries/**`).

- [ ] `savings.accounts`: add `[eq,in]` filters `savings_product_id`, `group_id`,
  `staff_id` (direct columns); add `office_id` as `mc.office_id` via the existing
  client join (no direct `office_id` on `m_savings_account`).
- [ ] `savings.account_charges`: add `[eq,in]` `savings_account_id`/`client_id`;
  `office_id` via client join; add the missing bounded `IN` (needs `row_cap`).
- [ ] Each `in` filter has a non-zero `row_cap` on every shape (validator rule).

**Validation:** `cargo test -p chat`; compose + PREPARE each affected shape;
then run one example question that narrows by `office_id`/`product_id` end-to-end
(verify-the-path).

### Task 5 — §5: general savings-activity path (replace fixture contract)

**Files:** `knowledge/datasets/savings/account_activity.yaml` (retire the
`account_match` fingerprint); capability/probe YAML wiring the two-stage path.

- [ ] Retire `search + product_name + latest_transaction_amount`; keep
  `latest_transaction_amount` only as optional disambiguator.
- [ ] Wire `savings.accounts/accounts_by_client` as a `probe:` so a
  `CardinalityBranch` picks 0/1/many.
- [ ] Route activity through `savings.transactions/activity_rows` (wired in Task 3)
  as the single activity source. Do **not** also add `savings.activity_list` — one
  path only, to keep the executed surface minimal.

**Validation:** **012 scenario A1** (client → account resolution) runs end-to-end
from a sentence (DoD #5). This is the acceptance gate for Task 5.

### Task 6 — D3: two catalog lints (land after Task 3)

**Files:** the catalog loader/validator module (same CI path as
`validate_dataset`); a test module alongside it.

- [ ] `unconsumed_shape` — **fails** on any dataset shape referenced by zero
  capabilities. Test with a fixture catalog.
- [ ] `unwired_resolver` — **warns** on any resolver/probe not attached to a
  `probe:`. Test the warning is emitted, not fatal.
- [ ] Wire both into CI. Confirm green **only after** Tasks 3/5 removed the stale
  shapes — order matters (spec §Risks).

**Validation:** `cargo test -p chat`; CI green.

### Task 7 — §6.4: surface the deposit sub-domain (in-scope)

**Files:** new `knowledge/datasets/savings/deposits.yaml` + fragment(s); ≥1
capability YAML. Columns per 013 §6.4, re-verified.

- [ ] Author `savings.deposits` (FD/RD; joins term-and-preclosure, recurring
  detail, mandatory schedule). Filters `savings_account_id`, `client_id`,
  `deposit_type_enum`. Passes D1/D3.

**Validation:** `cargo test -p chat`; one maturity/recurring question end-to-end.

### Task 8 — §8: fix the docs contradiction

**Files:** `docs/product/reporting-data-scope/06-5-explicitly-out-of-scope-permanent.md`.

- [ ] Change Loan/GL/Tax/custom-datatables from "permanent … never be built" to
  **`deferred (scheduled)`**, matching the machine-readable sources.

**P0 exit checkpoint:** DoD #1, #2, #3, #5, #6 satisfied; #8 unaffected. Request
code review (superpowers:requesting-code-review) before starting P1.

---

### The §7 gate is code-enforced — every P1–P3 activation clears both points

The gate is not paper policy (spec §§6.1–6.6/§7 "Where the gate is actually
enforced"). A dataset only becomes executable after:
- **Domain layer** — `lqr.rs:84` (`decide_domain_layer`) rejects a top domain
  whose status is `deferred|rejected` with `off_domain_<id>`. Flipping the domain
  YAML to `approved_mvp` is what clears it.
- **Capability filter** — every planning/data path filters
  `status == "approved_mvp"` (`planning.rs:135/167/246`, `tool/data.rs:105`,
  `tool/metadata.rs`, `pipeline.rs:92`). No `approved_mvp` capability = nothing runs.

**`candidate` caveat (group):** `lqr.rs:84` rejects only `deferred|rejected`, **not**
`candidate`. `group_center` is `candidate`/`conditional`, so the domain layer will
not stop it — group stays unexecutable solely because no `approved_mvp` capability
exists. Do not assume group is hard-gated by domain status.

**PII sign-off is a recorded, machine-checkable artifact (format locked).** Each
activating domain adds a **`pii_signoff:` block in its domain YAML** (spec §7 —
`.md` was rejected because it cannot be linted) with `signed_by`, `signed_on`, and
a `fields:` list where every output field of every shape has a `class` (PII-policy
vocabulary) and a `decision` (`allow`/`masked_when_no_pii`/`never_return`). No
field ships without a class + decision; empty `signed_by` ⇒ gate not passed.
`secret_never_expose` fields (payment account/check/receipt/bank/routing numbers)
stay excluded unless the capability explicitly approves **masked** display;
passwords/tokens/command JSON are never exposable regardless of activation.

**The single manual step.** Everything else in P1–P3 is agent-executable.
`signed_by`/`signed_on` and the per-field `decision` values are the **only**
human input — the agent authors the field list and proposed classes, then stops
for a human to review, set decisions, and sign. This is a policy boundary, not an
under-specification: it cannot be auto-filled.

## PHASE P1 — LOAN (§6.1, deferred → issue 008)

Highest value. Author `loan.{accounts,arrears,transactions,repayment_schedule,
products,charges}` per 013 §6.1, each behind the §7 gate.

- [ ] §7 gate for `loans`: flip `knowledge/domains/loan.yaml` `deferred→approved_mvp`;
  flip `knowledge/data-scope/areas/loans.yaml` `deferred→in_use`; add the
  `pii_signoff:` block to `knowledge/domains/loan.yaml` (loan rows carry identity +
  financial PII — every output field gets a `class` + human `decision`); ≥1 runnable
  `approved_mvp` capability per shape.
- [ ] Author datasets; each passes D1/D3 and the office-scope invariant.

**Validation:** per-dataset example question end-to-end; PII masking verified on
`pii`/`sensitive_business_identifier` output fields per the recorded sign-off.

## PHASE P2 — ACCOUNTING/GL (§6.2) + PAYMENTS/TRANSFERS (§6.3)

- [ ] GL: author `accounting.{gl_accounts,journal_entries}` behind the §7 gate —
  flip `knowledge/domains/accounting.yaml` + `knowledge/data-scope/areas/accounting-gl.yaml`;
  record GL PII sign-off. `journal_entries` = 21846 rows, the largest table —
  confirm budget/timeout.
- [ ] Payments: **no domain YAML and no area exist** — create
  `knowledge/domains/payments.yaml` and `knowledge/data-scope/areas/payments.yaml`
  from scratch (both `approved_mvp`/`in_use`) with the field boundary locked in
  spec §7: in-area returnable = `payment_type`, `amount`, dates, transfer-linkage
  ids (`from/to_savings_account_id`, `sensitive_business_identifier`); out-of-area
  unless masked-display approved = `m_payment_detail.{account_number,check_number,`
  `receipt_number,bank_number,routing_code}` (`secret_never_expose`). Then author
  `payments.{account_transfers,payment_details}`.

**Validation:** trial-balance / GL-activity and cross-account transfer questions
end-to-end; every payment reference field masked or excluded per the recorded sign-off.

## PHASE P3 — GROUP (§6.5) + TAX (§6.6)

- [ ] `group.groups` behind the §7 gate: promote `knowledge/domains/group_center.yaml`
  `candidate→approved_mvp`; enable the area by flipping
  `knowledge/data-scope/areas/group-center-foundation.yaml` `status: conditional→in_use`
  (the YAML flip **is** the per-deployment control — no env/flag machinery, spec §7);
  record group `pii_signoff:`. Note: `lqr.rs:84` does **not** reject `candidate`, so
  group's gate rests on the `approved_mvp` capability + area flip, not the
  domain-status reject.
- [ ] `tax.withholding` (thin, low volume) behind the §7 gate — flip
  `knowledge/domains/tax.yaml` + `knowledge/data-scope/areas/tax.yaml`; record sign-off.
- [ ] Skip `share` (0 rows).

**Cross-cutting after P3:** write the deferred-domain **activation order**
(loan → GL → payments → group/tax), each referencing the §7 gate, so DoD #7 is met.

---

## Validation commands (reference)

```bash
cargo test -p chat validate                 # D1 unit tests
cargo test -p chat                           # chat crate (catalog load, compose, lints)
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
# schema re-verify (per §4/§6 authoring): postgresql MCP describe_table on fineract_local_default
```

Per the memory note **"No full test suite runs"**, run the targeted `-p chat`
targets above, not the whole workspace sweep.

## Review checkpoints

1. After **Task 1 (D1)** — validator semantics reviewed before it gates authoring.
2. **P0 exit** — full P0 code review before any deferred-domain work.
3. Before each **§7 gate flip** (P1/P2/P3) — the domain YAML's `pii_signoff:` block
   covers every output field with a `class` + `decision`, `signed_by` is non-empty,
   and a human set the decisions. An agent may author the dataset and the proposed
   field/class list but must **not** flip the domain status, set `signed_by`, or
   choose the per-field `decision` values.

## Status ledger update

On completion, add a row to `docs/superpowers/README.md`:
`| 2026-08-11 | issue 013 dataset coverage & catalog completeness | ✓ | ✓ | <status> | <date> |`
and update `docs/current/status.md`; flip issue 013 toward resolved as phases land.
