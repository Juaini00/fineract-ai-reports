# Next Work

## Recommended next development order

1. Finish typed schema/metric/policy/response validation in the knowledge catalog.
2. Rebuild vector index after catalog changes and verify all 25 approved capabilities retrieve correctly.
3. Enable and verify LQR with scenarios 05, 06, 07, and 16 before making it default.
4. Add broader LLM prompt context consumption beyond clarification options.
5. Add response-formatting fallback for complex results.
6. Promote new non-savings domains only after data-scope and PII review.

## Documentation follow-up

1. Review split docs against `docs-old/`.
2. Remove or update stale references to older 16/16 catalog snapshots.
3. Keep `docs/current/status.md` updated after each completed feature.
4. Move resolved issue docs from `issues/active/` to `issues/resolved/` with a resolution note.

## Current blockers

- Typed catalog schemas are still pending for some generic knowledge layers.
- Loan/accounting/tax capability work is blocked by deferred data-scope status.
- LQR is implemented but not default until scenarios pass with `LQR_ENABLED=true`.
