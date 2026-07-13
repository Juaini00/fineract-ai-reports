# Pending deletions

- pending-uncommitted: removed legacy `chat::pending_intent` runtime and tests.
- pending-uncommitted: removed legacy generic formatter render/activity path and tests.
- pending-uncommitted: removed `chat::formatter` / `chat::pending_intent` module exports.
- pending-uncommitted: placed compile-blocking sentinels on `JobService::create` and `JobService::respond` until the semantic assistant graph runtime is wired.
- pending-uncommitted: removed classifier-first helper shortcuts from `JobService` (`classify_savings_activity_list`, prompt-shape/domain overrides, off-domain cue override).
- pending-uncommitted: removed obsolete dev/staging legacy assistant state wipe migration after dev DB reset.
- pending-uncommitted: removed dead legacy classifier/retrieval/executor bridge from `JobService` after graph skeleton wiring.
