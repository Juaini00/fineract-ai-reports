# Next Work

## Recommended next development order

1. Keep docs/status honest: semantic assistant foundation exists, but the full migration is incomplete.
2. Implement source-intent snapshots in clarification payloads/session memory.
3. Merge selected clarification options with original `AssistantIntent` constraints/entities/context.
4. Promote `rig-core` structured routing/planning/prose boundary into the real runtime path.
5. Introduce `petgraph` topology validation for assistant graph transitions and checkpoints.
6. Build the explicit session context window with soft/hard limits and warnings.
7. Limit `swiftide` to offline knowledge ingestion/indexing; keep runtime SQL execution Rust-owned.
8. Expand retrieval/evidence and tool execution coverage, including client lookup.
9. Remove or quarantine ad-hoc fallback/manual glue from the primary runtime.
10. Expand scenario/golden acceptance for domain, context, follow-up, clarification, policy, and response contracts.

## Documentation follow-up

1. Review split docs against `docs-old/`.
2. Remove or update stale references to older 16/16 catalog snapshots, classifier-first runtime flow, or completed semantic migration claims.
3. Keep `docs/current/status.md` updated after each completed acceptance gate.
4. Move issue 002 only after the full-brain migration gates pass.

## Current blockers

- Full semantic assistant brain is not implemented yet: `rig-core`, `petgraph`, source-intent clarification, and scenario acceptance are still active work.
- Typed catalog schemas are still pending for some generic knowledge layers.
- Loan/accounting/tax capability work is blocked by deferred data-scope status.
- LQR is implemented but not default until scenarios pass with `LQR_ENABLED=true`.
