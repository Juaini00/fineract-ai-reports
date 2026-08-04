# 012-capability-prose-contradicts-approved-sql

Status: resolved
Severity: high
Area: chat
Created: 2026-08-04
Resolved: 2026-08-04

## Problem

The reranker decides whether the catalog can answer a question. It decided that
from a capability's `title`, `description`, `examples` and `output_mode` — four
prose fields. What a capability actually returns is decided by the approved SQL
underneath it. Nothing kept the two in agreement.

When they disagreed, nothing failed. No query ran, no guard fired, no test went
red. The user was simply told the catalog could not answer a question it could
answer.

**Reported case.** `show me all clients from FOO office!` — an office named
`Foo` exists with 13 active clients — returned:

```
reranker decision = Unsupported, confidence 0.9
reason: "No candidate matches the query intent of listing all clients from a
         specific office; candidates either summarize counts, sample, or list
         recent clients, not a full list filtered by office."
```

The candidate that could have answered was `client_list_recent`, whose SQL is:

```sql
WHERE c.office_id = ANY($1::bigint[])
  AND c.status_enum = 300
  AND c.activation_date IS NOT NULL
  AND ($3::text IS NULL OR lower(o.name) = lower($3::text))
ORDER BY c.activation_date DESC, c.id DESC
LIMIT $2;
```

It restricts nothing by date. It returns every active client in scope,
optionally narrowed to one named office, and only *sorts* by activation date.
Its title said "Recently Activated Clients". The reranker read the adjective and
refused.

The entity extractor was never at fault: `Office` was extracted correctly, which
is provable from the sufficiency log dropping `savings_account_identity_lookup`
with `unhonoured=[Office]` — a line that is only reachable when `Office` is in
the expressed constraint set.

## Three defects, one root cause

1. **Prose overclaimed.** `client_list_recent` and `savings_account_charges_recent`
   were both titled for a recency filter neither query performs.

2. **`supported_intents` / `unsupported_intents` were a dead letter.** Authored
   in every capability YAML since the catalog was written and dropped on the
   floor by serde — the fields did not exist on `CapabilityKnowledge`, only on
   `DomainKnowledge`. Recorded as a finding on 2026-07-30 and never closed.

3. **The reranker could not see the capability's real boundary.** It received
   no parameter list, so it had no way to know that a capability *can* bind
   `office_name` when its title does not say so.

## Resolution

**Prose corrected.** `client_list_recent` → "Client List by Office, Newest
First"; `savings_account_charges_recent` → "Savings Account Charges, Newest
First". Both descriptions now state that every row in scope is returned and that
nothing is restricted to a recent period. Both gained `supported_intents` /
`unsupported_intents` that name what the SQL does and does not do. Capability
IDs are unchanged — they are referenced by API capability names, tests and job
history.

**Dead fields made load-bearing.** `supported_intents` / `unsupported_intents`
now deserialize onto `CapabilityKnowledge`, flow into the retrieval document's
embedding text and `metadata_json`, and reach the reranker.

**The boundary is now visible to the reranker.** Each candidate carries
`supported_intents`, `unsupported_intents` and `user_filters` — the query's
parameters minus `limit` and minus anything sourced from `authorized_scope`.
Office scope is excluded deliberately: it is the authorization boundary, bound
on every request, and treating it as a user filter is what made capabilities
look office-filterable when they could only return the caller's whole scope.
The system prompt now states that these fields are authoritative and that an
ordering word in a title describes sort order, not a restriction.

**Recurrence blocked at catalog load.**
`knowledge::catalog::prose_claims::validate_prose_claims` fails the catalog when
`display_name` or `description` claims a narrowing the query cannot perform:

| Claim | Must be backed by |
| --- | --- |
| recent / latest / newest / terbaru | a user-supplied date parameter |
| top / highest / largest / terbesar | `ORDER BY … DESC` in the SQL |
| random / sample / acak | `random()` in the SQL |

A recency or ranking word qualified as ordering ("newest first", "ordered by")
is accepted — that is the honest phrasing. A word that spells one of the query's
own field names is not read as a claim, so
`savings_account_activity_lookup`'s `latest_transaction_amount` parameter does
not trip the guard. Checked per field, not over the concatenation: the reranker
weights the title on its own, so a title that overclaims is not rescued by a
description that corrects it three lines later.

Running the guard over the whole catalog found exactly two offenders in 41
capabilities — the two above.

## Verification

Not a YAML load and not a `PREPARE`. The real service, the real DeepSeek
reranker, the real Fineract database, the user's original sentence.

| Question | Capability | Rows | Fineract truth |
| --- | --- | --- | --- |
| `show me all clients from FOO office!` | `client_list_recent` | 13 | 13 active in office `Foo` |
| `tampilkan semua nasabah di kantor Foo` | `client_list_recent` | 13 | 13 |
| `show me all clients from Head Office` | `client_list_recent` | 48 | 48 active in `Head Office` |
| `show me all clients from Atlantis Branch` | `client_list_recent` | 0 | office does not exist |
| `siapa saja nasabah yang paling baru diaktivasi` | `client_list_recent` | 73 | 73 active tenant-wide, newest first |

The nonexistent office returns zero rows rather than the caller's full scope,
which is the `office_name_narrows_only` invariant holding.

`retrieval_eval` gained five fixtures and now runs at **25/25 = 1.00** against
the real LLM, every bucket at 1.00.

## A test that was already red

`retrieval_eval` asserts a 0.90 accuracy floor only under `EVAL_USE_REAL_LLM=1`.
Measured on `master` before any of this work: **17/20 = 0.85 — already failing**,
with three `clarify` fixtures missed. The default stub run never asserts the
floor, so the failure was invisible to anyone not running the real-LLM mode.

Two of those three fixtures were miscalibrated and are reclassified, each with
the reasoning recorded in the fixture file:

- `setoran tabungan terbesar` → `savings_deposit_top_n`. "Terbesar" names the
  ranking; no other capability ranks individual deposits by amount.
- `show customer savings activity this week` → `savings_activity_list`. It is
  the only capability listing savings transactions over a date range, and "this
  week" binds its `from_date`/`to_date`.

The third was a genuine defect and is fixed rather than reclassified.
`berikan ringkasan office` was answered decisively with
`organization_office_summary` when `organization_office_client_summary` and
`organization_office_savings_summary` fit the words equally well. The reranker
now clarifies when candidates differ only in the *measure* they report and the
query asks for a summary without naming one.

Two harder ambiguity fixtures were added so the `clarify` bucket keeps its
required coverage of four rather than shrinking to the two that remained:
`give me a savings summary` and `tampilkan charge tabungan`.

`fixtures_cover_required_buckets` asserted `fixtures.len() == 20` exactly, which
made adding coverage a test failure. It is now a floor.

## Relationship to 011

[011](./011-dataset-filter-and-shape-coverage-gaps.md) put "client dengan office
name X" out of scope on the grounds that no client *dataset* existed yet. The
mechanism it described — a question that cannot be asked properly against data
that is present — applied to the client domain through the legacy query path the
whole time. 011's fix was real for the surface it named; the scope line was
drawn in the wrong place, and this was the gap it left.

## Still open

- `classification_semantic::ambiguous_prompt_produces_options_including_others_or_terminates_safely`
  fails in `spawn_app` with `VOYAGEAI_API_KEY is required when catalog embedding
  sync is enabled`. The test harness builds its config in code rather than from
  the environment, so the key in `.env` never reaches it. Verified identical on
  a stashed baseline — pre-existing, unrelated to this change, not fixed here.
- `client_list_recent` caps at 200 rows (`guards.max_limit`). "All clients" is
  accurate only below that cap; above it the answer is silently truncated. The
  row cap is not surfaced in the response.
