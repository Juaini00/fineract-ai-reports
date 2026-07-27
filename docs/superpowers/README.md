# Specs And Plans

Significant implementation work should follow this path:

```text
idea/request
  -> docs/superpowers/specs/YYYY-MM-DD-topic-design.md
  -> docs/superpowers/plans/YYYY-MM-DD-topic.md
  -> implementation
  -> update the Status ledger below
  -> update docs/current/status.md
  -> update or close docs/issues/*
```

## Specs

Specs describe what should be built and why. They should include:

- problem
- goal
- non-goals
- design
- affected docs/code areas
- risks
- success criteria

## Plans

Plans describe how to implement the approved spec. They should include:

- files to create/modify
- ordered tasks
- validation commands
- review checkpoints

Existing migrated specs and plans remain in [`specs/`](./specs/) and [`plans/`](./plans/).

## Status ledger

**Purpose:** stop the agent guessing which work is done. This session repeatedly hit
stale docs (issue text calling shipped work "broken", a plan showing 0/114 checkboxes
after its first phases had landed). The ledger records what is implemented so we don't
re-audit or re-do finished work.

**Golden rule:** the ledger is a *pointer, not the truth — code is always the arbiter.*
The `Verified (vs code)` column is the date someone last confirmed the status against the
actual tree. A blank means the status is inferred (from git history / merged PR) and has
**not** been re-verified — treat it as "probably done, confirm before relying on it".

**Convention:** when you finish implementing a plan, flip its `Status` to `implemented`
and stamp `Verified (vs code)` with the date you confirmed it. When a plan is only partly
done, say which parts. When a spec is replaced, mark it `superseded → <newer doc>`.

Status vocabulary: `draft` → `approved` → `in-progress` → `implemented` → `superseded`.

| Date | Topic | Spec | Plan | Status | Verified (vs code) |
|---|---|---|---|---|---|
| 2026-07-04 | classifier semantic gap | — | ✓ | implemented (issue 001 resolved) | — |
| 2026-07-05 | contract test fixtures | ✓ | — | implemented (per git) | — |
| 2026-07-07 | full RAG blueprint (strict) | ✓ | ✓ | superseded → semantic-assistant-platform-migration | — |
| 2026-07-07 | RAG LQR overlay | ✓ | ✓ | implemented (per git; LQR present) | — |
| 2026-07-09 | event-driven audit trail | — | ✓ | implemented (per git) | — |
| 2026-07-11 | documentation restructure | ✓ | ✓ | implemented (per git) | — |
| 2026-07-11 | user auth / authorization | ✓ | ✓ | implemented (PR #11) | — |
| 2026-07-12 | semantic assistant platform migration | ✓ | ✓ | implemented (PR #12; issue 002 still open) | — |
| 2026-07-12 | DELETIONS | — | ✓ | implemented (per git) | — |
| 2026-07-14 | API key / session ownership | ✓ | ✓ | implemented (per git; issue 004 open) | — |
| 2026-07-14 | verified payload extraction | ✓ | ✓ | implemented (per git; issue 003 open) | — |
| 2026-07-15 | AI gateway state + auth redesign | ✓ | (phase-1, phase-2) | implemented (PR #15) | — |
| 2026-07-15 | phase-1 bearer-admin chat auth | — | ✓ | implemented (PR #15) | — |
| 2026-07-15 | phase-2 canonical gateway state | — | ✓ | implemented (PR #15) | — |
| 2026-07-15 | savings response correctness | ✓ | ✓ | implemented (per git) | — |
| 2026-07-17 | retrieval pipeline rework | ✓ | (phase1, phase2) | implemented (PR #13, #14) | — |
| 2026-07-18 | Rust project/module structure refactor | ✓ | ✓ | implemented | 2026-07-27 |
| 2026-07-19 | clarification continuation correctness | ✓ | ✓ | implemented (per git) | — |
| 2026-07-22 | unified agentic clarification contract | ✓ | ✓ | implemented (PR #17) | — |
| 2026-07-23 | management observability + audit | ✓ | ✓ | implemented (PR #18) | — |
| 2026-07-24 | LLM extraction gateway | ✓ | ✓ | **partial** — Phases 0–3 implemented; Phases 4–8 pending (= issue 007 Bundle 12 / W-C) | 2026-07-27 |
| 2026-07-27 | legacy module cleanup | ✓ | ✓ | implemented (DB-backed tests not run) | 2026-07-27 |

### Issue 007 program (analyst-grade knowledge) — see the roadmap for the live table

Master roadmap: [`plans/2026-07-27-issue-007-program-roadmap.md`](./plans/2026-07-27-issue-007-program-roadmap.md).
Loans split to [`docs/issues/active/008-loan-domain-analyst-capabilities.md`](../issues/active/008-loan-domain-analyst-capabilities.md).

| Bundle | Topic | Spec | Plan | Status | Verified (vs code) |
|---|---|---|---|---|---|
| 0 | charge_due_date hotfix | — | — | implemented (pre-shipped) | 2026-07-27 |
| 1 | gating decisions (W-M/W-K/W-N) | — | — | implemented (008 created) | 2026-07-27 |
| 2 | W-O F1/F2/F7 safety | ✓ | ✓ | planned — not executed (F7 already code-fixed) | 2026-07-27 |
| 3 | W-A1 analyst inventory | — | ✓ | planned — not executed | 2026-07-27 |
| 4 | W-A2/A4 + W-J savings catalog | ✓ | ✓ | planned — not executed | 2026-07-27 |
| 5 | W-B business date | ✓ | ✓ | planned — not executed | 2026-07-27 |
| 6 | W-I / F3 query budget | ✓ | ✓ | planned — not executed | 2026-07-27 |
| 7 | W-D1 retrieval suite | gated | outline | not planned — gated by 3 | 2026-07-27 |
| 8 | W-A3 / W-D2 | gated | outline | not planned — gated by 7 | 2026-07-27 |
| 9 | W-G / W-J / F4 / F6 | gated | outline | not planned — gated by 8 | 2026-07-27 |
| 10 | W-E / F8 clarification suppression | ✓ | ✓ | planned — not executed | 2026-07-27 |
| 11 | W-L management observability | gated | outline | not planned — gated by 8, 6 | 2026-07-27 |
| 12 | W-C extraction gateway (Phase 4–8) | (2026-07-24) | ✓ (continuation) | planned — not executed | 2026-07-27 |
| 13 | W-H / F5 drill-down prep | — | ✓ | planned — not executed | 2026-07-27 |
| 14 | W-F / W-N contract docs | — | ✓ | planned — not executed | 2026-07-27 |
