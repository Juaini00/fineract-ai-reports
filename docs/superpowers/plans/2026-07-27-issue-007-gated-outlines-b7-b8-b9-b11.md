# Issue 007 — Gated Outlines for Bundles 7, 8, 9, 11

> **Outline-only, deliberately.** These four bundles are dependency-blocked: each
> consumes an artefact that an earlier bundle *produces*, and that artefact does not
> exist in the tree yet. Writing full task-level specs now would mean inventing the
> shape of unbuilt outputs (the W-A1 inventory rows, the D1 scoring gaps, the W-G
> presentation contract, the W-I audit events). This doc records, per bundle, the
> objective, the predecessor it waits on, the exact inputs it needs from that
> predecessor, and precisely what gets finalised into a full spec+plan once the
> predecessor executes green. **Do not execute from this doc — spec each bundle only
> after its dependency's plan is executed and green** (roadmap execution protocol §4).

**Roadmap:** `docs/superpowers/plans/2026-07-27-issue-007-program-roadmap.md`
**Issue:** `docs/issues/active/007-analyst-grade-knowledge-and-request-mapping.md`

## Current state (verified 2026-07-27)

Verified against the working tree, since issue 007 is dated 2026-07-24 and is stale.
Findings that ground the gating below:

- **B3 (W-A1) has not shipped.** `docs/product/analyst-question-inventory.md` is
  absent (`docs/product/` holds `README.md`, `capability-coverage/`, `pii-policy/`,
  `reporting-capabilities/`, `reporting-data-scope/`, `landing-page-invideo-prompt.txt`
  — no inventory). So B7, which asserts one retrieval row per inventory question, has
  nothing to enumerate yet. Gate holds.
- **B7's test home exists.** `crates/chat/tests/retrieval_scoring.rs` is present (also
  `retrieval_eval.rs`, `assistant_retrieval_evidence.rs`). D1 *extends* an existing
  suite; it does not create one from scratch.
- **W-G / F4 defect stands.** The only `output_mode` mention under
  `crates/chat/src/assistant/presentation/` is a hard-coded `"list"` default in a
  `builder.rs` test fixture (`builder.rs:405`); no production presentation code reads
  `plan.output_mode`. All six modes still render identically. B9 gate holds.
- **F6 defect stands.** `renderer.rs`'s `cell` does no `|`/newline escaping. B9's
  two-line escape fix is still needed (and per the issue may ship ahead of the rest of
  W-G — see B9 note).
- **W-I not enforced.** `hard_cap` appears in `assistant/**` only as the stale comment
  `execution/tool/parameters.rs:299` ("enforced elsewhere") plus test fixtures. B6
  (W-I) has not shipped, so B11's `execution.result_truncated` / `execution.timed_out`
  events have no producer yet. B11 gate on B6 holds.
- **W-L projection stands.** `management/knowledge.rs` still hard-codes
  `limitations: Vec::new()` (`:120`); the detail projection is still a field-by-field
  allowlist. B11 is largely test-hardening over a correct projection, blocked only by
  what B8 adds to the catalog and what B6 adds as events.

No drift in these four bundles' dependencies from the roadmap's stated order. The
roadmap's own stale-claim notes (W-A2 enrichment shipped, `charge_due_date` hotfix
shipped, F7 fixed) do not touch B7/B8/B9/B11 gating.

## Global constraints (apply to every bundle below when it is specced)

Approved-SQL only (no AI SQL); office scope bound **inside** SQL via
`office_ids = ANY($n::bigint[])`, never Rust post-filter; SQL only in repositories,
never in handlers/services/`assistant/**`; PII gating field-level; "today" = Fineract
tenant business date (wall clock only for audit timestamps); sanitized errors (no
SQL/prompt/stack leakage); PostgreSQL durable truth, Redis live-SSE only; same-job
clarification via `POST /chat/jobs/{job_id}/responses`; exactly three crates
(app/core/chat); no new dependencies, migrations, or knowledge/queries YAML surface
unless the bundle explicitly adds a capability; English-only copy.

---

## B7 — W-D1 bilingual retrieval regression suite

**Issue section:** W-D (007 lines 366–397), D1 specifically.
**Type:** test (plan, no spec — roadmap row 7).
**Depends on:** **B3** (W-A1 analyst-question-inventory) and **B4** (savings catalog).

**Objective (2–3 lines).** Prove that every analyst question in the W-A1 inventory
reaches its intended capability at rank 1 and clears the gap threshold, in both
Indonesian and English phrasings, via a fixture-driven suite extending
`crates/chat/tests/retrieval_scoring.rs`. This is the regression net that D2 (in B8)
then uses to find and close scoring gaps. It also proves the E1 representative
question maps to `savings_pending_charges_clients` at rank 1, and that an
out-of-catalog question yields `Unsupported` (empty options), not `Clarify`.

**Specific inputs required from B3 (and B4):**
1. The finalised `docs/product/analyst-question-inventory.md` — its concrete list of
   ≥25 questions, each with (a) the natural-language phrasing(s), (b) the target
   capability id, (c) the coverage verdict (`covered` / `partial` / `missing`). The
   test fixture is a direct transcription of this table; without the actual rows there
   is nothing to assert.
2. Which inventory rows are marked `missing`/`partial` at B7 time — those are asserted
   as *known gaps* (documented xfail or a gap-list assertion), not as rank-1 passes,
   because B8 is what closes them. B7 must not pre-empt B8's catalog work.
3. From B4: the final set of savings/client capability ids and their example/intent
   vocabulary as shipped, so the fixture's expected capability ids are real.

**Finalised once B3+B4 are green (the full plan will contain):**
- Exact fixture format decision: extend `retrieval_scoring.rs` in place vs. a data
  file it loads — chosen against whatever `retrieval_scoring.rs` looks like at that
  point (re-audit it; it exists today but its shape may change under B4).
- One test case per inventory question × {Indonesian, English}, each asserting
  intended capability id at rank 1 and gap-threshold cleared, with the real ids/phrasings.
- The E1 rank-1 assertion for `savings_pending_charges_clients`.
- The `Unsupported`-not-`Clarify` test for a deliberately out-of-catalog question,
  asserting the offered-options list is empty (W-D3 acceptance).
- The gap ledger: inventory rows still `missing`/`partial` recorded as B8's input list.
- Commands: `cargo test -p chat --test retrieval_scoring` (+ any renamed target),
  expected green for covered rows, gaps explicitly listed.

---

## B8 — W-A3 + W-D2: close catalog and scoring gaps D1 found

**Issue section:** W-A3 (007 lines 224–251) + W-D2 (re-read within 366–397).
**Type:** code (full spec — roadmap row 8).
**Depends on:** **B7** (the passing/failing retrieval suite is the definition of "which
gaps exist").

**Objective (2–3 lines).** For every W-A1 question that B7's suite proves does *not*
reach its capability at rank 1, fix it **in the catalog, not the scorer**: enrich
`examples:`, `supported_intents:`, `description:`, and domain/metric tags with the
analyst vocabulary the issue names (`belum bayar`, `hutang`, `tunggakan`, `jatuh tempo`,
`terlambat`, `outstanding`, `overdue`, `arrears`, `unpaid`, `pending`), and add any
genuinely-missing capability (capability YAML + query YAML + approved SQL + metric YAML)
— but only when a real A1 question backs it. Drive zero questions to `missing` for the
savings and client domains.

**Specific inputs required from B7:**
1. B7's **gap ledger** — the concrete list of (question, target capability, language,
   observed rank/score) tuples that failed the rank-1/threshold assertion. This is the
   exact, bounded work list. Nothing is enriched speculatively.
2. Per failing row, whether it failed on *retrieval vocabulary* (fix by enriching an
   existing capability's `examples`/`intents`/tags) or on *absence of a capability*
   (fix by authoring a new one). B7's suite output distinguishes these: a wrong-rank
   pass to a sibling capability vs. no candidate clearing the floor.
3. Any row where B7 shows a catalog fix genuinely cannot work — only those justify a
   `ClassificationPolicy` threshold touch (W-D2 rule), and each must be justified.

**Finalised once B7 is green (the full spec+plan will contain):**
- The per-question remediation table: for each B7 gap, the exact YAML file and the
  exact `examples`/`supported_intents`/`description`/tag additions.
- For any new capability: its capability YAML with per-parameter policy block, query
  YAML, approved SQL under `queries/` (office scope bound via `office_ids = ANY($n)`),
  and metric YAML — each traced to a specific A1 question id.
- W-A4 default review outcome per capability appended to the inventory doc
  (point-in-time / rolling-window / historical-required; `limit` default vs `hard_cap`).
- The updated B7 suite re-run turning every previously-failing row green (D2 closes
  what D1 found), plus `cargo test -p chat --test catalog_validation` green (W-A3
  acceptance).
- Re-audit note: whether any threshold change was unavoidable and its justification.

**Open decision to surface at B8 spec review:** whether B8 adds any *new* capability at
all (which introduces new knowledge/queries YAML surface — permitted here because the
bundle explicitly adds a backed capability) or closes every gap by enrichment only. This
is determined entirely by B7's ledger and cannot be pre-decided now.

---

## B9 — W-G + W-J remainder + F4 + F6: presentation, money, output_mode, escaping

**Issue sections:** W-G (007 lines 461–569), W-J (728–791), F4 (1229–1261), F6 (1291–1318).
**Type:** code (full spec — roadmap row 9).
**Depends on:** **B8** (the final catalog: column sets, `output_mode` values, currency
columns, and derived-column sensitivities are what presentation must render).

**Objective (2–3 lines).** Make an analyst-grade answer (up to 14 columns, thousands of
rows) readable without inventing a second response contract: bound `rendered_markdown`
to a leading row sample while `table.rows` stays complete; emit per-currency subtotal
cards and a row-count card driven by column kinds; render money at the account's
`currency_digits` (never `100.000000`) with a `multi_currency` warning and never a
cross-currency grand total; and resolve the F4 `output_mode`/`AnswerPlan` fork and the
F6 unescaped-`cell` bug.

**Specific inputs required from B8:**
1. The final `output_fields` **order and kinds** per savings/client capability (W-G
   decision 5 makes column order a catalog concern; presentation renders whatever B8
   ships). Card/label/ordering logic keys off these.
2. Which capabilities carry currency columns and under what names — W-J needs the money
   column plus a per-row currency column (`currency_code`, `currency_digits`, and the
   `display_symbol`/`display_name` fallback columns) to exist in the approved SQL B8
   finalises. If B8's SQL does not yet select `sa.currency_digits` and the symbol
   columns, that SQL change is a B8 input, not a B9 invention.
3. The derived-column sensitivities B8/W-L settle (`days_overdue`, `amount_levied_total`,
   `charge_timing_enum` → `public_business`) so the PII-gating test uses real values.
4. The resolved PII question: whether the catalog uses `pii` or `pii_conditional` for
   `client_display_name` — `is_hidden` must understand whatever B8 ships (W-G acceptance).

**Finalised once B8 is green (the full spec+plan will contain):**
- The F4 fork decision, made concretely: either teach `presentation/builder.rs` +
  `renderer.rs` to consume `plan.output_mode` (populate cards/sections per mode), **or**
  delete `AnswerPlan` and the dead `build_answer_plan` branch — "do not leave both"
  (F4 fix direction). Chosen after re-auditing the plan/presentation code at that point.
- `rendered_markdown` row cap (leading sample, e.g. 50) with a test asserting the
  markdown row count is capped while `table.rows.len()` is not (W-G acceptance).
- Per-currency subtotal cards + row-count card via existing `ResponseCard`
  (`response.rs:79–84`); no new response shape (W-G decision 4).
- `TableColumnKind::Money` actually produced; builder rounds at account `currency_digits`;
  `table.rows` keeps the raw number, `rendered_markdown` gets the formatted string
  (W-J decisions 1,2); `display_symbol IS NULL` → `currency_code` fallback (AED test).
- `multi_currency` warning when >1 `currency_code`; a test asserting no field equals the
  cross-currency sum (W-J acceptance).
- F6: escape `|` **and** newline in `cell`, one unit test per character (F6 fix
  direction). *Note:* F6 is a two-line change the issue says "should not wait for the
  rest of W-G" — if B9 slips, the escape fix may be pulled forward as a standalone
  micro-task; record that at spec review.
- Client-contract update in `docs/current/management-dashboard-integration.md` stating
  `table` is authoritative and `rendered_markdown` is a bounded fallback (W-G acceptance).

**Open decisions to surface at B9 spec review:** (a) F4 — consume `output_mode` vs.
delete `AnswerPlan`; (b) whether the fan-out-safe currency `LEFT JOIN LATERAL` and its
catalog `check` (W-J decision 4) land in B9 or belong to the B8 SQL that B9 depends on;
(c) whether to pull the F6 escape fix forward independently.

---

## B11 — W-L management observability alignment

**Issue section:** W-L (007 lines 838–916).
**Type:** code/test (plan, no spec — roadmap row 11).
**Depends on:** **B8** (the final catalog the management surface must project) **and**
**B6** (W-I: the truncation/timeout the new audit events describe).

**Objective (2–3 lines).** Keep `GET /management/knowledge`, its detail endpoint, and
the dashboard summary truthful as B8 grows the catalog, without leaking anything new:
prove every B8-added capability appears in the list and resolves in detail (iterating the
loaded catalog, not a hard-coded list), that the detail projection stays a field-by-field
allowlist leaking no SQL, and add the thin audit event types for B6's clamp/timeout so an
under-reported or timed-out answer is reconstructable.

**Specific inputs required from B8:**
1. The final capability set and each capability's `query_id`, so the "every list id
   resolves to 200 on detail" test (catching the missing-query-YAML 404,
   `knowledge.rs:88–94`) runs over the real catalog.
2. The derived-column sensitivities B8 settles, so the "detail response contains no SQL
   keyword / no substring of the approved SQL file" test uses a capability that actually
   has a derived column (`days_overdue` etc.).

**Specific inputs required from B6 (W-I):**
1. Whether B6 landed a truncation clamp (→ need `execution.result_truncated` event) and
   whether it landed a per-query timeout (→ need `execution.timed_out` event). W-L
   decision 2 adds *only* the events B6 actually produces — if B6 ships no timeout, the
   `timed_out` event is not added. The event set is entirely B6-conditional.
2. The exact payload B6's clamp/timeout carries (capability id, row count, "more than N"
   semantics) so the new `AuditEventType` variants match producer reality.

**Finalised once B8 and B6 are green (the full plan will contain):**
- The catalog-iterating test: assert every loaded capability appears in
  `GET /management/knowledge` and resolves in `GET /management/knowledge/{id}`
  (W-L acceptance), plus the list-id → detail-200 test.
- The no-leak projection test: serialised `KnowledgeDetailResponse` for a derived-column
  capability contains no SQL keyword and no substring of the approved SQL file.
- New `AuditEventType` variant(s) in `management/model.rs` — exactly those B6 produces —
  with a round-trip test through `management_audit_outbox` → `GET /management/audit`,
  matching the existing `business_date.fallback_used` test.
- Guard that the detail projection stays an explicit allowlist (no `#[serde(flatten)]`,
  no `sql` field added to `KnowledgeDetailResponse`).
- Dashboard 1000-ceiling comment (`management/dashboard.rs`) naming it as the ceiling;
  a test that dashboard knowledge counts equal the loaded-catalog capability count.
- Fixture update `crates/chat/tests/fixtures/management/knowledge-detail.json` and
  `docs/current/management-client-integration.md` for any new event type.
- Commands: `cargo test -p chat` targeting the management tests, expected green.

**Open decision to surface at B11 spec review:** whether "unbounded query executed"
deserves its own event — W-L decision 2 recommends **no** (derivable from resolved
parameters on `execution.authorized`); confirm against what B6 shipped.

---

## Gate summary

| Bundle | Waits on | Cannot spec until |
| --- | --- | --- |
| B7 (W-D1) | B3, B4 | `analyst-question-inventory.md` exists with real rows; B4 catalog ids final |
| B8 (W-A3 + W-D2) | B7 | B7's retrieval suite green, gap ledger produced |
| B9 (W-G + W-J + F4 + F6) | B8 | B8 catalog (columns, kinds, currency SQL, sensitivities) final |
| B11 (W-L) | B8, B6 | B8 catalog final **and** B6's clamp/timeout events known |

Each bundle's full spec/plan begins with a fresh code audit of the exact files it
touches (roadmap "re-audit at every spec" rule). Nothing above is task-level detail that
depends on an unbuilt output — those are deliberately deferred to the gated spec cycle.
