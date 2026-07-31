# Dataset Model: One Source, Many Questions

Date: 2026-07-31
Status: Design approved, ready for planning
Scope: All 32 approved capabilities across every domain

## Problem

A capability today freezes four independent decisions into one file:

| | Decision | Reusable? | Frozen today? |
|---|---|---|---|
| 1 | **Source** — tables and joins | Highly | yes |
| 2 | **Filter** — which rows (`WHERE`) | Per question | yes |
| 3 | **Shape** — list / total / rank / trend / group-by | Per question | yes |
| 4 | **Projection** — which columns (`SELECT`) | Per question | yes |

Because all four are frozen together, changing *any one* forces a new capability
YAML, a new query YAML, a new SQL file, and a catalog reindex. The catalog grows
as a cross product of four axes when only axis 1 is a genuinely distinct thing.

Measured across the repo: **32 capabilities over 12 distinct table-sets, and
those 12 are mostly subsets of one another — roughly 5 real sources.** Two
examples of what the split costs:

```
deposit_total.sql  vs  withdrawal_total.sql   →  differs by ONE literal:
    WHERE t.transaction_type_enum = 1    vs    = 2

pending_charges.sql  vs  strictly_overdue_charges.sql  →  differs by:
    one extra  AND sac.charge_due_date < $2::date
    one different ORDER BY
```

The triggering incident: the question *"saya ingin tahu charge paling baru yg
telah dibuat?"* ("I want to know the newest charge created") could only be
answered by authoring a new capability. Worse, when it could not be answered,
the system did not say so — it returned a confident report about unrelated
savings deposits. See `docs/` git history for the three routing defects fixed
alongside this design (commit `3be144d`).

## Non-goals

- **Arbitrary AI-generated SQL.** The LLM never contributes SQL text. This is a
  read-only replica of a banking core; the invariant in `AGENTS.md` stands.
- **Answering everything.** This design makes the answerable set *declared* and
  everything outside it *loudly refused*. It does not widen what is answerable
  except along declared axes.
- **Free sources.** A genuinely new subject (loans, GL, accounting) still needs
  a new dataset authored. Questions become free within a source; sources do not.

## Target model

Files grow with **sources**, not questions. One dataset per source; the other
three axes become declared whitelists the LLM *fills* but never *invents*.

```
question → pick DATASET   (~5 candidates, not ~31)
         → fill declared FILTER slots  → bound parameters
         → pick declared SHAPE         → approved SQL fragment
         → pick PROJECTION             → post-fetch column subset
```

### Dataset contract

```yaml
# knowledge/datasets/savings/account_charges.yaml
id: savings.account_charges
database: fineract
tables: [m_savings_account_charge, m_savings_account, m_client, m_office, m_charge]

source_sql: queries/savings/account_charges.source.sql   # joins + office scope

filters:                       # AXIS 2 — the only filterable columns, ever
  - id: due_date               # LLM refers to this id, never a SQL identifier
    expr: sac.charge_due_date
    type: date
    operators: [eq, lt, lte, gt, gte, between]
  - id: is_paid
    expr: sac.is_paid_derived
    type: boolean
    operators: [eq]
  - id: is_penalty
    expr: sac.is_penalty
    type: boolean
    operators: [eq]

shapes:                        # AXIS 3 — each carries its own full request_shape
  - id: list
    request_shape: {operation: list, subject: savings_account_charge, grouping: none, output: list}
    fragment: queries/savings/account_charges.list.frag.sql
    order_by: [created_desc, due_date_asc, outstanding_desc]
  - id: total
    request_shape: {operation: total, subject: savings_account_charge, grouping: none, output: scalar}
    fragment: queries/savings/account_charges.total.frag.sql
  - id: rank_by_client
    request_shape: {operation: rank, subject: client, grouping: none, output: ranking}
    fragment: queries/savings/account_charges.rank.frag.sql

order_by:                      # whitelisted, referenced by id
  - {id: created_desc,     expr: "sac.created_on_utc DESC, sac.id DESC"}
  - {id: due_date_asc,     expr: "sac.charge_due_date ASC, sac.id DESC"}
  - {id: outstanding_desc, expr: "sac.amount_outstanding_derived DESC, sac.id DESC"}

output_fields:                 # AXIS 4 — `core` renders always, rest are opt-in
  - {name: savings_account_charge_id, type: bigint,  sensitivity: public_business, core: true}
  - {name: charge_name,               type: string,  sensitivity: public_business, core: true}
  - {name: amount_outstanding,        type: decimal, sensitivity: public_business, core: true}
  - {name: client_display_name,       type: string,  sensitivity: pii}
  - {name: due_date,                  type: date,    sensitivity: public_business}
  - {name: office_name,               type: string,  sensitivity: public_business}
```

### Composition

One statement, assembled from exactly two authored files plus one declared
expression:

```sql
WITH base AS (
    <source_sql>                                            -- authored file, verbatim
    WHERE c.office_id = ANY($1::bigint[])
      AND ($2::date IS NULL OR sac.charge_due_date = $2)     -- one pair per filter slot
      AND ($3::date IS NULL OR sac.charge_due_date < $3)
      AND ($4::bool IS NULL OR sac.is_paid_derived = $4)
)
<shape fragment>                                            -- authored file, verbatim
ORDER BY <order_by expr, selected by id>                    -- declared string, by id
LIMIT $n
```

**The safety invariant:** every character of SQL originates in a file on disk or
a declared `expr`. The LLM contributes only *ids* (`due_date`, `list`,
`created_desc`) and *values*, which become bound parameters. `select_only`,
`single_statement`, `parameterized_only` and `require_office_filter` are all
preserved.

**No generic aggregate DSL.** Shapes are authored SQL fragments, not
`{fn: sum, column: x}`. Inventing a mini query language is how this design would
rot; a fragment stays readable and reviewable.

### Authoring cost

| New question | Today | After |
|---|---|---|
| "charges due today" | new YAML + new SQL + reindex | 0 files |
| "unpaid charges" | new YAML + new SQL | 0 files |
| "total outstanding charges" | new YAML + new SQL | 0 files |
| "top 10 clients by outstanding charges" | new YAML + new SQL | 0 files |
| "charges on **loans**" | new YAML + new SQL | new dataset — correctly so |

## Runtime flow

The existing two-layer split is preserved. The new axes ride on it; no new LLM
call is added.

```
L1  LLM Gateway (existing call) — emits HINTS only, never SQL
      intent_kind, domain, entities, temporal_hint, quantity_hint
    + shape_hint   : "list" | "total" | "rank"
    + filter_hints : [{id: "due_date", op: "eq", value: "today"}]
    + column_hints : ["amount_paid", "due_date"]
                            ↓
Retrieval — ranks DATASETS (~5) instead of capabilities (~31)
                            ↓
L2  Deterministic Resolver (Rust — the trust boundary)
      filter_hint → id declared? op in operators[]? value parses as type?
      shape_hint  → in shapes[]?
      column_hint → in output_fields[]?
      ANY failure → rejected, never silently dropped
                            ↓
Execution — source + filters + fragment + order_by → one prepared statement
                            ↓
Presentation — columns = core ∪ resolved column_hints, then PII gate
```

### Hint rejection matrix

| Rejection | Outcome |
|---|---|
| Filter id not declared on this dataset | `unsupported` — name what cannot be filtered |
| Operator not allowed for that slot | `unsupported`, listing allowed operators |
| Value fails type parse | `clarification` — ask for the value again |
| Shape not supported by dataset | `unsupported` — e.g. "I can list these but not total them" |
| Column hint not in `output_fields` | dropped silently |

The last row is the only permitted silent drop: projection changes which columns
are *displayed*, never which rows are *returned*, so it cannot make an answer
wrong. Dropping an unknown filter changes the answer set, so it is never silent.

## Retrieval changes

`request_shape` becomes a set. Each shape carries its own `RequestShape`; the
dataset's score is the max across its shapes, and the argmax shape becomes the
deterministic default when the LLM emits no `shape_hint`:

```rust
fn shape_score(plan: &RetrievalPlan, dataset: &DatasetKnowledge) -> (f32, ShapeId) {
    dataset.shapes.iter()
        .map(|s| (score_one(plan, &s.request_shape), s.id.clone()))
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap_or((0.0, ShapeId::default()))
}
```

This also lets `subject` differ per shape — `list` over charges is
`subject: savings_account_charge` while `rank_by_client` is `subject: client` —
which resolves the subject conflict when merging the three charge capabilities.

**Risk (must be measured, not assumed):** with ~5 broad datasets instead of ~31
narrow capabilities, each dataset's retrieval text becomes the union of all its
questions' examples. `catalog_fallback` scores `hits / terms.len()`, so a larger
haystack raises every score and may compress the gaps between candidates.
Judgement is that five semantically distinct sources are more separable than 31
overlapping capabilities, and the reranker now performs the final pick — but the
implementation plan must measure score-gap distribution before and after rather
than assume.

## Validator changes

The validator enumerates every executable combination per dataset and prepares
each:

```
combinations = |shapes| × |order_by|   ≈ 3 × 3 per dataset   ≈ 60 total
```

**Filters do not multiply this.** Every filter placeholder is always present via
null-passthrough, so statement text is identical whether a filter is active or
not. This property is why the cross product stays small and must be protected
deliberately: switching to "append WHERE clauses only when active" turns 60
statements into `2^N × shapes × order_by`.

New rules:

- `filters[].expr` and `order_by[].expr` must match a strict grammar — qualified
  identifier, optional `ASC`/`DESC`, optional `NULLS FIRST`/`LAST`, comma
  separated. Reject `;`, `--`, `/*`, parentheses, and anything else. These
  strings are concatenated into SQL and are therefore a trust boundary even
  though they are authored rather than user-supplied.
- Every `filters[].expr` column must resolve to a table in the dataset's
  declared `tables`.
- Every shape fragment must be SELECT-only and a single statement.
- Every `shapes[].order_by` reference must exist in `order_by[]`.
- Each shape must declare at least one `core: true` output field.
- Existing per-query `timeout_ms` applies to every composed statement.

## Open risk: null-passthrough and index usage

`($2::date IS NULL OR sac.charge_due_date = $2)` is known to inhibit index scans
in PostgreSQL — the planner often cannot prove the column predicate is usable
when disjoined with a parameter null-check. On large Fineract tables this could
turn an index lookup into a sequential scan.

Mitigating factors: the office-scope predicate and `LIMIT` already bound most
result sets, `timeout_ms: 3000` is a hard backstop, and Postgres custom plans
can specialise on actual parameter values after five executions (not
guaranteed).

**This is a measured decision, not a design assumption.** The implementation
plan must include an `EXPLAIN` task on the largest table with filters active and
inactive, treated as a go/no-go on the idiom. If it fails, the fallback is to
permit filter slots only on indexed columns, enforced by the validator against
`pg_indexes`. Preparing one statement per active filter combination would fix it
definitively but explodes the cross product to `2^N` and is not recommended.

## Migration map

All 32 capabilities collapse to 5 datasets:

| Dataset | Capabilities today | Shapes needed | Filters replacing the split |
|---|---:|---|---|
| `organization.offices` | 5 | list, summary, trend(month) | — |
| `client.clients` | 8 | list, summary, rank(office), trend(month), lookup, random_sample | — (date range already handled by the existing `temporal_hint` path) |
| `savings.accounts` | 4 | rank_by_balance, rank_by_account_count, rank(office), summary | — |
| `savings.transactions` | 12 | list, rank(none), rank(month), rank(office), trend(month), total | **`transaction_type`** |
| `savings.account_charges` | 3 | list, total, rank(client) | `is_paid`, `is_waived`, `due_date` |

**Measure is not a separate axis.** Capabilities that differ only by what they
aggregate — `client_top_n_by_savings_balance` vs
`client_top_n_by_savings_account_count`, or
`organization_office_activity_ranking` vs `organization_office_dormant` — become
**distinct shapes with distinct fragments**, not a new declaration type. This
follows directly from the no-aggregate-DSL decision: a different measure is a
different authored `SELECT`, so it is a different fragment over the same source.
Such capabilities therefore share joins, filters, and projection while keeping
their own reviewed aggregate SQL.

The single highest-value line is `transaction_type`. Twelve capabilities exist
because `WHERE transaction_type_enum = 1` vs `= 2` was baked into SQL instead of
bound:

```
savings_deposit_total                 total  grp=none   scalar       ─┐
savings_withdrawal_total              total  grp=none   scalar       ─┤
savings_deposit_top_n                 rank   grp=none   ranking      ─┤
savings_withdrawal_top_n              rank   grp=none   ranking      ─┼→ savings.transactions
savings_deposit_monthly_top_n         rank   grp=month  ranking      ─┤    6 shapes
savings_withdrawal_monthly_top_n      rank   grp=month  ranking      ─┤    1 filter slot
savings_deposit_monthly_breakdown     trend  grp=month  time_series  ─┤
savings_withdrawal_monthly_breakdown  trend  grp=month  time_series  ─┤
savings_activity_list                 list   grp=none   list         ─┤
client_top_n_by_deposit_volume        rank   grp=none   ranking      ─┤
organization_office_activity_ranking  rank   grp=office ranking      ─┤
organization_office_dormant           rank   grp=office ranking      ─┘
```

### Sequencing

Today's capability is a **degenerate dataset** — one shape, zero filters, fixed
order. Migration is therefore mechanical before it is clever.

**Phase A — mechanism, zero behaviour change.** Build gateway hints, resolver
validation, composition, projection, and the validator cross-product. Convert
all 32 capabilities 1:1 into single-shape, zero-filter datasets. Acceptance:
every query returns byte-identical output. This is the only point at which
"identical results" is a provable criterion, which is why it is worth paying for
despite shipping no user-visible change.

**Phase B — `savings.account_charges`, 3 → 1.** Smallest group and the one whose
failure is already understood. Proves filter slots and multi-shape end to end.

**Phase C — `savings.transactions`, 12 → 1.** The largest win, once B validates
the mechanism.

**Phase D — `client.clients` 8 → 1, `savings.accounts` 4 → 1,
`organization.offices` 5 → 1.**

Each phase is independently revertible: an unmerged dataset remains a valid
single-shape dataset.

**Planning scope.** This spec defines the whole target model, but Phase A alone
is one implementation plan. Phases B–D each get their own plan, written after
the preceding phase lands, so that measured outcomes — retrieval score gaps
(§Retrieval changes) and the null-passthrough `EXPLAIN` result (§Open risk) —
inform them rather than being guessed up front.

### Cost: superset joins

Merging by source means one `source_sql` carries every join any of its shapes
needs. `savings.transactions` would join `m_savings_product` (needed only by
`savings_activity_list`) and `m_client` (not needed by
`organization_office_dormant`). Postgres can eliminate provably-unused
`LEFT JOIN`s on unique keys, but this is not guaranteed and does not apply to
`INNER JOIN`. On the largest table in the schema this is the most likely source
of latency regression. Mitigation if measured bad: split a source when its join
set genuinely diverges — 6 datasets is far cheaper than 32.

## Error handling

| Failure | Wrong behaviour | Required behaviour |
|---|---|---|
| Query exceeds `timeout_ms` | render "Found 0 row(s)" | explicit error — a timeout is not an empty result |
| Filters legitimately match nothing | error | "Found 0 row(s)" + echo applied filters |
| Shape fragment file missing | runtime 500 | **startup failure** |
| Composed statement fails `PREPARE` | runtime 500 | **startup failure** |

Composition errors are catalog errors and the catalog is fully enumerable at
boot; a user request must never be what discovers a broken fragment.

When rows are returned, the response echoes which filters were applied. Without
this the user cannot distinguish "50 charges are due today" from "50 charges
exist and the date filter was ignored" — the exact ambiguity that motivated this
design.

## Testing

All three routing defects fixed alongside this design passed `cargo test`
cleanly. That is the design input for this section:

| Defect | Why the suite missed it |
|---|---|
| Stale vector index | Tests build a fresh index; nothing exercised "YAML changed, index did not" |
| Reranker 400 on every call | `FakeLlmClient` never reproduces a provider rejecting `json_schema`; the real provider path had zero coverage |
| Omitted `intent` → silent `Greeting` | Tests construct `AssistantIntent` fully populated; nothing tested a partial LLM response |

**T1 — Catalog grammar tests** (no DB, no LLM). `expr` grammar, column
resolution against declared tables, ≥1 `core` field per shape, `order_by`
reference integrity. Runs on every commit.

**T2 — Cross-product `PREPARE`** (real schema). All ~60
`dataset × shape × order_by` statements prepared; output columns asserted equal
to declared `output_fields`. Catches composition and column-type errors — the
class that produced the INT2/INT8 bugs already fixed in this repo.

**T3 — Resolver rejection table** (pure). Exhaustive table-driven coverage of
the hint rejection matrix. This is the trust boundary and receives the densest
coverage in the design.

**T4 — Golden-output migration oracle** (Phase A acceptance). For all 32
capabilities, capture columns, row ordering, and a row-set hash under fixed
parameters against a seeded database. Phase A asserts byte-identical output.
From Phase B onward goldens change deliberately and diffs are reviewed rather
than regenerated blindly.

**T5 — Degraded-LLM contract tests** *(new category)*.
- *Partial responses.* A fake LLM omitting `intent`, `shape_hint`, and
  `filter_hints` individually. Assert none yields a confident wrong answer.
  **Spec rule: no LLM-facing required field may carry a serde default whose
  value is a valid-looking outcome.** This obliges removing `#[serde(default)]`
  from `AssistantIntent.intent`, since `#[default] Greeting` converts a missing
  field into a plausible wrong answer instead of a loud failure.
- *Provider rejection.* A stub HTTP layer returning 400 for `json_schema`,
  asserting fallback occurs and that the fallback prompt carries both the
  literal `json` token and the schema.

**T6 — Live smoke set** *(new category, not `cargo test`)*. A checked-in file of
real questions — including Indonesian phrasings such as
`saya ingin tahu charge paling baru yg telah dibuat?` — with expected dataset,
shape, and bound filters, executed by a script against a live instance. Every
defect fixed alongside this design was visible within a minute live and
invisible to the suite.

## Thesis

The failure that motivated this work was not a missing capability. It was that
the system **answered anyway**. This design makes the answerable set declared,
and everything outside it loudly refused.
