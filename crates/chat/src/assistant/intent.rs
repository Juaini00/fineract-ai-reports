use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RequestShape {
    #[serde(default)]
    pub operation: RequestOperation,
    #[serde(default)]
    pub subject: RequestSubject,
    #[serde(default)]
    pub grouping: RequestGrouping,
    #[serde(default)]
    pub output: RequestOutput,
    #[serde(default)]
    pub pii: RequestPii,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequestOperation {
    Total,
    Summary,
    List,
    Rank,
    Trend,
    Lookup,
    RandomSample,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequestSubject {
    SavingsTransaction,
    SavingsAccount,
    Client,
    Office,
    OrganizationHierarchy,
    Product,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequestGrouping {
    None,
    Month,
    Office,
    Product,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequestOutput {
    Scalar,
    Summary,
    List,
    Ranking,
    TimeSeries,
    Lookup,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequestPii {
    None,
    ClientIdentity,
    ConditionalClientIdentity,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AssistantIntent {
    #[serde(default)]
    pub intent: AssistantIntentKind,
    #[serde(default)]
    pub domain: AssistantDomain,
    #[serde(default)]
    pub request_shape: RequestShape,
    #[serde(default = "default_language")]
    pub language: AssistantLanguage,
    #[serde(default)]
    pub entities: Vec<AssistantEntity>,
    #[serde(default)]
    pub constraints: AssistantConstraints,
    #[serde(default)]
    pub context_reference: ContextReference,
    #[serde(default)]
    pub source: Option<SourceIntentSnapshot>,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssistantIntentKind {
    #[default]
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
    #[default]
    Unknown,
}

fn default_language() -> AssistantLanguage {
    AssistantLanguage::En
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssistantLanguage {
    En,
    Id,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AssistantEntity {
    pub entity_type: AssistantEntityType,
    pub value: String,
    #[serde(default)]
    pub canonical: Option<String>,
    #[serde(default)]
    pub confidence: Option<f32>,
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
    AccountNumber,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssistantConstraints {
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub quantity: Option<Quantity>,
    pub currency_code: Option<String>,
    pub product_ids: Option<Vec<i64>>,
    #[serde(default)]
    pub office_ids: Option<Vec<i64>>,
    #[serde(default)]
    pub metric: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Quantity {
    All,
    Default,
    Limit { value: i64 },
    TopN { value: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContextReference {
    #[default]
    None,
    PreviousJob,
    PendingClarification,
    SessionTopic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceIntentSnapshot {
    pub prompt: String,
    #[serde(default)]
    pub normalized_prompt: Option<String>,
    pub intent: AssistantIntentKind,
    pub domain: AssistantDomain,
    #[serde(default)]
    pub request_shape: RequestShape,
    #[serde(default)]
    pub entities: Vec<AssistantEntity>,
    #[serde(default)]
    pub constraints: AssistantConstraints,
    #[serde(default)]
    pub context_reference: ContextReference,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub reason: String,
}
