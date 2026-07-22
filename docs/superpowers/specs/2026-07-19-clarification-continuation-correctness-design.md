# Clarification Continuation Correctness Design

## Goal

Make `POST /chat/jobs/{job_id}/responses` behave correctly when clients always send both `option_id` and `message`, without losing the original report intent or entering repeated clarification loops.

## Behavioral contract

- `message` is always the user's authoritative text. `option_id` is only a routing discriminator.
- A regular option id must belong to the active clarification and selects that capability. The message may add trusted parameters, but an explicitly conflicting metric or domain must re-clarify instead of silently executing.
- `option_id = "others"` never becomes the semantic request text.
  - For capability-choice clarification, a meaningful message is rerouted immediately as a new request in the same turn.
  - For missing-execution-parameter clarification, the message continues the existing capability and supplies parameters.
  - Only an empty or boilerplate Others message produces the separate “Describe your request” prompt.
- Missing-parameter replies are recognized from the active clarification and deterministic extraction, not an arbitrary six-word limit.
- Invalid or unavailable option ids do not execute. Repeated unresolved clarification increments its attempt and reaches a bounded free-text recovery path rather than returning an identical payload forever.
- Existing requests that omit `option_id` remain supported.

## State and execution

- Preserve the source intent and already extracted constraints when a parameter reply arrives.
- Prefer current-turn parameter facts over source-turn facts, while retaining source facts not mentioned in the reply.
- Clear pending clarification only after a capability executes, an explicit new request is rerouted, or bounded recovery is entered.
- Keep authorization unchanged: only options available in the active payload and authorized capability set may execute.

## Compatibility and scope

- Keep the existing request schema (`message`, optional `option_id`) and response envelope.
- Do not add crates, generated SQL, auth changes, or new reporting capabilities.
- Do not add a new issue. Update current integration documentation to state the server-side semantics for clients that always send both fields.

## Acceptance scenarios

1. Missing `from_date` + `{option_id:"others", message:"this month"}` continues the same capability with the resolved month.
2. Capability-choice + `{option_id:"others", message:"Rank offices by savings transaction volume this month"}` reroutes that message immediately and never routes the literal word `others`.
3. Valid capability option + matching message executes or asks only for genuinely missing parameters.
4. Valid capability option + explicitly conflicting report text returns a conflict clarification and does not execute silently.
5. Invalid/stale option never executes and repeated attempts do not return an identical clarification indefinitely.
6. A parameter reply longer than six words remains a continuation.
7. The original transcript’s `this month` facts remain available after report selection; only the missing limit is requested.
