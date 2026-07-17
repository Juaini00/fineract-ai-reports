# Implementation plan — verified payload extraction

Date: 2026-07-14
Spec: `docs/superpowers/specs/2026-07-14-verified-payload-extraction-design.md`
Issue: `docs/issues/active/003-verified-payload-extraction.md`

## Phase 1 — Define payload evidence types

- Added first candidate field, provenance, and trust types in `crates/chat` extraction models.
- Keep LLM structured claims separate from trusted facts; unverified hard facts are not promoted to SQL params.
- Add serialization for job checkpoints/events.
- Tests: type-level JSON round-trip for candidates, conflicts, and verified payload.

## Phase 2 — Expand deterministic hard-fact extraction

- Moved current quantity/date/currency/domain/metric extraction behind the candidate field model.
- Add field keys for entity names/ids, sort direction, grouping, and query filters needed by approved catalog queries.
- Preserve text-span evidence where practical.
- Tests: quantity regression, date/currency/domain/metric/entity/sort/group extraction cases.

## Phase 3 — Enrich catalog/query metadata

- Extend capability/query metadata with concepts, aliases, paraphrases, language variants, required params, optional params, safe defaults, allowed values, and clarification options.
- Keep SQL files unchanged unless metadata proves a required bound parameter is missing.
- Validate catalog load fails on missing required-param declarations or invalid option ids.
- Tests: catalog fixture validation for aliases, required params, defaults, and options.

## Phase 4 — Add semantic agreement layer

- Map retrieval results to catalog concepts and param candidates.
- Require catalog/retrieval agreement before promoting semantic claims.
- Reject global keyword maps; all language/paraphrase behavior comes from catalog concepts and aliases.
- Tests: paraphrase/language scenario cases select the same catalog concept without prompt-specific shortcuts.

## Phase 5 — Gate LLM structured claims

- Ask the LLM for structured claims in the existing planner boundary.
- Store claims as untrusted candidates.
- Promote only when grounded by user text, accepted clarification, session context, retrieval agreement, or approved catalog default; first gate trusts deterministic user-text candidates only.
- Tests: hallucinated quantity/metric/filter claims remain rejected and do not reach execution.

## Phase 6 — Build validator and conflict detector

- Detect duplicate incompatible values for one field.
- Detect selected capability/query mismatch with trusted domain or metric candidates.
- Enforce required execution params before approved SQL planning.
- Emit clarification requests for missing, untrusted, or conflicting params.
- Tests: conflict matrix for quantity, metric/capability, date range, and clarification contradictions.

## Phase 7 — Enforce verified execution payload

- Changed approved SQL planning to accept deterministic verified payload alongside intent.
- Removed silent row limit replacement.
- Permit only metadata-declared safe defaults, with `catalog_default` provenance.
- Tests: `show 10 clients with the most savings accounts` executes with limit `10` or clarifies; never returns 20 from fallback.

## Phase 8 — Clarification API hardening

- Enforce `option_id` for non-`other` option replies.
- Enforce `message` for `other` replies.
- Reject ambiguous clarification responses with sanitized `ApiError`.
- Continue the same job through `POST /chat/jobs/{job_id}/responses`.
- Tests: valid option, valid other, missing option id, missing other message, and conflicting reply.

## Phase 9 — Audit trail and diagnostics

- Persist extraction candidate summaries, rejected candidates, conflicts, clarification decisions, and final verified payload in job events/checkpoints.
- Keep raw prompt text and accepted clarification references linked.
- Tests: scenario audit snapshot includes provenance for every final payload field.

## Phase 10 — Acceptance suite

- Add scenario/golden coverage for top-N savings, client lookups, date/currency filters, metric aliases, paraphrases, language variants, missing params, conflicts, and LLM hallucinations.
- Run targeted chat tests first, then `cargo check`.
- Acceptance is complete only when all gates in issue 003 pass.

## Rollout notes

- Keep existing approved SQL and office-scope enforcement intact.
- Prefer deleting fallback paths over wrapping them.
- Do not add new crates.
- If a query lacks metadata needed for safe verification, block with clarification or catalog validation instead of guessing.
