use super::*;

pub(super) fn apply_constraint_patch(
    intent: &mut AssistantIntent,
    patch: &crate::assistant::ConstraintPatch,
) {
    for (field, value) in patch {
        match (field, value) {
            (ConstraintField::FromDate, TypedFactValue::Date(value)) => {
                intent.constraints.from_date = Some(value.clone())
            }
            (ConstraintField::ToDate, TypedFactValue::Date(value)) => {
                intent.constraints.to_date = Some(value.clone())
            }
            (ConstraintField::LimitValue, TypedFactValue::Integer(value)) => {
                match intent.constraints.quantity {
                    Some(Quantity::TopN { .. }) => {
                        intent.constraints.quantity = Some(Quantity::TopN { value: *value })
                    }
                    _ => intent.constraints.quantity = Some(Quantity::Limit { value: *value }),
                }
            }
            (ConstraintField::LimitMode, TypedFactValue::LimitMode(mode)) => match mode {
                LimitMode::TopN => {
                    intent.constraints.quantity = Some(Quantity::TopN {
                        value: patch
                            .get(&ConstraintField::LimitValue)
                            .and_then(|v| match v {
                                TypedFactValue::Integer(v) => Some(*v),
                                _ => None,
                            })
                            .unwrap_or(1),
                    })
                }
                LimitMode::Limit => {
                    intent.constraints.quantity = Some(Quantity::Limit {
                        value: patch
                            .get(&ConstraintField::LimitValue)
                            .and_then(|v| match v {
                                TypedFactValue::Integer(v) => Some(*v),
                                _ => None,
                            })
                            .unwrap_or(1),
                    })
                }
                LimitMode::All => intent.constraints.quantity = Some(Quantity::All),
                LimitMode::Default => intent.constraints.quantity = Some(Quantity::Default),
            },
            _ => {}
        }
    }
}

pub(super) fn clarification_facts(
    intent: Option<&AssistantIntent>,
    patch: &crate::assistant::ConstraintPatch,
) -> ClarificationFacts {
    let mut values = patch.clone();
    if let Some(intent) = intent {
        if let Some(value) = &intent.constraints.from_date {
            values
                .entry(ConstraintField::FromDate)
                .or_insert_with(|| TypedFactValue::Date(value.clone()));
        }
        if let Some(value) = &intent.constraints.to_date {
            values
                .entry(ConstraintField::ToDate)
                .or_insert_with(|| TypedFactValue::Date(value.clone()));
        }
        if let Some(quantity) = &intent.constraints.quantity {
            let (mode, value) = match quantity {
                Quantity::TopN { value } => (LimitMode::TopN, Some(*value)),
                Quantity::Limit { value } => (LimitMode::Limit, Some(*value)),
                Quantity::All => (LimitMode::All, None),
                Quantity::Default => (LimitMode::Default, None),
            };
            values
                .entry(ConstraintField::LimitMode)
                .or_insert(TypedFactValue::LimitMode(mode));
            if let Some(value) = value {
                values
                    .entry(ConstraintField::LimitValue)
                    .or_insert(TypedFactValue::Integer(value));
            }
        }
    }
    ClarificationFacts { values }
}

pub(super) fn planned_clarification(
    catalog: &KnowledgeCatalog,
    candidate_ids: &[String],
    intent: Option<&AssistantIntent>,
    patch: &crate::assistant::ConstraintPatch,
    source_intent: Option<SourceIntentSnapshot>,
    existing: Option<&ClarificationPayload>,
) -> ClarificationPlanResult {
    let id = existing
        .map(|payload| payload.id)
        .unwrap_or_else(uuid::Uuid::new_v4);
    match ClarificationPlanner::new(catalog).plan(
        candidate_ids,
        &clarification_facts(intent, patch),
        id,
    ) {
        ClarificationPlanResult::Clarify {
            mut payload,
            approved_defaults,
        } => {
            payload.revision = existing.map(|payload| payload.revision + 1).unwrap_or(1);
            payload.attempt = existing.map(|payload| payload.attempt).unwrap_or(1);
            payload.source_intent = source_intent;
            ClarificationPlanResult::Clarify {
                payload,
                approved_defaults,
            }
        }
        complete => complete,
    }
}

pub(super) const MAX_CLARIFICATION_ATTEMPTS: u32 = 3;

pub(super) fn incremented_clarification(payload: &ClarificationPayload) -> ClarificationPayload {
    let mut next = payload.clone();
    next.attempt = next.attempt.saturating_add(1);
    next.revision = next.revision.saturating_add(1);
    next
}

pub(super) fn is_meaningful_free_text(message: &str) -> bool {
    let normalized = message.trim().to_ascii_lowercase();
    !normalized.is_empty()
        && normalized != OTHER_CLARIFICATION_OPTION_ID
        && normalized != "other"
        && !normalized.starts_with("let me describe")
        && !normalized.starts_with("i'll describe")
}

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
        canonical_query_en: String::new(),
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
                if !payload
                    .options
                    .iter()
                    .any(|option| option.id.eq_ignore_ascii_case(id))
                {
                    ClarificationOutcome::Unresolved {
                        reason: "selected option is not available".into(),
                    }
                } else if is_meaningful_free_text(&input.source_message) {
                    ClarificationOutcome::NewRequest {
                        message: input.source_message.clone(),
                        confidence: 1.0,
                    }
                } else {
                    ClarificationOutcome::FreeFormOther {
                        text: String::new(),
                        confidence: 1.0,
                    }
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
    if payload.kind == crate::assistant::ClarificationKind::SelectEntity {
        return id
            .strip_prefix("client:")
            .is_some_and(|value| value.parse::<i64>().is_ok());
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
            canonical_query_en: String::new(),
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
        "clarification_id": payload.id,
        "clarification_revision": payload.revision,
        "clarification_kind": payload.kind,
        "source": source,
        "structured": input.clarification_id.is_some(),
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
                description: clarification_option_description(
                    e.metadata
                        .get("description")
                        .or_else(|| e.metadata.get("summary"))
                        .and_then(|value| value.as_str()),
                    source_intent.as_ref(),
                ),
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
        workflow_id: None,
        node_id: None,
        resume_node_id: None,
        entity_kind: None,
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
            description: clarification_option_description(
                item.metadata
                    .get("description")
                    .or_else(|| item.metadata.get("summary"))
                    .and_then(|value| value.as_str()),
                source_intent.as_ref(),
            ),
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
        workflow_id: None,
        node_id: None,
        resume_node_id: None,
        entity_kind: None,
    }
}

fn clarification_option_description(
    description: Option<&str>,
    source_intent: Option<&SourceIntentSnapshot>,
) -> Option<String> {
    let period = source_intent.and_then(|intent| {
        Some(format!(
            "{} to {}",
            intent.constraints.from_date.as_ref()?,
            intent.constraints.to_date.as_ref()?,
        ))
    });
    description.map(|description| match period {
        Some(period) => description.replace("a date range", &period),
        None => description.to_owned(),
    })
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
