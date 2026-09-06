use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SafeIdentifier(String);

impl TryFrom<String> for SafeIdentifier {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
            return Err("identifier must be 1-128 ASCII identifier characters");
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizedErrorCode {
    DatabaseUnavailable,
    DispatcherUnavailable,
    SerializationFailed,
    ProviderUnavailable,
    ProviderTimeout,
    ProviderMalformed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizedError {
    pub code: NormalizedErrorCode,
}

/// Typed, allowlisted payload persisted in the management audit outbox.
/// This intentionally has no arbitrary JSON field: audit producers must select
/// one of the safe summaries below.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuditSummary {
    JobCreated,
    JobCompleted,
    JobFailed,
    ClarificationRequested,
    ClarificationReceived,
    PolicyEvaluated {
        capability_id: SafeIdentifier,
        result: PolicyResult,
    },
    Execution {
        query_id: SafeIdentifier,
        row_count: Option<u64>,
    },
    SessionArchived,
    BusinessDateFallback {
        resolved_date: SafeIdentifier,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyResult {
    Allowed,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeAuditEvent {
    pub aggregate_type: AuditAggregateType,
    pub aggregate_id: Uuid,
    pub event_type: AuditEventType,
    pub outcome: AuditOutcome,
    pub summary: AuditSummary,
    pub sanitized_error: Option<SanitizedError>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAggregateType {
    ChatJob,
    ChatSession,
    Management,
}

impl AuditAggregateType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ChatJob => "chat_job",
            Self::ChatSession => "chat_session",
            Self::Management => "management",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AuditEventType {
    #[serde(rename = "chat.job_created")]
    ChatJobCreated,
    #[serde(rename = "knowledge.retrieval_completed")]
    KnowledgeRetrievalCompleted,
    #[serde(rename = "context.assembled")]
    ContextAssembled,
    #[serde(rename = "policy.evaluated")]
    PolicyEvaluated,
    #[serde(rename = "execution.authorized")]
    ExecutionAuthorized,
    #[serde(rename = "execution.blocked")]
    ExecutionBlocked,
    #[serde(rename = "execution.completed")]
    ExecutionCompleted,
    #[serde(rename = "chat.clarification_requested")]
    ChatClarificationRequested,
    #[serde(rename = "chat.clarification_received")]
    ChatClarificationReceived,
    #[serde(rename = "chat.job_completed")]
    ChatJobCompleted,
    #[serde(rename = "chat.job_failed")]
    ChatJobFailed,
    #[serde(rename = "chat.session_archived")]
    ChatSessionArchived,
    #[serde(rename = "chat.session_deleted")]
    ChatSessionDeleted,
    #[serde(rename = "business_date.fallback_used")]
    BusinessDateFallback,
    #[serde(rename = "execution.result_truncated")]
    ExecutionResultTruncated,
    #[serde(rename = "execution.timed_out")]
    ExecutionTimedOut,
}

impl AuditEventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ChatJobCreated => "chat.job_created",
            Self::KnowledgeRetrievalCompleted => "knowledge.retrieval_completed",
            Self::ContextAssembled => "context.assembled",
            Self::PolicyEvaluated => "policy.evaluated",
            Self::ExecutionAuthorized => "execution.authorized",
            Self::ExecutionBlocked => "execution.blocked",
            Self::ExecutionCompleted => "execution.completed",
            Self::ChatClarificationRequested => "chat.clarification_requested",
            Self::ChatClarificationReceived => "chat.clarification_received",
            Self::ChatJobCompleted => "chat.job_completed",
            Self::ChatJobFailed => "chat.job_failed",
            Self::ChatSessionArchived => "chat.session_archived",
            Self::ChatSessionDeleted => "chat.session_deleted",
            Self::BusinessDateFallback => "business_date.fallback_used",
            Self::ExecutionResultTruncated => "execution.result_truncated",
            Self::ExecutionTimedOut => "execution.timed_out",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Success,
    Blocked,
    Clarification,
    Unsupported,
    Failed,
}

impl AuditOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Blocked => "blocked",
            Self::Clarification => "clarification",
            Self::Unsupported => "unsupported",
            Self::Failed => "failed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_and_timeout_event_types_round_trip() {
        for (variant, expected) in [
            (
                AuditEventType::ExecutionResultTruncated,
                "execution.result_truncated",
            ),
            (AuditEventType::ExecutionTimedOut, "execution.timed_out"),
        ] {
            assert_eq!(variant.as_str(), expected);
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, format!("\"{expected}\""));
            let parsed: AuditEventType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.as_str(), expected);
        }
    }

    #[test]
    fn audit_identifiers_reject_sql_and_unbounded_text() {
        assert!(SafeIdentifier::try_from("approved_query".to_string()).is_ok());
        assert!(SafeIdentifier::try_from("select * from accounts".to_string()).is_err());
        assert!(SafeIdentifier::try_from("x".repeat(129)).is_err());
    }
}
