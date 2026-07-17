# Pending deletions

- pending-uncommitted: removed legacy `chat::pending_intent` runtime and tests.
- pending-uncommitted: removed legacy generic formatter render/activity path and tests.
- pending-uncommitted: removed `chat::formatter` / `chat::pending_intent` module exports.
- pending-uncommitted: placed compile-blocking sentinels on `JobService::create` and `JobService::respond` until the semantic assistant graph runtime is wired.
- pending-uncommitted: removed classifier-first helper shortcuts from `JobService` (`classify_savings_activity_list`, prompt-shape/domain overrides, off-domain cue override).
- pending-uncommitted: removed obsolete dev/staging legacy assistant state wipe migration after dev DB reset.
- pending-uncommitted: removed dead legacy classifier/retrieval/executor bridge from `JobService` after graph skeleton wiring.
- pending-uncommitted: removed primary runtime `fallback_intent` and `fallback_capability_hint` keyword/capability routing.
- pending-uncommitted: quarantined formatter-first response authority; `AssistantResponse` plus `assistant::renderer::MarkdownRenderer` is the response source/render target.
- pending-uncommitted: primary runtime has no references to `pending_intent`, `capability_matches_prompt_shape`, `capability_matches_domain_terms`, `context_overrides_capability`, `has_off_domain_cue`, `classify_savings_activity_list`, or `initial_assistant_memory`.
