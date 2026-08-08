# Issue 012 — legacy baseline (Phase 0)

Measured at commit `d99d898`, 2026-08-04. Produced by Task 0.1 of
`docs/superpowers/plans/2026-08-04-agentic-workflow-runtime.md`.

This file is what the Phase 7 deletion gate is diffed against. A count that is already
`0` here must **stay** `0` — a later non-empty result is a regression, not a new finding.

## Measurement method

All counts use `command grep -rEn`, never a bare `rg`.

**Why:** this machine's shell wraps `rg` and `wc` in a helper (`_lc`) that can fail and
report **zero hits for a pattern that does match**. It was caught during this measurement:
a bare `rg` reported `ExecutionPlanType` as absent from `crates/` when the symbol has 25
occurrences. Any gate run through the wrapper can silently pass while the legacy code is
still there.

```sh
count() { command grep -rEn "$1" "$2" 2>/dev/null | command wc -l | tr -d ' '; }
```

## Legacy symbol counts

| ID | Pattern | Scope | Baseline hits |
| --- | --- | --- | ---: |
| V-L1 | `ExecutionPlanType\|build_execution_plan` | `crates/` | 25 |
| V-L2 | `defaultless_missing_fields` | `crates/` | 7 |
| V-L3 | `selected_capability = Some\(option_id` | `crates/` | 2 |
| V-L4 | `client_name_lookup.*client_relationship_lookup` | `crates/` | 1 |
| V-L5 | `capability_id\.as_str\(\)` | `crates/` | 8 |
| V-L6 | `deterministic_simple_response` | `crates/` | 2 |
| V-L7 | `AI_REPORT_GATEWAY_PIPELINE\|run_via_gateway_pipeline\|route_via_gateway_pipeline` | `.` | 8 |
| V-L8 | `CanonicalGatewayMode\|CHAT_CANONICAL_GATEWAY_MODE` | `.` | 28 |
| V-L9 | `SemanticRouter\|ClassificationResult\|ClassificationOutcome` | `crates/` | 40 |
| V-L10 | `AssistantGraphRuntime` | `crates/` | 23 |
| V-L11 | `knowledge::dataset::legacy` | `crates/` | 1 |
| V-L12 | `reqwest` | `crates/chat/src/assistant/llm/` | 8 |
| V-L13 | `size_of::<rig_core` | `crates/` | 1 |
| V-L14 | `swiftide` | `.` | 14 |
| V-L15 | `Swiftide` | `crates/` | 14 |
| V-L16 | `AssistantGraphTopology` | `crates/` | 20 |
| V-L17 | `phase0_rig_poc` | `crates/chat/examples/` | 1 |
| V-L19 | `one query\|single query\|atomic execution` | docs + `AGENTS.md` + `CLAUDE.md` | 0 |
| V-L20 | `capability_id\|output_mode ==` | `crates/chat/src/assistant/presentation/` | 2 |
| V-L21 | `MIN_SELECT_CONFIDENCE\|RerankerDecision::clarify` | `crates/` | 5 |

**V-L19 is 0 at baseline and that is not evidence of anything.** The docs describe
one-query terminal execution in prose this pattern does not match. L19 stays a manual doc
review in the Phase 7 PR; the grep is a tripwire, not the check.

**V-L20 is 2 and both are benign** — `presentation/builder.rs:183` (a message template
naming `{capability_id}`) and `:759` (a literal `output_mode: "list"`). No capability-ID
behaviour switch exists in the presentation layer. The row stays in the inventory so
Phase 7 proves this rather than assuming it.

## Runtime flags in `crates/*/src`

`command grep -rEn 'env::var' crates/app/src crates/core/src crates/chat/src` → 4 hits:

| Location | Verdict |
| --- | --- |
| `core/src/config/mod.rs:325` | config helper — allowed to survive |
| `core/src/config/mod.rs:329` | config helper — allowed to survive |
| `chat/src/assistant/execution/runtime/mod.rs:592` | `AI_REPORT_GATEWAY_PIPELINE` — **must be gone** (V-L7) |
| `chat/src/execution/repository.rs:513` | `FINERACT_DATABASE_URL` inside a test-only fallback — allowed |

Phase 7 target: 3 hits, none selecting a runtime.

## Catalog counts

| Resource | Baseline |
| --- | ---: |
| Capabilities (`knowledge/capabilities/**.yaml`) | 41 |
| Query contracts (`knowledge/queries/**.yaml`) | 41 |
| Datasets (`knowledge/datasets/**.yaml`) | 4 |

Phase 2 adds 7 datasets → expected 11. Capability count is **not** a gate: tests asserting
a fixed catalog count are themselves legacy (plan Task 7.4).

## Test volume facing Phase 7

| File | Lines |
| --- | ---: |
| `assistant/execution/runtime/tests.rs` | 1742 |
| `assistant/execution/tool/tests.rs` | 888 |

These two files are the bulk of Task 7.4. They are rewritten to behaviour and security
contracts as each source deletion lands, not all at the end.

## Operational metrics baseline

**Not measured — no baseline.** Spec §17 lists 13 metrics; none has a recorded
pre-migration value, because the local `chat_jobs` / `chat_job_events` history is
development traffic and not representative of production behaviour.

Recorded explicitly rather than left blank, per plan Task 0.1. Consequence for Phase 8:
the two gating metrics (**wrong-answer rate**, **unauthorized-data rate**) must be
measured from the A1–A7 acceptance runs on a production-like database, not compared
against a nonexistent baseline. The remaining 11 metrics are observational only for this
migration.
