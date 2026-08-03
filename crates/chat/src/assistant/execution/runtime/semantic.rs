use super::*;

async fn clarify_retrieval_candidates(
    mut memory: JobMemory,
    context: &ContextWindow,
    _plan: &RetrievalPlan,
    evidence: &[Evidence],
    alternatives: &[String],
    catalog: Option<&Arc<KnowledgeCatalog>>,
    client: Option<&PrincipalContext>,
    fineract_pool: Option<&PgPool>,
    canonical: Option<&CanonicalRuntimeContext>,
    input: &RuntimeUserInput,
) -> GraphRuntimeResult {
    let candidate_ids: Vec<String> = if alternatives.is_empty() {
        evidence
            .iter()
            .take(3)
            .map(|item| item.capability_id.clone())
            .collect()
    } else {
        // Alternatives are only accepted when backed by current authorized evidence.
        alternatives
            .iter()
            .filter(|id| evidence.iter().any(|item| &item.capability_id == *id))
            .cloned()
            .collect()
    };
    let source = memory
        .intent
        .as_ref()
        .map(|intent| source_intent_snapshot(intent, &input.source_message));
    if let Some(catalog) = catalog {
        match planned_clarification(
            catalog,
            &candidate_ids,
            memory.intent.as_ref(),
            &input.constraint_patch,
            source,
            None,
        ) {
            ClarificationPlanResult::Complete { capability_id, .. } => {
                memory.selected_capability = Some(capability_id.clone());
                return execute_selected_capability(
                    memory,
                    context.recent_messages.len(),
                    capability_id,
                    Some(catalog),
                    client,
                    fineract_pool,
                    canonical,
                    None,
                    None,
                    input.sensitive_identifier.as_ref(),
                )
                .await;
            }
            ClarificationPlanResult::Clarify { payload, .. } => {
                return graph_result(
                    memory,
                    TerminalState::WaitingForUserInput,
                    "weak_retrieval_evidence",
                    ResponseBuilder::clarification(payload.clone()),
                    context.recent_messages.len(),
                    Some(Some(payload)),
                    clarification_transitions(
                        TerminalState::WaitingForUserInput,
                        "weak_retrieval_evidence",
                    ),
                );
            }
        }
    }
    // Catalog-less test/runtime mode retains the legacy evidence projection.
    let payload = clarification_payload_for(_plan, evidence, alternatives, source);
    graph_result(
        memory,
        TerminalState::WaitingForUserInput,
        "weak_retrieval_evidence",
        ResponseBuilder::clarification(payload.clone()),
        context.recent_messages.len(),
        Some(Some(payload)),
        clarification_transitions(
            TerminalState::WaitingForUserInput,
            "weak_retrieval_evidence",
        ),
    )
}

pub(super) async fn complete_semantic_route(
    mut memory: JobMemory,
    context: ContextWindow,
    route: anyhow::Result<AssistantIntent>,
    llm: Option<&SharedLlmClient>,
    knowledge: Option<&KnowledgeRepository>,
    fineract_pool: Option<&PgPool>,
    catalog: Option<&Arc<KnowledgeCatalog>>,
    client: Option<&PrincipalContext>,
    canonical: Option<&CanonicalRuntimeContext>,
    input: RuntimeUserInput,
) -> GraphRuntimeResult {
    let message = input.message.as_str();
    let mut pending_clarification = None;
    let mut retrieval_trace: Option<serde_json::Value> = None;
    let (terminal, reason, response) = match route {
        Ok(mut intent) => {
            merge_deterministic_extraction_at(
                &mut memory,
                &mut intent,
                &input.source_message,
                canonical,
            );
            if intent.intent == AssistantIntentKind::ClarificationReply
                && let (Some(payload), Some(llm)) = (&context.pending_clarification, llm)
            {
                let resolve_text = input
                    .selected_option_id
                    .as_deref()
                    .unwrap_or(&input.source_message);
                match ClarificationResolver::resolve(resolve_text, payload, &context, llm.as_ref())
                    .await
                {
                    Ok(ClarificationOutcome::SelectedOption { option_id, .. })
                        if option_id == OTHER_CLARIFICATION_OPTION_ID =>
                    {
                        memory.intent = Some(intent_from_source(payload, &context, canonical));
                        record_source_extraction_metadata(
                            &mut memory,
                            payload,
                            canonical,
                            &input.source_message,
                        );
                        memory.retrieval_evidence = json!({ "clarification_outcome": "free_form_other", "clarification_id": payload.id, "clarification_revision": payload.revision, "clarification_kind": payload.kind });
                        pending_clarification = Some(None);
                        return graph_result(
                            memory,
                            TerminalState::WaitingForUserInput,
                            "clarification_other_selected",
                            ResponseBuilder::free_form_other_prompt(),
                            context.recent_messages.len(),
                            pending_clarification,
                            clarification_transitions(
                                TerminalState::WaitingForUserInput,
                                "clarification_other_selected",
                            ),
                        );
                    }
                    Ok(ClarificationOutcome::SelectedOption { option_id, .. })
                        if payload.kind == crate::assistant::ClarificationKind::SelectEntity =>
                    {
                        let Some(client_id) = option_id
                            .strip_prefix("client:")
                            .and_then(|value| value.parse::<i64>().ok())
                        else {
                            return graph_result(
                                memory,
                                TerminalState::FailedOperational,
                                "invalid_client_selection",
                                ResponseBuilder::error(),
                                context.recent_messages.len(),
                                None,
                                simple_intent_transitions(
                                    TerminalState::FailedOperational,
                                    "invalid_client_selection",
                                ),
                            );
                        };
                        let mut selected_intent = intent_from_source(payload, &context, canonical);
                        selected_intent
                            .entities
                            .push(crate::assistant::AssistantEntity {
                                entity_type: AssistantEntityType::ClientId,
                                value: client_id.to_string(),
                                canonical: Some(client_id.to_string()),
                                confidence: Some(1.0),
                            });
                        memory.intent = Some(selected_intent);
                        memory.selected_capability = Some("client_relationship_by_id".into());
                        pending_clarification = Some(None);
                        return execute_selected_capability(
                            memory,
                            context.recent_messages.len(),
                            "client_relationship_by_id".into(),
                            catalog,
                            client,
                            fineract_pool,
                            canonical,
                            Some(payload),
                            pending_clarification,
                            None,
                        )
                        .await;
                    }
                    Ok(ClarificationOutcome::SelectedOption { option_id, .. }) => {
                        memory.intent = Some(intent_from_source(payload, &context, canonical));
                        record_source_extraction_metadata(
                            &mut memory,
                            payload,
                            canonical,
                            &input.source_message,
                        );
                        memory.selected_capability = Some(option_id.clone());
                        memory.source_intent = payload
                            .source_intent
                            .as_ref()
                            .map(serde_json::to_value)
                            .transpose()
                            .ok()
                            .flatten();
                        memory.retrieval_evidence =
                            clarification_audit("semantic", &option_id, &input, payload);
                        pending_clarification = Some(None);
                        return execute_selected_capability(
                            memory,
                            context.recent_messages.len(),
                            option_id,
                            catalog,
                            client,
                            fineract_pool,
                            canonical,
                            Some(payload),
                            pending_clarification,
                            input.sensitive_identifier.as_ref(),
                        )
                        .await;
                    }
                    Ok(ClarificationOutcome::FreeFormOther { .. }) => {
                        memory.intent = Some(intent_from_source(payload, &context, canonical));
                        record_source_extraction_metadata(
                            &mut memory,
                            payload,
                            canonical,
                            &input.source_message,
                        );
                        memory.retrieval_evidence = json!({ "clarification_outcome": "free_form_other", "clarification_id": payload.id, "clarification_revision": payload.revision, "clarification_kind": payload.kind });
                        pending_clarification = Some(None);
                        return graph_result(
                            memory,
                            TerminalState::WaitingForUserInput,
                            "clarification_other_selected",
                            ResponseBuilder::free_form_other_prompt(),
                            context.recent_messages.len(),
                            pending_clarification,
                            clarification_transitions(
                                TerminalState::WaitingForUserInput,
                                "clarification_other_selected",
                            ),
                        );
                    }
                    Ok(outcome) => {
                        memory.intent = Some(intent);
                        memory.retrieval_evidence = json!({ "clarification_outcome": outcome });
                        return graph_result(
                            memory,
                            TerminalState::WaitingForUserInput,
                            "clarification_unresolved",
                            ResponseBuilder::clarification(payload.clone()),
                            context.recent_messages.len(),
                            None,
                            clarification_transitions(
                                TerminalState::WaitingForUserInput,
                                "clarification_unresolved",
                            ),
                        );
                    }
                    Err(error) => {
                        memory.warnings = json!([{ "message": error.to_string() }]);
                    }
                }
            }
            let retrieval_query = if intent.canonical_query_en.trim().is_empty() {
                message
            } else {
                intent.canonical_query_en.as_str()
            };
            let plan = RetrievalPlan::new(
                retrieval_query,
                &intent,
                allow_all_capabilities(&context),
                allowed_capabilities(&context),
            );
            memory.intent = Some(intent);
            match memory.intent.as_ref().map(|intent| &intent.intent) {
                Some(AssistantIntentKind::Greeting) => {
                    return graph_result(
                        memory,
                        TerminalState::Completed,
                        "greeting",
                        ResponseBuilder::greeting(),
                        context.recent_messages.len(),
                        None,
                        simple_intent_transitions(TerminalState::Completed, "greeting"),
                    );
                }
                Some(AssistantIntentKind::Help) => {
                    return graph_result(
                        memory,
                        TerminalState::Completed,
                        "help",
                        ResponseBuilder::help(),
                        context.recent_messages.len(),
                        None,
                        simple_intent_transitions(TerminalState::Completed, "help"),
                    );
                }
                Some(AssistantIntentKind::UnsafeRequest) => {
                    return graph_result(
                        memory,
                        TerminalState::BlockedByPolicy,
                        "unsafe_request",
                        ResponseBuilder::policy_blocked("This request is blocked by policy."),
                        context.recent_messages.len(),
                        None,
                        simple_intent_transitions(TerminalState::BlockedByPolicy, "unsafe_request"),
                    );
                }
                // Nothing else terminates here. `OutOfDomain` in particular is
                // a hint that rides into the plan and lowers the prior in
                // `catalog_fallback`; only the reranker, which sees real
                // capability ids/descriptions/examples, may answer
                // "unsupported".
                _ => {}
            }
            tracing::info!(
                target: "assistant::mapping",
                query = %plan.query_text,
                domain = ?plan.domain,
                request_shape = ?plan.request_shape,
                allow_all_capabilities = plan.allow_all_capabilities,
                allowed_capabilities = ?plan.allowed_capabilities,
                compatible_ids = ?catalog.map(|c| crate::assistant::retrieval::compatible_ids(&plan, c)),
                "retrieval plan"
            );
            crate::job::progress::started(crate::job::progress::Stage::Retrieval);
            let retrieval_started_at = std::time::Instant::now();
            let evidence = RetrievalEngine::retrieve(&plan, llm, knowledge, catalog).await;
            crate::job::progress::finished(
                crate::job::progress::Stage::Retrieval,
                retrieval_started_at.elapsed().as_millis() as u64,
            );
            let (evidence, warning) = match evidence {
                Ok(evidence) => (evidence, None),
                Err(error) => (Vec::new(), Some(error.to_string())),
            };
            tracing::info!(
                target: "assistant::mapping",
                evidence_count = evidence.len(),
                evidence = ?evidence.iter().map(|e| (&e.capability_id, e.score)).collect::<Vec<_>>(),
                // Issue 011 item 6: measurement before tuning. `score_gap` is
                // top-1 minus top-2; `tied_at_top` counts candidates sharing
                // the leader's score. A gap near zero with several tied means
                // ranking was decided by the reranker alone, with no prior.
                score_gap = evidence
                    .first()
                    .zip(evidence.get(1))
                    .map(|(first, second)| first.score - second.score),
                tied_at_top = evidence.first().map(|first| evidence
                    .iter()
                    .filter(|item| (item.score - first.score).abs() < f32::EPSILON)
                    .count()),
                warning = ?warning,
                "retrieval evidence"
            );
            crate::job::progress::started(crate::job::progress::Stage::Reranking);
            let reranking_started_at = std::time::Instant::now();
            let decision = LlmReranker::new(llm)
                .rerank(&plan.query_text, &evidence)
                .await;
            crate::job::progress::finished(
                crate::job::progress::Stage::Reranking,
                reranking_started_at.elapsed().as_millis() as u64,
            );
            tracing::info!(
                target: "assistant::mapping",
                decision = ?decision,
                "reranker decision"
            );
            memory.retrieval_plan = json!(plan);
            memory.retrieval_evidence = json!(evidence);
            memory.evidence_decision = json!(decision);
            if let Some(message) = warning {
                memory.warnings = json!([{ "message": message }]);
            }
            if let Some(routed_intent) = memory.intent.as_ref() {
                retrieval_trace = Some(build_retrieval_trace(
                    routed_intent,
                    &plan,
                    &evidence,
                    &decision,
                ));
            }
            match decision.decision {
                RerankerVerdict::Select => {
                    // capability_id is required when Select; treat a missing/
                    // unknown id as ambiguity and Clarify with alternatives.
                    let capability_id = decision.capability_id.clone().and_then(|id| {
                        evidence.iter().any(|e| e.capability_id == id).then_some(id)
                    });
                    match capability_id {
                        Some(capability_id) => {
                            memory.selected_capability = Some(capability_id.clone());
                            let mut result = execute_selected_capability(
                                memory,
                                context.recent_messages.len(),
                                capability_id,
                                catalog,
                                client,
                                fineract_pool,
                                canonical,
                                None,
                                None,
                                input.sensitive_identifier.as_ref(),
                            )
                            .await;
                            result.retrieval_trace = retrieval_trace.clone();
                            return result;
                        }
                        None => {
                            return clarify_retrieval_candidates(
                                memory,
                                &context,
                                &plan,
                                &evidence,
                                &decision.alternatives,
                                catalog,
                                client,
                                fineract_pool,
                                canonical,
                                &input,
                            )
                            .await;
                        }
                    }
                }
                RerankerVerdict::Clarify => {
                    return clarify_retrieval_candidates(
                        memory,
                        &context,
                        &plan,
                        &evidence,
                        &decision.alternatives,
                        catalog,
                        client,
                        fineract_pool,
                        canonical,
                        &input,
                    )
                    .await;
                }
                RerankerVerdict::Unsupported => (
                    TerminalState::Unsupported,
                    "unsupported_in_domain",
                    ResponseBuilder::unsupported(),
                ),
            }
        }
        Err(error) => {
            memory.warnings = json!([{ "message": error.to_string() }]);
            (
                TerminalState::FailedOperational,
                "intent_route_failed",
                ResponseBuilder::error(),
            )
        }
    };
    memory.graph_state = "complete_or_wait".into();
    memory.terminal_state = Some(terminal);
    memory.execution_summary = json!({
        "runtime": "semantic_assistant_graph",
        "recent_message_count": context.recent_messages.len(),
    });
    memory.structured_response = Some(response);
    let transitions = vec![
        GraphTransition {
            from: GraphState::ReceiveMessage,
            to: Some(GraphState::BuildContextWindow),
            terminal: None,
            reason: "message_received".into(),
        },
        GraphTransition {
            from: GraphState::BuildContextWindow,
            to: Some(GraphState::RouteIntent),
            terminal: None,
            reason: "context_built".into(),
        },
        GraphTransition {
            from: GraphState::RouteIntent,
            to: Some(GraphState::PlanRetrieval),
            terminal: None,
            reason: "intent_routed".into(),
        },
        GraphTransition {
            from: GraphState::PlanRetrieval,
            to: Some(GraphState::RetrieveKnowledge),
            terminal: None,
            reason: "retrieval_planned".into(),
        },
        GraphTransition {
            from: GraphState::RetrieveKnowledge,
            to: Some(GraphState::EvaluateEvidence),
            terminal: None,
            reason: "knowledge_retrieved".into(),
        },
        GraphTransition {
            from: GraphState::EvaluateEvidence,
            to: Some(GraphState::CompleteOrWait),
            terminal: None,
            reason: "evidence_evaluated".into(),
        },
        GraphTransition {
            from: GraphState::CompleteOrWait,
            to: None,
            terminal: Some(terminal),
            reason: reason.into(),
        },
    ];
    AssistantGraphTopology::new()
        .validate_sequence(&transitions)
        .expect("assistant runtime produced illegal graph transitions");
    GraphRuntimeResult {
        memory,
        transitions,
        pending_clarification,
        retrieval_trace,
    }
}
