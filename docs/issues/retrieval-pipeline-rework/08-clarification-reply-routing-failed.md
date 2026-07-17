# 08 — Clarification reply routes to `Routing failed` for valid option

**Parent:** [Epic](./README.md) · **Priority:** P1 · **Effort:** M · **Surfaced by:** phase 1 merge (broader candidate surface)

## Problem

Real session after phase 1 merged (2026-07-18):

Turn 1 — user asks:
```
"berikan 3 office yg ada pada system saat ini"
```

Assistant returns a clarification with 4 valid options:
```
- organization_office_hierarchy_tree  (label + description present)
- organization_office_summary          (label = id, no description)  ← Bug A
- savings_activity_list
- others
```

Turn 2 — user selects `organization_office_hierarchy_tree`:
```json
{ "selected_option_id": "organization_office_hierarchy_tree" }
```

Assistant response:
```json
{
  "response_type": "error",
  "title": "Routing failed",
  "message": "I could not route this request safely. Please try again."
}
```

The selected id is valid, exists in the catalog, and is in the caller's `allowed_capabilities`. Routing should succeed.

## Two bugs in one report

### Bug A — clarification option missing display_name/description

`organization_office_summary` renders with `label = "organization_office_summary"` (raw id) and `description = null`. All three other options render with human labels and prose descriptions. Either:

- The `organization_office_summary` capability YAML is missing `display_name` and/or `description` fields, or
- The clarification builder does not fall back to a humanized id when metadata is missing.

Both are low-effort fixes.

### Bug B — routing failure for valid option (the serious one)

After user selects a valid capability id, the runtime returns `ResponseBuilder::error("Routing failed")`. Path lives in `crates/chat/src/assistant/runtime/mod.rs` around the `ClarificationReply` block (~lines 315-410 in the pre-phase-1 layout):

1. `ClarificationResolver::resolve` — returns `Ok(ClarificationOutcome::SelectedOption { option_id })`.
2. Runtime calls `execute_selected_capability(memory, ..., option_id, catalog, client, fineract_pool, canonical, ...)`.
3. One of the steps inside returns an error → response builder emits the generic error.

Most likely root cause candidates:

- `verify_capability_metric` in `crates/chat/src/assistant/tool.rs` uses stale `deterministic_extraction` metadata when the user selects a different-metric capability via `option_id` after a clarification (already flagged in Task 2 report, not fixed there). Phase 1's broader candidate surface makes this path reachable more often.
- `execute_selected_capability` may enforce a `subject`/`shape` compatibility check against the original router intent — but the clarification target's shape can differ from the original intent by design.
- Auth/policy check on the selected capability may fail if the intent's `pii` flag doesn't match the capability's `pii` requirement (e.g., original intent was `pii=none`, but `organization_office_hierarchy_tree` needs office scope — should pass, but worth verifying).

## Reproduction

```bash
curl -s -X POST http://127.0.0.1:3007/chat/jobs \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $API_KEY" \
  -d '{"message":"berikan 3 office yg ada pada system saat ini"}' | jq '.data.id'

# Wait for status=waiting_for_user_input, then:

curl -s -X POST http://127.0.0.1:3007/chat/jobs/<job_id>/responses \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $API_KEY" \
  -d '{"selected_option_id":"organization_office_hierarchy_tree","source_message":"Return authorized offices as a walkable tree with parent_id and depth per node, ordered by path."}' | jq
```

Expected: job resumes and executes `organization_office_hierarchy_tree`. Actual: `Routing failed`.

## Proposed change

Two parts, in order:

**Part 1 — Bug A (display metadata):**
- `crates/chat/src/assistant/clarification.rs` (or wherever `ClarificationOption` is built) — fallback:
  ```rust
  label: cap.display_name.clone().unwrap_or_else(|| humanize_id(&cap.id)),
  description: cap.description.clone(),  // description=null is acceptable, don't fabricate
  ```
- Add `display_name` and `description` to `knowledge/capabilities/organization/office_summary.yaml` (audit all capabilities and file follow-ups for any others missing).

**Part 2 — Bug B (routing failure):**
Step 1: read `state_json.retrieval_trace` (added by phase 1 issue 06) after reproducing the failure — should show the clarification-reply's replan attempt.

Step 2: instrument the `ClarificationReply` block in `runtime/mod.rs` with `tracing::warn!` at every `Err` branch inside `execute_selected_capability`. Reproduce, read log, root-cause.

Step 3: fix at the identified layer:
- If `verify_capability_metric`: refresh from turn's current message, not memory snapshot.
- If shape/pii mismatch: allow clarification target to override the original intent's shape (that's the whole point of clarification).

Add a regression test to `crates/chat/tests/chat_full_flow.rs`:
```rust
#[test]
fn clarification_selecting_hierarchy_tree_from_office_query_routes_successfully() {
    // Reproduces the 2026-07-18 Turn 2 failure.
}
```

## Acceptance criteria

- Bug A: `organization_office_summary` clarification option renders with a human-readable label. Any other capability missing metadata is either fixed (YAML) or falls back gracefully.
- Bug B: Selecting `organization_office_hierarchy_tree` after "berikan 3 office" returns a populated result, not `Routing failed`.
- Regression test committed and green.
- No auth boundary widened (caller must still have `organization_office_hierarchy_tree` in `allowed_capabilities`).

## Out of scope

- Reranker changes (issue 02). This bug is in the clarification-reply execution path, not selection.
- Multi-turn conversational memory changes.

## Dependencies

- Reuses `state_json.retrieval_trace` (issue 06 — landed in phase 1) for debugging.
- Overlaps thematically with issue 02 (reranker) — a stronger reranker makes fewer clarifications necessary, which reduces Bug B's blast radius but does not fix it.

## Prior evidence

- Task 2 report (`.superpowers/sdd/task-2-report.md`) already flagged the `verify_capability_metric` stale-metadata bug as pre-existing, out-of-scope-for-phase-1.
- Phase 1 whole-branch review (final) explicitly named this failure as a phase-1-surfaced but not phase-1-caused issue, deferring it here.
