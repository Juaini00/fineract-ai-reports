# 011-dataset-filter-and-shape-coverage-gaps

Status: active
Severity: high
Area: chat
Created: 2026-08-03
Resolved:

## Problem

A dataset can return a column it cannot filter on, and can be asked a question
whose shape it has no way to answer. Nothing detects either condition, so the
request pipeline silently substitutes the nearest capability it can find.

Two of the four authored datasets declare no filter slots at all:

| Dataset | `filters` |
| --- | --- |
| `savings.account_activity` | `client_name`, `product_name`, `latest_transaction_amount` |
| `savings.accounts` | `account_number`, … |
| `savings.account_charges` | **`[]`** |
| `organization.office_summary` | **`[]`** |

`savings.account_charges` returns `charge_name`, `is_penalty`,
`charge_timing_enum`, `due_date` and `amount_outstanding` as output columns, yet
declares `filters: []`. The value a user wants to narrow by is present in the
result and unreachable by the request.

This is not specific to charges. Any question that narrows by a value living in
an output column of a filterless dataset hits the same wall — office questions
against `organization.office_summary` included.

## Impact

The same missing filter produces an **honest refusal** under one phrasing and a
**confident wrong answer** under another. The inconsistency is worse than either
outcome alone: a user who gets a plausible table has no signal that their filter
was discarded.

For a banking reporting system, a silently unfiltered result presented as an
answer is the most serious failure mode available.

## Current behavior

Two consecutive production requests, same intent, opposite outcomes.

**Request A — "ada berapa saving weekly charge di sistem saat ini"**

```
router intent   request_shape=RequestShape { operation: Summary,
                subject: SavingsAccountCharge, grouping: None,
                output: Scalar, pii: None }
                entities=[AssistantEntity { entity_type: Metric,
                          value: "weekly charge" }]
retrieval plan  compatible_ids=Some([])
retrieval       [("savings_account_charges_recent", 0.99),
                 ("savings_deposit_total", 0.99),
                 ("savings_withdrawal_total", 0.99),
                 ("savings_balance_summary", 0.99),
                 ("savings_account_terms_lookup", 0.99), …]
reranker        Select savings_pending_charges_clients, confidence 0.9
                reason: "aligns with the pending charges capability that
                         lists outstanding charges as of today"
executing       query_id=savings.pending_charges_clients bind_count=3
```

Result: a list of every client with outstanding charges. The user asked *how
many charges of one type exist*. Three separate failures compounded:

1. **Shape ignored.** `operation: Summary` / `output: Scalar` (a count) was
   answered by a list-of-clients capability.
2. **Semantics substituted.** "weekly charge" is a charge *type*;
   `pending_charges_clients` filters on *unpaid*. The reranker's own stated
   reason swaps "weekly" for "outstanding".
3. **Filter silently dropped.** `"weekly charge"` was extracted, then discarded,
   because no slot exists to bind it to.

**Request B — "berikan saya semua changes dengan tipe weekly charge pada saving"**

```
reranker        Unsupported, confidence 0.9
                reason: "No candidate capability specifically filters charges
                         by type 'weekly charge'; the closest candidates list
                         charges but do not support a type filter."
```

Result: correct. This is the behaviour the system should have produced for
Request A as well. The reranker's reasoning is accurate — `filters: []` is real.

### Three further defects visible in the same logs

**Shape compatibility contributes nothing.** `compatible_ids=Some([])` in *both*
requests. The router emits `pii: None`; `savings_account_charges_recent`
declares `pii: client_identity`; `shape_compatible` requires equality (or
`Unknown`), so every genuine candidate is excluded. The router has no way to
know that listing charges implies returning client identity. Retrieval scores
rather than gates, so this did not block the request — it means the shape signal
is dead weight on this path.

**Retrieval scores are saturated.** Five candidates tie at 0.99 in Request A and
five again in Request B. Ranking is decided by the reranker alone, with no
useful prior. This is the score-gap compression recorded as a risk in
`docs/superpowers/specs/2026-07-31-dataset-model-design.md`; it is now
observable in production.

**The entity vocabulary has no slot for a charge type.** `AssistantEntityType`
offers `person_name, client_id, office, date_period, currency, product, metric,
capability_hint, account_number`. "weekly charge" is a value of
`m_charge.name` — none of those fit, so the model forced it into `metric`.
The same gap will recur for any domain noun outside that list (charge type,
transaction type, account status, staff, group/center).

## Scope — what this issue does and does not cover

The gap is the *mechanism*: a dataset that exists but cannot be narrowed or
counted. It is not "every question the system cannot answer".

**In scope.** Any narrowing or counting question against a dataset that already
exists:

- "apakah ada office dengan nama X" / "ada berapa office …" —
  `organization.office_summary` declares `filters: []`, and neither
  `office_list_basic` nor `organization_office_client_summary` accepts an office
  name at all. Their only parameters are `office_ids` (sourced from
  `authorized_scope`, `user_may_override: false`) and `limit`, so the system can
  only list every office in scope. Note the office-scope parameter is an
  authorization boundary and must stay non-overridable — narrowing by office
  *name* therefore requires its own declared filter, not a relaxation of that
  parameter.
- "semua charge dengan tipe X", "charge jatuh tempo hari ini" — the reported case.
- The same shape of question against `savings.accounts` and
  `savings.account_activity` for any column they return but do not declare a
  filter for.

**Out of scope — tracked elsewhere or not a chat concern.**

- **Client-domain filtering** (e.g. "client dengan office name X"). There is no
  client dataset yet — `knowledge/datasets/` holds only `organization/` and
  `savings/`. Client capabilities still run the legacy query path, so this is a
  migration gap, not a filter gap. It belongs to the dataset-model rollout in
  `docs/superpowers/specs/2026-07-31-dataset-model-design.md`, not here. Once a
  client dataset exists, this issue's rules apply to it.
- **Loan domain** — no capabilities or datasets exist. Tracked by
  [008-loan-domain-analyst-capabilities](./008-loan-domain-analyst-capabilities.md).
- **Audit** — not a chat domain. It is a management/compliance surface with
  different authentication (`AuthenticatedManagementAdmin`) served by
  `/management/audit`, and is not reachable through the assistant by design.

The distinction that matters: this issue is about datasets that exist and cannot
be asked properly. Making a domain exist at all is separate work.

## Expected behavior

- A question that narrows by a value the dataset returns is either answerable or
  explicitly refused — never answered with the filter silently dropped.
- The refusal is consistent across phrasings of the same intent.
- A count question (`operation: Summary`, `output: Scalar`) is never satisfied by
  a list capability.
- Shape compatibility either contributes real ranking signal or is not consulted;
  it must not silently exclude every candidate.
- Catalog validation refuses a dataset that returns a column commonly used for
  narrowing while declaring no filter for it, rather than leaving the gap to be
  discovered by a user.

## Proposed fix

Ordered by value-to-effort, not by dependency.

1. **Declare the missing filter slots.** `savings.account_charges` needs at
   minimum `charge_name` (string, `case_insensitive: true`, `eq`), and plausibly
   `is_penalty` (boolean) and `due_date` (date, range operators).
   `organization.office_summary` needs whatever narrowing office questions
   actually use. Follow the shape already proven in
   `knowledge/datasets/savings/account_activity.yaml`.

2. **Add aggregate shapes.** Datasets that can be counted need a `summary` /
   count shape, or every "ada berapa …" question will keep being answered with a
   list. This is the `operation: Summary` / `output: Scalar` case in Request A.

3. **Extend the entity vocabulary** with the domain nouns the catalog actually
   filters on — starting with a charge/type slot — so an extracted value has a
   correct place to live instead of being coerced into `metric`. Note
   `AssistantEntityType` now has a `#[serde(other)] Unknown` arm (see 3e8fd91),
   so adding variants is safe, but an unmapped value still means a dropped filter.

4. **Fix or retire the `pii` dimension in shape compatibility.** Either teach the
   router to infer PII from the subject, relax `shape_compatible` on the `pii`
   axis, or stop consulting it. Today it excludes every candidate and provides no
   signal.

5. **Make the gap a validation error, not a runtime surprise.** Catalog
   validation should reject a dataset whose shape returns a narrowable column with
   no corresponding filter slot — the same class of guard already applied to
   `order_by` references and `core` output fields in
   `crates/chat/src/knowledge/dataset/validate.rs`.

6. **Address score saturation** so shape and metric signals can break ties before
   the reranker sees them. Measurement first: capture the score-gap distribution
   across a sample of real requests, as the dataset-model spec requires.

Item 5 is the one that prevents recurrence. Items 1–2 fix the reported symptom;
without 5, the next authored dataset reintroduces it.

## Links

- `knowledge/datasets/savings/account_charges.yaml` — `filters: []`
- `knowledge/datasets/organization/office_summary.yaml` — `filters: []`
- `knowledge/datasets/savings/account_activity.yaml` — the filter shape to copy
- `crates/chat/src/knowledge/dataset/validate.rs` — where guard 5 belongs
- `crates/chat/src/assistant/retrieval/engine.rs` — `shape_compatible`, `shape_score`
- `crates/chat/src/assistant/understanding/intent.rs` — `AssistantEntityType`
- `docs/superpowers/specs/2026-07-31-dataset-model-design.md` — score-gap risk, recorded before it was observed
- Commit `3e8fd91` — the router no longer dies on an invented entity kind, which is what made these two requests reach retrieval at all
