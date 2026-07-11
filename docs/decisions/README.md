# Architecture Decisions

Use this folder for short ADR-style records. Long design belongs in `docs/architecture/`; current progress belongs in `docs/current/`.

## Format

```text
# Decision title

Status: accepted | superseded
Date: YYYY-MM-DD

## Context
## Decision
## Consequences
```

## Initial decisions to extract when needed

- Three crates only: `app`, `core`, `chat`.
- No runtime SQL generation by LLM.
- PostgreSQL is durable job state; Redis is live coordination only.
- Knowledge YAML and approved SQL are controlled runtime context.
- Office scope is enforced in SQL, not post-filtered in Rust.
