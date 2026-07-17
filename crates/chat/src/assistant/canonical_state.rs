use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::assistant::{
    AssistantDomain, AssistantEntity, AssistantIntent, AssistantIntentKind, AssistantLanguage,
    DeterministicExtraction, PayloadField, Quantity,
};

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintField {
    LimitMode,
    LimitValue,
    FromDate,
    ToDate,
    CurrencyCode,
    Metric,
    Domain,
    PersonName,
    Office,
    Product,
    ProductIds,
    OfficeIds,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LimitMode {
    All,
    Default,
    Limit,
    TopN,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", content = "values", rename_all = "snake_case")]
pub enum ListPatch<T> {
    Replace(Vec<T>),
    Add(Vec<T>),
    Remove(Vec<T>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum TypedFactValue {
    Clear,
    LimitMode(LimitMode),
    Integer(i64),
    Date(String),
    CurrencyCode(String),
    Metric(String),
    Domain(AssistantDomain),
    PersonName(String),
    Office(String),
    Product(String),
    IdList(ListPatch<i64>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtractionProvenance {
    pub extractor: String,
    pub version: String,
    #[serde(default)]
    pub source_identifiers: Vec<String>,
    #[serde(default)]
    pub source_spans: Vec<[usize; 2]>,
    #[serde(default)]
    pub rule: Option<String>,
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    pub reference_instant: Option<DateTime<Utc>>,
    #[serde(default)]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OriginalIntent {
    #[schemars(with = "String")]
    pub id: Uuid,
    #[schemars(with = "String")]
    pub job_id: Uuid,
    pub schema_version: i32,
    #[schemars(with = "String")]
    pub raw_message_id: Uuid,
    pub locale: AssistantLanguage,
    pub action: AssistantIntentKind,
    pub entities: Vec<AssistantEntity>,
    pub metrics: Vec<String>,
    pub groupings: Vec<String>,
    pub output: Option<String>,
    pub parameters: BTreeMap<String, TypedFactValue>,
    pub pii_request: bool,
    pub extraction_provenance: Vec<ExtractionProvenance>,
    #[schemars(with = "String")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FactSourceKind {
    OriginalRequest,
    Clarification,
    DeterministicResolver,
    ApprovedDefault,
    LlmAdvisory,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FactObservation {
    #[schemars(with = "String")]
    pub id: Uuid,
    #[schemars(with = "String")]
    pub job_id: Uuid,
    pub sequence: i64,
    pub source_kind: FactSourceKind,
    pub source_id: String,
    pub field_path: ConstraintField,
    pub typed_value: TypedFactValue,
    #[serde(default, deserialize_with = "deserialize_confidence")]
    pub confidence: Option<f32>,
    pub extractor_version: String,
    #[schemars(with = "String")]
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EffectiveConstraints {
    #[schemars(with = "String")]
    pub id: Uuid,
    #[schemars(with = "String")]
    pub job_id: Uuid,
    pub revision: i64,
    pub schema_version: i32,
    pub values: BTreeMap<ConstraintField, TypedFactValue>,
    #[schemars(with = "BTreeMap<ConstraintField, String>")]
    pub winning_observation_ids: BTreeMap<ConstraintField, Uuid>,
    #[schemars(with = "String")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrincipalProjection {
    #[schemars(with = "String")]
    pub user_id: Uuid,
    pub role: String,
    pub capability_ids: Vec<String>,
    pub office_ids: Vec<i64>,
    pub can_view_pii: bool,
    #[schemars(with = "Option<String>")]
    pub legacy_api_key_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlannerInputSnapshot {
    #[schemars(with = "String")]
    pub id: Uuid,
    #[schemars(with = "String")]
    pub job_id: Uuid,
    pub revision: i64,
    #[schemars(with = "String")]
    pub original_intent_id: Uuid,
    #[schemars(with = "String")]
    pub effective_constraints_id: Uuid,
    #[schemars(with = "String")]
    pub capability_catalog_version: Uuid,
    pub principal_projection: PrincipalProjection,
    #[schemars(with = "String")]
    pub reference_instant: DateTime<Utc>,
    pub timezone: String,
    pub selected_capability_id: String,
    pub normalized_parameters: serde_json::Value,
    #[schemars(with = "String")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConstraintContract {
    pub clearable: bool,
    pub list: bool,
}

pub type ConstraintContracts = BTreeMap<ConstraintField, ConstraintContract>;
pub type ConstraintPatch = BTreeMap<ConstraintField, TypedFactValue>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeError {
    JobMismatch,
    InvalidConfidence,
    MissingContract(ConstraintField),
    InvalidValue(ConstraintField),
    ClearRejected(ConstraintField),
    ConflictingReplay(ConstraintField),
}

impl std::fmt::Display for MergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for MergeError {}

pub fn executable_constraint_contracts() -> ConstraintContracts {
    use ConstraintField::*;
    [
        (LimitMode, false, false),
        (LimitValue, true, false),
        (FromDate, true, false),
        (ToDate, true, false),
        (CurrencyCode, true, false),
        (Metric, true, false),
        (Domain, false, false),
        (PersonName, true, false),
        (Office, true, false),
        (Product, true, false),
        (ProductIds, true, true),
        (OfficeIds, true, true),
    ]
    .into_iter()
    .map(|(field, clearable, list)| (field, ConstraintContract { clearable, list }))
    .collect()
}

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

pub fn observations_from_patch(
    job_id: Uuid,
    source_id: &str,
    first_sequence: i64,
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
                id: Uuid::new_v4(),
                job_id,
                sequence: first_sequence + offset as i64,
                source_kind: FactSourceKind::Clarification,
                source_id: source_id.into(),
                field_path: field.clone(),
                typed_value: value.clone(),
                confidence: None,
                extractor_version: "clarification_patch_v1".into(),
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

fn rank(source: FactSourceKind) -> u8 {
    match source {
        FactSourceKind::LlmAdvisory => 0,
        FactSourceKind::ApprovedDefault => 1,
        FactSourceKind::DeterministicResolver => 2,
        FactSourceKind::OriginalRequest => 3,
        FactSourceKind::Clarification => 4,
    }
}

fn deserialize_confidence<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<f32>::deserialize(deserializer)?;
    if value.is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v)) {
        return Err(serde::de::Error::custom(
            "confidence must be finite and between 0 and 1",
        ));
    }
    Ok(value)
}

fn validate(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(
        sequence: i64,
        source: FactSourceKind,
        source_id: &str,
        field: ConstraintField,
        value: TypedFactValue,
    ) -> FactObservation {
        FactObservation {
            id: Uuid::from_u128(sequence as u128 + 1),
            job_id: Uuid::from_u128(1),
            sequence,
            source_kind: source,
            source_id: source_id.into(),
            field_path: field,
            typed_value: value,
            confidence: Some(1.0),
            extractor_version: "test".into(),
            observed_at: DateTime::<Utc>::UNIX_EPOCH,
        }
    }

    #[test]
    fn precedence_clear_and_list_algebra() {
        let observations = vec![
            fact(
                1,
                FactSourceKind::ApprovedDefault,
                "d",
                ConstraintField::Metric,
                TypedFactValue::Metric("balance".into()),
            ),
            fact(
                2,
                FactSourceKind::OriginalRequest,
                "r",
                ConstraintField::Metric,
                TypedFactValue::Metric("sum".into()),
            ),
            fact(
                3,
                FactSourceKind::Clarification,
                "c1",
                ConstraintField::Metric,
                TypedFactValue::Metric("count".into()),
            ),
            fact(
                4,
                FactSourceKind::Clarification,
                "c1",
                ConstraintField::ProductIds,
                TypedFactValue::IdList(ListPatch::Replace(vec![1, 2])),
            ),
            fact(
                5,
                FactSourceKind::Clarification,
                "c2",
                ConstraintField::ProductIds,
                TypedFactValue::IdList(ListPatch::Add(vec![2, 3])),
            ),
            fact(
                6,
                FactSourceKind::Clarification,
                "c3",
                ConstraintField::ProductIds,
                TypedFactValue::IdList(ListPatch::Remove(vec![1])),
            ),
        ];
        for input in [
            observations.clone(),
            observations.iter().rev().cloned().collect(),
        ] {
            let result = merge_observations(
                Uuid::from_u128(1),
                1,
                &input,
                &executable_constraint_contracts(),
            )
            .unwrap();
            assert_eq!(
                result.values[&ConstraintField::Metric],
                TypedFactValue::Metric("count".into())
            );
            assert_eq!(
                result.values[&ConstraintField::ProductIds],
                TypedFactValue::IdList(ListPatch::Replace(vec![2, 3]))
            );
            assert_eq!(
                result.winning_observation_ids[&ConstraintField::Metric],
                observations[2].id
            );
        }
    }

    #[test]
    fn replay_clear_and_patch_contracts() {
        let contracts = executable_constraint_contracts();
        let a = fact(
            1,
            FactSourceKind::Clarification,
            "same",
            ConstraintField::Metric,
            TypedFactValue::Metric("a".into()),
        );
        let duplicate = FactObservation {
            id: Uuid::from_u128(9),
            sequence: 2,
            ..a.clone()
        };
        assert!(merge_observations(a.job_id, 1, &[a.clone(), duplicate], &contracts).is_ok());
        let conflict = fact(
            2,
            FactSourceKind::Clarification,
            "same",
            ConstraintField::Metric,
            TypedFactValue::Metric("b".into()),
        );
        assert!(matches!(
            merge_observations(a.job_id, 1, &[a, conflict], &contracts),
            Err(MergeError::ConflictingReplay(_))
        ));
        let clear = fact(
            1,
            FactSourceKind::Clarification,
            "c",
            ConstraintField::Domain,
            TypedFactValue::Clear,
        );
        assert_eq!(
            merge_observations(clear.job_id, 1, &[clear], &contracts),
            Err(MergeError::ClearRejected(ConstraintField::Domain))
        );
        let patch = BTreeMap::from([(
            ConstraintField::Metric,
            TypedFactValue::Metric("count".into()),
        )]);
        assert_eq!(
            observations_from_patch(Uuid::nil(), "c", 1, &patch, Utc::now(), &contracts)
                .unwrap()
                .len(),
            1
        );
    }
}
