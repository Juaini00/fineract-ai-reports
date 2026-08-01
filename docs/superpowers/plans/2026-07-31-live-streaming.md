# Live Streaming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make chat feel alive — real pipeline stages stream as they happen, and the answer's prose arrives progressively instead of appearing all at once.

**Architecture:** Pipeline stages report through a `tokio::task_local!` sink so no function signature changes. `JobService` forwards those reports to Postgres (durable) and Redis Pub/Sub (live). The SSE handler subscribes instead of polling, and replays history on reconnect. The final markdown's prose is chunked into ordered `delta` events server-side; tables are never chunked.

**Tech Stack:** Rust 2024 (`crates/chat`), `tokio`, `redis 1.2.3` (tokio-comp), `axum` SSE, `sqlx`/Postgres. FE: React 19 + Vite + TypeScript in `/Users/tabrezakhlaque/project/personal/rust/projects/ai_report_dashboard`, tests via `vitest`.

## Global Constraints

- Workspace locked to three crates — `app`, `core`, `chat`. Do not add a crate.
- No `sqlx` in handlers or services — repositories only.
- Redis is **live coordination only**. Postgres (`chat_job_events`) stays the source of truth. Never make the job's success depend on Redis.
- Schema changes only via `migrations/*.sql`. Application startup must not create or alter tables.
- All HTTP responses use the `{ success, data, error }` envelope via `ApiError`. Never leak Serde/SQL/prompt/stack text.
- Progress reporting is strictly best-effort: a closed or full channel must never fail, block, or alter a job.
- Stage detail strings are fixed server-side constants. No user text, SQL, prompt content, or row data may enter a progress event.
- Pre-commit hook runs `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`. Both must pass or the commit is rejected.
- FE: run `npm run lint` and `npm run test` before committing.

## Known pre-existing failure

`assistant::llm::router::tests::router_rejects_invalid_request_shape_operation_with_field_level_error` fails on master, unrelated to this plan. Ignore it; do not "fix" it here.

## File Structure

| File | Responsibility |
|---|---|
| `migrations/20260731120000_extend_chat_job_event_types.sql` | Allow `stage` and `delta` event types |
| `crates/chat/src/job/progress/mod.rs` | Re-exports |
| `crates/chat/src/job/progress/stage.rs` | `Stage` enum + fixed labels |
| `crates/chat/src/job/progress/sink.rs` | task-local `ProgressSink`, `report()`, `scope()` |
| `crates/chat/src/job/progress/chunk.rs` | Prose chunker (markdown-safe) |
| `crates/chat/src/job/service/events.rs` | Publish to Pub/Sub alongside the Postgres insert |
| `crates/chat/src/api/handlers/job.rs` | SSE: subscribe, replay, no 2-minute cap |
| FE `src/module/chat/service/stream.ts` | Accept `stage`/`delta`/`final`/`error` |
| FE `src/module/chat/components/JobProgress.tsx` | Live stage checklist |
| FE `src/module/chat/components/AssistantResponse.tsx` | Progressive prose accumulation |

---

### Task 1: Stage vocabulary and task-local progress sink

The mechanism everything else depends on. Ships alone, wired to nothing.

**Files:**
- Create: `crates/chat/src/job/progress/mod.rs`, `stage.rs`, `sink.rs`
- Modify: `crates/chat/src/job/mod.rs` (add `pub mod progress;`)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub enum Stage { Routing, Retrieval, Reranking, Policy, Execution, Formatting }` with `pub fn as_str(self) -> &'static str` and `pub fn label(self) -> &'static str`.
  - `pub struct ProgressEvent { pub stage: Stage, pub state: ProgressState, pub ms: Option<u64>, pub detail: Option<String> }`
  - `pub enum ProgressState { Started, Finished }`
  - `pub struct ProgressSink` with `pub fn new() -> (ProgressSink, mpsc::UnboundedReceiver<ProgressEvent>)`
  - `pub async fn scope<F, T>(sink: ProgressSink, future: F) -> T where F: Future<Output = T>`
  - `pub fn started(stage: Stage)` and `pub fn finished(stage: Stage, ms: u64)` — no-ops when no sink is installed.

- [ ] **Step 1: Write the failing test**

Create `crates/chat/src/job/progress/sink.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::progress::stage::Stage;

    #[tokio::test]
    async fn reports_reach_the_receiver_in_order() {
        let (sink, mut rx) = ProgressSink::new();
        scope(sink, async {
            started(Stage::Routing);
            finished(Stage::Routing, 12);
            started(Stage::Retrieval);
        })
        .await;

        let first = rx.recv().await.expect("routing started");
        assert_eq!(first.stage, Stage::Routing);
        assert_eq!(first.state, ProgressState::Started);
        assert_eq!(first.ms, None);

        let second = rx.recv().await.expect("routing finished");
        assert_eq!(second.state, ProgressState::Finished);
        assert_eq!(second.ms, Some(12));

        let third = rx.recv().await.expect("retrieval started");
        assert_eq!(third.stage, Stage::Retrieval);
    }

    #[tokio::test]
    async fn reporting_without_a_sink_is_a_silent_no_op() {
        // Must not panic: every existing test calls the runtime with no sink.
        started(Stage::Execution);
        finished(Stage::Execution, 5);
    }

    #[tokio::test]
    async fn reporting_after_the_receiver_is_dropped_does_not_fail() {
        let (sink, rx) = ProgressSink::new();
        drop(rx);
        // A dropped receiver must never turn into a job failure.
        scope(sink, async {
            started(Stage::Policy);
            finished(Stage::Policy, 1);
        })
        .await;
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p chat --lib job::progress`
Expected: FAIL — compile error, `ProgressSink` not found.

- [ ] **Step 3: Write the stage vocabulary**

Create `crates/chat/src/job/progress/stage.rs`:

```rust
//! The closed set of pipeline stages a client may be shown.
//!
//! Labels are fixed server-side constants. User text, SQL, prompt content and
//! row data must never reach a progress event.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Routing,
    Retrieval,
    Reranking,
    Policy,
    Execution,
    Formatting,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Routing => "routing",
            Self::Retrieval => "retrieval",
            Self::Reranking => "reranking",
            Self::Policy => "policy",
            Self::Execution => "execution",
            Self::Formatting => "formatting",
        }
    }

    /// English fallback label. The client is expected to localise from
    /// `as_str()`; this exists so a bare client still shows something useful.
    pub fn label(self) -> &'static str {
        match self {
            Self::Routing => "Understanding the request",
            Self::Retrieval => "Finding a matching report",
            Self::Reranking => "Choosing the best match",
            Self::Policy => "Checking access permissions",
            Self::Execution => "Running the query",
            Self::Formatting => "Composing the answer",
        }
    }
}
```

- [ ] **Step 4: Write the sink**

Prepend to `crates/chat/src/job/progress/sink.rs`:

```rust
//! Ambient progress reporting.
//!
//! Pipeline stages live inside `assistant::execution::runtime`, which cannot
//! reach `JobService::emit_event`. Threading a handle down is not viable:
//! `run_with_router` already takes ten parameters across a dozen test call
//! sites that do not care about progress. A task-local sink keeps every
//! signature unchanged and no-ops when unset.

use std::future::Future;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::job::progress::stage::Stage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressState {
    Started,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub stage: Stage,
    pub state: ProgressState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone)]
pub struct ProgressSink(mpsc::UnboundedSender<ProgressEvent>);

impl ProgressSink {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<ProgressEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self(tx), rx)
    }

    /// Best-effort by contract: a closed receiver is ignored, never surfaced.
    fn send(&self, event: ProgressEvent) {
        let _ = self.0.send(event);
    }
}

tokio::task_local! {
    static PROGRESS: ProgressSink;
}

/// Installs `sink` for the duration of `future`.
pub async fn scope<F, T>(sink: ProgressSink, future: F) -> T
where
    F: Future<Output = T>,
{
    PROGRESS.scope(sink, future).await
}

fn report(event: ProgressEvent) {
    let _ = PROGRESS.try_with(|sink| sink.send(event));
}

pub fn started(stage: Stage) {
    report(ProgressEvent {
        stage,
        state: ProgressState::Started,
        ms: None,
        detail: None,
    });
}

pub fn finished(stage: Stage, ms: u64) {
    report(ProgressEvent {
        stage,
        state: ProgressState::Finished,
        ms: Some(ms),
        detail: None,
    });
}
```

Create `crates/chat/src/job/progress/mod.rs`:

```rust
pub mod sink;
pub mod stage;

pub use sink::{ProgressEvent, ProgressSink, ProgressState, finished, scope, started};
pub use stage::Stage;
```

Add `pub mod progress;` to `crates/chat/src/job/mod.rs`.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p chat --lib job::progress`
Expected: PASS — 3 tests.

- [ ] **Step 6: Commit**

```bash
cargo fmt -p chat
git add crates/chat/src/job/
git commit -m "feat(chat): add task-local pipeline progress sink"
```

---

### Task 2: Markdown-safe prose chunker

Pure function, no I/O. Splits the final markdown into ordered `delta` payloads so a partially rendered message is always valid markdown.

**Files:**
- Create: `crates/chat/src/job/progress/chunk.rs`
- Modify: `crates/chat/src/job/progress/mod.rs`

**Interfaces:**
- Produces: `pub fn chunk_markdown(markdown: &str) -> Vec<String>` — concatenating the result must reproduce the input exactly.

- [ ] **Step 1: Write the failing test**

Create `crates/chat/src/job/progress/chunk.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn assert_lossless(input: &str) {
        let chunks = chunk_markdown(input);
        assert_eq!(
            chunks.concat(),
            input,
            "chunking must be lossless; the client reassembles by concatenation"
        );
    }

    #[test]
    fn concatenating_chunks_reproduces_the_input() {
        assert_lossless("Found 50 charges. The newest is Weekly Charge. Older ones follow.");
        assert_lossless("");
        assert_lossless("One sentence only");
        assert_lossless("Line one\n\nLine two\n\nLine three");
    }

    #[test]
    fn splits_prose_into_multiple_chunks() {
        let chunks = chunk_markdown("First sentence here. Second sentence here. Third one here.");
        assert!(chunks.len() > 1, "prose should stream in pieces, got {chunks:?}");
    }

    #[test]
    fn never_splits_inside_a_word() {
        let input = "Ditemukan lima puluh charge terbaru pada delapan kantor cabang hari ini.";
        for chunk in chunk_markdown(input) {
            assert!(
                !chunk.starts_with(char::is_alphanumeric)
                    || input.contains(chunk.trim_start()),
                "chunk `{chunk}` appears to split mid-word"
            );
        }
    }

    #[test]
    fn emits_a_table_block_whole() {
        let input = "Summary text.\n\n|a|b|\n|---|---|\n|1|2|\n|3|4|\n\nTrailing text.";
        let chunks = chunk_markdown(input);
        let table_chunks: Vec<&String> = chunks.iter().filter(|c| c.contains('|')).collect();
        assert_eq!(
            table_chunks.len(),
            1,
            "a table must arrive whole, not row by row: {chunks:?}"
        );
        assert_lossless(input);
    }

    #[test]
    fn emits_a_code_fence_whole() {
        let input = "Before.\n\n```sql\nSELECT 1;\nSELECT 2;\n```\n\nAfter.";
        let chunks = chunk_markdown(input);
        let fenced: Vec<&String> = chunks.iter().filter(|c| c.contains("```")).collect();
        assert_eq!(fenced.len(), 1, "a code fence must arrive whole: {chunks:?}");
        assert_lossless(input);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p chat --lib job::progress::chunk`
Expected: FAIL — `chunk_markdown` not found.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/chat/src/job/progress/chunk.rs`:

```rust
//! Splits rendered markdown into stream-safe pieces.
//!
//! Two rules. Concatenating the output must reproduce the input exactly, so the
//! client can rebuild the message by appending. And a block that is only
//! meaningful whole — a table, a fenced code block — is never split, because
//! revealing a table row by row is theatre rather than feedback.

/// Roughly one clause. Small enough to feel like typing, large enough that a
/// long answer does not turn into hundreds of events.
const TARGET_CHARS: usize = 48;

pub fn chunk_markdown(markdown: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    for block in split_blocks(markdown) {
        if is_atomic(&block) {
            chunks.push(block);
        } else {
            chunks.extend(split_prose(&block));
        }
    }
    chunks
}

/// Splits on blank-line boundaries, keeping the separators attached so the
/// result stays lossless. A fenced block is kept together even when it contains
/// blank lines.
fn split_blocks(markdown: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    let mut in_fence = false;

    for line in markdown.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            current.push_str(line);
            if !in_fence {
                blocks.push(std::mem::take(&mut current));
            }
            continue;
        }
        if !in_fence && line.trim().is_empty() && !current.is_empty() {
            current.push_str(line);
            blocks.push(std::mem::take(&mut current));
            continue;
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

fn is_atomic(block: &str) -> bool {
    block.contains("```") || block.lines().any(|line| line.trim_start().starts_with('|'))
}

/// Accumulates whole words until the target length, then breaks. Never splits
/// inside a word, because a half-word looks like corruption rather than typing.
fn split_prose(block: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for word in block.split_inclusive(char::is_whitespace) {
        current.push_str(word);
        let long_enough = current.len() >= TARGET_CHARS;
        let at_boundary = word.trim_end().ends_with(['.', '!', '?', ',', ';', ':']);
        if long_enough && at_boundary {
            chunks.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    if chunks.is_empty() && !block.is_empty() {
        chunks.push(block.to_string());
    }
    chunks
}
```

Add `pub mod chunk;` and `pub use chunk::chunk_markdown;` to `crates/chat/src/job/progress/mod.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p chat --lib job::progress::chunk`
Expected: PASS — 5 tests.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p chat
git add crates/chat/src/job/
git commit -m "feat(chat): add markdown-safe prose chunker for streaming"
```

---

### Task 3: Allow stage and delta event types

**Files:**
- Create: `migrations/20260731120000_extend_chat_job_event_types.sql`

**Interfaces:**
- Produces: `chat_job_events.event_type` additionally accepts `stage` and `delta`.

- [ ] **Step 1: Write the migration**

The existing constraint is in `migrations/20260617130000_create_chat_tables.sql:79` and allows only `status`, `clarification`, `partial_result`, `final`, `error`, `heartbeat`. Inserting a `stage` row without this migration violates it.

Create `migrations/20260731120000_extend_chat_job_event_types.sql`:

```sql
-- Streaming adds two event types: `stage` for pipeline progress and `delta`
-- for progressive prose. Both are durable in Postgres so a reconnecting client
-- can replay them; Redis remains live coordination only.
ALTER TABLE chat_job_events
    DROP CONSTRAINT IF EXISTS chk_chat_job_events_type;

ALTER TABLE chat_job_events
    ADD CONSTRAINT chk_chat_job_events_type
    CHECK (event_type IN (
        'status', 'clarification', 'partial_result',
        'final', 'error', 'heartbeat',
        'stage', 'delta'
    ));
```

- [ ] **Step 2: Apply and verify**

```bash
sqlx migrate run --database-url "postgres://root:password@127.0.0.1:5432/ai_reports"
psql "postgres://root:password@127.0.0.1:5432/ai_reports" -c "\d+ chat_job_events" | grep chk_chat_job_events_type
```

Expected: the constraint lists `stage` and `delta`.

- [ ] **Step 3: Prove the constraint accepts the new types**

```bash
psql "postgres://root:password@127.0.0.1:5432/ai_reports" -c "BEGIN; INSERT INTO chat_job_events (id, job_id, event_type, payload_json) SELECT gen_random_uuid(), id, 'stage', '{}'::jsonb FROM chat_jobs LIMIT 1; ROLLBACK;"
```

Expected: `INSERT 0 1` then `ROLLBACK`. If there are no rows in `chat_jobs`, `INSERT 0 0` is also a pass — the point is that no constraint violation is raised.

- [ ] **Step 4: Commit**

```bash
git add migrations/20260731120000_extend_chat_job_event_types.sql
git commit -m "feat(chat): allow stage and delta chat job event types"
```

---

### Task 4: Publish events to Redis Pub/Sub

Keeps the Postgres insert as the durable record and adds a live channel. The
single overwritten `latest_event` key is what loses events today.

**Files:**
- Modify: `crates/chat/src/job/service/events.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: every `emit_event` call additionally `PUBLISH`es the same JSON body to channel `chat_job:{job_id}:events`. Task 5 subscribes to it.

- [ ] **Step 1: Add the publish alongside the existing set**

In `crates/chat/src/job/service/events.rs`, keep the existing `set_ex` of
`chat_job:{job_id}:latest_event` (the current SSE path still reads it during
rollout) and add, immediately after it:

```rust
// Live fan-out. The Postgres insert above is the durable record; this is
// best-effort and must never fail the job.
let channel = format!("chat_job:{job_id}:events");
let published: redis::RedisResult<()> =
    redis::AsyncCommands::publish(&mut conn, channel, body.clone()).await;
if let Err(error) = published {
    warn!(
        job_id = %job_id,
        redis_url = %redis_url_log_value(&self.redis_url),
        error = %error,
        "redis publish to job channel failed",
    );
}
```

`body` is already built above; add `.clone()` at the `set_ex` call site if the borrow checker requires it.

- [ ] **Step 2: Verify it compiles and the suite still passes**

Run: `cargo test -p chat`
Expected: PASS apart from the known pre-existing router failure.

- [ ] **Step 3: Prove publish happens against live Redis**

Start the app, subscribe in one terminal, send a chat request in another:

```bash
redis-cli -p 6380 psubscribe 'chat_job:*:events'
```

Expected: at least one message on `chat_job:<uuid>:events` when a job completes.
Record the observed output in your report.

- [ ] **Step 4: Commit**

```bash
cargo fmt -p chat
git add crates/chat/src/job/service/events.rs
git commit -m "feat(chat): publish job events to a redis channel"
```

---

### Task 5: SSE subscribes instead of polling

**Files:**
- Modify: `crates/chat/src/api/handlers/job.rs:97-183` (`stream`)

**Interfaces:**
- Consumes: the `chat_job:{job_id}:events` channel from Task 4.
- Produces: an SSE stream emitting named events `status`, `stage`, `delta`, `update`, `final`, `error`, with `id:` set to the event's sequence so `Last-Event-ID` works.

- [ ] **Step 1: Replace the polling unfold with a subscription**

Replace the body after the initial snapshot with a Redis pub/sub subscription.
Keep the existing no-Redis fallback (single `status` snapshot) exactly as it is —
chat must keep working with Redis disabled.

Requirements the implementation must satisfy:

1. `client.get_async_pubsub().await`, then `subscribe(format!("chat_job:{job_id}:events"))`.
2. Map each message to an SSE `Event` whose **name is the event's `kind`** (so
   `stage`, `delta`, `final`, `error` reach the client under their own names)
   and whose data is the message payload verbatim.
3. Emit the `status` snapshot first, before subscribing results arrive.
4. Terminate the stream on `kind` of `final` or `error`.
5. **Remove the 120-tick cap and `.take(125)`.** Replace the liveness mechanism
   with a comment-documented keep-alive: `Sse::new(..).keep_alive(KeepAlive::default())`.
   A long job must not lose its stream.
6. On subscribe failure, degrade to the single `status` snapshot rather than
   returning an error — the job is unaffected by stream health.

- [ ] **Step 2: Verify the app boots and streams**

```bash
cargo run -p app
```

Then start a job and consume its stream:

```bash
curl -N -H "Authorization: Bearer <token>" http://127.0.0.1:3007/chat/jobs/<job_id>/stream
```

Expected: a `status` event immediately, then at least a `final` event, with no
repeated empty `update` payloads. Record the observed output in your report.

- [ ] **Step 3: Verify the Redis-disabled path**

Set `REDIS_ENABLED=false`, boot, send a chat request, confirm it still completes.
Record the result.

- [ ] **Step 4: Commit**

```bash
cargo fmt -p chat
git add crates/chat/src/api/handlers/job.rs
git commit -m "feat(chat): stream job events over redis pub/sub instead of polling"
```

---

### Task 6: Run the pipeline in a background task

**Without this task, nothing downstream can work.** `JobService::create` and
`::respond` currently `await` `run_graph_skeleton` inline — there is no
`tokio::spawn` anywhere in the job service (the only spawns in `crates/chat` are
the audit worker at `audit/mod.rs:83` and the outbox dispatcher at
`management/outbox.rs:86`). So `POST /chat/jobs` does not return until the job
has already finished, and by the time a client holds a `job_id` there is nothing
left to observe. Tasks 7 and 8 would publish into a channel no one can subscribe
to in time.

`CLAUDE.md:70` claims the opposite ("`JobService::create`/`respond` spawn
pipeline work via `tokio::spawn`"). That documentation is wrong and this task
corrects it.

**Files:**
- Modify: `crates/chat/src/job/service/mod.rs` (`create` ~line 185, `respond` ~line 314)
- Modify: `CLAUDE.md` (correct the false Phase 9 claim)

**Interfaces:**
- Produces: `POST /chat/jobs` returns as soon as the job row exists, with
  `status` reflecting the queued state rather than a finished result. `GET
  /chat/jobs/{id}` and the SSE stream remain the way a client learns the outcome.

**Frontend impact — already compatible, verified.** `useChat.ts`'s
`adoptStartedJob` consumes only `session_id`, `user_message_id` and `job_id`
from the create response; the result is obtained through `useChatJob`. So this
change does not require an FE change to keep working.

- [ ] **Step 1: Establish the current behaviour in a test**

Add a test asserting that `create` returns before the pipeline has produced a
terminal result — i.e. the returned `status` is the queued/running state, not
`completed`. Place it beside the existing job-service tests. Run it and watch it
FAIL against today's synchronous implementation; that failure is the proof the
bug is real.

- [ ] **Step 2: Spawn the pipeline in `create`**

`JobService` is `Clone` (it lives inside `ChatServices`, which is `Clone`).
Clone what the spawned task needs, move it in, and return the created job
immediately:

```rust
let service = self.clone();
let spawn_client = client.clone();
tokio::spawn(async move {
    if let Err(error) = service
        .run_graph_skeleton(session_id, job_id, &spawn_client, input, canonical_turn)
        .await
    {
        tracing::error!(job_id = %job_id, error = %error, "chat job pipeline failed");
    }
});
```

Requirements:
- The spawned task must record failure into the job row (status `failed`) and
  emit an `error` event, so a client watching the stream is not left waiting
  forever. Reuse the existing failure path rather than inventing a new one.
- A panic inside the spawned task must not take down the process, and must still
  mark the job failed. Handle the `JoinHandle` result or guard the body.
- Do not change the returned `CreatedChatJob` shape beyond `status` /
  `current_step`.

- [ ] **Step 3: Do the same for `respond`**

`respond` has the same inline `await` at ~line 314. Clarification answers must
also run in the background, or the clarification round trip has the same
problem.

- [ ] **Step 4: Run the test suite**

Run: `cargo test -p chat`

Existing tests that assert a completed result directly from `create` will now
fail — that is expected and correct. Update each to await the outcome through
the job record (poll `get` until terminal, with a bounded timeout) rather than
asserting on `create`'s return. **Do not** re-serialise the pipeline to make a
test pass; that would undo the task.

- [ ] **Step 5: Prove the window exists**

This is the acceptance evidence for the whole plan. Boot from the repository
root (`cargo run -p app`), then:

1. `POST /auth/login` (`admin` / `password123`) for an `access_token`.
2. `POST /chat/jobs` with `{"session_id":null,"message":"berapa total deposit bulan ini?"}`.
3. Confirm the response returns **immediately** with a non-terminal status.
4. Open `GET /chat/jobs/{job_id}/stream` and confirm a `final` event arrives
   **after** the stream was opened — not before.

Record the timing and the actual SSE output in your report. If the response
still blocks until completion, the task is not done.

- [ ] **Step 6: Correct CLAUDE.md**

`CLAUDE.md:70` documents this as already wired. After this task it is true;
before it, it was not. Adjust the sentence so it describes reality, including
that the SSE stream now subscribes to a Redis channel rather than polling
`latest_event` at 1s (changed in Task 5).

- [ ] **Step 7: Commit**

```bash
cargo fmt -p chat
git add crates/chat/src/job/service/mod.rs CLAUDE.md
git commit -m "feat(chat): run the chat pipeline in a background task"
```

---

### Task 7: Emit stage progress from the pipeline

Wires Task 1's sink into the real pipeline. This is the task that makes the
stream show something before the answer exists.

**Files:**
- Modify: `crates/chat/src/job/service/run.rs` (install the sink, forward events)
- Modify: `crates/chat/src/assistant/execution/runtime/semantic.rs` (report stages)

**Interfaces:**
- Consumes: `crate::job::progress::{Stage, ProgressSink, scope, started, finished}`.

- [ ] **Step 1: Install the sink around the runtime call**

In `run_graph_skeleton`, create a sink, spawn a forwarder that drains the
receiver into `emit_event` with kind `"stage"`, and wrap the runtime call in
`progress::scope(sink, ...)`. The forwarder must not hold up the job: drain it
concurrently, and let it end when the sender drops.

- [ ] **Step 2: Report stages inside the graph**

In `crates/chat/src/assistant/execution/runtime/semantic.rs`, bracket the
existing phases. The retrieval and rerank call sites are already visible around
the `RetrievalEngine::retrieve` and `LlmReranker::new(llm).rerank` calls:

```rust
progress::started(Stage::Retrieval);
let started_at = std::time::Instant::now();
let evidence = RetrievalEngine::retrieve(&plan, llm, knowledge, catalog).await;
progress::finished(Stage::Retrieval, started_at.elapsed().as_millis() as u64);
```

Apply the same bracket to: routing (`router.route`), reranking (`rerank`),
policy (`evaluate_policy`), execution (`execute_plan`), and formatting
(response building). Six stages total, each with `started` before and
`finished` after.

- [ ] **Step 3: Verify stages reach the stream end to end**

Boot the app, open the SSE stream for a new job, and confirm `stage` events
arrive **before** the `final` event, in pipeline order. Record the observed
sequence in your report — this is the acceptance evidence for the whole plan.

- [ ] **Step 4: Run the suite**

Run: `cargo test -p chat`
Expected: PASS apart from the known pre-existing router failure. Existing tests
call the runtime without a sink, so `report()` must no-op — if any test now
panics, the sink is not degrading correctly.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p chat
git add crates/chat/src/
git commit -m "feat(chat): emit pipeline stage progress over SSE"
```

---

### Task 8: Emit progressive prose deltas

**Files:**
- Modify: `crates/chat/src/job/service/run.rs` (around the existing final `emit_event` at line ~297)

- [ ] **Step 1: Emit deltas before the final event**

Immediately before the existing `emit_event(job_id, outcome.event_kind, ...)`
call, chunk the rendered markdown and emit each piece:

```rust
for (seq, text) in chunk_markdown(&rendered).into_iter().enumerate() {
    self.emit_event(
        job_id,
        "delta",
        Some("formatting"),
        json!({ "seq": seq, "text": text }),
    )
    .await?;
}
```

The `final` event still carries the complete `markdown` and
`structured_response` unchanged — it remains authoritative, and the client
reconciles against it.

- [ ] **Step 2: Verify deltas stream before the final**

Boot, open the stream, send a request that returns prose. Confirm ordered
`delta` events arrive before `final`, and that concatenating their `text`
equals the `final` event's `markdown`. Record both in your report.

- [ ] **Step 3: Run the suite and commit**

```bash
cargo test -p chat
cargo fmt -p chat
git add crates/chat/src/job/service/run.rs
git commit -m "feat(chat): stream response prose as ordered deltas"
```

---

### Task 9: FE consumes the new events

**Files:**
- Modify: FE `src/module/chat/service/stream.ts`
- Modify: FE `src/module/chat/types/index.ts`
- Test: FE `src/module/chat/service/stream.test.ts`

Repo: `/Users/tabrezakhlaque/project/personal/rust/projects/ai_report_dashboard`

- [ ] **Step 1: Write the failing test**

`stream.ts:44-46` currently drops any event whose name is not `status` or
`update`. Add tests asserting `stage`, `delta`, `final` and `error` are
delivered to the handler, and that `final`/`error` terminate the stream.

- [ ] **Step 2: Run it, see it fail**

```bash
cd /Users/tabrezakhlaque/project/personal/rust/projects/ai_report_dashboard
npm run test -- stream
```

- [ ] **Step 3: Widen the accepted event names**

Replace the `event !== "status" && event !== "update"` guard with membership of
a `KNOWN_EVENTS` set containing `status`, `update`, `stage`, `delta`, `final`,
`error`. Keep discarding genuinely unknown names — the client must not forward
arbitrary server strings into the UI.

- [ ] **Step 4: Run tests, lint, commit**

```bash
npm run test && npm run lint
git add src/module/chat/
git commit -m "feat(chat): accept stage, delta, final and error stream events"
```

---

### Task 10: FE renders live stages and progressive prose

**Files:**
- Modify: FE `src/module/chat/hooks/useChatJob.ts`
- Modify: FE `src/module/chat/components/JobProgress.tsx`
- Modify: FE `src/module/chat/components/AssistantResponse.tsx`

- [ ] **Step 1: Write failing component tests**

`JobProgress.test.tsx`: given a sequence of `stage` events, renders each stage
with its state, and marks finished stages with their elapsed ms.

`AssistantResponse.test.tsx`: given ordered `delta` events, renders the
accumulated text; when `final` arrives, renders the final markdown — and if the
accumulated text disagrees with `final.markdown`, `final` wins.

- [ ] **Step 2: Run them, see them fail**

```bash
npm run test -- JobProgress AssistantResponse
```

- [ ] **Step 3: Implement**

- `useChatJob.ts`: accumulate `stage` events into an ordered list and `delta`
  events into a growing string; clear both when a new job starts.
- `JobProgress.tsx`: render the stage checklist, current stage active.
- `AssistantResponse.tsx`: render accumulated prose while streaming, then the
  authoritative `final` markdown. Tables render only from `final`.

- [ ] **Step 4: Run tests, lint, commit**

```bash
npm run test && npm run lint
git add src/module/chat/
git commit -m "feat(chat): render live pipeline stages and progressive prose"
```

---

## Definition of Done

- [ ] `cargo test -p chat` passes apart from the known pre-existing router failure.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [ ] FE `npm run test` and `npm run lint` pass.
- [ ] A live request shows stages advancing **before** any answer text exists.
- [ ] Prose arrives progressively; tables arrive whole.
- [ ] Concatenated `delta` text equals the `final` event's markdown.
- [ ] A job longer than two minutes keeps its stream.
- [ ] With `REDIS_ENABLED=false`, chat still completes end to end.

## Out of Scope

`Last-Event-ID` replay on reconnect; the FE integration gaps catalogued in the
spec (session rename/archive, `/catalog/capabilities`, `/management/*`,
`/vector-index/*`). Both are follow-up plans.
