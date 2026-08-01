use std::panic::AssertUnwindSafe;

use futures::FutureExt;

use super::*;

use crate::job::progress::{self, ProgressSink};
use crate::job::repository::ExecutionAuditContext;
use crate::management::model::SafeIdentifier;

impl JobService {
    /// Runs the pipeline in the background-task context: catches both a
    /// returned `Err` and a panic, and in either case reuses the existing
    /// failure path (`JobRepository::fail` + an `error` event) so a client
    /// watching the stream is never left waiting forever. Terminal outcomes
    /// that `run_graph_skeleton` already persists itself (including its own
    /// `FailedOperational` branch) are left untouched here.
    pub(super) async fn run_graph_skeleton_recording_failure(
        &self,
        session_id: Uuid,
        job_id: Uuid,
        client: &PrincipalContext,
        input: RuntimeUserInput,
        canonical_turn: CanonicalTurn,
    ) {
        let outcome = AssertUnwindSafe(self.run_graph_skeleton(
            session_id,
            job_id,
            client,
            input,
            canonical_turn,
        ))
        .catch_unwind()
        .await;

        let Some(error) = classify_run_outcome(outcome) else {
            return;
        };
        tracing::error!(job_id = %job_id, error = %error, "chat job pipeline failed");

        // `fail` refuses to overwrite a job that already reached a terminal
        // status, so a late failure here (e.g. the terminal event insert
        // below) can never flip an already-completed job back to failed
        // (I1). Only publish the "error" event when we actually transitioned
        // the job — otherwise the client already has (or will replay) the
        // correct terminal event and this would be a misleading duplicate.
        let job_newly_failed = match self
            .jobs
            .fail(
                job_id,
                json!({
                    "code": "assistant_failed",
                    "message": "The assistant could not complete this request.",
                }),
            )
            .await
        {
            Ok(newly_failed) => newly_failed,
            Err(fail_error) => {
                tracing::error!(job_id = %job_id, error = %fail_error, "failed to record chat job failure");
                false
            }
        };
        if !job_newly_failed {
            return;
        }
        if let Err(event_error) = self
            .emit_event(
                job_id,
                "error",
                None,
                json!({ "message": "The assistant could not complete this request." }),
            )
            .await
        {
            tracing::error!(job_id = %job_id, error = %event_error, "failed to emit chat job error event");
        }
    }

    /// Installs a task-local progress sink around the pipeline run and forwards
    /// every stage event to `emit_event` concurrently. The forwarder is driven
    /// alongside the pipeline via `tokio::join!` (not run-then-drain), and
    /// `tokio::join!` only resolves once *both* futures finish — so by the
    /// time this function proceeds to emit the terminal event below, every
    /// stage event is already durably persisted and published. That ordering
    /// is what guarantees a client sees `stage` events before `final`.
    pub(super) async fn run_graph_skeleton(
        &self,
        session_id: Uuid,
        job_id: Uuid,
        client: &PrincipalContext,
        input: RuntimeUserInput,
        canonical_turn: CanonicalTurn,
    ) -> Result<Option<JobRunOutcome>> {
        let (sink, mut receiver) = ProgressSink::new();
        let forward_stage_events = async {
            while let Some(event) = receiver.recv().await {
                let stage = event.stage.as_str();
                if let Err(error) = self
                    .emit_event(
                        job_id,
                        "stage",
                        Some(stage),
                        json!({ "stage": stage, "state": event.state, "ms": event.ms }),
                    )
                    .await
                {
                    tracing::warn!(job_id = %job_id, error = %error, "failed to emit chat job stage event");
                }
            }
        };
        let pipeline = progress::scope(
            sink,
            self.run_graph_skeleton_body(session_id, job_id, client, input, canonical_turn),
        );
        let (body_result, ()) = tokio::join!(pipeline, forward_stage_events);
        let Some((outcome, payload)) = body_result? else {
            return Ok(None);
        };
        // Stream the rendered prose as ordered deltas before the authoritative
        // `final` (or `clarification`/`error`) event below. Every terminal
        // outcome here carries an assistant-visible message — a clarification
        // question or an error explanation is still prose the user reads, so
        // it gets the same typing effect as a completed answer. Never let
        // chunking or a delta emit fail the job: `final` must still land with
        // the untouched, complete markdown either way.
        //
        // Pub/sub delivery is lossy (I2): a dropped `delta` silently
        // corrupts a client that reconstructs prose by concatenation. The
        // `payload` emitted below as `final`/`clarification`/`error` always
        // carries the complete, authoritative `markdown` (built once, above,
        // from the same `rendered` value every delta chunk is sliced from) —
        // clients MUST treat that field as the full text and replace their
        // accumulated buffer with it, never append to it.
        if let Some(markdown) = payload.get("markdown").and_then(Value::as_str) {
            for (seq, text) in progress::chunk_markdown(markdown).into_iter().enumerate() {
                if let Err(error) = self
                    .emit_event(
                        job_id,
                        "delta",
                        Some("formatting"),
                        json!({ "seq": seq, "text": text }),
                    )
                    .await
                {
                    tracing::warn!(job_id = %job_id, error = %error, "failed to emit chat job delta event");
                }
            }
        }
        self.emit_event(
            job_id,
            outcome.event_kind,
            Some("complete_or_wait"),
            payload,
        )
        .await?;
        Ok(Some(outcome))
    }

    async fn run_graph_skeleton_body(
        &self,
        session_id: Uuid,
        job_id: Uuid,
        client: &PrincipalContext,
        input: RuntimeUserInput,
        canonical_turn: CanonicalTurn,
    ) -> Result<Option<(JobRunOutcome, Value)>> {
        let memory = match self.job_memory.get(job_id, client.user_id).await? {
            Some(memory) => memory,
            None => {
                self.job_memory
                    .create(job_id, client.user_id, "receive_message")
                    .await?
            }
        };
        let context = self
            .context_builder
            .build_with_pending(
                session_id,
                client,
                memory.pending_clarification.clone(),
                memory.pending_clarification.is_none()
                    && matches!(
                        memory.terminal_state,
                        Some(TerminalState::WaitingForUserInput)
                    ),
            )
            .await?;
        let expected_revision = memory.revision;
        let runtime_llm = self.llm.as_ref().map(|llm| {
            Arc::new(TracedLlmClient::new(
                llm.clone(),
                self.llm_traces.clone(),
                Some(LlmTraceContext {
                    job_id: Some(job_id),
                    session_id: Some(session_id),
                    user_id: client.user_id,
                    legacy_api_key_id: client.legacy_api_key_id,
                    graph_state: Some("route_intent".into()),
                    correlation_id: None,
                    context_contract_version: None,
                    catalog_version_id: None,
                    index_version_id: None,
                }),
            )) as SharedLlmClient
        });
        let router = runtime_llm
            .as_ref()
            .map(|llm| SemanticRouter::new(llm.clone(), &self.catalog));
        let catalog_version = if self.canonical_mode == CanonicalGatewayMode::Authoritative {
            self.knowledge
                .latest_embedded_catalog()
                .await?
                .map(|version| version.id)
        } else {
            None
        };
        let today = self
            .business_date
            .today()
            .await
            .map_err(|error| anyhow::anyhow!("business_date resolution failed: {error}"))?;
        let canonical = CanonicalRuntimeContext {
            mode: self.canonical_mode,
            repository: self.canonical_state.clone(),
            catalog_version,
            message_id: canonical_turn.message_id,
            observed_at: canonical_turn.observed_at,
            reference_instant: canonical_turn.reference_instant,
            timezone: "Asia/Jakarta".into(),
            revision: expected_revision,
            initial: canonical_turn.initial,
            business_today: today.date,
            business_date_source: today.source,
            execution_limits: crate::execution::repository::ExecutionLimits {
                default_timeout_ms: self.query_config.default_timeout_ms,
                global_max_rows: self.query_config.global_max_rows,
            },
        };
        let mut result = AssistantGraphRuntime::run_with_router(
            memory,
            context,
            router.as_ref(),
            runtime_llm.as_ref(),
            self.runtime_knowledge_enabled.then_some(&self.knowledge),
            Some(&self.fineract_pool),
            Some(&self.catalog),
            Some(client),
            Some(&canonical),
            input,
        )
        .await;
        if self.canonical_mode == CanonicalGatewayMode::Shadow
            && let Err(_error) = self
                .shadow_write(
                    &mut result.memory,
                    client,
                    canonical_turn,
                    expected_revision,
                )
                .await
        {
            warn!(job_id = %job_id, "canonical shadow write failed");
        }
        // Best-effort audit trace (issue 06): never fail the request on this write.
        if let Some(trace) = result.retrieval_trace.clone() {
            self.jobs
                .merge_retrieval_trace(job_id, client.user_id, trace)
                .await
                .ok();
        }
        AssistantGraphTopology::new().validate_sequence(&result.transitions)?;
        if let Some(pending_clarification) = result.pending_clarification.clone() {
            result.memory.pending_clarification = pending_clarification;
        }
        let memory = self
            .job_memory
            .save(&result.memory, expected_revision)
            .await?;
        // The session value remains a temporary projection for legacy consumers; job memory is authoritative.
        self.session_memory
            .update_after_job(
                session_id,
                client.user_id,
                &memory,
                result.pending_clarification.as_ref().map(|p| p.as_ref()),
            )
            .await?;
        let clarification_summary = safe_clarification_summary(
            memory.structured_response.as_ref(),
            memory.terminal_state,
            memory.planner_snapshot_id,
        );
        self.job_memory
            .insert_checkpoint(
                &memory,
                json!({
                    "transitions": result.transitions.clone(),
                    "execution_summary": memory.execution_summary,
                    "planner_snapshot_id": memory.planner_snapshot_id,
                    "clarification_summary": clarification_summary.clone(),
                }),
            )
            .await?;
        for transition in &result.transitions {
            AssistantGraphTopology::new().validate_transition(transition)?;
            self.job_memory
                .checkpoint_transition(
                    memory.job_id,
                    transition,
                    memory.revision,
                    json!({
                        "transition": transition,
                        "execution_summary": memory.execution_summary,
                        "planner_snapshot_id": memory.planner_snapshot_id,
                        "clarification_summary": clarification_summary.clone(),
                    }),
                )
                .await?;
        }

        let Some(response) = &memory.structured_response else {
            return Ok(None);
        };
        progress::started(progress::Stage::Formatting);
        let formatting_started_at = std::time::Instant::now();
        // Serialize once so every durable and live public projection carries the
        // exact same client-safe response contract.
        let structured_response = serde_json::to_value(response)?;
        let rendered = response
            .rendered_markdown
            .clone()
            .unwrap_or_else(|| MarkdownRenderer.render(response));
        progress::finished(
            progress::Stage::Formatting,
            formatting_started_at.elapsed().as_millis() as u64,
        );
        let result_json = json!({
            "structured_response": structured_response.clone(),
            "warnings": response.warnings.clone(),
            "markdown": rendered.clone(),
            "graph_state": memory.graph_state.clone(),
            "terminal_state": memory.terminal_state,
            "selected_capability": memory.selected_capability.clone(),
        });
        let metadata_json = json!({
            "type": "assistant_response",
            "assistant_response": structured_response.clone(),
        });
        let terminal_state = memory
            .terminal_state
            .unwrap_or(TerminalState::FailedOperational);
        let outcome = JobRunOutcome::from_terminal_state(terminal_state);
        let execution_ctx = execution_audit_from_memory(&memory.execution_summary, terminal_state);
        match terminal_state {
            TerminalState::WaitingForUserInput => {
                self.messages
                    .insert_assistant_message(session_id, job_id, rendered.clone(), metadata_json)
                    .await?;
                self.jobs
                    .store_assistant_response_result(job_id, result_json.clone())
                    .await?;
                self.jobs
                    .wait_for_user_input_and_record_clarification_requested(
                        session_id,
                        job_id,
                        client.user_id,
                    )
                    .await?;
            }
            TerminalState::Completed => {
                self.jobs
                    .persist_assistant_response_and_terminal_state(
                        session_id,
                        job_id,
                        client.user_id,
                        rendered.clone(),
                        metadata_json,
                        result_json.clone(),
                        AssistantResponseTerminal::Completed {
                            outcome: AuditOutcome::Success,
                        },
                        execution_ctx.clone(),
                    )
                    .await?;
            }
            TerminalState::FailedOperational => {
                self.jobs
                    .persist_assistant_response_and_terminal_state(
                        session_id,
                        job_id,
                        client.user_id,
                        rendered.clone(),
                        metadata_json,
                        result_json.clone(),
                        AssistantResponseTerminal::Failed {
                            error_json: json!({
                                "code": "assistant_failed",
                                "message": "The assistant could not complete this request.",
                            }),
                        },
                        execution_ctx.clone(),
                    )
                    .await?;
            }
            TerminalState::BlockedByPolicy => {
                self.jobs
                    .persist_assistant_response_and_terminal_state(
                        session_id,
                        job_id,
                        client.user_id,
                        rendered.clone(),
                        metadata_json,
                        result_json.clone(),
                        AssistantResponseTerminal::Completed {
                            outcome: AuditOutcome::Blocked,
                        },
                        execution_ctx.clone(),
                    )
                    .await?;
            }
            TerminalState::Unsupported
            | TerminalState::OutOfDomain
            | TerminalState::ContextWindowExceeded => {
                self.jobs
                    .persist_assistant_response_and_terminal_state(
                        session_id,
                        job_id,
                        client.user_id,
                        rendered.clone(),
                        metadata_json,
                        result_json.clone(),
                        AssistantResponseTerminal::Completed {
                            outcome: AuditOutcome::Unsupported,
                        },
                        None,
                    )
                    .await?;
            }
        }
        let mut audit_event = AuditEvent::new(
            client.user_id,
            job_id,
            "assistant_response_projected",
            "service",
            outcome.status,
        );
        audit_event.session_id = Some(session_id);
        audit_event.legacy_api_key_id = client.legacy_api_key_id;
        audit_event.output_summary_json = json!({
            "response_type": response.response_type,
            "clarification": clarification_summary,
        });
        self.audit.record(audit_event);

        let payload = json!({
            "response_type": response.response_type,
            "structured_response": structured_response,
            "markdown": rendered,
        });
        Ok(Some((outcome, payload)))
    }
}

/// Public-audit and checkpoint summary only. Never copy the private
/// clarification payload, source intent, principal projection, or tool data.
fn safe_clarification_summary(
    response: Option<&crate::assistant::response::AssistantResponse>,
    terminal_state: Option<TerminalState>,
    planner_snapshot_id: Option<Uuid>,
) -> serde_json::Value {
    let Some(response) = response else {
        return serde_json::Value::Null;
    };
    let Some(clarification) = response.clarification.as_ref() else {
        return serde_json::Value::Null;
    };
    let field_keys = clarification
        .fields
        .iter()
        .chain(
            clarification
                .options
                .iter()
                .flat_map(|option| &option.fields),
        )
        .map(|field| field.key.clone())
        .collect::<Vec<_>>();
    let evidence_refs = response
        .evidence_refs
        .iter()
        .map(|reference| reference.id.clone())
        .collect::<Vec<_>>();

    json!({
        "id": clarification.id,
        "revision": clarification.revision,
        "kind": clarification.kind,
        "option_ids": clarification.options.iter().map(|option| &option.id).collect::<Vec<_>>(),
        "field_keys": field_keys,
        "resolution_outcome": terminal_state,
        "provenance_identifiers": {
            "planner_snapshot_id": planner_snapshot_id,
            "evidence_refs": evidence_refs,
        },
    })
}

#[derive(Clone, Copy)]
pub(super) struct CanonicalTurn {
    pub(super) message_id: Uuid,
    pub(super) observed_at: DateTime<Utc>,
    pub(super) reference_instant: DateTime<Utc>,
    pub(super) initial: bool,
}

pub(super) struct JobRunOutcome {
    pub(super) status: &'static str,
    pub(super) event_kind: &'static str,
}

impl JobRunOutcome {
    fn from_terminal_state(state: TerminalState) -> Self {
        match state {
            TerminalState::WaitingForUserInput => Self {
                status: "waiting_for_user_input",
                event_kind: "clarification",
            },
            TerminalState::FailedOperational => Self {
                status: "failed",
                event_kind: "error",
            },
            _ => Self {
                status: "completed",
                event_kind: "final",
            },
        }
    }
}

/// Decides whether a finished pipeline attempt must be routed down the
/// failure path (`jobs.fail` + `error` event), and if so, with what error.
///
/// `Ok(None)` — the body ran to completion but produced no structured
/// response (an abandoned run, e.g. `memory.structured_response == None`) —
/// is treated the same as `Err`: without this, the job would stay in a
/// non-terminal status forever and an SSE client would wait indefinitely
/// (C2). A bare `Ok(_)` match here would silently swallow that case, since
/// it matches both `Ok(Some(_))` and `Ok(None)`.
fn classify_run_outcome(
    outcome: std::thread::Result<Result<Option<JobRunOutcome>>>,
) -> Option<anyhow::Error> {
    match outcome {
        Ok(Ok(Some(_))) => None,
        Ok(Ok(None)) => Some(anyhow::anyhow!(
            "chat job pipeline produced no terminal outcome (abandoned run)"
        )),
        Ok(Err(error)) => Some(error),
        Err(panic) => {
            let message = panic
                .downcast_ref::<&str>()
                .map(|message| message.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            Some(anyhow::anyhow!("chat job pipeline panicked: {message}"))
        }
    }
}

/// Extract capability/query identifiers and row count from the graph's
/// `execution_summary` blob. Returns `None` when no capability was executed
/// (clarification, unsupported, or intent-only flows).
fn execution_audit_from_memory(
    summary: &Value,
    terminal_state: TerminalState,
) -> Option<ExecutionAuditContext> {
    let plan = summary.get("plan")?;
    let capability_id = plan.get("capability_id")?.as_str()?;
    let query_id = plan.get("query_id")?.as_str()?;
    let result = summary.get("result");
    let row_count = result
        .and_then(|result| result.get("rows"))
        .and_then(|rows| rows.as_array())
        .map(|rows| rows.len() as u64);
    let truncated = result
        .and_then(|result| result.get("truncated"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let timed_out = result
        .and_then(|result| result.get("timed_out"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(ExecutionAuditContext {
        capability_id: SafeIdentifier::try_from(capability_id.to_string()).ok()?,
        query_id: SafeIdentifier::try_from(query_id.to_string()).ok()?,
        row_count,
        allowed: !matches!(terminal_state, TerminalState::BlockedByPolicy),
        truncated,
        timed_out,
    })
}

#[cfg(test)]
mod classify_run_outcome_tests {
    use super::*;

    #[test]
    fn some_outcome_is_not_a_failure() {
        let outcome: std::thread::Result<Result<Option<JobRunOutcome>>> =
            Ok(Ok(Some(JobRunOutcome {
                status: "completed",
                event_kind: "final",
            })));
        assert!(classify_run_outcome(outcome).is_none());
    }

    #[test]
    fn abandoned_run_is_classified_as_a_failure() {
        // C2: `Ok(None)` must not be treated the same as success — an
        // abandoned run still needs to reach the fail+emit path so the job
        // becomes terminal and a waiting SSE client gets an `error` event.
        let outcome: std::thread::Result<Result<Option<JobRunOutcome>>> = Ok(Ok(None));
        assert!(classify_run_outcome(outcome).is_some());
    }

    #[test]
    fn returned_error_is_classified_as_a_failure() {
        let outcome: std::thread::Result<Result<Option<JobRunOutcome>>> =
            Ok(Err(anyhow::anyhow!("boom")));
        assert!(classify_run_outcome(outcome).is_some());
    }
}
