# Issue 009 — Conversational drill-down

**Status**: **not started**. Deferred follow-up out of scope for issue 007.
**Depends on**: Bundle 12 (W-C, resolver precedence) landing in the runtime.
**Origin**: named by issue 007 §W-H acceptance and §F5 audit.

## Why this exists

Issue 007 §W-H described a "drill down into the last answer" pattern
(e.g. "which of those clients have loans overdue > 30 days after this
report?"). The audit found that no code path carries a prior result set,
capability, or `context_reference` forward today — `ContextReference::PreviousJob`
and `::SessionTopic` are declared but never produced or consumed
(`crates/chat/src/assistant/understanding/intent.rs`). Rather than build
this inside issue 007, W-H recorded it as a follow-up and Bundle 13
preserved the surface it will use.

## What Bundle 13 preserved (do not re-litigate)

- `ContextReference::PreviousJob` and `::SessionTopic` are marked reserved
  with doc comments (`intent.rs`). They must not be deleted.
- `PayloadSource` is `#[non_exhaustive]` with a `#[serde(other)] Unknown`
  catch-all so this issue can add a `PriorJob` variant without breaking
  audit-payload deserialisation of state written by older builds.
- A deserialisation test guards the forward-compatibility contract
  (`extraction/tests.rs::payload_source_unknown_variant_deserialises_safely`).

## What this issue must do

1. Add a `PayloadSource::PriorJob` variant (do NOT remove `Unknown`; producers
   emit specific known sources, `Unknown` remains the forward-compat sink).
2. Wire an execution path that either honours a non-`None` `context_reference`
   or normalises it to `None` **before** planning. **The gateway must not
   silently accept a `context_reference` it cannot honour** — recorded
   requirement from issue 007 §W-H.
3. Decide re-execution vs. cache: 007 §W-H already recommended re-execute
   over cached result sets (stale-answer risk trumps latency win).
4. Extend the resolver (spec 2026-07-24 §5.4) with prior-job precedence
   after user-text and before LLM-hint, gated on `context_reference !=
   None`.

## What is out of scope

- LLM prompt work for context extraction ("in that report, only the
  ones with X"). Layer 1 already emits `context_reference` in extraction
  when it detects follow-up phrasing; this issue makes it honoured.
- Any UI change to the chat client — the client already knows about
  same-job continuation via `POST /chat/jobs/{job_id}/responses`.

## Non-goals

- Building a session-topic memory. `ContextReference::SessionTopic` stays
  reserved for a *later* issue; it is not a 009 goal.
