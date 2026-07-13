use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AssistantIntent {
    pub intent: AssistantIntentKind,
    pub domain: AssistantDomain,
    pub language: AssistantLanguage,
    #[serde(default)]
    pub entities: Vec<AssistantEntity>,
    #[serde(default)]
    pub constraints: AssistantConstraints,
    pub context_reference: ContextReference,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssistantIntentKind {
    Greeting,
    Help,
    ReportRequest,
    DataLookup,
    ClarificationReply,
    FollowUp,
    UnsafeRequest,
    OutOfDomain,
    UnsupportedInDomain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssistantDomain {
    Savings,
    Client,
    Organization,
    GroupCenter,
    Loan,
    Accounting,
    Tax,
    Audit,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssistantLanguage {
    En,
    Id,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssistantEntity {
    pub entity_type: AssistantEntityType,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssistantEntityType {
    PersonName,
    Office,
    DatePeriod,
    Currency,
    Product,
    Metric,
    CapabilityHint,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssistantConstraints {
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub quantity: Option<Quantity>,
    pub currency_code: Option<String>,
    pub product_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Quantity {
    All,
    Default,
    Limit { value: i64 },
    TopN { value: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextReference {
    None,
    PreviousJob,
    PendingClarification,
    SessionTopic,
}
