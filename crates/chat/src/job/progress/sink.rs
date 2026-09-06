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
    let detail = stage.node_id().map(str::to_owned);
    report(ProgressEvent {
        stage,
        state: ProgressState::Started,
        ms: None,
        detail,
    });
}

pub fn finished(stage: Stage, ms: u64) {
    let detail = stage.node_id().map(str::to_owned);
    report(ProgressEvent {
        stage,
        state: ProgressState::Finished,
        ms: Some(ms),
        detail,
    });
}

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
