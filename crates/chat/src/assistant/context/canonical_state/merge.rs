use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{
    ConstraintContract, ConstraintContracts, ConstraintField, EffectiveConstraints,
    FactObservation, FactSourceKind, ListPatch, MergeError, TypedFactValue,
};

pub fn merge_observations(
    job_id: Uuid,
    revision: i64,
    observations: &[FactObservation],
    contracts: &ConstraintContracts,
) -> Result<EffectiveConstraints, MergeError> {
    let mut ordered = observations.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|o| (rank(o.source_kind), o.sequence, o.id));
    let mut seen: HashMap<(FactSourceKind, &str, ConstraintField), &TypedFactValue> =
        HashMap::new();
    let mut values = BTreeMap::new();
    let mut winners = BTreeMap::new();
    let mut created_at = DateTime::<Utc>::UNIX_EPOCH;
    for o in ordered {
        if o.job_id != job_id {
            return Err(MergeError::JobMismatch);
        }
        if o.confidence
            .is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        {
            return Err(MergeError::InvalidConfidence);
        }
        let contract = contracts
            .get(&o.field_path)
            .ok_or_else(|| MergeError::MissingContract(o.field_path.clone()))?;
        validate(&o.field_path, &o.typed_value, contract)?;
        let key = (o.source_kind, o.source_id.as_str(), o.field_path.clone());
        if let Some(old) = seen.insert(key, &o.typed_value) {
            if old != &o.typed_value {
                return Err(MergeError::ConflictingReplay(o.field_path.clone()));
            }
            continue;
        }
        if o.source_kind == FactSourceKind::LlmAdvisory {
            continue;
        }
        apply(&mut values, &o.field_path, &o.typed_value);
        winners.insert(o.field_path.clone(), o.id);
        created_at = created_at.max(o.observed_at);
    }
    Ok(EffectiveConstraints {
        id: Uuid::nil(),
        job_id,
        revision,
        schema_version: 1,
        values,
        winning_observation_ids: winners,
        created_at,
    })
}

fn rank(source: FactSourceKind) -> u8 {
    match source {
        FactSourceKind::LlmAdvisory => 0,
        FactSourceKind::ApprovedDefault => 1,
        FactSourceKind::DeterministicResolver => 2,
        FactSourceKind::OriginalRequest => 3,
        FactSourceKind::Clarification => 4,
    }
}

pub(super) fn validate(
    field: &ConstraintField,
    value: &TypedFactValue,
    contract: &ConstraintContract,
) -> Result<(), MergeError> {
    if value == &TypedFactValue::Clear {
        return contract
            .clearable
            .then_some(())
            .ok_or_else(|| MergeError::ClearRejected(field.clone()));
    }
    let valid = matches!(
        (field, value),
        (ConstraintField::LimitMode, TypedFactValue::LimitMode(_))
            | (ConstraintField::LimitValue, TypedFactValue::Integer(_))
            | (
                ConstraintField::FromDate | ConstraintField::ToDate,
                TypedFactValue::Date(_)
            )
            | (
                ConstraintField::CurrencyCode,
                TypedFactValue::CurrencyCode(_)
            )
            | (ConstraintField::Metric, TypedFactValue::Metric(_))
            | (ConstraintField::Domain, TypedFactValue::Domain(_))
            | (ConstraintField::PersonName, TypedFactValue::PersonName(_))
            | (ConstraintField::Office, TypedFactValue::Office(_))
            | (ConstraintField::Product, TypedFactValue::Product(_))
            | (
                ConstraintField::ProductIds | ConstraintField::OfficeIds,
                TypedFactValue::IdList(_)
            )
    ) && contract.list == matches!(value, TypedFactValue::IdList(_));
    valid
        .then_some(())
        .ok_or_else(|| MergeError::InvalidValue(field.clone()))
}

fn apply(
    values: &mut BTreeMap<ConstraintField, TypedFactValue>,
    field: &ConstraintField,
    value: &TypedFactValue,
) {
    match value {
        TypedFactValue::Clear => {
            values.remove(field);
        }
        TypedFactValue::IdList(patch) => {
            let mut ids = match values.get(field) {
                Some(TypedFactValue::IdList(ListPatch::Replace(ids))) => ids.clone(),
                _ => vec![],
            };
            match patch {
                ListPatch::Replace(new) => ids = new.clone(),
                ListPatch::Add(new) => {
                    for id in new {
                        if !ids.contains(id) {
                            ids.push(*id);
                        }
                    }
                }
                ListPatch::Remove(old) => ids.retain(|id| !old.contains(id)),
            }
            values.insert(
                field.clone(),
                TypedFactValue::IdList(ListPatch::Replace(ids)),
            );
        }
        _ => {
            values.insert(field.clone(), value.clone());
        }
    }
}
