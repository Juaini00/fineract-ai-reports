# Documentation Restructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a focused `docs/` tree that preserves the old documentation content while making current state and relationships easier for humans and agents to read.

**Architecture:** Documentation is split by responsibility. Current state is short and authoritative; old detailed context is preserved in split section files.

**Tech Stack:** Markdown documentation, existing `docs-old/`, no Rust code changes.

## Global Constraints

- Do not delete or modify `docs-old/`.
- Do not change runtime code, YAML, SQL, or migrations.
- Preserve old content in the new docs tree for review.
- Keep files focused by responsibility.

---

### Task 1: Create entrypoint and current context

**Files:**
- Create: `docs/index.md`
- Create: `docs/current/status.md`
- Create: `docs/current/active-context.md`
- Create: `docs/current/next-work.md`

**Steps:**
- [x] Create a top-level docs index with the recommended reading path.
- [x] Create current status with completed, partial, pending, and known sync notes.
- [x] Create active context with architecture, runtime, security, and docs rules.
- [x] Create next-work priorities.

### Task 2: Preserve old long-form content as split docs

**Files:**
- Create: `docs/roadmap/phases/*`
- Create: `docs/knowledge/catalog/*`
- Create: `docs/architecture/*`
- Create: `docs/product/*`

**Steps:**
- [x] Split monolithic old docs by top-level `##` sections.
- [x] Add source links back to `docs-old/`.
- [x] Add index files for each split document.

### Task 3: Copy focused legacy folders

**Files:**
- Create/copy: `docs/scenarios/*`
- Create/copy: `docs/reporting-data/*`
- Create/copy: `docs/superpowers/specs/*`
- Create/copy: `docs/superpowers/plans/*`

**Steps:**
- [x] Copy scenario verification docs.
- [x] Copy per-area reporting data docs.
- [x] Copy existing spec and plan history.

### Task 4: Add workflow docs

**Files:**
- Create: `docs/issues/README.md`
- Create: `docs/issues/active/*`
- Create: `docs/decisions/README.md`
- Create: `docs/superpowers/README.md`

**Steps:**
- [x] Define active/resolved issue ownership.
- [x] Move existing issue docs into active issues.
- [x] Add ADR format.
- [x] Add spec/plan workflow.

### Task 5: Validate documentation migration

**Steps:**
- [x] Confirm new docs files exist.
- [x] Confirm `docs-old/` still exists.
- [x] Confirm old major docs have split indexes.
- [x] Confirm no Rust/source runtime files changed.
