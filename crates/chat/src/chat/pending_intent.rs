use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::chat::classifier::{
    ClarificationOption, ClassificationCandidate, ClassificationOutcome, ClassificationResult,
    OTHER_ACTIVITY_CAPABILITY,
};
use crate::knowledge::model::{CapabilityKnowledge, KnowledgeCatalog};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingIntentStatus {
    CollectingSlots,
    WaitingForCapabilityChoice,
    ReadyToExecute,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingIntent {
    pub schema_version: u32,
    pub revision: u32,
    pub original_message: String,
    pub status: PendingIntentStatus,
    pub domain: Option<String>,
    pub target_entity: Option<String>,
    pub requested_shape: Option<String>,
    pub metric: Option<String>,
    pub candidate_capabilities: Vec<String>,
    pub selected_capability: Option<String>,
    pub params: Value,
    pub missing_slots: Vec<String>,
    pub last_user_response: Option<String>,
    pub invalid_attempts: u32,
}

pub enum PendingIntentResolution {
    Matched(ClassificationResult, PendingIntent),
    StillWaiting(ClassificationResult, PendingIntent),
    StartNewRequest,
    NoPending,
}

impl PendingIntent {
    pub fn is_active(&self) -> bool {
        !matches!(self.status, PendingIntentStatus::Resolved)
    }

    pub fn from_classification(
        original_message: &str,
        classification: &ClassificationResult,
        catalog: &KnowledgeCatalog,
    ) -> Option<Self> {
        if classification.outcome != ClassificationOutcome::ClarificationRequired {
            return None;
        }

        let candidate_capabilities = if classification.options.is_empty() {
            classification
                .candidates
                .iter()
                .map(|candidate| candidate.capability.clone())
                .collect::<Vec<_>>()
        } else {
            classification
                .options
                .iter()
                .map(|option| option.capability.clone())
                .filter(|capability| capability != OTHER_ACTIVITY_CAPABILITY)
                .collect::<Vec<_>>()
        };
        let selected_capability = classification.capability.clone();
        let selected = selected_capability
            .as_deref()
            .and_then(|id| capability(catalog, id));
        let first = candidate_capabilities
            .first()
            .and_then(|id| capability(catalog, id));
        let cap = selected.or(first);

        Some(Self {
            schema_version: 1,
            revision: 1,
            original_message: original_message.to_string(),
            status: if classification.options.is_empty() {
                PendingIntentStatus::CollectingSlots
            } else {
                PendingIntentStatus::WaitingForCapabilityChoice
            },
            domain: classification
                .domain
                .clone()
                .or_else(|| cap.map(|capability| capability.domain.clone())),
            target_entity: cap.map(|capability| capability.domain.clone()),
            requested_shape: cap.map(|capability| capability.output_mode.clone()),
            metric: cap.map(|capability| infer_metric(&capability.id)),
            candidate_capabilities,
            selected_capability,
            params: classification.params.clone(),
            missing_slots: missing_slots(classification),
            last_user_response: None,
            invalid_attempts: 0,
        })
    }
}

pub fn resolve_pending_intent(
    pending: Option<PendingIntent>,
    response: &str,
    today: NaiveDate,
    catalog: &KnowledgeCatalog,
) -> PendingIntentResolution {
    let Some(mut pending) = pending else {
        return PendingIntentResolution::NoPending;
    };
    let response = response.trim();
    if starts_new_request(response) {
        return PendingIntentResolution::StartNewRequest;
    }
    pending.last_user_response = Some(response.to_string());

    if pending.missing_slots.iter().any(|slot| slot == "limit")
        && let Some(limit) = crate::chat::classifier::limit_from_message(response)
    {
        pending.params["limit"] = json!(limit);
    }
    let selected_by_response = select_capability(&pending, response, catalog);
    if selected_by_response.as_ref().map(|(id, _)| id.as_str()) == Some(OTHER_ACTIVITY_CAPABILITY) {
        pending.status = PendingIntentStatus::Resolved;
        pending.revision += 1;
        return PendingIntentResolution::StillWaiting(others_classification(), pending);
    }

    if let Some((from_date, to_date)) = crate::chat::classifier::date_range(response, today) {
        pending.params["from_date"] = json!(from_date.to_string());
        pending.params["to_date"] = json!(to_date.to_string());
    }
    let filled_requested_slot = fills_missing_slot(&pending);
    let selected_explicitly = selected_by_response.is_some();
    let selected_source = selected_by_response.as_ref().map(|(_, source)| *source);
    let acknowledged = is_affirmative(response);

    let selected = selected_by_response
        .as_ref()
        .map(|(id, _)| id.clone())
        .or_else(|| pending.selected_capability.clone())
        .or_else(|| single_candidate(&pending));
    if let Some(capability_id) = selected
        && let Some(capability) = capability(catalog, &capability_id)
    {
        pending.domain = Some(capability.domain.clone());
        pending.target_entity = Some(capability.domain.clone());
        pending.requested_shape = Some(capability.output_mode.clone());
        pending.metric = Some(infer_metric(&capability.id));
        pending.missing_slots = required_missing_slots(capability, &pending.params);
        if selected_explicitly
            || pending.selected_capability.as_deref() == Some(capability.id.as_str())
        {
            pending.selected_capability = Some(capability.id.clone());
            pending.status = PendingIntentStatus::CollectingSlots;
        }
        if pending.missing_slots.is_empty()
            && (selected_explicitly || filled_requested_slot || acknowledged)
        {
            pending.status = PendingIntentStatus::Resolved;
            pending.revision += 1;
            let mut classification = classification_from_pending(pending.clone(), capability);
            if let Some(source) = selected_source {
                classification.source = Some(source.to_string());
            }
            return PendingIntentResolution::Matched(classification, pending);
        }
    }

    if !selected_explicitly
        && !filled_requested_slot
        && !acknowledged
        && looks_like_report_request(response)
    {
        return PendingIntentResolution::StartNewRequest;
    }

    pending.invalid_attempts += 1;
    pending.revision += 1;
    let classification = waiting_classification(&pending, catalog);
    PendingIntentResolution::StillWaiting(classification, pending)
}

fn fills_missing_slot(pending: &PendingIntent) -> bool {
    pending
        .missing_slots
        .iter()
        .any(|slot| pending.params.get(slot).is_some())
}

fn is_affirmative(response: &str) -> bool {
    matches!(
        response.trim().to_lowercase().as_str(),
        "ok" | "okay" | "yes" | "y" | "run" | "execute" | "lanjut" | "ya"
    )
}

fn starts_new_request(response: &str) -> bool {
    let normalized = response.to_lowercase();
    normalized.contains("other_activity")
        || normalized.contains("actually")
        || normalized.contains("instead")
        || normalized.contains("new request")
        || normalized.contains("forget that")
        || normalized.contains("ganti")
}

fn looks_like_report_request(response: &str) -> bool {
    let tokens = comparable_tokens(response);
    let has_action = has_token(
        &tokens,
        &["show", "list", "top", "total", "summary", "rank", "ranking"],
    );
    let has_domain = has_token(
        &tokens,
        &[
            "client",
            "office",
            "saving",
            "savings",
            "balance",
            "deposit",
            "withdrawal",
            "activity",
        ],
    );
    has_action && has_domain
}

fn has_token(tokens: &[String], needles: &[&str]) -> bool {
    tokens
        .iter()
        .any(|token| needles.iter().any(|needle| token == needle))
}

fn classification_from_pending(
    pending: PendingIntent,
    capability: &CapabilityKnowledge,
) -> ClassificationResult {
    ClassificationResult {
        outcome: ClassificationOutcome::Matched,
        domain: Some(capability.domain.clone()),
        capability: Some(capability.id.clone()),
        confidence: 0.8,
        params: pending.params,
        clarification: None,
        options: Vec::new(),
        source: Some("pending_intent".to_string()),
        candidates: vec![ClassificationCandidate {
            capability: capability.id.clone(),
            confidence: 0.8,
            source_type: Some("pending_intent".to_string()),
        }],
        layers: Vec::new(),
    }
}

fn waiting_classification(
    pending: &PendingIntent,
    catalog: &KnowledgeCatalog,
) -> ClassificationResult {
    let mut options = if pending.missing_slots.is_empty() || pending.selected_capability.is_none() {
        pending
            .candidate_capabilities
            .iter()
            .filter(|id| id.as_str() != OTHER_ACTIVITY_CAPABILITY)
            .map(|id| ClarificationOption {
                label: capability_display_label(catalog, id),
                capability: id.clone(),
                output_mode: None,
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if pending.missing_slots.is_empty() || pending.selected_capability.is_none() {
        options.push(ClarificationOption {
            label: catalog.classification.others_label.clone(),
            capability: OTHER_ACTIVITY_CAPABILITY.to_string(),
            output_mode: None,
        });
    }
    ClassificationResult {
        outcome: ClassificationOutcome::ClarificationRequired,
        domain: pending.domain.clone(),
        capability: None,
        confidence: 0.6,
        params: pending.params.clone(),
        clarification: Some(clarification_text(pending)),
        options,
        source: Some("pending_intent".to_string()),
        candidates: Vec::new(),
        layers: Vec::new(),
    }
}

fn clarification_text(pending: &PendingIntent) -> String {
    if !pending.missing_slots.is_empty() {
        return format!("Please clarify: {}.", pending.missing_slots.join(", "));
    }
    "Please choose one of the available report options.".to_string()
}

fn select_capability(
    pending: &PendingIntent,
    response: &str,
    catalog: &KnowledgeCatalog,
) -> Option<(String, &'static str)> {
    if !(pending.status == PendingIntentStatus::CollectingSlots
        && pending.missing_slots.iter().any(|slot| slot == "limit"))
        && let Ok(idx) = response.trim().parse::<usize>()
        && idx >= 1
        && idx <= pending.candidate_capabilities.len() + 1
    {
        return if idx <= pending.candidate_capabilities.len() {
            Some((
                pending.candidate_capabilities[idx - 1].clone(),
                "pending_intent_numeric",
            ))
        } else {
            Some((
                OTHER_ACTIVITY_CAPABILITY.to_string(),
                "clarification_other_selected",
            ))
        };
    }

    if let Some(idx) = ordinal_index(response)
        && idx >= 1
        && idx <= pending.candidate_capabilities.len() + 1
    {
        return if idx <= pending.candidate_capabilities.len() {
            Some((
                pending.candidate_capabilities[idx - 1].clone(),
                "pending_intent_numeric",
            ))
        } else {
            Some((
                OTHER_ACTIVITY_CAPABILITY.to_string(),
                "clarification_other_selected",
            ))
        };
    }

    if is_others_response(response) {
        return Some((
            OTHER_ACTIVITY_CAPABILITY.to_string(),
            "clarification_other_selected",
        ));
    }

    pending
        .candidate_capabilities
        .iter()
        .filter_map(|id| {
            capability(catalog, id).map(|capability| (id, semantic_score(response, capability)))
        })
        .filter(|(_, score)| *score > 0)
        .max_by_key(|(_, score)| *score)
        .map(|(id, _)| (id.clone(), "pending_intent_semantic"))
}

fn others_classification() -> ClassificationResult {
    ClassificationResult {
        outcome: ClassificationOutcome::ClarificationRequired,
        domain: None,
        capability: None,
        confidence: 0.6,
        params: json!({}),
        clarification: Some(
            "Sure — describe what you'd like to know in your own words.".to_string(),
        ),
        options: Vec::new(),
        source: Some("clarification_other_selected".to_string()),
        candidates: Vec::new(),
        layers: Vec::new(),
    }
}

fn is_others_response(response: &str) -> bool {
    comparable_tokens(response)
        .iter()
        .any(|token| token == "other" || token == "others")
}

fn ordinal_index(response: &str) -> Option<usize> {
    comparable_tokens(response)
        .iter()
        .find_map(|token| match token.as_str() {
            "first" | "pertama" => Some(1),
            "second" | "kedua" => Some(2),
            "third" | "ketiga" => Some(3),
            "fourth" | "keempat" => Some(4),
            _ => None,
        })
}

fn semantic_score(response: &str, capability: &CapabilityKnowledge) -> i32 {
    let response_tokens = comparable_tokens(response);
    let text = format!(
        "{} {} {} {} {}",
        capability.id.replace('_', " "),
        capability.display_name.as_deref().unwrap_or(""),
        capability.description.as_deref().unwrap_or(""),
        capability.metrics.join(" "),
        capability.examples.join(" "),
    );
    let candidate_tokens = comparable_tokens(&text);
    response_tokens
        .iter()
        .filter(|token| !is_stopword(token))
        .map(|token| {
            let has_token = candidate_tokens.iter().any(|candidate| candidate == token);
            match token.as_str() {
                "balance" | "balances" if capability.id.contains("balance") => 4,
                "count" | "number" if capability.id.contains("count") => 4,
                "deposit" | "deposits" if capability.id.contains("deposit") => 4,
                "withdraw" | "withdrawal" | "withdrawals"
                    if capability.id.contains("withdrawal") =>
                {
                    4
                }
                _ if has_token => 1,
                _ => 0,
            }
        })
        .sum()
}

fn comparable_tokens(value: &str) -> Vec<String> {
    value
        .to_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.trim_end_matches('s').to_string())
        .collect()
}

fn is_stopword(token: &str) -> bool {
    matches!(
        token,
        "the" | "a" | "an" | "one" | "please" | "by" | "with" | "show" | "me" | "yang"
    )
}

fn capability_display_label(catalog: &KnowledgeCatalog, id: &str) -> String {
    catalog
        .capabilities
        .iter()
        .find(|capability| capability.id == id)
        .and_then(|capability| capability.display_name.clone())
        .unwrap_or_else(|| id.replace('_', " "))
}

fn single_candidate(pending: &PendingIntent) -> Option<String> {
    (pending.candidate_capabilities.len() == 1).then(|| pending.candidate_capabilities[0].clone())
}

fn capability<'a>(catalog: &'a KnowledgeCatalog, id: &str) -> Option<&'a CapabilityKnowledge> {
    catalog
        .capabilities
        .iter()
        .find(|capability| capability.id == id)
}

fn missing_slots(classification: &ClassificationResult) -> Vec<String> {
    classification
        .clarification
        .as_deref()
        .filter(|text| {
            text.to_lowercase().contains("date") || text.to_lowercase().contains("period")
        })
        .map(|_| vec!["from_date".to_string(), "to_date".to_string()])
        .unwrap_or_default()
}

fn required_missing_slots(capability: &CapabilityKnowledge, params: &Value) -> Vec<String> {
    capability
        .required_parameters
        .iter()
        .filter(|name| params.get(name.as_str()).is_none())
        .cloned()
        .collect()
}

fn infer_metric(capability_id: &str) -> String {
    if capability_id.contains("savings_account_count") {
        "savings_account_count"
    } else if capability_id.contains("savings_balance") {
        "savings_balance"
    } else if capability_id.contains("deposit_volume") {
        "deposit_volume"
    } else if capability_id.contains("activity") {
        "activity"
    } else if capability_id.contains("dormant") {
        "dormant"
    } else {
        capability_id
    }
    .to_string()
}

#[cfg(test)]
mod tests;
