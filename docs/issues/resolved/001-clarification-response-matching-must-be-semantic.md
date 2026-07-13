# 001 — Clarification response matching must be semantic, not literal

Status: resolved
Resolved: 2026-07-13

## Resolution note

The semantic assistant graph now models clarification replies as assistant intents, keeps pending clarification in session context, and resolves replies through `ClarificationResolver` instead of the old classifier-first/pending-intent path. Phase 9 adds the scenario matrix coverage for `yang balance aja` and documents the graph runtime as current behavior.

## Summary

The pipeline must interpret a user's clarification reply by **meaning**, not by string equality against option labels. A literal-match approach traps users into repeating the exact label, causes infinite clarification loops when they don't, and does not align with the RAG blueprint's principle that the LLM is the reasoning component.

## Symptom

Session `73baf4a2-…` (Jul 10 2026): user asked "show 10 clients with the most savings accounts" — an ambiguous request. System offered three options, user replied "Top Clients by Savings Balance" (exact label). System failed to match, re-emitted clarification. User then tried "client top n by savings account count" (the fallback label from round 2). Still no match. Loop persisted across five rounds until the session was abandoned.

Two mechanical defects contributed:

1. Round-1 labels came from `capability_display_label` (nice title-case). Round-2 labels came from `id.replace('_', ' ')` (snake→space), because the pending-intent path did not consult the catalog. Users saw two different vocabularies for the same options.
2. `select_capability` performed only a substring check `response.contains(id)`. The user's reply used the *label*, so no substring of the raw id ever appeared.

Immediate fix (already merged) removed the literal-only comparison and added multi-strategy matching (numeric pick, catalog label, id, bidirectional substring). This unblocks the loop for well-behaved replies but does not satisfy the deeper requirement below.

## Root cause

Clarification is currently a **string-alignment** problem in code, when it should be an **intent-alignment** problem for the reasoning layer. The system encodes the assumption that a user will echo an option verbatim. That assumption is wrong for:

- Users who paraphrase ("give me the savings balance one", "the balance one, please").
- Users who select semantically ("show balances instead", "actually I want deposits").
- Users who type a new request entirely ("Others: show top 5 by deposit last month").

There is currently no first-class "Others / free-form" affordance surfaced in the options payload — the `OTHER_ACTIVITY_CAPABILITY` sentinel exists but is only injected in a narrow branch of the savings-activity classifier, not for LQR-driven clarifications.

## Blueprint alignment

- **Section 4 — Ambiguity Detection.** The pipeline correctly detects ambiguity (gap between top-1 and top-2 confidence). It fails at the *next* step: interpreting the disambiguating reply.
- **Section 2 — Intent Routing.** The clarification reply is itself an intent that must be routed. Options: (a) selection of a prior candidate, (b) refinement of the prior request with new slot values, (c) a fresh request that supersedes the prior one.
- **Design Principle 1** — "LLM is a reasoning component, not the workflow controller." String equality is neither. Semantic matching is a reasoning task; it belongs to a small, deterministic call to the LLM (or a local embedding similarity check) inside a backend-owned controller.

## Requirements this issue must satisfy

1. **Semantic alignment.** Match user reply → candidate by *meaning*. "Top clients by savings balance", "the balance one", "balance please", "yang kedua" all resolve to `client_top_n_by_savings_balance` when it is on offer.
2. **Explicit "Others" affordance.** Every clarification MUST include an "Others — describe in your own words" option. Selecting it drops the current candidate set and reclassifies the next user message from scratch.
3. **No literal-match trap.** Removing exact-string equality entirely from the resolution path. Even a numeric pick (`"1"`) or slot-fill reply (`"last month"`) must work without echoing option labels.
4. **Deterministic outer loop.** The LLM answers a bounded question ("does this reply align with candidate X?") — the backend still owns retry, timeout, invalid-attempt counting, and the decision to escalate or abandon.
5. **Auditability.** Every clarification decision emits an audit event with: candidates offered, user reply, resolution outcome (matched / refined / superseded / abandoned), and the reasoning source (embedding, LLM, exact label, numeric).

## Options considered

**(A) Embedding cosine similarity, backend-owned.**
Embed the user reply, embed each candidate's `display_name` + `description`. Pick the highest cosine above a threshold; if the gap between top-1 and top-2 is small, escalate. Zero LLM cost per clarification, deterministic, uses infrastructure that already exists (Voyage embeddings, `KnowledgeRepository::search_context`). Weakness: bad at negation ("not the balance one") and at fresh-topic detection ("actually show deposits").

**(B) LLM structured-output classifier.**
One small LLM call: `{"reply": "...", "candidates": [{"id": "...", "label": "..."}], "options": ["MATCHED", "REFINED", "OTHERS", "UNRELATED"]}`. Returns the selected id + reasoning. Handles negation, paraphrase, code-switching, and fresh-topic detection. Cost: one extra call per clarification round. Fits the blueprint's "LLM reasons, backend orchestrates" principle exactly.

**(C) Hybrid.**
Try embeddings first. If gap < threshold or the reply contains negation markers ("not", "bukan", "instead"), escalate to (B). Trades a little complexity for cost predictability. This is the recommended path.

## Chosen approach

**Hybrid (C)**, phased:

1. **Phase 1 — Explicit "Others" option in every clarification payload.** Regardless of match strategy, surface a first-class free-form escape hatch so users are never stuck. Small change; ship first.
2. **Phase 2 — Embedding-based semantic match.** Replace the multi-strategy string matcher in `select_capability` with cosine similarity over `display_name` + `description`. Keep numeric-pick as a fast-path (`"1"`, `"2"`).
3. **Phase 3 — LLM tie-breaker.** When embedding gap < threshold OR user reply contains negation/reset markers, call a small structured-output LLM classifier to decide MATCHED / OTHERS / UNRELATED. Backend still enforces retries.
4. **Phase 4 — Audit event.** Emit `chat_job_events` row per clarification round with candidates + reply + resolution + source.

Each phase is independently shippable. Phase 1 is a blocker for user experience today.

## Interim workaround

The multi-strategy string matcher merged today (numeric + display_name + id + bidirectional substring) covers well-behaved replies. Its ceiling: still fails on paraphrase and negation, still has no explicit "Others" affordance. Do not rely on it beyond Phase 1 delivery.

## Related

- `docs/Modern_RAG_Architecture_Blueprint.md` — Sections 2, 4, 10.
- `crates/chat/src/chat/pending_intent.rs::select_capability` — current matcher.
- `crates/chat/src/chat/classifier.rs::classify_clarification_response` + `OTHER_ACTIVITY_CAPABILITY` — the partial "Others" precedent.
