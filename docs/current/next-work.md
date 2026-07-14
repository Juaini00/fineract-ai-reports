# Next Work

## Recommended next development order

1. Implement API key/session ownership from issue 004 so sessions are user-owned and jobs use selected API-key scope.
2. Continue verified payload extraction from issue 003, including provenance, conflicts, catalog/query required-param validation, and clarification gates.
3. Keep semantic assistant scenario/golden coverage green in CI and live-gated checks.
4. Add fixtures for more real Fineract client/report rows where environments can safely provide them.
5. Tighten typed catalog schemas for remaining generic knowledge layers.
6. Promote deferred loan/accounting/tax domains only after data-scope docs and approved SQL are ready.
7. Evaluate enabling LQR by default after semantic assistant scenarios pass with `LQR_ENABLED=true`.

## Documentation follow-up

1. Review split docs against `docs-old/`.
2. Remove or update stale references to older 16/16 catalog snapshots, classifier-first runtime flow, or accepted semantic migration claims.
3. Keep `docs/current/status.md` updated after each completed acceptance gate.
4. Move issue 002 after final workspace validation is accepted.

## Current blockers

- Typed catalog schemas are still pending for some generic knowledge layers.
- API key/session ownership is conceptually wrong for multi-key users; issue 004 tracks the migration to user-owned sessions plus job-level API-key scope.
- Verified payload extraction is not complete; issue 003 tracks the required provenance, semantic agreement, clarification, and execution gates.
- Loan/accounting/tax capability work is blocked by deferred data-scope status.
- LQR is implemented but not default until scenarios pass with `LQR_ENABLED=true`.
