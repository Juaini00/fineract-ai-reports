use super::*;

pub(super) fn pending_clarification_intent(context: &ContextWindow) -> AssistantIntent {
    let quantity = pending_clarification_quantity(context);
    AssistantIntent {
        intent: AssistantIntentKind::ClarificationReply,
        domain: match context.active_domain.as_deref() {
            Some("savings") => AssistantDomain::Savings,
            Some("client") => AssistantDomain::Client,
            Some("organization") => AssistantDomain::Organization,
            _ => AssistantDomain::Unknown,
        },
        request_shape: Default::default(),
        language: AssistantLanguage::En,
        entities: Vec::new(),
        constraints: crate::assistant::AssistantConstraints {
            quantity,
            ..Default::default()
        },
        context_reference: ContextReference::PendingClarification,
        source: None,
        confidence: 1.0,
        reason: "exact pending clarification option".into(),
    }
}

pub(super) fn pending_clarification_quantity(context: &ContextWindow) -> Option<Quantity> {
    context
        .recent_messages
        .iter()
        .rev()
        .filter(|message| message.role == "user")
        .find_map(|message| first_standalone_limit(&message.content))
        .map(|value| Quantity::TopN { value })
}

pub(super) fn first_standalone_limit(content: &str) -> Option<i64> {
    content
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .find_map(|part| {
            part.parse::<i64>()
                .ok()
                .filter(|value| (1..=100).contains(value))
        })
}

pub(super) fn is_parameter_reply(message: &str) -> bool {
    message.split_whitespace().count() <= 6
}

pub(super) fn resolve_pending_clarification(
    input: &RuntimeUserInput,
    payload: &ClarificationPayload,
    memory: &JobMemory,
    context: &ContextWindow,
) -> Option<ClarificationOutcome> {
    input
        .selected_option_id
        .as_deref()
        .map(|id| {
            if id.eq_ignore_ascii_case(OTHER_CLARIFICATION_OPTION_ID) {
                ClarificationOutcome::FreeFormOther {
                    text: String::new(),
                    confidence: 1.0,
                }
            } else if clarification_candidate_allowed(id, payload, memory, context) {
                ClarificationOutcome::SelectedOption {
                    option_id: id.to_string(),
                    confidence: 1.0,
                }
            } else {
                ClarificationOutcome::Unresolved {
                    reason: "selected option is not available".into(),
                }
            }
        })
        .or_else(|| ClarificationResolver::resolve_exact(&input.source_message, payload))
}

pub(super) fn clarification_candidate_allowed(
    id: &str,
    payload: &ClarificationPayload,
    _memory: &JobMemory,
    context: &ContextWindow,
) -> bool {
    let is_candidate = payload.options.iter().any(|option| option.id == id);
    if !is_candidate {
        return false;
    }
    let has_scope = context.client_scope.get("allow_all_capabilities").is_some()
        || context.client_scope.get("capabilities").is_some();
    if !has_scope {
        return true;
    }
    allow_all_capabilities(context)
        || allowed_capabilities(context)
            .iter()
            .any(|capability| capability == id)
}

pub(super) fn continuation_capability(payload: &ClarificationPayload) -> Option<String> {
    if !payload.is_missing_execution_parameters {
        return None;
    }
    let mut options = payload
        .options
        .iter()
        .filter(|option| option.id != OTHER_CLARIFICATION_OPTION_ID);
    let option = options.next()?;
    options.next().is_none().then(|| option.id.clone())
}

pub(super) fn source_intent_snapshot(
    intent: &AssistantIntent,
    prompt: &str,
) -> SourceIntentSnapshot {
    SourceIntentSnapshot {
        prompt: prompt.into(),
        normalized_prompt: Some(prompt.trim().to_lowercase()),
        intent: intent.intent.clone(),
        domain: intent.domain.clone(),
        request_shape: intent.request_shape.clone(),
        entities: intent.entities.clone(),
        constraints: intent.constraints.clone(),
        context_reference: intent.context_reference.clone(),
        confidence: intent.confidence,
        reason: intent.reason.clone(),
    }
}

pub(super) fn intent_from_source(
    payload: &ClarificationPayload,
    context: &ContextWindow,
    canonical: Option<&CanonicalRuntimeContext>,
) -> AssistantIntent {
    if let Some(source) = &payload.source_intent {
        let mut intent = AssistantIntent {
            intent: source.intent.clone(),
            domain: source.domain.clone(),
            request_shape: source.request_shape.clone(),
            language: AssistantLanguage::En,
            entities: source.entities.clone(),
            constraints: source.constraints.clone(),
            context_reference: ContextReference::PendingClarification,
            source: Some(source.clone()),
            confidence: source.confidence,
            reason: format!(
                "clarification resolved from source intent: {}",
                source.reason
            ),
        };
        if matches!(intent.constraints.quantity, None | Some(Quantity::Default)) {
            intent.constraints.quantity = pending_clarification_quantity(context);
        }
        let extraction = extract_for_context(&source.prompt, canonical);
        extraction.merge_into(&mut intent);
        return intent;
    }
    pending_clarification_intent(context)
}

pub(super) fn clarification_audit(
    source: &str,
    option_id: &str,
    input: &RuntimeUserInput,
    payload: &ClarificationPayload,
) -> serde_json::Value {
    json!({
        "clarification_outcome": "selected_option",
        "option_id": option_id,
        "source_message": input.source_message,
        "source": source,
        "source_intent": payload.source_intent,
    })
}

pub(super) fn allowed_capabilities(context: &ContextWindow) -> Vec<String> {
    context
        .client_scope
        .get("capabilities")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect()
}

pub(super) fn allow_all_capabilities(context: &ContextWindow) -> bool {
    context
        .client_scope
        .get("allow_all_capabilities")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}
/// Variant of `clarification_payload` that prefers the reranker's `alternatives`
/// (capability ids) as the option pool, filtering evidence to those ids. Falls
/// back to the top-3 evidence when `alternatives` is empty (parity with the
/// pre-reranker payload builder).
pub(super) fn clarification_payload_for(
    plan: &RetrievalPlan,
    evidence: &[Evidence],
    alternatives: &[String],
    source_intent: Option<SourceIntentSnapshot>,
) -> ClarificationPayload {
    if alternatives.is_empty() {
        return clarification_payload(plan, evidence, source_intent);
    }
    let by_id: std::collections::HashMap<&str, &Evidence> = evidence
        .iter()
        .map(|e| (e.capability_id.as_str(), e))
        .collect();
    let mut options: Vec<ClarificationOption> = alternatives
        .iter()
        .filter_map(|id| {
            by_id.get(id.as_str()).map(|e| ClarificationOption {
                id: e.capability_id.clone(),
                label: e.title.clone(),
                description: e
                    .metadata
                    .get("description")
                    .or_else(|| e.metadata.get("summary"))
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned),
                fields: Vec::new(),
            })
        })
        .collect();
    if options.is_empty() {
        options = fallback_clarification_options(&plan.domain);
    }
    options.push(ClarificationOption {
        id: OTHER_CLARIFICATION_OPTION_ID.into(),
        label: "Others".into(),
        description: Some("Let me describe what I need in my own words.".into()),
        fields: Vec::new(),
    });
    ClarificationPayload {
        version: crate::assistant::clarification::CLARIFICATION_VERSION_1,
        id: uuid::Uuid::new_v4(),
        revision: 0,
        kind: crate::assistant::clarification::ClarificationKind::SelectOption,
        question: "Which report should I use?".into(),
        options,
        fields: Vec::new(),
        attempt: 1,
        source_intent,
        allow_free_text: true,
        is_missing_execution_parameters: false,
    }
}

pub(super) fn clarification_payload(
    plan: &RetrievalPlan,
    evidence: &[Evidence],
    source_intent: Option<SourceIntentSnapshot>,
) -> ClarificationPayload {
    let mut options: Vec<ClarificationOption> = evidence
        .iter()
        .take(3)
        .map(|item| ClarificationOption {
            id: item.capability_id.clone(),
            label: item.title.clone(),
            description: item
                .metadata
                .get("description")
                .or_else(|| item.metadata.get("summary"))
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            fields: Vec::new(),
        })
        .collect();
    if options.is_empty() {
        options = fallback_clarification_options(&plan.domain);
    }
    options.push(ClarificationOption {
        id: OTHER_CLARIFICATION_OPTION_ID.into(),
        label: "Others".into(),
        description: Some("Let me describe what I need in my own words.".into()),
        fields: Vec::new(),
    });
    ClarificationPayload {
        version: crate::assistant::clarification::CLARIFICATION_VERSION_1,
        id: uuid::Uuid::new_v4(),
        revision: 0,
        kind: crate::assistant::clarification::ClarificationKind::SelectOption,
        question: "Which report should I use?".into(),
        options,
        fields: Vec::new(),
        attempt: 1,
        source_intent,
        allow_free_text: true,
        is_missing_execution_parameters: false,
    }
}

pub(super) fn fallback_clarification_options(domain: &AssistantDomain) -> Vec<ClarificationOption> {
    let choices = match domain {
        AssistantDomain::Savings | AssistantDomain::Client => vec![
            (
                "client_top_n_by_savings_account_count",
                "Top clients by number of savings accounts",
                "Rank clients by savings account count.",
            ),
            (
                "client_top_n_by_savings_balance",
                "Top clients by savings balance",
                "Rank clients by total savings balance.",
            ),
            (
                "client_top_n_by_deposit_volume",
                "Top clients by deposit volume",
                "Rank clients by deposit transaction volume.",
            ),
        ],
        AssistantDomain::Organization => vec![
            (
                "organization_office_summary",
                "Office summary",
                "Summarize offices in the organization.",
            ),
            (
                "organization_office_savings_summary",
                "Office savings summary",
                "Summarize savings by office.",
            ),
            (
                "organization_office_activity_ranking",
                "Office activity ranking",
                "Rank offices by activity.",
            ),
        ],
        _ => vec![
            (
                "savings_deposit_top_n",
                "Top savings deposits",
                "Rank savings accounts by deposits.",
            ),
            (
                "savings_balance_summary",
                "Savings balance summary",
                "Summarize savings balances.",
            ),
            (
                "organization_office_summary",
                "Office summary",
                "Summarize offices in the organization.",
            ),
        ],
    };

    choices
        .into_iter()
        .map(|(id, label, description)| ClarificationOption {
            id: id.into(),
            label: label.into(),
            description: Some(description.into()),
            fields: Vec::new(),
        })
        .collect()
}

pub(super) fn clarification_transitions(
    terminal: TerminalState,
    reason: &str,
) -> Vec<GraphTransition> {
    vec![
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
            to: Some(GraphState::ResolveClarification),
            terminal: None,
            reason: "clarification_reply".into(),
        },
        GraphTransition {
            from: GraphState::ResolveClarification,
            to: Some(GraphState::CompleteOrWait),
            terminal: None,
            reason: "clarification_resolved".into(),
        },
        GraphTransition {
            from: GraphState::CompleteOrWait,
            to: None,
            terminal: Some(terminal),
            reason: reason.into(),
        },
    ]
}
