use chrono::{DateTime, Duration, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::{Validate, ValidationError, ValidationErrors};

#[derive(Debug, Serialize)]
pub struct ManagementStatusResponse {
    pub provider: ProviderStatusResponse,
    pub catalog: CatalogStatusResponse,
    pub index: IndexStatusResponse,
    pub audit: AuditStatusResponse,
    pub features: ManagementFeaturesResponse,
}

#[derive(Debug, Serialize)]
pub struct ProviderStatusResponse {
    pub name: String,
    pub model: String,
}

#[derive(Debug, Serialize)]
pub struct CatalogStatusResponse {
    pub content_hash: String,
    pub validation_status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct IndexStatusResponse {
    pub status: &'static str,
    pub version_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuditStatusResponse {
    pub decision_audit_status: &'static str,
    pub telemetry: TelemetryStatusResponse,
}

#[derive(Debug, Serialize)]
pub struct AuditListResponse {
    pub items: Vec<AuditEventResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Deliberately typed projection of the immutable audit store; raw JSON is not exposed.
#[derive(Debug, Serialize)]
pub struct AuditEventResponse {
    pub id: Uuid,
    pub job_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub aggregate_type: AuditAggregateTypeResponse,
    pub event_type: AuditEventType,
    pub outcome: AuditOutcome,
    pub summary: crate::management::model::AuditSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sanitized_error: Option<crate::management::model::SanitizedError>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAggregateTypeResponse {
    ChatJob,
    ChatSession,
    Management,
}

#[derive(Debug, Serialize)]
pub struct TelemetryStatusResponse {
    pub dropped_events: u64,
    pub last_persisted_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ManagementFeaturesResponse {
    pub reference_knowledge: bool,
    pub cost_warnings: bool,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeListResponse {
    pub items: Vec<KnowledgeItemResponse>,
    pub next_cursor: Option<String>,
    pub catalog_version: String,
    pub index_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_knowledge_status: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeItemResponse {
    pub id: String,
    pub kind: KnowledgeKind,
    pub title: String,
    pub status: KnowledgeStatus,
    pub execution_mode: ExecutionMode,
    pub domain_id: String,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeDetailResponse {
    pub id: String,
    pub kind: KnowledgeKind,
    pub title: String,
    pub status: KnowledgeStatus,
    pub execution_mode: ExecutionMode,
    pub domain_id: String,
    pub data_area_ids: Vec<String>,
    pub parameters: Vec<ParameterResponse>,
    pub output_fields: Vec<OutputFieldResponse>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ParameterResponse {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub required: bool,
}

#[derive(Debug, Serialize)]
pub struct OutputFieldResponse {
    pub name: String,
    pub sensitivity: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct KnowledgePath {
    #[validate(length(min = 1, max = 256))]
    pub id: String,
}

pub const MANAGEMENT_MAX_PAGE_SIZE: u16 = 100;
pub const MANAGEMENT_MAX_AUDIT_RANGE_DAYS: i64 = 90;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeKind {
    Catalog,
    Reference,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeStatus {
    Available,
    Deferred,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    ApprovedCatalogQuery,
    CatalogMetadataOnly,
    ReferenceOnly,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    ChatJobCreated,
    KnowledgeRetrievalCompleted,
    ContextAssembled,
    PolicyEvaluated,
    ExecutionAuthorized,
    ExecutionBlocked,
    ExecutionCompleted,
    ChatClarificationRequested,
    ChatClarificationReceived,
    ChatJobCompleted,
    ChatJobFailed,
    ChatSessionArchived,
    ChatSessionDeleted,
    BusinessDateFallbackUsed,
    ExecutionResultTruncated,
    ExecutionTimedOut,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Success,
    Blocked,
    Clarification,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Delayed,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryStatus {
    Healthy,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmGroupBy {
    Day,
    Model,
    Purpose,
    Status,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WarningCode {
    UsageMissing,
    CostEstimateUnavailable,
    PriceVersionMismatch,
    TelemetryDropped,
    UnusualUsageDetected,
}

/// Opaque cursors are transport values; their encoded contents stay private to
/// the repository implementation.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(transparent)]
pub struct OpaqueCursor(String);

impl OpaqueCursor {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct KnowledgeQuery {
    pub kind: Option<KnowledgeKind>,
    pub status: Option<KnowledgeStatus>,
    #[validate(length(max = 128))]
    pub domain_id: Option<String>,
    pub cursor: Option<OpaqueCursor>,
    #[validate(range(min = 1, max = MANAGEMENT_MAX_PAGE_SIZE))]
    pub limit: Option<u16>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct AuditQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub event_type: Option<AuditEventType>,
    pub outcome: Option<AuditOutcome>,
    pub job_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub cursor: Option<OpaqueCursor>,
    #[validate(range(min = 1, max = MANAGEMENT_MAX_PAGE_SIZE))]
    pub limit: Option<u16>,
}

impl AuditQuery {
    pub fn with_job_id(mut self, job_id: Uuid) -> Self {
        self.job_id = Some(job_id);
        self
    }

    pub fn validate_time_range(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if self.from >= self.to
            || self.to - self.from > Duration::days(MANAGEMENT_MAX_AUDIT_RANGE_DAYS)
        {
            errors.add("range", invalid_time_range());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct DashboardQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

impl DashboardQuery {
    pub fn validate_time_range(&self) -> Result<(), ValidationErrors> {
        AuditQuery {
            from: self.from,
            to: self.to,
            event_type: None,
            outcome: None,
            job_id: None,
            session_id: None,
            cursor: None,
            limit: None,
        }
        .validate_time_range()
    }
}

#[derive(Debug, Serialize)]
pub struct DashboardResponse {
    pub range: DashboardRange,
    pub generated_at: DateTime<Utc>,
    pub status: ManagementStatusResponse,
    pub jobs: DashboardJobSummary,
    pub activity_by_day: Vec<DashboardDailyActivity>,
    pub llm_usage: DashboardLlmUsage,
    pub knowledge: DashboardKnowledgeSummary,
    pub recent_audit_events: Vec<AuditEventResponse>,
    pub attention_items: Vec<AttentionItem>,
}

#[derive(Debug, Serialize)]
pub struct DashboardRange {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(Debug, Serialize, Default)]
pub struct DashboardJobSummary {
    pub created: i64,
    pub completed: i64,
    pub failed: i64,
    pub blocked: i64,
    pub awaiting_clarification: i64,
    pub active: i64,
}

#[derive(Debug, Serialize)]
pub struct DashboardDailyActivity {
    pub date: String,
    pub created: i64,
    pub completed: i64,
    pub failed: i64,
    pub blocked: i64,
}

#[derive(Debug, Serialize)]
pub struct DashboardLlmUsage {
    pub calls: i64,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub unknown_usage_calls: i64,
    pub errors: i64,
    pub p95_latency_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost: Option<crate::management::usage::UsageCost>,
    pub warnings: Vec<WarningCode>,
}

#[derive(Debug, Serialize)]
pub struct DashboardKnowledgeSummary {
    pub total: i64,
    pub available: i64,
    pub deferred: i64,
    pub unavailable: i64,
    pub domains: i64,
    pub catalog_version: String,
    pub index_version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AttentionItem {
    pub id: String,
    pub kind: String,
    pub severity: AttentionSeverity,
    pub message: String,
    pub occurred_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<AttentionResource>,
}

#[derive(Debug, Serialize)]
pub struct AttentionResource {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AttentionSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Deserialize, Validate)]
pub struct LlmUsageQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub group_by: LlmGroupBy,
}

impl LlmUsageQuery {
    pub fn validate_time_range(&self) -> Result<(), ValidationErrors> {
        AuditQuery {
            from: self.from,
            to: self.to,
            event_type: None,
            outcome: None,
            job_id: None,
            session_id: None,
            cursor: None,
            limit: None,
        }
        .validate_time_range()
    }
}

fn invalid_time_range() -> ValidationError {
    let mut error = ValidationError::new("invalid_time_range");
    error.message = Some("Time range must be ordered and no longer than 90 days".into());
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_enums_use_stable_snake_case() {
        assert_eq!(
            serde_json::to_string(&AuditEventType::ChatJobCompleted).unwrap(),
            "\"chat_job_completed\""
        );
        assert_eq!(
            serde_json::to_string(&ExecutionMode::ApprovedCatalogQuery).unwrap(),
            "\"approved_catalog_query\""
        );
    }

    #[test]
    fn unknown_execution_mode_is_rejected() {
        assert!(serde_json::from_str::<ExecutionMode>("\"privileged_sql\"").is_err());
    }

    #[test]
    fn audit_range_rejects_inverted_or_unbounded_values() {
        let from = "2026-07-23T00:00:00Z".parse().unwrap();
        let to = "2026-07-22T00:00:00Z".parse().unwrap();
        let query = AuditQuery {
            from,
            to,
            event_type: None,
            outcome: None,
            job_id: None,
            session_id: None,
            cursor: None,
            limit: None,
        };
        assert!(query.validate_time_range().is_err());
    }
}
