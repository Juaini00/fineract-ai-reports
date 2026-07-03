---
type: Category
title: Architecture
description: How a chat message becomes a report answer. Component boundaries, request lifecycle, and durability contract.
tags: [architecture]
---

# Architecture

- [request-flow](./request-flow.md) — end-to-end lifecycle of a `POST /chat/messages` call
- Layer order invariant: `route → service → repository → database`. No `sqlx` in handlers or services.
- Crates: `app` (composition root) → `core` (foundation) → `chat` (feature). No `crates/knowledge`, no `crates/reporting`.
