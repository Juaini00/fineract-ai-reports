use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::assistant::{AssistantDomain, AssistantEntity, AssistantIntentKind, AssistantLanguage};

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
