---
type: Log
title: doc-knowledge changelog
description: Chronological changes to the OKF bundle under doc-knowledge/.
---

# Log

## [2026-07-03] enrich | Add narrative layers — glossary, request flow, traces

Added three business-narrative concepts that the pure YAML mirror was missing:

- [glossary](./glossary.md) — Fineract / microfinance vocabulary, PII vs secret_never_expose, auth model, chat/job lifecycle terms.
- [architecture/request-flow](./architecture/request-flow.md) — end-to-end lifecycle of `POST /chat/messages`, component boundaries, durability contract.
- [examples/](./examples/index.md) — 5 canonical bilingual (EN/ID) traces covering snapshot, date-bounded aggregate, conditional-PII top-N, clarification loop, and deferred-domain hard reject.

## [2026-07-03] populate | Full OKF v0.1 bundle from runtime catalog

Mirrored the entire runtime `knowledge/*.yaml` catalog into 59 OKF concept files across 7 categories: [capabilities](./capabilities/index.md), [queries](./queries/index.md), [metrics](./metrics/index.md), [policies](./policies/index.md), [responses](./responses/index.md), [domains](./domains/index.md), [data-areas](./data-areas/index.md). Every concept `resource:`-links the source YAML — no duplication.

## [2026-07-03] create | Seed OKF v0.1 bundle

Created `doc-knowledge/` as an additive OKF bundle. Neither `docs/` nor runtime `knowledge/*.yaml` were changed. Seeded [capabilities/](./capabilities/index.md) with `savings.balance_summary` as the first concept.
