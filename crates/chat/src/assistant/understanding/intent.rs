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
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequestSubject {
    SavingsTransaction,
    SavingsAccount,
    SavingsAccountCharge,
    Client,
    Office,
    OrganizationHierarchy,
    Product,
    #[default]
    #[serde(other)]
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
    #[serde(other)]
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
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequestPii {
    None,
    ClientIdentity,
    ConditionalClientIdentity,
    #[default]
    #[serde(other)]
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
    /// The user's message translated to English, preserving all reporting
    /// terminology, entities, and dates. Used to build the embedding query
    /// against the (English) knowledge base instead of the raw message, so
    /// non-English requests still retrieve the right capability. Leave equal
    /// to the original message when it is already English.
    #[serde(default)]
    pub canonical_query_en: String,
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
    #[serde(other)]
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
    #[serde(other)]
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
    /// Exact decimal text for an explicitly stated transaction amount. Never a
    /// floating-point value; dataset filters parse this as `Decimal`.
    #[serde(default)]
    pub transaction_amount: Option<String>,
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
    /// RESERVED for the drill-down follow-up (issue 009), not issue 007.
    /// Never produced or consumed today: no code carries a prior result set or
    /// capability forward, and nothing branches on this value. Do not delete —
    /// removing it would silently narrow the extractor's accepted contract, and
    /// the follow-up depends on it. See issue 007 §W-H / §F5.
    // ponytail: reserved surface, kept deliberately dead until issue 009 starts.
    PreviousJob,
    PendingClarification,
    /// RESERVED for the drill-down follow-up (issue 009), not issue 007.
    /// Never produced or consumed today. Do not delete — see `PreviousJob` above
    /// and issue 007 §W-H / §F5.
    // ponytail: reserved surface, kept deliberately dead until issue 009 starts.
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
