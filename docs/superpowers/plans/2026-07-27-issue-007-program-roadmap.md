# Issue 007 — Execution Program Roadmap

**Purpose:** Turn the 14-workstream issue `docs/issues/active/007-analyst-grade-knowledge-and-request-mapping.md` (2659 lines) into an executable program: a dependency-ordered queue of spec→plan cycles, grounded against the **current code**, not the issue text.

> **Why this exists and is not one giant plan.** Issue 007 states every workstream is independently executable, and its own suggested order shows that early *decisions* gate the *content* of later specs (e.g. the loan-scope decision W-M bounds what W-A3 means). A single monolithic spec would be unexecutable and would bake in wrong scope. So "complete coverage" = this roadmap + one spec→plan cycle per bundle, executed in order with a check-in between.

## ⚠️ The issue text is stale — re-audit at every spec

Verified 2026-07-27 against the working tree. Several claims in 007 no longer hold:

- **W-A2 is effectively DONE.** `savings_pending_charges_clients` already exports `days_overdue`, `amount_paid`, `amount_waived`, `amount_written_off`, `amount_due_current`, `amount_outstanding`, `charge_timing_enum` (query YAML + `queries/savings/pending_charges_clients.sql`). Issue evidence E3 ("these fields are missing") is out of date.
- **Phases 0–3 of the extraction-gateway plan shipped** (per 007's commit table) but that plan shows 0/114 checkboxes ticked — its tracking is stale too.

**Rule for every bundle below:** its spec cycle begins with a fresh code audit of the exact files it touches. Trust the code, then the issue, never the issue alone.

## Grounded status (2026-07-27)

| WS | Scope | Type | Status (verified) |
| --- | --- | --- | --- |
| W-A1 | Analyst question inventory doc (≥25) | doc | **TODO** — `docs/product/analyst-question-inventory.md` absent |
| W-A2 | Enrich `savings_pending_charges_clients` | code | **DONE-ish** — fields present; re-verify `amount_original` naming + `days_overdue` clamp convention only |
| W-A3 | Close catalog gaps A1 finds | code | **TODO** — gated by W-A1 + W-M |
| W-A4 | Per-capability default review (E4) | code/YAML | **PARTIAL** — policies migrated uniformly; human per-cap review pending |
| W-B | Relative temporal → business date | code | **PARTIAL** — `business_today` wired; `kemarin/minggu lalu/…` re-point pending |
| W-C | LLM gateway 3-layer (Phase 4–8) | code | **TODO** — `gateway/`,`resolver.rs`,`decider.rs` absent; partial plan `2026-07-24-llm-extraction-gateway.md` Phase 4–8 |
| W-D | Bilingual retrieval regression suite | test | **TODO** — gated by W-A1 |
| W-E | Clarification suppression guarantee | test | **PARTIAL** — `params_from_verified` defaults land; runtime-level guard + validator rule pending |
| W-F | Frontend contract docs (backend side) | doc | **TODO** |
| W-G | Analyst-grade presentation/rendering | code | **TODO** — `output_mode` not read by presentation |
| W-H | Drill-down preparation only | prep | **TODO** — decision: out of scope, keep enum extensible |
| W-I | `hard_cap`/`timeout_ms`/backstop | code | **TODO** — `hard_cap` only parsed (stale comment `parameters.rs:299`); query `timeout_ms` not loaded |
| W-J | Currency & money semantics | code | **TODO** — no currency-aware rendering; `Money` kind never produced |
| W-K | Export | decision | **TODO** — record as non-goal + named follow-up |
| W-L | Mgmt observability alignment (006) | code/test | **TODO** — gated by W-A3, W-I |
| W-M | Loan domain scope decision | decision | **TODO** — recommend split to issue 008 (zero loan capabilities today) |
| W-N | Frontend dependency | decision/doc | **TODO** — record + create `ai_report_dashboard` issue link |
| W-O F1–F8 | Latent contract violations | code | **MIXED** — ride with host workstreams (see order) |

## Execution program (follows issue §"Suggested execution order")

Each **bundle** = one brainstorm→spec→plan→execute cycle. Decision/doc bundles are lightweight (a recorded decision, no code). `spec?` = whether it warrants a full design spec vs a short plan.

| # | Bundle | Contains | Type | Depends on | spec? |
| --- | --- | --- | --- | --- | --- |
| 0 | **`charge_due_date` hotfix** | Open-Q #2 one-predicate fix | hotfix | — | no (direct fix + test) |
| 1 | **Gating decisions** | W-M (loan→008), W-K (export non-goal), W-N (frontend dep) | decision/doc | — | no (record in issue + create 008/dashboard issue) |
| 2 | **Safety pre-catalog** | W-O F1 (PII gate), F2 (`hard_cap` enforce), F7 (409 vs 404) | code | — (parallel w/ 1) | yes (small, safety) |
| 3 | **W-A1 inventory** | analyst-question-inventory.md ≥25 | doc | 1 (scope) | no (authoring doc) |
| 4 | **Savings catalog** | W-A2 verify-close, W-A4 defaults, W-J {1,4,5} in the SQL rewrite | code | 3 | yes |
| 5 | **W-B business date** | relative-expr re-point + tests (parallel w/ 4) | code | — | yes |
| 6 | **W-I budget** | global backstop, truncation warning, F3 `timeout_ms` load | code | 4 | yes |
| 7 | **W-D1 retrieval suite** | bilingual assertions over W-A1 | test | 3,4 | no (plan) |
| 8 | **W-A3 + W-D2** | close catalog+scoring gaps D1 finds | code | 7 | yes |
| 9 | **W-G + W-J rest + F4 + F6** | presentation, money format, output_mode, cell escaping | code | 8 | yes |
| 10 | **W-E + F8** | clarification guarantee both directions | test/code | 4 | yes |
| 11 | **W-L** | mgmt observability over final catalog + new events | code/test | 8,6 | no (plan) |
| 12 | **W-C** | gateway→resolver→decider Phase 4–8 (continue 24-Jul plan) | code | 4 | reuse+extend existing spec |
| 13 | **W-H + F5** | drill-down prep (enum extensibility, reserved variants) | prep | 12 | no (plan) |
| 14 | **W-F1/F2 + W-N docs** | contract docs, cross-repo link | doc | 4,9 | no |

Loans → **separate issue 008** (W-M decision **LOCKED 2026-07-27**); 007 stays savings+client. Bundle 1 must create issue 008 with the W-M "must contain" list (A.2.1 priority order, office-scope per ownership, arrears-vs-schedule choice, `loan_status_id` confirmation, `days_in_arrears` clamp, A.2.5 due-date caveat, `m_delinquency_range` buckets, inheritance of W-G/I/J/L). W-A1 still enumerates loan questions marked `missing` so the gap stays visible from 007.

## Progress log

- **Bundle 0 — DONE (already shipped).** `queries/savings/pending_charges_clients.sql` already uses the A.1.4 predicate (four flags + office scope); the debt-hiding `charge_due_date <= $2` filter is gone, and `days_overdue` uses the clamp-at-zero / NULL convention. Residual (folded into Bundle 4, not the hotfix): no DB-backed regression test guards the row set, and `amount_levied_total` (A.1.3 Finding 1) is not yet shipped.
- **Bundle 1 — DONE.** W-M locked → `docs/issues/active/008-loan-domain-analyst-capabilities.md` created; 007's loan open-question marked resolved. W-K (export) already recorded in 007 Non-goals. W-N: 007 already documents backend-independent resolution. **Outstanding W-N action (yours, cross-repo):** open a matching issue in `ai_report_dashboard` for the `{from,to}` date-range control and link its id into 007 E5.

## Document + execution status (2026-07-27, post spec/plan fan-out)

All specifiable bundles now have their spec+plan; gated bundles have outline docs. **Nothing below is executed** except Bundles 0 and 1.

| Bundle | Spec | Plan | Executed? | Notes |
|---|---|---|---|---|
| 0 | — | — | ✅ | `charge_due_date` hotfix already shipped |
| 1 | — | — | ✅ | 008 created; W-K/W-N decisions recorded |
| 2 (F1/F2/F7) | `…-b2-safety-pii-hardcap-status-design.md` | `…-b2-safety-pii-hardcap-status.md` | ❌ | F7 already code-fixed → verify + 1 test; 4 open decisions flagged |
| 3 (W-A1) | — (doc task) | `…-b3-analyst-question-inventory.md` | ❌ | 12-question starter seeded in plan |
| 4 (W-A2/A4/W-J) | `…-b4-savings-catalog-currency-design.md` | `…-b4-savings-catalog-currency.md` | ❌ | W-A2 mostly done; real work = amount_levied_total, A4 defaults, currency |
| 5 (W-B) | `…-b5-business-date-everywhere-design.md` | `…-b5-business-date-everywhere.md` | ❌ | parallel with 4 |
| 6 (W-I/F3) | `…-b6-query-budget-timeout-design.md` | `…-b6-query-budget-timeout.md` | ❌ | global backstop + truncation + timeout loading |
| 7 (W-D1) | gated | outline in `…-gated-outlines-b7-b8-b9-b11.md` | ❌ | finalize after Bundle 3 |
| 8 (W-A3/W-D2) | gated | outline (same file) | ❌ | finalize after Bundle 7 |
| 9 (W-G/W-J/F4/F6) | gated | outline (same file) | ❌ | finalize after Bundle 8 |
| 10 (W-E/F8) | `…-b10-clarification-suppression-design.md` | `…-b10-clarification-suppression.md` | ❌ | |
| 11 (W-L) | gated | outline (same file) | ❌ | finalize after Bundles 8 + 6 |
| 12 (W-C) | `specs/2026-07-24-llm-extraction-gateway-design.md` (existing) | `…-b12-extraction-gateway-continuation.md` (+ existing 24-Jul plan) | ❌ | last among code bundles |
| 13 (W-H/F5) | — (prep) | `…-b13-drilldown-preparation.md` | ❌ | prep only |
| 14 (W-F/W-N) | — (docs) | `…-b14-frontend-contract-docs.md` | ❌ | needs `ai_report_dashboard#TBD` issue (yours) |

**Not-yet-executed queue (dependency order):** 2 → 3 → 4 ∥ 5 → 6 → 7 → 8 → 9 → 10 → 11 → 12 → 13 → 14.

### Open decisions to confirm at review (from the fan-out)
- **B2-1 `client_id` sensitivity** → recommend `public_business` everywhere (only `client_display_name` gated). Alternative flips 6 savings YAMLs to `pii`.
- **B2-2** delete vs keep the inert capability-level `pii:`/`output_fields:` blocks → recommend delete.
- **B2-3** `hard_cap` clamp audit shape → recommend `tracing::warn!` + clamped value in plan snapshot (durable audit row deferred to B6).
- **B2-4** new F7 test → tolerant same-job/non-404 assertion (does not pull W-E turn-1 behavior forward).
- **B3-1** W-A4 default table stays in Bundle 4, not the inventory doc.
- **B3-2** inventory may surface 2 savings `partial` rows (feed to W-A3) rather than forcing all to `covered`.

## Execution protocol

1. Do bundle 0 and bundle 1 first — they are cheap and they change what later bundles mean.
2. For each code bundle in order: run `superpowers:brainstorming` (fresh code audit → design) → `superpowers:writing-plans` → execute → check-in. One bundle per session-slice; do not batch.
3. Bundles 4 and 5 may run in parallel (disjoint files). So may bundle 1 and 2.
4. Never spec a bundle before its dependency's plan is executed and green.
5. Keep the legacy-cleanup change (already in the working tree, uncommitted) separate from 007 work — commit or stash it first so 007 diffs stay reviewable.

## Cross-cutting invariants (apply to every bundle)

Approved-SQL only (no AI SQL); office scope bound in SQL via `office_ids = ANY($n::bigint[])`, never Rust post-filter; PII field-level gating; "today" = tenant business date, wall-clock only for audit; sanitized errors; PostgreSQL durable / Redis live-only; English-only copy; three crates unchanged. Full list: issue §"Cross-cutting constraints" (line 2260) and §"Overall acceptance criteria" (line 2281).
