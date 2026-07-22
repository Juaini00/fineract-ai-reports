use super::*;

pub(super) async fn execute_selected_capability(
    mut memory: JobMemory,
    recent_message_count: usize,
    capability_id: String,
    catalog: Option<&Arc<KnowledgeCatalog>>,
    client: Option<&PrincipalContext>,
    fineract_pool: Option<&PgPool>,
    canonical: Option<&CanonicalRuntimeContext>,
    active_payload: Option<&ClarificationPayload>,
    pending_clarification: Option<Option<ClarificationPayload>>,
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
            match crate::assistant::plan_selected_capability_verified(
                catalog,
                &capability_id,
                intent,
                deterministic_extraction.as_ref(),
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
    let policy = crate::assistant::guard_selected_capability(&execution_client, catalog, &plan);
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
    match execute_plan(pool, catalog, &plan, &policy).await {
        Ok(result) => {
            let tool_result =
                super::super::tool::tool_result_from_execution(&tool_request, result.clone());
            let response = ResponseBuilder::from_tool_result(
                intent.as_ref().expect("successful execution has intent"),
                &plan,
                &policy,
                &tool_result,
                catalog,
            );
            let mut result_state = graph_result(
                memory,
                TerminalState::Completed,
                "execution_completed",
                response,
                recent_message_count,
                pending_clarification.clone(),
                execution_transitions(TerminalState::Completed, "execution_completed"),
            );
            result_state.memory.execution_summary = json!({ "plan": plan, "policy": policy, "tool_request": tool_request, "tool_result": tool_result, "result": result });
            result_state
        }
        Err(error) => {
            tracing::warn!(
                target: "assistant::execute_selected_capability",
                capability_id = %capability_id,
                query_id = %plan.query_id,
                error = %error,
                "clarification-reply execute_plan failed; returning routing error"
            );
            memory.warnings = json!([{ "message": error.to_string() }]);
            graph_result(
                memory,
                TerminalState::FailedOperational,
                "execution_failed",
                ResponseBuilder::error(),
                recent_message_count,
                pending_clarification,
                execution_transitions(TerminalState::FailedOperational, "execution_failed"),
            )
        }
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
