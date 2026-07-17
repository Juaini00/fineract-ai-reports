# 01 — Invert retrieval: semantic first, shape as score

**Parent:** [Epic](./README.md) · **Priority:** P0 · **Effort:** S (~100 LoC)

## Problem

`crates/chat/src/assistant/retrieval.rs:22-25`:

```rust
let compatible = catalog.map(|catalog| compatible_ids(plan, catalog));
if compatible.as_ref().is_some_and(Vec::is_empty) {
    return Ok(Vec::new());
}
```

And lines 51-56:

```rust
if let Some(catalog) = catalog {
    evidence.extend(catalog_fallback(plan, catalog));
}
if let Some(compatible) = compatible {
    evidence.retain(|item| compatible.contains(&item.capability_id));
}
```

`compatible_ids` uses strict enum equality (`enum_compatible` at line 159). One dimension mismatch → filter returns `[]` → embedding search is skipped → decision is `UnsupportedInDomain`. Confirmed by log: `compatible_ids=Some([])` followed by `evidence_count=0`.

## Proposed change

Retrieval flow becomes:

1. Always run embedding hybrid search over ALL approved capabilities (status `approved_mvp` or `active`), limited to the caller's `allowed_capabilities` (auth boundary, kept).
2. Compute `shape_score` per candidate: fraction of shape dimensions that match (0.0-1.0). Domain skipped (see issue 04).
3. Combined score: `final = cosine * 0.6 + shape_score * 0.3 + hits * 0.1` (hits = keyword overlap already in `catalog_fallback`).
4. Return top-K sorted, never gated to empty.

Delete the early return and the final `retain`. Repurpose `compatible_ids` into `shape_score(plan, cap) -> f32` used by the scorer.

## Files

- `crates/chat/src/assistant/retrieval.rs` — refactor `retrieve`, replace `compatible_ids` with scoring helper.
- `crates/chat/src/assistant/evidence.rs` — `EvidenceEvaluator::evaluate` already reads `evidence[0].score`; threshold may need tuning (issue 02 will replace it entirely).

## Acceptance criteria

- Query "3 clients where have the most savings account" returns non-empty evidence with `client_top_n_by_savings_account_count` as top-1.
- Query with zero matching capabilities still returns evidence (top-K by cosine), lets downstream decide `Clarify` vs `Unsupported` — never dead-ends at retrieval.
- No auth regression: caller still cannot see capabilities outside their `allowed_capabilities`.
- Existing tests in `retrieval.rs` and `evidence.rs` pass after threshold tuning.

## Test plan

- Unit: score function with mismatched vs matched shapes on identical cosine → matched ranks higher.
- Integration: replay the 3 failing queries from the epic README, assert none return `unsupported` at retrieval stage.

## Out of scope

- Replacing `EvidenceEvaluator` (issue 02).
- Adding new capabilities (issue 03).

## Dependencies

- None. Can ship independently.
