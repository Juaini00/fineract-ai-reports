# Issue 007 · Bundle 12 (W-C) — Extraction Gateway Continuation Plan

**Bundle:** B12 / W-C — LLM extraction gateway, Phases 4–8.
**Type:** continuation. This is NOT a new spec or a new full plan.
**Date:** 2026-07-27.

## What this document is

The design and implementation for W-C already exist and remain authoritative:

- **Spec (authoritative):** `docs/superpowers/specs/2026-07-24-llm-extraction-gateway-design.md`
  — read §§3–6 for the three-layer contract (LLM gateway → deterministic
  resolver → clarification decider), the per-parameter policy block, the
  default-expression DSL, and the `BusinessDateProvider` semantics.
- **Plan (authoritative task list):** `docs/superpowers/plans/2026-07-24-llm-extraction-gateway.md`
  — Phases 4–8 (Task 4.1 through Task 8.1) are the executable tasks for this
  bundle. Phase 9 (docs) rides along.

**Do not re-derive those tasks here.** This continuation only records three
things the 24-Jul plan cannot know: (1) how much of it already shipped, (2) the
path corrections the recent legacy-module cleanup forces, and (3) the ordering
constraint that this bundle is deliberately last among code bundles.

## Current state (verified 2026-07-27)

Audited the working tree directly. The 24-Jul plan shows `0/114` checkboxes;
that tracking is stale. Actual shipped state:

| Plan phase | Plan claim | Verified reality |
| --- | --- | --- |
| Phase 0–1 (BusinessDateProvider) | "to build" | **SHIPPED.** `crates/chat/src/assistant/temporal/business_date.rs` defines `BusinessDate`, `BusinessDateSource`, `BusinessDateError`, `BusinessDateProvider` trait, `StaticBusinessDateProvider`, `FineractBusinessDateProvider`, and `AuditingBusinessDateProvider`. Wired through `api/mod.rs`, `job/service/mod.rs`, `job/service/run.rs`, `management/model.rs`. |
| Phase 2 (parameter policy + DSL) | "to build" | **SHIPPED.** `crates/chat/src/knowledge/catalog/parameter_policy.rs` exists alongside `loader.rs`, `validator.rs`; `knowledge/model.rs` present. |
| Phase 3 (YAML migration) | "to build" | Treat as shipped with Phase 2 (roadmap groups this under W-A4 policy migration). Re-audit `knowledge/capabilities/**` at execution start before assuming. |
| Phase 7 Task 7.1 Step 1–2 (runtime context field) | "to build" | **PARTIALLY SHIPPED.** `CanonicalRuntimeContext` (`assistant/execution/runtime/mod.rs:86-87`) already carries `pub business_today: NaiveDate` and `pub business_date_source: BusinessDateSource`, populated in `job/service/run.rs`. |
| Phase 4 (Layer 1 gateway) | "to build" | **ABSENT.** No `assistant/understanding/gateway/`; no `GatewayClient` / `LlmGatewayExtraction` symbols anywhere. |
| Phase 5 (Layer 2 resolver) | "to build" | **ABSENT.** `assistant/understanding/resolver.rs` does not exist. |
| Phase 6 (Layer 3 decider) | "to build" | **ABSENT.** `assistant/understanding/decider.rs` does not exist. |
| Phase 7 Task 7.1 Step 3–4, Task 7.2 | "to build" | **ABSENT.** Gateway is not called from the runtime; deterministic extractors under `assistant/understanding/extraction/` are still the primary path. |
| Phase 8 (scenario tests) | "to build" | **ABSENT.** `crates/chat/tests/extraction_gateway_scenarios.rs` does not exist. |

**Net remaining work for this bundle:** Phase 4, Phase 5, Phase 6, the
still-unshipped portion of Phase 7 (gateway call insertion + demoting the
legacy extractor), Phase 8, and Phase 9 docs. Everything the 24-Jul plan lists
under Phases 0–3 and Phase 7 Task 7.1 Steps 1–2 is done — skip those tasks, do
not redo them.

## Path corrections from the legacy-module cleanup

The uncommitted cleanup in the working tree relocated modules. The 24-Jul plan
predates it. Apply these substitutions when executing Phases 4–8; everything
else in the plan's file paths is still correct (verified: `understanding/`,
`understanding/extraction/`, `understanding/clarification_resolver.rs`,
`understanding/classifier/mod.rs`, `understanding/intent.rs`,
`knowledge/catalog/*`, `knowledge/model.rs`,
`assistant/execution/runtime/{mod,execution}.rs`, `api/mod.rs` all still exist
at the paths the plan names).

1. **`legacy_pipeline/` is gone.** The cleanup moved
   `assistant/legacy_pipeline/{answer,model,parser,retrieval}.rs` →
   `assistant/llm/semantic/` and `assistant/legacy_pipeline/lqr.rs` →
   `assistant/understanding/lqr.rs`. The 24-Jul plan never references
   `legacy_pipeline` paths, so no task path breaks — but if any Phase-7 wiring
   needs the semantic answer/retrieval helpers, they now live under
   `assistant/llm/semantic/` (not `legacy_pipeline/`), and LQR is now
   `assistant/understanding/lqr.rs`.

2. **`JobService::run_graph_skeleton` does not exist** (plan Task 7.1 Step 2).
   The runtime entry and context construction now live in
   `crates/chat/src/job/service/run.rs`. `business_today` is already populated
   there — Task 7.1 Steps 1–2 are done; start Phase 7 at Step 3 (insert the
   gateway call into the entry step in `assistant/execution/runtime/`).

3. **`crates/chat/src/job/service/mod.rs`** (plan Phase 1 / File Structure) is a
   module directory (`job/service/{mod,run,events,clarification_response,…}.rs`),
   not a single file. The `BusinessDateProvider` injection is already present;
   no further edit needed there for this bundle.

4. **LLM client type is `SharedLlmClient = Arc<dyn LlmClient>`**
   (`assistant/llm/mod.rs:170`), backed by `TracedLlmClient`
   (`assistant/llm/traced_client.rs`). Plan Task 4.3 names `SharedLlmClient`
   correctly — confirmed, no change.

5. **`decide_from_scores` / classification policy** referenced by spec §5.5
   lives at `assistant/understanding/classifier/mod.rs` — confirmed present,
   reuse it unchanged. Do not reimplement candidate scoring.

6. **Phase 3 auto-migration binary** (`crates/chat/src/bin/migrate_capability_policies.rs`)
   is a throw-away already-executed step; do not create it. Phase 3's real
   acceptance is `cargo test -p chat --test catalog_validation` staying green.

## Ordering constraint — this bundle is deliberately last

W-C sits at position 12 of 14 in the roadmap. It is the last **code** bundle
before the prep/doc bundles (13, 14). This is intentional:

- **W-C must not block W-A..W-E.** Bundles 3–11 (inventory, savings catalog,
  business date re-point, budget, retrieval suite, catalog gaps, presentation,
  clarification guarantee, observability) ship first and independently. The
  gateway consumes the finalized capability catalogue and the per-parameter
  policy block; specifying it earlier would bake in a catalogue that later
  bundles still reshape.
- **Depends on:** Bundle 4 (savings catalog) per the roadmap dependency column.
  Do not begin Phase 4 execution until Bundle 4's plan is executed and green —
  the gateway's `catalog_summary` and the resolver's per-parameter defaults
  read the catalogue Bundle 4 finalizes.
- **Feeds:** Bundle 13 (W-H drill-down prep) depends on this bundle. Keep the
  gateway's `intent_kind` / candidate enums extensible so 13 can add reserved
  variants without a schema break.

## How to execute this bundle

1. Commit or stash the uncommitted legacy-cleanup working tree first (roadmap
   execution protocol step 5) so the W-C diff stays reviewable.
2. Re-audit before starting: confirm `gateway/`, `resolver.rs`, `decider.rs`
   are still absent and that Phases 0–3 remain shipped (this document's table).
3. Run `superpowers:executing-plans` (or `subagent-driven-development`) against
   `docs/superpowers/plans/2026-07-24-llm-extraction-gateway.md`, **starting at
   Task 4.1**, applying the path corrections above. Skip every task in Phases
   0–3 and Task 7.1 Steps 1–2 (already shipped).
4. Preserve all cross-cutting invariants (roadmap §"Cross-cutting invariants"):
   approved-SQL only; office scope bound in SQL via
   `office_ids = ANY($n::bigint[])`; SQL only in repositories; PII field-level
   gating; "today" = tenant business date (wall clock only for audit
   timestamps); sanitized errors; PostgreSQL durable / Redis live-SSE only;
   same-job clarification via `POST /chat/jobs/{job_id}/responses`; three crates
   unchanged; English-only copy. The gateway adds no new migration, no new
   `knowledge/queries` YAML surface, and no new crate.

## Decisions to confirm at spec-review

- Spec §12 open questions were deferred to planning; two now resolve from the
  verified tree — the Fineract business-date source and fallback observability
  are already implemented in `temporal/business_date.rs`
  (`FineractBusinessDateProvider` + `AuditingBusinessDateProvider`), so spec
  §12 bullets 1 and 3 are settled by shipped code. Confirm the executor treats
  the shipped provider as final and does not re-open the source lookup.
- Spec §7's seven worked examples remain the correctness bar, but some rows use
  loan capabilities that W-M moved to issue 008. Confirm at review whether the
  loan rows in §7 (`loan_arrears_clients`, `loan_repayments_today`,
  `loan_interest_recent`) are exercised with stubbed catalogue entries or
  dropped from the 007 scenario suite until 008 ships. This is the one scope
  decision the executor cannot make alone.
