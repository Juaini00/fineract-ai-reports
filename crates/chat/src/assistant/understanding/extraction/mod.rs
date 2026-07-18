use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::assistant::{
    AssistantConstraints, AssistantDomain, AssistantEntity, AssistantEntityType, AssistantIntent,
    Quantity,
};

mod domain;
mod quantity;
mod temporal;
#[cfg(test)]
mod tests;
mod token;

use domain::{extract_domain, extract_metric};
use quantity::{extract_quantity, quantity_parts};
use temporal::resolve_temporal;
use token::{extract_currency, extract_person_name, words};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeterministicExtraction {
    pub constraints: AssistantConstraints,
    pub domain: Option<AssistantDomain>,
    pub entities: Vec<AssistantEntity>,
    #[serde(default)]
    pub candidates: Vec<PayloadCandidate>,
    #[serde(default)]
    pub temporal_provenance: Option<TemporalProvenance>,
    #[serde(default)]
    pub temporal_error: Option<TemporalValidationError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemporalProvenance {
    pub rule: String,
    pub phrase_span: [usize; 2],
    #[schemars(with = "String")]
    pub reference_instant: DateTime<Utc>,
    pub timezone: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemporalValidationError {
    pub code: String,
    pub message: String,
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
            && self.temporal_error.is_none()
    }

    pub fn merge_into(&self, intent: &mut AssistantIntent) {
        self.merge_into_legacy_non_authoritative(intent);
    }

    pub fn merge_into_legacy_non_authoritative(&self, intent: &mut AssistantIntent) {
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
    extract_message_facts_at(message, Utc::now(), 366)
}

pub fn extract_message_facts_at(
    message: &str,
    reference_instant: DateTime<Utc>,
    max_range_days: i64,
) -> DeterministicExtraction {
    let lower = message.to_lowercase();
    let words = words(&lower);
    let mut extraction = DeterministicExtraction::default();

    extraction.constraints.quantity = extract_quantity(&lower, &words);
    match resolve_temporal(message, reference_instant, max_range_days) {
        Ok(Some(resolved)) => {
            extraction.constraints.from_date = Some(resolved.from.to_string());
            extraction.constraints.to_date = Some(resolved.to.to_string());
            extraction.temporal_provenance = Some(resolved.provenance);
        }
        Ok(None) => {}
        Err(error) => extraction.temporal_error = Some(error),
    }
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
