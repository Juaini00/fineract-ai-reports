# 04 — Drop redundant domain strict filter

**Parent:** [Epic](./README.md) · **Priority:** P1 · **Effort:** XS (~10 LoC)

## Problem

`crates/chat/src/assistant/retrieval.rs:134-137`:

```rust
fn domain_compatible(plan: &RetrievalPlan, domain: &str) -> bool {
    matches!(plan.domain, crate::assistant::AssistantDomain::Unknown)
        || format!("{:?}", plan.domain).eq_ignore_ascii_case(domain)
}
```

Domain is already implied by `subject`: `subject=client` ⇒ domain client, `subject=office|organization_hierarchy` ⇒ domain organization, `subject=savings_*` ⇒ domain savings. The separate domain check adds a failure point without a distinct signal.

Real failure captured in log:
```
router intent domain=Savings request_shape=RequestShape { subject: Client, ... }
compatible_ids=Some([])
```
Shape was correct. Domain was noun-driven ("savings account" in the sentence). Every `client` capability was filtered out by `domain_compatible`, and every `savings` capability was filtered out by `subject` mismatch. Zero survivors.

## Proposed change

Remove `domain_compatible` from the `compatible_ids` filter chain (or from whatever replaces it in issue 01). Subject remains as the primary domain signal.

Prompt still asks the LLM for `domain` — keep it for logging/analytics, but do not gate on it.

## Files

- `crates/chat/src/assistant/retrieval.rs` — remove `domain_compatible` call at line 126 (and the fn if unused).

## Acceptance criteria

- Query "top clients by savings account" with router-classified `domain=Savings` still finds `client_top_n_by_savings_account_count`.
- Existing tests pass. No new tests needed — this is a pure removal that widens matches.

## Test plan

- Manual replay of the query above using `curl` against `/chat/jobs`.
- Confirm log shows non-empty `compatible_ids` (before issue 01) or non-empty evidence (after issue 01).

## Out of scope

- Reasoning about which domain "owns" cross-subject queries. Subject wins by construction.

## Dependencies

- None. Ship first — cheapest, safest win. Can land before issue 01.
