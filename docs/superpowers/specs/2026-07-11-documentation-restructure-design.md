# Documentation Restructure Design

## Problem

The old `docs-old/` tree preserved important information but several files mixed multiple responsibilities. Large files such as `implementation-steps.md`, `knowledge-catalog.md`, and `ai-reporting-design.md` were hard for humans and agents to scan safely.

## Goal

Create a new `docs/` tree with focused folders and files while preserving old information for review before `docs-old/` is manually deleted.

## Non-goals

- Do not change Rust code.
- Do not change runtime YAML or SQL catalog files.
- Do not delete `docs-old/`.
- Do not rewrite product meaning beyond known current-status corrections.

## Design

The new docs tree is organized by responsibility:

- `current/` — short current state, active context, next work.
- `architecture/` — stable system design and split old architecture docs.
- `product/` — reporting scope, capabilities, PII, coverage.
- `runtime/` — running behavior and job-memory references.
- `knowledge/` — knowledge catalog explanation and validation relationships.
- `api/` — endpoint map linking to executable scenarios.
- `roadmap/` — implementation roadmap and split phase details.
- `issues/` — active/resolved problem records.
- `decisions/` — ADR-style decision records.
- `scenarios/` — copied manual verification flows.
- `superpowers/` — specs and implementation plans.

## Preservation strategy

Old multipurpose docs are split by top-level `##` sections. Each generated section file links back to `docs-old/<file>` and preserves the section body. Already-focused folders such as `scenarios`, `reporting-data`, and existing `superpowers` files are copied forward.

## Success criteria

- `docs/index.md` gives a clear start path.
- Current state is readable without opening a 1000-line file.
- Old long-form information is present in new split docs.
- Issues, specs, and plans have clear ownership.
- `docs-old/` remains untouched for manual review.
