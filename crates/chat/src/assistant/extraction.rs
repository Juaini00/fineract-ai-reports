use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::assistant::{
    AssistantConstraints, AssistantDomain, AssistantEntity, AssistantEntityType, AssistantIntent,
    Quantity,
};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeterministicExtraction {
    pub constraints: AssistantConstraints,
    pub domain: Option<AssistantDomain>,
    pub entities: Vec<AssistantEntity>,
    #[serde(default)]
    pub candidates: Vec<PayloadCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PayloadCandidate {
    pub field: PayloadField,
    pub value: serde_json::Value,
    pub source: PayloadSource,
    pub trust: PayloadTrust,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeterministicExtractionConflict {
    pub field: PayloadField,
    pub llm_value: serde_json::Value,
    pub trusted_value: serde_json::Value,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PayloadField {
    Limit,
    FromDate,
    ToDate,
    CurrencyCode,
    Metric,
    Domain,
    PersonName,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PayloadSource {
    UserText,
    LlmClaim,
    CatalogDefault,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PayloadTrust {
    Trusted,
    Untrusted,
    Rejected,
}

impl DeterministicExtraction {
    pub fn is_empty(&self) -> bool {
        self.constraints == AssistantConstraints::default()
            && self.domain.is_none()
            && self.entities.is_empty()
            && self.candidates.is_empty()
    }

    pub fn merge_into(&self, intent: &mut AssistantIntent) {
        if let Some(quantity) = &self.constraints.quantity {
            match (&intent.constraints.quantity, quantity) {
                (None | Some(Quantity::Default), _) => {
                    intent.constraints.quantity = Some(quantity.clone());
                }
                (
                    Some(Quantity::Limit { value: old } | Quantity::TopN { value: old }),
                    Quantity::Limit { value: new } | Quantity::TopN { value: new },
                ) if old != new => {
                    intent.constraints.quantity = Some(quantity.clone());
                }
                _ => {}
            }
        }
        if self.constraints.from_date.is_some() {
            intent.constraints.from_date = self.constraints.from_date.clone();
        }
        if self.constraints.to_date.is_some() {
            intent.constraints.to_date = self.constraints.to_date.clone();
        }
        if self.constraints.currency_code.is_some() {
            intent.constraints.currency_code = self.constraints.currency_code.clone();
        }
        if let Some(metric) = &self.constraints.metric
            && intent.constraints.metric.is_none()
        {
            intent.constraints.metric = Some(metric.clone());
        }
        if matches!(intent.domain, AssistantDomain::Unknown)
            && let Some(domain) = &self.domain
        {
            intent.domain = domain.clone();
        }
        for entity in &self.entities {
            let canonical = entity.canonical.as_deref().unwrap_or(&entity.value);
            let exists = intent.entities.iter().any(|existing| {
                existing.entity_type == entity.entity_type
                    && existing.canonical.as_deref().unwrap_or(&existing.value) == canonical
            });
            if !exists {
                intent.entities.push(entity.clone());
            }
        }
    }

    pub fn conflicts_with(&self, intent: &AssistantIntent) -> Vec<DeterministicExtractionConflict> {
        let mut conflicts = Vec::new();
        if let (Some(llm), Some(trusted)) =
            (&intent.constraints.quantity, &self.constraints.quantity)
            && let (Some((llm_mode, llm_value)), Some((trusted_mode, trusted_value))) =
                (quantity_parts(llm), quantity_parts(trusted))
            && (llm_mode != trusted_mode || llm_value != trusted_value)
        {
            conflicts.push(conflict(
                PayloadField::Limit,
                serde_json::json!({ "mode": llm_mode, "value": llm_value }),
                serde_json::json!({ "mode": trusted_mode, "value": trusted_value }),
            ));
        }
        push_string_conflict(
            &mut conflicts,
            PayloadField::FromDate,
            intent.constraints.from_date.as_deref(),
            self.constraints.from_date.as_deref(),
        );
        push_string_conflict(
            &mut conflicts,
            PayloadField::ToDate,
            intent.constraints.to_date.as_deref(),
            self.constraints.to_date.as_deref(),
        );
        push_string_conflict(
            &mut conflicts,
            PayloadField::CurrencyCode,
            intent.constraints.currency_code.as_deref(),
            self.constraints.currency_code.as_deref(),
        );
        push_string_conflict(
            &mut conflicts,
            PayloadField::Metric,
            intent.constraints.metric.as_deref(),
            self.constraints.metric.as_deref(),
        );
        if let Some(trusted) = &self.domain
            && !matches!(intent.domain, AssistantDomain::Unknown)
            && intent.domain != *trusted
        {
            conflicts.push(conflict(
                PayloadField::Domain,
                serde_json::json!(intent.domain),
                serde_json::json!(trusted),
            ));
        }
        conflicts
    }
}

fn quantity_parts(quantity: &Quantity) -> Option<(&'static str, i64)> {
    match quantity {
        Quantity::Limit { value } => Some(("limit", *value)),
        Quantity::TopN { value } => Some(("top_n", *value)),
        Quantity::All | Quantity::Default => None,
    }
}

fn push_string_conflict(
    conflicts: &mut Vec<DeterministicExtractionConflict>,
    field: PayloadField,
    llm: Option<&str>,
    trusted: Option<&str>,
) {
    if let (Some(llm), Some(trusted)) = (llm, trusted)
        && llm != trusted
    {
        conflicts.push(conflict(
            field,
            serde_json::json!(llm),
            serde_json::json!(trusted),
        ));
    }
}

fn conflict(
    field: PayloadField,
    llm_value: serde_json::Value,
    trusted_value: serde_json::Value,
) -> DeterministicExtractionConflict {
    DeterministicExtractionConflict {
        field,
        llm_value,
        trusted_value,
        reason: "deterministic_extraction_preferred".into(),
    }
}

pub fn extract_message_facts(message: &str) -> DeterministicExtraction {
    let lower = message.to_lowercase();
    let words = words(&lower);
    let mut extraction = DeterministicExtraction::default();

    extraction.constraints.quantity = extract_quantity(&lower, &words);
    extract_dates(&words, &mut extraction.constraints);
    extraction.constraints.currency_code = extract_currency(message);
    extraction.domain = extract_domain(&lower);
    if let Some(metric) = extract_metric(&lower) {
        extraction.constraints.metric = Some(metric.into());
        extraction.entities.push(AssistantEntity {
            entity_type: AssistantEntityType::Metric,
            value: metric.into(),
            canonical: Some(metric.into()),
            confidence: Some(1.0),
        });
    }
    if let Some(name) = extract_person_name(message) {
        extraction.entities.push(AssistantEntity {
            entity_type: AssistantEntityType::PersonName,
            value: name.clone(),
            canonical: Some(name),
            confidence: Some(1.0),
        });
    }

    record_candidates(&mut extraction);

    extraction
}

fn record_candidates(extraction: &mut DeterministicExtraction) {
    if let Some(quantity) = &extraction.constraints.quantity {
        let value = match quantity {
            Quantity::Limit { value } | Quantity::TopN { value } => *value,
            Quantity::Default => return,
            Quantity::All => return,
        };
        extraction
            .candidates
            .push(candidate(PayloadField::Limit, serde_json::json!(value)));
    }
    if let Some(value) = &extraction.constraints.from_date {
        extraction
            .candidates
            .push(candidate(PayloadField::FromDate, serde_json::json!(value)));
    }
    if let Some(value) = &extraction.constraints.to_date {
        extraction
            .candidates
            .push(candidate(PayloadField::ToDate, serde_json::json!(value)));
    }
    if let Some(value) = &extraction.constraints.currency_code {
        extraction.candidates.push(candidate(
            PayloadField::CurrencyCode,
            serde_json::json!(value),
        ));
    }
    if let Some(value) = &extraction.constraints.metric {
        extraction
            .candidates
            .push(candidate(PayloadField::Metric, serde_json::json!(value)));
    }
    if let Some(value) = &extraction.domain {
        extraction
            .candidates
            .push(candidate(PayloadField::Domain, serde_json::json!(value)));
    }
    for entity in &extraction.entities {
        if entity.entity_type == AssistantEntityType::PersonName {
            extraction.candidates.push(candidate(
                PayloadField::PersonName,
                serde_json::json!(entity.canonical.as_deref().unwrap_or(&entity.value)),
            ));
        }
    }
}

fn candidate(field: PayloadField, value: serde_json::Value) -> PayloadCandidate {
    PayloadCandidate {
        field,
        value,
        source: PayloadSource::UserText,
        trust: PayloadTrust::Trusted,
    }
}

fn words(message: &str) -> Vec<&str> {
    message
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-')
        .filter(|part| !part.is_empty())
        .collect()
}

fn extract_quantity(message: &str, words: &[&str]) -> Option<Quantity> {
    for (idx, word) in words.iter().enumerate() {
        let Ok(value) = word.parse::<i64>() else {
            continue;
        };
        if !(1..=100).contains(&value) {
            continue;
        }
        let near = words[idx.saturating_sub(2)..usize::min(words.len(), idx + 3)].join(" ");
        return Some(
            if near.contains("top")
                || message.contains(" most ")
                || message.contains("highest")
                || message.contains("rank")
            {
                Quantity::TopN { value }
            } else {
                Quantity::Limit { value }
            },
        );
    }
    None
}

fn extract_dates(words: &[&str], constraints: &mut AssistantConstraints) {
    let dates = words
        .iter()
        .copied()
        .filter(|word| is_iso_date(word))
        .map(str::to_string)
        .collect::<Vec<_>>();
    if let Some(from_idx) = words.iter().position(|word| *word == "from")
        && let Some(date) = words
            .iter()
            .skip(from_idx + 1)
            .copied()
            .find(|word| is_iso_date(word))
    {
        constraints.from_date = Some(date.into());
    }
    if let Some(to_idx) = words.iter().position(|word| *word == "to")
        && let Some(date) = words
            .iter()
            .skip(to_idx + 1)
            .copied()
            .find(|word| is_iso_date(word))
    {
        constraints.to_date = Some(date.into());
    }
    if constraints.from_date.is_none() {
        constraints.from_date = dates.first().cloned();
    }
    if constraints.to_date.is_none() {
        constraints.to_date = dates.get(1).cloned();
    }
}

fn is_iso_date(word: &str) -> bool {
    let bytes = word.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(idx, byte)| idx == 4 || idx == 7 || byte.is_ascii_digit())
}

fn extract_currency(message: &str) -> Option<String> {
    message
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .find(|word| matches!(*word, "IDR" | "USD" | "EUR" | "AED" | "SAR"))
        .map(str::to_string)
}

fn extract_domain(message: &str) -> Option<AssistantDomain> {
    if message.contains("client") {
        Some(AssistantDomain::Client)
    } else if message.contains("office") || message.contains("organization") {
        Some(AssistantDomain::Organization)
    } else if message.contains("saving") {
        Some(AssistantDomain::Savings)
    } else {
        None
    }
}

fn extract_metric(message: &str) -> Option<&'static str> {
    if message.contains("most savings account") || message.contains("number of savings account") {
        Some("savings_account_count")
    } else if message.contains("highest balance") || message.contains("savings balance") {
        Some("savings_balance")
    } else if message.contains("deposit volume") || message.contains("deposited the most") {
        Some("deposit_volume")
    } else {
        None
    }
}

fn extract_person_name(message: &str) -> Option<String> {
    let parts = message
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    for pair in parts.windows(2) {
        if matches!(pair[0].to_ascii_lowercase().as_str(), "named" | "name")
            && pair[1].chars().any(char::is_alphabetic)
        {
            return Some(pair[1].to_string());
        }
    }
    for pair in parts.windows(2) {
        let next = pair[1].to_ascii_lowercase();
        if matches!(pair[0].to_ascii_lowercase().as_str(), "client" | "find")
            && !matches!(next.as_str(), "client" | "name" | "named")
            && pair[1].chars().any(char::is_alphabetic)
        {
            return Some(pair[1].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_quantity_currency_date_metric() {
        let extraction = extract_message_facts(
            "show top 10 clients with the most savings accounts in USD from 2026-01-01 to 2026-01-31",
        );

        assert_eq!(
            extraction.constraints.quantity,
            Some(Quantity::TopN { value: 10 })
        );
        assert_eq!(extraction.constraints.currency_code.as_deref(), Some("USD"));
        assert_eq!(
            extraction.constraints.from_date.as_deref(),
            Some("2026-01-01")
        );
        assert_eq!(
            extraction.constraints.to_date.as_deref(),
            Some("2026-01-31")
        );
        assert_eq!(
            extraction.constraints.metric.as_deref(),
            Some("savings_account_count")
        );
        assert!(extraction.candidates.iter().any(|candidate| {
            candidate.field == PayloadField::Limit && candidate.trust == PayloadTrust::Trusted
        }));
    }

    #[test]
    fn extraction_merges_metric_when_absent() {
        let extraction = extract_message_facts("show 10 clients with the most savings accounts");
        let mut intent = AssistantIntent {
            intent: Default::default(),
            domain: AssistantDomain::Unknown,
            language: crate::assistant::AssistantLanguage::En,
            entities: Vec::new(),
            constraints: Default::default(),
            context_reference: Default::default(),
            source: None,
            confidence: 0.0,
            reason: String::new(),
        };

        extraction.merge_into(&mut intent);

        assert_eq!(
            intent.constraints.quantity,
            Some(Quantity::TopN { value: 10 })
        );
        assert_eq!(
            intent.constraints.metric.as_deref(),
            Some("savings_account_count")
        );
        assert_eq!(intent.domain, AssistantDomain::Client);
        assert!(
            intent
                .entities
                .iter()
                .any(|entity| entity.entity_type == AssistantEntityType::Metric)
        );
    }

    #[test]
    fn extracts_trusted_person_name() {
        let extraction = extract_message_facts("find client named Tony");

        assert!(extraction.entities.iter().any(|entity| {
            entity.entity_type == AssistantEntityType::PersonName && entity.value == "Tony"
        }));
        assert!(extraction.candidates.iter().any(|candidate| {
            candidate.field == PayloadField::PersonName && candidate.trust == PayloadTrust::Trusted
        }));
    }
}
