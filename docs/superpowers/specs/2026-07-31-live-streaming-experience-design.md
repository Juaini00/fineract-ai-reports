# Live Streaming Experience: Real Pipeline Progress + Progressive Response

Date: 2026-07-31
Status: Design approved (two decisions locked by the product owner)
Scope: `crates/chat` (BE) + `ai_report_dashboard` (FE)

## Problem

The chat experience presents a long silence followed by a sudden dump of the
final answer. The user cannot tell whether the request is alive, where in the
pipeline it is, or whether anything is happening at all.

The cause is not that SSE is under-used. It is that **SSE has nothing to send.**

`emit_event` is called from exactly ONE place in the entire crate
(`crates/chat/src/job/service/run.rs:297`), and that call happens *after* the
pipeline has already finished:

```rust
self.emit_event(job_id, outcome.event_kind, Some("complete_or_wait"), ...)
```

No progress event is ever published. The SSE endpoint polls Redis every second,
finds nothing, emits `{}` repeatedly, and then delivers the whole result at once.

Four supporting defects on the same path:

| Location | Defect |
|---|---|
| `api/handlers/job.rs:131-176` | Emits an `update` event every second **without comparing to the previous value**. The comment says "emit on change"; the code does not. Clients receive duplicate empty payloads. |
| `job/service/events.rs:25` | Redis holds a **single `latest_event` key that is overwritten**. Any event occurring between two 1-second polls is lost permanently. |
| `api/handlers/job.rs:150,178` | `ticks >= 120` plus `.take(125)` kills the stream after ~2 minutes. A longer job loses its connection. |
| FE `src/module/chat/service/stream.ts:44-46` | Only accepts events named `status` or `update`; any other event name is silently discarded. |

`current_step` also only ever takes three values — `queued`, `taking_decision`,
`response` — which is far too coarse to answer "where in the pipeline is it?".

## Decisions locked

1. **Progressive text is a server-side staged reveal, not LLM token streaming.**
   The final markdown is chunked and emitted as ordered `delta` events. The LLM
   path is untouched, the rendered result stays byte-identical to the validated
   output, and no text reaches the user before passing validation and the PII
   gate. Tables and figures are not chunked — they are not prose, and revealing
   a table cell-by-cell would be theatre, not feedback.

2. **Transport is Redis Pub/Sub with Postgres history replay.** Events publish
   to a per-job channel and the SSE handler subscribes rather than polls. The
   durable record already exists in `chat_job_events`, so a reconnecting client
   replays history via `Last-Event-ID` instead of losing everything.

## Architecture

### The instrumentation problem

Pipeline stages run deep inside `assistant/execution/runtime/`, which has no
access to `JobService::emit_event`. The natural fix — threading a progress
handle through — is bad here: `AssistantGraphRuntime::run_with_router` already
takes **10 parameters** and is called from a dozen test sites, so an 11th
parameter means touching every one of them for a concern none of them care
about.

**Use a task-local sink instead.** `JobService` scopes it around the runtime
call; stages report into it if it is set, and it is a no-op when it is not.

```rust
// crates/chat/src/job/progress.rs
tokio::task_local! {
    pub static PROGRESS: ProgressSink;
}

#[derive(Clone)]
pub struct ProgressSink(mpsc::UnboundedSender<ProgressEvent>);

/// No-op when no sink is installed — tests and non-job callers are unaffected.
pub fn report(stage: Stage, detail: impl Into<String>) {
    let _ = PROGRESS.try_with(|sink| sink.send(stage, detail.into()));
}
```

Zero signature changes, zero test churn, works across the whole async call tree.

### Stages

A closed enum, so the FE can localise labels rather than render server prose:

| Stage | Emitted when |
|---|---|
| `routing` | Intent/domain routing begins |
| `retrieval` | Candidate datasets retrieved |
| `reranking` | Reranker selects or asks to clarify |
| `policy` | Capability/office/PII guards evaluated |
| `execution` | Approved SQL runs (carries row count on completion) |
| `formatting` | Response assembled |

Each stage emits `started` and `finished` with elapsed milliseconds, so the FE
can show a live checklist with timings.

### Event contract

```
event: stage      {"stage":"retrieval","state":"finished","ms":412}
event: delta      {"seq":7,"text":"Ditemukan 50 charge "}
event: final      {"structured_response":{...},"markdown":"...","table":{...}}
event: error      {"code":"execution_timed_out"}
```

`stage` and `delta` are additive; `status` and `update` are retained so the
current FE keeps working during rollout.

### Chunking

Prose is split on sentence and clause boundaries — never mid-word, never
mid-markdown-token, so a partially rendered message is always valid markdown.
Table blocks and code fences are emitted whole. Cadence is a fixed interval
chosen for readability, not derived from token timing.

## Error handling

- A timeout emits `error`, never a `final` with zero rows. An empty result and a
  failed query must never look alike — the same rule the dataset spec applies to
  the query layer.
- If Redis is unavailable, SSE degrades to a single `status` snapshot rather than
  failing the request. The job itself must not depend on the stream.
- Progress reporting is strictly best-effort: a full or closed channel never
  fails, delays, or alters the job. Observability must not become a failure mode.
- Stage detail strings are fixed server-side constants. No user text, no SQL, no
  prompt content, no row data ever enters a progress event.

## FE work

1. `stream.ts` — accept `stage`, `delta`, `final`, `error`; keep `status`/`update`.
2. `JobProgress.tsx` — live stage checklist with per-stage timing.
3. `AssistantResponse.tsx` — accumulate `delta` into a growing message with a
   caret, then reconcile against `final`. `final` is authoritative: if the
   accumulated text and the final markdown disagree, the final wins.
4. Reconnect via `Last-Event-ID`, replaying missed events instead of restarting.

## Integration gaps (separate track)

The FE currently calls auth, `/chat/sessions*` and `/chat/jobs*`. These BE
endpoints exist and are unused:

```
PATCH  /chat/sessions/{id}        rename
DELETE /chat/sessions/{id}        archive
GET    /chat/jobs/{id}/audit
GET    /catalog/capabilities
GET    /management/dashboard      GET /management/status
GET    /management/audit          GET /management/audit/jobs/{id}
GET    /management/llm-usage
GET    /management/knowledge      GET /management/knowledge/{id}
POST   /vector-index/rebuild      GET /vector-index/status
```

This is real work but independent of streaming, and it is sequenced after it —
mixing a transport change with a new dashboard surface would make both harder to
review.

## Non-goals

- LLM token streaming (explicitly rejected above).
- Streaming table data progressively.
- Replacing Postgres as the durable job record. Redis stays live-coordination
  only, per the existing architecture invariant.

## Acceptance

- A request shows stages advancing live, before any answer text exists.
- Answer prose arrives progressively; tables arrive whole.
- Refreshing mid-request resumes without losing earlier stages.
- A job longer than two minutes keeps its stream.
- With Redis disabled, chat still works end to end.
