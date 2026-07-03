---
type: Knowledge Bundle
title: ai_report OKF Knowledge Bundle
description: Open Knowledge Format v0.1 bundle mirroring the runtime knowledge/*.yaml catalog in human-readable form.
tags: [okf, ai-reporting, fineract]
---

# ai_report Knowledge Bundle (OKF v0.1)

This directory follows the [Open Knowledge Format v0.1](https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf): one markdown file per concept, YAML frontmatter with `type:` required, cross-references via markdown links, `index.md` per category, `log.md` for changes.

Additive to — never replacing — the two other trees:

- `../docs/` — long-form prose, design notes, scenarios.
- `../knowledge/*.yaml` — runtime catalog consumed by `KnowledgeSyncService`.

Every concept here `resource:`-links back to the source of truth. Don't inline SQL/YAML — link, don't duplicate.

## Categories

- [capabilities/](./capabilities/index.md) — 9 approved MVP capabilities (all savings)
- [queries/](./queries/index.md) — 9 approved SQL queries (1:1 with capabilities)
- [metrics/](./metrics/index.md) — 5 savings metrics
- [policies/](./policies/index.md) — 5 cross-cutting guards (pii, office_scope, query_safety, execution_limits, unsupported_requests)
- [responses/](./responses/index.md) — 3 template sets (reporting, clarification, unsupported)
- [domains/](./domains/index.md) — 7 business domains (savings/client/organization approved; group_center candidate; loan/accounting/tax deferred)
- [data-areas/](./data-areas/index.md) — 13 Fineract data areas by scope
- [architecture/](./architecture/index.md) — how a chat message becomes a report answer
- [examples/](./examples/index.md) — 5 canonical end-to-end traces (bilingual EN/ID)
- [glossary](./glossary.md) — Fineract / microfinance business vocabulary
- [log.md](./log.md) — bundle changelog

## Growth

When a new capability, metric, or policy is approved in `../knowledge/`, add a matching concept file here. When something is retired, remove the concept and note it in `log.md`. Keep bodies short — link, don't duplicate.

## Tool leverage

- `understand-anything:understand` — build a knowledge graph over the repo, then compare with this bundle for coverage gaps.
- `jcodemunch` `get_file_outline` / `search_symbols` — surface Rust module structure before drafting new concepts.
- `lean-ctx` `ctx_read mode=signatures` — pull minimal excerpts from source files without loading full contents.

## Conventions

- Every file starts with YAML frontmatter with at minimum `type:`.
- `type:` values used: `Knowledge Bundle`, `Category`, `Capability`, `Query`, `Metric`, `Policy`, `Response`, `Domain`, `Data Area`, `Log`.
- Cross-refs use relative markdown links, not `[[wiki]]` syntax.
- Never inline SQL, YAML, or Rust — always `resource:` link the source.
