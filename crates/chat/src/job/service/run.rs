use super::*;

use crate::job::repository::ExecutionAuditContext;
use crate::management::model::SafeIdentifier;

impl JobService {
    pub(super) async fn run_graph_skeleton(
        &self,
        session_id: Uuid,
        job_id: Uuid,
        client: &PrincipalContext,
        input: RuntimeUserInput,
        canonical_turn: CanonicalTurn,
    ) -> Result<Option<JobRunOutcome>> {
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
        // Serialize once so every durable and live public projection carries the
        // exact same client-safe response contract.
        let structured_response = serde_json::to_value(response)?;
        let rendered = response
            .rendered_markdown
            .clone()
            .unwrap_or_else(|| MarkdownRenderer.render(response));
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

        self.emit_event(
            job_id,
            outcome.event_kind,
            Some("complete_or_wait"),
            json!({
                "response_type": response.response_type,
                "structured_response": structured_response,
                "markdown": rendered,
            }),
        )
        .await?;
        Ok(Some(outcome))
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
    pub(super) current_step: &'static str,
    pub(super) event_kind: &'static str,
}

impl JobRunOutcome {
    fn from_terminal_state(state: TerminalState) -> Self {
        match state {
            TerminalState::WaitingForUserInput => Self {
                status: "waiting_for_user_input",
                current_step: "taking_decision",
                event_kind: "clarification",
            },
            TerminalState::FailedOperational => Self {
                status: "failed",
                current_step: "response",
                event_kind: "error",
            },
            _ => Self {
                status: "completed",
                current_step: "response",
                event_kind: "final",
            },
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
    let row_count = summary
        .get("result")
        .and_then(|result| result.get("rows"))
        .and_then(|rows| rows.as_array())
        .map(|rows| rows.len() as u64);
    Some(ExecutionAuditContext {
        capability_id: SafeIdentifier::try_from(capability_id.to_string()).ok()?,
        query_id: SafeIdentifier::try_from(query_id.to_string()).ok()?,
        row_count,
        allowed: !matches!(terminal_state, TerminalState::BlockedByPolicy),
    })
}
