use super::*;

pub(super) async fn execute_selected_capability(
    mut memory: JobMemory,
    recent_message_count: usize,
    capability_id: String,
    catalog: Option<&Arc<KnowledgeCatalog>>,
    client: Option<&PrincipalContext>,
    fineract_pool: Option<&PgPool>,
    workflow_state: Option<&WorkflowStateRepository>,
    canonical: Option<&CanonicalRuntimeContext>,
    active_payload: Option<&ClarificationPayload>,
    pending_clarification: Option<Option<ClarificationPayload>>,
    sensitive_identifier: Option<&crate::assistant::understanding::extraction::SensitiveIdentifier>,
    source_message: &str,
) -> GraphRuntimeResult {
    let (Some(catalog), Some(client)) = (catalog, client) else {
        return graph_result(
            memory,
            TerminalState::Completed,
            "capability_selected",
            ResponseBuilder::selected(capability_id),
            recent_message_count,
            pending_clarification.clone(),
            execution_transitions(TerminalState::Completed, "capability_selected"),
        );
    };
    let intent = memory.intent.clone();
    if intent.is_none()
        && canonical.is_none_or(|context| context.mode != CanonicalGatewayMode::Authoritative)
    {
        return graph_result(
            memory,
            TerminalState::WaitingForUserInput,
            "missing_intent",
            ResponseBuilder::missing_parameter("Please include the client name to search for."),
            recent_message_count,
            pending_clarification.clone(),
            execution_transitions(TerminalState::WaitingForUserInput, "missing_intent"),
        );
    }
    // No pre-query missing-parameter gate here (issue-012 inventory item #2):
    // it ignored acquisition strategy. Missing required parameters are caught by
    // `plan_selected_capability_verified` below (`params_from_verified` bails on
    // a missing required param), whose `Err` fallback re-clarifies through the
    // acquisition-aware `planned_clarification` — carrying the same stable field
    // metadata a `CollectFields` payload needs.
    if let Some(error) = memory
        .current_user_message_metadata
        .get("deterministic_extraction")
        .cloned()
        .and_then(|value| serde_json::from_value::<DeterministicExtraction>(value).ok())
        .and_then(|extraction| extraction.temporal_error)
    {
        tracing::warn!(
            target: "assistant::execute_selected_capability",
            capability_id = %capability_id,
            error_code = %error.code,
            error_message = %error.message,
            "clarification-reply execution blocked: invalid temporal input"
        );
        let payload = match planned_clarification(
            catalog,
            std::slice::from_ref(&capability_id),
            intent.as_ref(),
            &Default::default(),
            intent
                .as_ref()
                .map(|intent| source_intent_snapshot(intent, &intent.reason)),
            active_payload,
        ) {
            ClarificationPlanResult::Clarify { mut payload, .. } => {
                // The payload carries stable field metadata; do not expose parser errors.
                payload.question = error.message.clone();
                if let Some(active_payload) = active_payload {
                    payload.attempt = active_payload.attempt.saturating_add(1);
                }
                payload
            }
            ClarificationPlanResult::Complete { .. } => ClarificationPayload {
                version: crate::assistant::clarification::CLARIFICATION_VERSION_1,
                id: uuid::Uuid::new_v4(),
                revision: 1,
                kind: crate::assistant::clarification::ClarificationKind::FreeText,
                question: "Please provide a valid date range for this report.".into(),
                options: Vec::new(),
                fields: Vec::new(),
                attempt: 1,
                source_intent: intent
                    .as_ref()
                    .map(|intent| source_intent_snapshot(intent, &intent.reason)),
                allow_free_text: true,
                is_missing_execution_parameters: true,
                workflow_id: None,
                node_id: None,
                resume_node_id: None,
                entity_kind: None,
            },
        };
        return graph_result(
            memory,
            TerminalState::WaitingForUserInput,
            &error.code,
            ResponseBuilder::clarification(payload.clone()),
            recent_message_count,
            Some(Some(payload)),
            execution_transitions(TerminalState::WaitingForUserInput, "invalid_temporal_input"),
        );
    }
    let authoritative =
        canonical.filter(|context| context.mode == CanonicalGatewayMode::Authoritative);
    let authoritative_plan = match authoritative {
        Some(context) => {
            authoritative_plan(context, &mut memory, catalog, client, &capability_id).await
        }
        None => Ok(None),
    };
    let (plan, execution_client) = match authoritative_plan {
        Ok(Some((plan, principal))) => (plan, principal),
        Ok(None) => {
            let intent = intent.as_ref().expect("legacy path checked intent");
            let deterministic_extraction = memory
                .current_user_message_metadata
                .get("deterministic_extraction")
                .cloned()
                .and_then(|value| serde_json::from_value::<DeterministicExtraction>(value).ok());
            let eval_ctx =
                canonical.map(
                    |c| crate::knowledge::catalog::parameter_policy::EvaluationContext {
                        business_today: c.business_today,
                        wall_today: chrono::Utc::now().date_naive(),
                        authorized_office_ids: client.office_ids.clone(),
                    },
                );
            match crate::assistant::plan_selected_capability_verified(
                catalog,
                &capability_id,
                intent,
                deterministic_extraction.as_ref(),
                eval_ctx.as_ref(),
                Some(source_message),
            ) {
                Ok(plan) => (plan, client.clone()),
                Err(error) => {
                    tracing::warn!(
                        target: "assistant::execute_selected_capability",
                        capability_id = %capability_id,
                        error = %error,
                        "clarification-reply plan_selected_capability_verified failed; \
                         re-clarifying instead of executing"
                    );
                    let payload = match planned_clarification(
                        catalog,
                        std::slice::from_ref(&capability_id),
                        Some(intent),
                        &Default::default(),
                        Some(source_intent_snapshot(intent, &intent.reason)),
                        active_payload,
                    ) {
                        ClarificationPlanResult::Clarify { mut payload, .. } => {
                            payload.question = error.to_string();
                            if let Some(active_payload) = active_payload {
                                payload.attempt = active_payload.attempt.saturating_add(1);
                            }
                            payload
                        }
                        ClarificationPlanResult::Complete { .. } => {
                            tracing::error!(target: "assistant::execute_selected_capability", capability_id = %capability_id, "planner reported complete after missing parameters");
                            return graph_result(
                                memory,
                                TerminalState::FailedOperational,
                                "planning_inconsistent",
                                ResponseBuilder::error(),
                                recent_message_count,
                                pending_clarification,
                                execution_transitions(
                                    TerminalState::FailedOperational,
                                    "planning_inconsistent",
                                ),
                            );
                        }
                    };
                    return graph_result(
                        memory,
                        TerminalState::WaitingForUserInput,
                        "missing_execution_parameters",
                        ResponseBuilder::clarification(payload.clone()),
                        recent_message_count,
                        Some(Some(payload)),
                        execution_transitions(
                            TerminalState::WaitingForUserInput,
                            "missing_execution_parameters",
                        ),
                    );
                }
            }
        }
        Err(error) => {
            tracing::warn!(
                target: "assistant::execute_selected_capability",
                capability_id = %capability_id,
                error = %error,
                "clarification-reply authoritative_plan failed; returning routing error"
            );
            memory.warnings = json!([{ "message": error.to_string() }]);
            return graph_result(
                memory,
                TerminalState::FailedOperational,
                "canonical_snapshot_invalid",
                ResponseBuilder::error(),
                recent_message_count,
                pending_clarification,
                execution_transitions(
                    TerminalState::FailedOperational,
                    "canonical_snapshot_invalid",
                ),
            );
        }
    };
    let evidence_refs = evidence_refs(&memory.retrieval_evidence);
    let tool_request = super::super::tool::tool_request_from_plan(&plan, evidence_refs);
    memory.selected_tool = Some(tool_request.tool_name.clone());
    memory.tool_params = json!(tool_request);
    crate::job::progress::started(crate::job::progress::Stage::Policy);
    let policy_started_at = std::time::Instant::now();
    let policy = crate::assistant::guard_selected_capability(&execution_client, catalog, &plan);
    crate::job::progress::finished(
        crate::job::progress::Stage::Policy,
        policy_started_at.elapsed().as_millis() as u64,
    );
    memory.policy_decision = json!(policy);
    if policy.status != PolicyDecisionStatus::Allowed {
        return graph_result(
            memory,
            TerminalState::BlockedByPolicy,
            "blocked_by_policy",
            ResponseBuilder::policy_blocked(policy.reason.as_deref().unwrap_or("policy blocked")),
            recent_message_count,
            pending_clarification.clone(),
            execution_transitions(TerminalState::BlockedByPolicy, "blocked_by_policy"),
        );
    }
    let Some(pool) = fineract_pool else {
        return graph_result(
            memory,
            TerminalState::Completed,
            "execution_not_configured",
            ResponseBuilder::selected(capability_id),
            recent_message_count,
            pending_clarification.clone(),
            execution_transitions(TerminalState::Completed, "execution_not_configured"),
        );
    };
    let limits = canonical
        .map(|context| context.execution_limits)
        .unwrap_or_default();
    // Workflow-engine execution path (Phase 7 cutover). The `plan`/`policy`
    // built above still gate policy and feed `workflow_response` + audit; the
    // SQL now runs through `WorkflowRunner` over a compiled single-capability
    // workflow instead of the direct `execute_plan_with_sensitive` call.
    let Some(state) = workflow_state else {
        // No app-DB state repository wired (e.g. no-DB test harness) — mirror
        // the `fineract_pool`-absent guard above rather than run a workflow
        // whose durable node-run ledger has nowhere to live.
        return graph_result(
            memory,
            TerminalState::Completed,
            "execution_not_configured",
            ResponseBuilder::selected(capability_id),
            recent_message_count,
            pending_clarification.clone(),
            execution_transitions(TerminalState::Completed, "execution_not_configured"),
        );
    };

    let proposal =
        match crate::assistant::llm::tool::propose_workflow(catalog, vec![capability_id.clone()]) {
            Ok(proposal) => proposal,
            Err(error) => {
                tracing::warn!(
                    target: "assistant::execute_selected_capability",
                    capability_id = %capability_id,
                    error = ?error,
                    "propose_workflow failed; returning routing error"
                );
                memory.warnings = json!([{ "message": format!("{error:?}") }]);
                return graph_result(
                    memory,
                    TerminalState::FailedOperational,
                    "canonical_snapshot_invalid",
                    ResponseBuilder::error(),
                    recent_message_count,
                    pending_clarification.clone(),
                    execution_transitions(
                        TerminalState::FailedOperational,
                        "canonical_snapshot_invalid",
                    ),
                );
            }
        };

    let catalog_version = canonical
        .and_then(|context| context.catalog_version)
        .unwrap_or_else(Uuid::nil);
    // ponytail: proven single-capability budget literal (mirrors the Task 2
    // node_executor tests); `WorkflowBudgets` has no Default constructor.
    let budgets = crate::assistant::workflow::WorkflowBudgets {
        shared_timeout_ms: 30_000,
        shared_row_cap: 1_000,
        max_query_count: 5,
        max_parallel_queries: 1,
        max_model_turns: 2,
        max_node_retries: 0,
    };
    // Feed the already-resolved plan into compilation and execution. The
    // verified plan (`plan_selected_capability_verified`) is the source of
    // truth for parameter values; without it the compiler forwards empty facts
    // and inserts a `ClarificationInterrupt` for every required user parameter
    // (e.g. `search`) even though the value is already in hand. `facts` opens
    // the acquisition gate (bind, don't clarify); `resolved_params` carries the
    // concrete values out-of-band into `FineractDataExecutor` (the runner's
    // bindings stay `Null` and are never persisted). Note `plan.params` already
    // excludes `transient_sensitive_input` parameters (e.g. `account_number`),
    // which flow only via `sensitive_identifier`.
    let mut facts = crate::assistant::workflow::compile::AcquisitionFacts::default();
    let mut resolved_params: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    if let Some(params) = plan.params.as_object() {
        for (key, value) in params {
            if value.is_null() {
                continue;
            }
            resolved_params.insert(key.clone(), value.clone());
            for field in catalog.binding_fields(key) {
                if !facts.deterministic.contains(field) {
                    facts.deterministic.push(field.clone());
                }
            }
        }
    }
    let workflow = match crate::assistant::workflow::compile::compile_with_facts(
        proposal,
        catalog,
        catalog_version,
        budgets,
        &facts,
    ) {
        Ok(workflow) => workflow,
        Err(error) => {
            tracing::warn!(
                target: "assistant::execute_selected_capability",
                capability_id = %capability_id,
                error = %error,
                "workflow compile failed; returning routing error"
            );
            memory.warnings = json!([{ "message": error.to_string() }]);
            return graph_result(
                memory,
                TerminalState::FailedOperational,
                "workflow_compile_failed",
                ResponseBuilder::error(),
                recent_message_count,
                pending_clarification.clone(),
                execution_transitions(TerminalState::FailedOperational, "workflow_compile_failed"),
            );
        }
    };

    if let Err(error) = state
        .install_workflow(memory.job_id, execution_client.user_id, &workflow)
        .await
    {
        tracing::warn!(
            target: "assistant::execute_selected_capability",
            capability_id = %capability_id,
            error = %error,
            "install_workflow failed; returning routing error"
        );
        memory.warnings = json!([{ "message": error.to_string() }]);
        return graph_result(
            memory,
            TerminalState::FailedOperational,
            "execution_failed",
            ResponseBuilder::error(),
            recent_message_count,
            pending_clarification.clone(),
            execution_transitions(TerminalState::FailedOperational, "execution_failed"),
        );
    }

    // The sensitive identifier reaches the executor out-of-band (Task 3): it is
    // carried by `FineractDataExecutor`, never through node bindings/parameters,
    // so it is bound straight into approved SQL without being persisted.
    let executor = crate::assistant::llm::tool::FineractDataExecutor::new(
        pool.clone(),
        catalog.clone(),
        policy.clone(),
        limits,
        sensitive_identifier.cloned(),
        resolved_params,
    );
    let node_executor = crate::assistant::workflow::CapabilityNodeExecutor::new(
        executor,
        execution_client.clone(),
        catalog.clone(),
        state.clone(),
        memory.job_id,
        workflow.clone(),
    );
    let runner = crate::assistant::workflow::WorkflowRunner::new(
        state.clone(),
        node_executor,
        catalog.clone(),
    );

    crate::job::progress::started(crate::job::progress::Stage::Execution);
    let execution_started_at = std::time::Instant::now();
    let run_outcome = runner
        .run(
            memory.job_id,
            execution_client.user_id,
            &execution_client,
            &workflow,
        )
        .await;
    crate::job::progress::finished(
        crate::job::progress::Stage::Execution,
        execution_started_at.elapsed().as_millis() as u64,
    );

    let outcome = match run_outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            let reason = if error.to_string() == "execution_timed_out" {
                "execution_timed_out"
            } else {
                "execution_failed"
            };
            tracing::warn!(
                target: "assistant::execute_selected_capability",
                capability_id = %capability_id,
                query_id = %plan.query_id,
                error = %error,
                %reason,
                "workflow run failed; returning routing error"
            );
            memory.warnings = json!([{ "message": error.to_string() }]);
            // Preserve the legacy failure summary shape so the audit producer
            // can still emit `execution.timed_out` (Bundle 11 / W-L).
            memory.execution_summary = json!({
                "plan": plan,
                "policy": policy,
                "result": { "timed_out": reason == "execution_timed_out" },
            });
            return graph_result(
                memory,
                TerminalState::FailedOperational,
                reason,
                ResponseBuilder::error(),
                recent_message_count,
                pending_clarification.clone(),
                execution_transitions(TerminalState::FailedOperational, reason),
            );
        }
    };

    let intent_ref = intent.as_ref().expect("successful execution has intent");
    let response_outcome = match crate::assistant::workflow::response::workflow_response(
        outcome,
        state,
        memory.job_id,
        workflow.id,
        &capability_id,
        intent_ref,
        &plan,
        &policy,
        catalog,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            tracing::warn!(
                target: "assistant::execute_selected_capability",
                capability_id = %capability_id,
                error = %error,
                "workflow_response mapping failed; returning routing error"
            );
            memory.warnings = json!([{ "message": error.to_string() }]);
            memory.execution_summary = json!({ "plan": plan, "policy": policy });
            return graph_result(
                memory,
                TerminalState::FailedOperational,
                "execution_failed",
                ResponseBuilder::error(),
                recent_message_count,
                pending_clarification.clone(),
                execution_transitions(TerminalState::FailedOperational, "execution_failed"),
            );
        }
    };

    match response_outcome {
        crate::assistant::workflow::WorkflowResponseOutcome::Response(mut response) => {
            // Re-apply the business-vs-wall reporting-date note here: Task 4's
            // `workflow_response` intentionally does not port it, so it lives at
            // the one call site that has the canonical reference instant.
            if let Some(context) = canonical {
                let jakarta =
                    chrono::FixedOffset::east_opt(7 * 3600).expect("valid Jakarta offset");
                let wall_today = context
                    .reference_instant
                    .with_timezone(&jakarta)
                    .date_naive();
                if let Some(note) = ResponseBuilder::reporting_date_note(
                    context.business_today,
                    context.business_date_source,
                    wall_today,
                ) {
                    response.warnings.push(note);
                }
            }
            let mut result_state = graph_result(
                memory,
                TerminalState::Completed,
                "execution_completed",
                response,
                recent_message_count,
                pending_clarification.clone(),
                execution_transitions(TerminalState::Completed, "execution_completed"),
            );
            // Audit shape: the completed-path row data now lives in the node-run
            // ledger, not an in-memory `ExecutionResult`. `execution_audit_from_memory`
            // only reads `plan`/`policy` here, so plan+policy is behavior-equivalent.
            result_state.memory.execution_summary = json!({ "plan": plan, "policy": policy });
            result_state
        }
        crate::assistant::workflow::WorkflowResponseOutcome::Clarification(payload) => {
            graph_result(
                memory,
                TerminalState::WaitingForUserInput,
                "ambiguous_client_identity",
                ResponseBuilder::clarification(payload.clone()),
                recent_message_count,
                Some(Some(payload)),
                execution_transitions(
                    TerminalState::WaitingForUserInput,
                    "ambiguous_client_identity",
                ),
            )
        }
        crate::assistant::workflow::WorkflowResponseOutcome::Failed => graph_result(
            memory,
            TerminalState::FailedOperational,
            "execution_failed",
            ResponseBuilder::error(),
            recent_message_count,
            pending_clarification.clone(),
            execution_transitions(TerminalState::FailedOperational, "execution_failed"),
        ),
    }
}
pub(super) fn evidence_refs(evidence: &serde_json::Value) -> Vec<String> {
    evidence
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.get("capability_id")
                .or_else(|| item.get("id"))
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect()
}
