# Dataset Coverage & Catalog Completeness (Issue 013)

Date: 2026-08-11
Status: Design draft, ready for planning
Scope: The 10 authored datasets + catalog lints + specs for every uncovered domain
Source of truth: [`docs/issues/active/013-dataset-coverage-and-catalog-completeness.md`](../../issues/active/013-dataset-coverage-and-catalog-completeness.md)

> This spec does not restate the issue's per-domain dataset specs (013 §6) or the
> gap matrix (013 §9) — those stay authoritative in the issue. This spec records
> the **engineering design** for the format/lint/filter/wiring work (013 §2–§5,
> §10 P0) and the **execution contract** for activating deferred domains (§7).

## Problem

Three defect classes, all verified against the live Fineract DB and the catalog
source (013 §0):

1. **Format has a completeness loophole.** All 10 datasets pass `validate_dataset`,
   but the 011 "must be filterable" rule only fires for narrowable **string**
   columns (`validate.rs:186–206`). A returned/joinable **stable-id `bigint`**
   column (`office_id`, `product_id`, `savings_account_id`) validates with **no
   filter slot** — the structural reason "filter by name, not by id" survives.
   Plus vestigial `order_by: []`/`output_fields: []`/`parameters: []` junk (D2)
   and no lint for shapes/resolvers nothing consumes (D3).
2. **Stale / unwired surface.** 5 of 10 datasets have zero consuming capability;
   5 of 6 resolver/probe shapes are dead; 6 fragment SQL files back nothing.
   Only one probe is live today.
3. **Whole domains uncovered.** Loan, Accounting/GL, payments/transfers, the
   savings deposit sub-domain, group/center, tax — none have a dataset. Loan/GL/
   tax are `deferred` by a locked MVP decision; deposits are **in-scope but
   unsurfaced**; payments has no data area yet.

## Goal (owner's bar)

No dataset may be non-conforming to format, stale, or left incomplete because of
the MVP boundary. `Unsupported` must become a **shrinking, scheduled** set, never
a silent permanent ceiling. Concretely: id-narrowing is expressible on the
executed path; every shape is consumed or retired-on-record; every uncovered
domain has an authored dataset (in-scope) or a complete spec + recorded
activation-gate status (deferred).

## Non-goals

- **No arbitrary AI-generated SQL.** The LLM still contributes only ids/values.
  Every executable SQL character originates in a file or a declared `expr`.
- **No weakening of any 012 invariant.** Security, PII masking, office-scope
  enforcement inside bound SQL, and query budget/timeout stay exactly as they are.
  Office scope stays enforced through the bound `office_ids` parameter — never a
  post-fetch Rust filter.
- **No governance bypass.** Authoring a deferred domain's dataset does **not**
  activate it. The §7 gate (domain flip + area flip + per-field PII sign-off +
  ≥1 runnable capability) is mandatory and recorded, not hidden.
- **Not widening answerability except along declared axes.** New sources still
  need authored datasets; this closes gaps, it does not open free-form querying.
- **Do not build `share`** (`m_share_account`/`m_share_product` = 0 rows).

## Design

### D1 — id-filter enforcement (symmetric with the 011 string rule)

Extend `validate_dataset` so the completeness rule at `validate.rs:186–206` also
covers **stable-id columns**. A shape output field that is an id-class `bigint`
(FK or entity id) must be a declared filter slot **or** listed in
`filters_exempt`, exactly as string columns already are.

- **Which columns.** An output field whose `kind == "bigint"` and whose name
  matches the id shape (`*_id`, or the entity `id_field`). Aggregate-only outputs
  (counts, sums) are `integer`/`decimal`, not `bigint`, so summary datasets like
  `organization.office_summary` are unaffected — verified against `OUTPUT_TYPES`.
- **Escape hatch.** `filters_exempt` already exists and is validated
  (`validate.rs:288`); id columns that are genuinely display-only (e.g. an entity
  `id_field` surfaced for labelling but never a narrowing axis) list themselves
  there with the same discipline strings use.
- **Error message.** Mirror the existing string message so authors get one
  consistent instruction ("declare a filter or list it under filters_exempt").
- **Test-first.** A failing unit test in `validate.rs` for a bigint FK output
  with no filter, before touching the check — keeps D1 symmetric and provable.

### D2 — delete vestigial junk

Remove dataset-level empty `order_by: []`/`output_fields: []`/`parameters: []`
from `savings/account_activity`, `savings/account_charges`, `savings/accounts`.
These are serde-default-valid but misleading. No code change — YAML only.

### D3 — two catalog lints

A catalog-level check (runs where the catalog is loaded/validated, wired into the
same CI path as `validate_dataset`), not per-dataset:

- **`unconsumed_shape` — fails.** Any dataset shape referenced by **zero**
  capabilities is an error. This is what silently allowed the 5 stale datasets.
- **`unwired_resolver` — warns.** Any resolver/probe shape not attached to a
  `probe:` is a warning (a resolver may legitimately land a step ahead of its
  consumer). Warnings are recorded, not fatal.

Both lints read the already-loaded capability + dataset sets; no new data source.

### §4 — id + `IN` filters on the live savings shapes (DB-verified)

Add the DB-verified filters from 013 §4 to the four live savings datasets.
Columns confirmed in `fineract_local_default`:

- `savings.accounts`: add `savings_product_id`, `group_id`, `staff_id` `[eq,in]`
  (direct columns); `office_id` as `mc.office_id` **via the existing client join**
  (`m_savings_account` has no direct `office_id`).
- `savings.account_charges`: add `savings_account_id`/`client_id` `[eq,in]`;
  `office_id` via the client join. Add bounded `IN` (currently missing).
- `office_id` is absent as a filter on **every** savings dataset today; this makes
  office-narrowing expressible inside bound SQL on the executed path.

### §3 — wire the stale surface (all five, decisions locked)

Every 013 §3 row names an in-scope question, so **all five shapes are wired**,
none retired. Retirement criterion (recorded for future shapes): retire only when
**no named in-scope question** consumes a shape. Locked mapping:
`counts_by_client → clients_with_account_counts` (new capability);
`office_candidates → office-name probe` (feeds §4 `office_id` filters);
`products_by_client → per-client product probe`;
`activity_rows → §5 activity source`;
`charge_type_candidates → A3 charge-type probe`. Author the consuming capability
where absent (in-scope, not speculative — the question is named). No
authored-but-unreachable fragment may remain — the D3 lint enforces this.

### §5 — general savings-activity path (replace the fixture contract)

Retire `savings.account_activity/account_match`'s `search + product_name +
latest_transaction_amount` fingerprint (the fixture-specific shape 012 #9 flagged
for removal). Replace with the two-stage resolution 012 scenario A1 needs, using
shapes that already exist:
1. `client.identity/identity_candidates` (live) → resolve client.
2. `savings.accounts/accounts_by_client` (wire as `probe:`) → `CardinalityBranch`
   picks 0/1/many.
3. `savings.transactions/activity_rows` (wired in §3) → the activity. This is the
   single activity source; `savings.activity_list` is **not** added (one path only).
`latest_transaction_amount` stays only as an optional disambiguator.

### §6.4 — surface the deposit sub-domain (in-scope, no approval needed)

Author `savings.deposits` per 013 §6.4 — it is `included`, only unsurfaced.

### §6.1–§6.6 + §7 — deferred / new-area domains (gated)

Loan (§6.1), GL (§6.2), payments (§6.3, needs a new data area), group (§6.5),
tax (§6.6) are fully spec'd in the issue. Authoring a dataset is a scheduled step,
**not** the activation — activation is the §7 gate below, and it is a distinct,
human-signed step.

#### Where the gate is actually enforced (not just governance)

The gate is not a paper policy; it is load-bearing in code today. Any new dataset
must clear both enforcement points before a question can execute:

- **Domain layer** — `decide_domain_layer` (`crates/chat/src/assistant/understanding/lqr.rs:84`)
  returns `Reject { off_domain_<id> }` when the top-ranked domain's status is
  `deferred` or `rejected`. This is what makes loan/GL/tax questions
  `Unsupported` honestly today.
- **Capability filter** — every planning/data path filters
  `capability.status == "approved_mvp"` (`llm/agent/planning.rs:135/167/246`,
  `llm/tool/data.rs:105`, `llm/tool/metadata.rs`, `understanding/pipeline.rs:92`).
  A domain with no `approved_mvp` capability has nothing to execute regardless of
  its domain status.

**Consequence for `candidate` domains (group).** `lqr.rs:84` rejects only
`deferred|rejected` — it does **not** reject `candidate`. `group_center` is
`status: candidate` (domain) / `conditional` (area), so the domain layer will
**not** stop it; the *only* thing keeping group unexecutable is the absence of an
`approved_mvp` capability. Group's gate therefore rests entirely on the capability
filter + area enablement, not on the domain-status reject. This must be stated
explicitly so group is never assumed hard-gated the way loan/GL/tax are.

#### Per-domain gate mechanics (they are not uniform)

| Domain | Domain YAML today | Area today | Gate action |
| --- | --- | --- | --- |
| Loan | `deferred` | `loans: deferred` | flip domain `deferred→approved_mvp`; flip area `deferred→in_use` |
| Accounting/GL | `deferred` | `accounting_gl: deferred` | same flip pair |
| Tax | `deferred` | `tax: deferred` | same flip pair |
| Group | **`candidate`** | `group_center_foundation: conditional` | promote domain `candidate→approved_mvp`; enable `conditional` area **per-deployment**; no domain-status reject applies (above) |
| Payments | **none exists** | **none exists** | **create** `knowledge/domains/payments.yaml` and a new `knowledge/data-scope/areas/payments.yaml` from scratch, both `approved_mvp`/`in_use` |

Every gate action, for all five, additionally requires steps 3 and 4:
3. **per-output-field PII sign-off**, recorded (below);
4. ≥1 runnable `approved_mvp` capability YAML per shape.

**Group "per-deployment" is the YAML flip itself — no new runtime config.**
`conditional` is activated exactly like a `deferred` flip: set
`knowledge/data-scope/areas/group-center-foundation.yaml` `status: conditional →
in_use`. A deployment that wants group ships the flipped YAML; one that does not
ships it unflipped. There is **no** env var / feature-flag machinery to build —
the area-YAML status *is* the per-deployment control. This keeps the gate to
one enforceable surface (area status + capability set), consistent with
loan/GL/tax.

**Payments area is authored from scratch with an explicit field boundary.**
`knowledge/data-scope/areas/payments.yaml` (new, `status: in_use` on activation)
scopes `m_payment_detail` / `m_account_transfer`. In-area, returnable fields:
`payment_type`, `amount`, transaction/transfer dates, and the account-transfer
linkage ids (`from_savings_account_id`, `to_savings_account_id`) — the last two
are `sensitive_business_identifier`, returned only through explicit capability
approval. **Out of area unless a capability explicitly approves masked display:**
`m_payment_detail.{account_number,check_number,receipt_number,bank_number,`
`routing_code}` (`secret_never_expose`, PII policy §3). No `share` area is created.

#### PII sign-off must be a recorded artifact, not a verbal approval

Today the gate names "per-output-field PII sign-off" but no location exists for
it. **Decision (locked): a `pii_signoff:` block in the domain YAML**, not a
separate `.md` — a YAML block is machine-checkable, so the D3 lint family can
later assert "domain is `approved_mvp` ⇒ every output field has a signed class"
(a `.md` file cannot be checked). Shape:

```yaml
pii_signoff:
  signed_by: "<human name/role>"     # never an agent; empty ⇒ gate not passed
  signed_on: "<YYYY-MM-DD>"
  fields:
    - name: m_client.display_name
      class: pii                      # from the PII policy class vocabulary
      decision: masked_when_no_pii    # allow | masked_when_no_pii | never_return
```

Every output field of every shape in the domain must appear with a `class` and a
`decision`. The class vocabulary is the PII policy's
(`docs/product/pii-policy/02-2-sensitivity-classes.md`):

- `pii` (client/staff identity) → returned only with `can_view_pii=true` **and**
  the capability explicitly allowing that field; loan/group rows carry these.
- `sensitive_business_identifier` (account numbers, external ids) → excluded by
  default; returned only through explicit capability approval.
- `secret_never_expose` → **never** returned regardless of activation. For
  payments, `m_payment_detail.{account_number,check_number,receipt_number,`
  `bank_number,routing_code}` are excluded unless a capability explicitly approves
  **masked** display (PII policy §3). Payments' sign-off is specifically the
  decision on masked display per field — some fields stay permanently excluded.

Until a domain passes all four gate steps, its questions return
`Unsupported(off_domain_<id>)` honestly (loan/GL/tax via `lqr.rs:84`; group/payments
via the empty `approved_mvp` set).

### §8 — documentation fix

`docs/product/reporting-data-scope/06-5-explicitly-out-of-scope-permanent.md:5–14`
mislabels Loan/GL/Tax/custom-datatables as *permanent, will never be built*,
contradicting every machine-readable source (`status: deferred`, milestone map,
active issue 008). Correct it to **`deferred (scheduled)`**.

## Affected areas

- **Code:** `crates/chat/src/knowledge/dataset/validate.rs` (D1); catalog
  loader/validator (D3 lints).
- **Knowledge (YAML):** `knowledge/datasets/**` (D2, §4 filters, §6 new datasets),
  `knowledge/domains/**` + `knowledge/data-scope/areas/**` (§7 flips, new payments
  area), capability YAML for wiring (§3, §5).
- **Docs:** `docs/product/reporting-data-scope/06-5-…` (§8); this spec + its plan;
  the superpowers status ledger.

## Risks

- **D1 false positives** on legit display-only id columns → mitigated by
  `filters_exempt` and a test corpus of all 10 current datasets staying green.
- **D3 lint flips CI red** the moment it lands (5 datasets are stale today) →
  land D3 **after** §3 wire/retire decisions, or in the same change, never before.
- **Deferred-domain authoring drifts into activation** without the gate →
  the plan keeps P1–P3 authoring separate from the §7 flip, and the flip is a
  distinct, human-signed step.
- **DB-column drift** — every §4/§6 column is verified against
  `fineract_local_default`; re-verify before authoring if the replica schema moved.

## Success criteria (013 §11, Definition of Done)

1. `validate_dataset` enforces id-column filters (D1); the two catalog lints (D3)
   are in CI and green; no vestigial junk (D2).
2. Every dataset shape has a live consuming capability or a recorded retirement;
   no dead resolver/probe or orphan fragment remains.
3. Live savings shapes are filterable by `office_id`/`product_id`/stable-id +
   bounded `IN`; narrowing by id (not name) is expressible on the executed path.
4. Every ❌ domain in §9 has an authored dataset (in-scope) or a complete spec +
   recorded activation-gate status (deferred).
5. 012 scenario A1 (client → account resolution) runs end-to-end from a sentence.
6. Doc §6-5 no longer mislabels deferred domains as permanent.
7. Deferred domains have a written activation order (loan → GL → payments →
   group/tax) each referencing the §7 gate.
8. No 012 security/PII/office-scope/budget invariant weakened.
