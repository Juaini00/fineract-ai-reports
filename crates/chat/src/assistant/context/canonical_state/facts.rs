use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::assistant::{
    AssistantDomain, AssistantIntent, DeterministicExtraction, PayloadField, Quantity,
};

use super::merge::validate;
use super::{
    ConstraintContracts, ConstraintField, ConstraintPatch, FactObservation, FactSourceKind,
    LimitMode, MergeError, TypedFactValue,
};

pub fn observations_from_patch(
    job_id: Uuid,
    source_id: &str,
    first_sequence: i64,
    patch: &ConstraintPatch,
    observed_at: DateTime<Utc>,
    contracts: &ConstraintContracts,
) -> Result<Vec<FactObservation>, MergeError> {
    observations_from_patch_with_source_kind(
        job_id,
        source_id,
        first_sequence,
        FactSourceKind::Clarification,
        patch,
        observed_at,
        contracts,
    )
}

pub fn approved_default_observations(
    job_id: Uuid,
    source_id: &str,
    first_sequence: i64,
    patch: &ConstraintPatch,
    observed_at: DateTime<Utc>,
    contracts: &ConstraintContracts,
) -> Result<Vec<FactObservation>, MergeError> {
    observations_from_patch_with_source_kind(
        job_id,
        source_id,
        first_sequence,
        FactSourceKind::ApprovedDefault,
        patch,
        observed_at,
        contracts,
    )
}

pub fn observations_from_patch_with_source_kind(
    job_id: Uuid,
    source_id: &str,
    first_sequence: i64,
    source_kind: FactSourceKind,
    patch: &ConstraintPatch,
    observed_at: DateTime<Utc>,
    contracts: &ConstraintContracts,
) -> Result<Vec<FactObservation>, MergeError> {
    patch
        .iter()
        .enumerate()
        .map(|(offset, (field, value))| {
            let contract = contracts
                .get(field)
                .ok_or_else(|| MergeError::MissingContract(field.clone()))?;
            validate(field, value, contract)?;
            Ok(FactObservation {
                id: Uuid::new_v3(&job_id, format!("{source_id}:{field:?}").as_bytes()),
                job_id,
                sequence: first_sequence + offset as i64,
                source_kind,
                source_id: source_id.into(),
                field_path: field.clone(),
                typed_value: value.clone(),
                confidence: None,
                extractor_version: match source_kind {
                    FactSourceKind::Clarification => "clarification_patch_v1",
                    FactSourceKind::ApprovedDefault => "approved_default_v1",
                    _ => "constraint_patch_v1",
                }
                .into(),
                observed_at,
            })
        })
        .collect()
}

pub fn original_request_observations(
    job_id: Uuid,
    source_id: &str,
    intent: &AssistantIntent,
    extraction: &DeterministicExtraction,
    observed_at: DateTime<Utc>,
) -> Vec<FactObservation> {
    let mut facts = Vec::new();
    if let Some(quantity) = &intent.constraints.quantity {
        quantity_facts(quantity, &mut facts);
    }
    let strings = [
        (
            ConstraintField::FromDate,
            intent.constraints.from_date.as_ref(),
            0,
        ),
        (
            ConstraintField::ToDate,
            intent.constraints.to_date.as_ref(),
            0,
        ),
        (
            ConstraintField::CurrencyCode,
            intent.constraints.currency_code.as_ref(),
            1,
        ),
        (
            ConstraintField::Metric,
            intent.constraints.metric.as_ref(),
            2,
        ),
    ];
    for (field, value, kind) in strings {
        if let Some(value) = value {
            facts.push((field, string_value(kind, value.clone())));
        }
    }
    if let Some(amount) = intent.constraints.transaction_amount.as_ref()
        && amount.parse::<rust_decimal::Decimal>().is_ok()
    {
        facts.push((
            ConstraintField::TransactionAmount,
            TypedFactValue::Decimal(amount.clone()),
        ));
    }
    if intent.domain != AssistantDomain::Unknown {
        facts.push((
            ConstraintField::Domain,
            TypedFactValue::Domain(intent.domain.clone()),
        ));
    }
    let original_len = facts.len();
    facts.extend(
        extraction
            .candidates
            .iter()
            .filter_map(|candidate| candidate_fact(&candidate.field, &candidate.value)),
    );
    facts
        .into_iter()
        .enumerate()
        .map(|(index, (field_path, typed_value))| FactObservation {
            id: stable_uuid(job_id, index as u128 + 10),
            job_id,
            sequence: index as i64 + 1,
            source_kind: if index < original_len {
                FactSourceKind::OriginalRequest
            } else {
                FactSourceKind::DeterministicResolver
            },
            source_id: source_id.into(),
            field_path,
            typed_value,
            confidence: None,
            extractor_version: if index < original_len {
                "initial_request_v1"
            } else {
                "deterministic_extraction_v1"
            }
            .into(),
            observed_at,
        })
        .collect()
}

pub fn deterministic_observations_excluding_fields(
    job_id: Uuid,
    source_id: &str,
    first_sequence: i64,
    source_kind: FactSourceKind,
    extraction: &DeterministicExtraction,
    excluded_fields: &std::collections::BTreeSet<ConstraintField>,
    observed_at: DateTime<Utc>,
) -> Vec<FactObservation> {
    let mut filtered = extraction.clone();
    filtered.candidates.retain(|candidate| {
        candidate_fact(&candidate.field, &candidate.value)
            .is_none_or(|(field, _)| !excluded_fields.contains(&field))
    });
    deterministic_observations(
        job_id,
        source_id,
        first_sequence,
        source_kind,
        &filtered,
        observed_at,
    )
}

pub fn deterministic_observations(
    job_id: Uuid,
    source_id: &str,
    first_sequence: i64,
    source_kind: FactSourceKind,
    extraction: &DeterministicExtraction,
    observed_at: DateTime<Utc>,
) -> Vec<FactObservation> {
    extraction
        .candidates
        .iter()
        .filter_map(|candidate| candidate_fact(&candidate.field, &candidate.value))
        .enumerate()
        .map(|(offset, (field_path, typed_value))| FactObservation {
            id: stable_uuid(job_id, first_sequence as u128 + offset as u128 + 1000),
            job_id,
            sequence: first_sequence + offset as i64,
            source_kind,
            source_id: source_id.into(),
            field_path,
            typed_value,
            confidence: None,
            extractor_version: if source_kind == FactSourceKind::Clarification {
                "clarification_patch_v1"
            } else {
                "deterministic_extraction_v1"
            }
            .into(),
            observed_at,
        })
        .collect()
}

pub fn stable_uuid(job_id: Uuid, discriminator: u128) -> Uuid {
    Uuid::from_u128(job_id.as_u128() ^ discriminator.rotate_left(61))
}

fn quantity_facts(quantity: &Quantity, facts: &mut Vec<(ConstraintField, TypedFactValue)>) {
    let (mode, value) = match quantity {
        Quantity::All => (LimitMode::All, None),
        Quantity::Default => (LimitMode::Default, None),
        Quantity::Limit { value } => (LimitMode::Limit, Some(*value)),
        Quantity::TopN { value } => (LimitMode::TopN, Some(*value)),
    };
    facts.push((ConstraintField::LimitMode, TypedFactValue::LimitMode(mode)));
    if let Some(value) = value {
        facts.push((ConstraintField::LimitValue, TypedFactValue::Integer(value)));
    }
}

fn string_value(kind: u8, value: String) -> TypedFactValue {
    match kind {
        0 => TypedFactValue::Date(value),
        1 => TypedFactValue::CurrencyCode(value),
        _ => TypedFactValue::Metric(value),
    }
}

fn candidate_fact(
    field: &PayloadField,
    value: &serde_json::Value,
) -> Option<(ConstraintField, TypedFactValue)> {
    let text = || value.as_str().map(str::to_owned);
    match field {
        PayloadField::Limit => value
            .as_i64()
            .map(|v| (ConstraintField::LimitValue, TypedFactValue::Integer(v))),
        PayloadField::FromDate => {
            text().map(|v| (ConstraintField::FromDate, TypedFactValue::Date(v)))
        }
        PayloadField::ToDate => text().map(|v| (ConstraintField::ToDate, TypedFactValue::Date(v))),
        PayloadField::CurrencyCode => text().map(|v| {
            (
                ConstraintField::CurrencyCode,
                TypedFactValue::CurrencyCode(v),
            )
        }),
        PayloadField::Metric => {
            text().map(|v| (ConstraintField::Metric, TypedFactValue::Metric(v)))
        }
        PayloadField::Domain => serde_json::from_value(value.clone())
            .ok()
            .map(|v| (ConstraintField::Domain, TypedFactValue::Domain(v))),
        PayloadField::PersonName => {
            text().map(|v| (ConstraintField::PersonName, TypedFactValue::PersonName(v)))
        }
    }
}
