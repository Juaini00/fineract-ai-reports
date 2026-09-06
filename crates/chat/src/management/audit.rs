use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::api::dto::management::{
    AuditAggregateTypeResponse, AuditEventResponse, AuditEventType, AuditListResponse,
    AuditOutcome, AuditQuery, OpaqueCursor,
};

use super::model::{
    AuditEventType as ModelEventType, AuditOutcome as ModelOutcome, AuditSummary, SanitizedError,
};

#[derive(Clone)]
pub struct ManagementAuditRepository {
    pool: PgPool,
}

impl ManagementAuditRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(&self, query: &AuditQuery) -> Result<AuditListResponse, AuditLookupError> {
        let cursor = query.cursor.as_ref().map(decode_cursor).transpose()?;
        let limit = query.limit.unwrap_or(50) as i64;
        let rows = sqlx::query_as::<_, AuditEventRow>(
            "SELECT id, job_id, session_id, aggregate_type, event_type, outcome, summary_json, sanitized_error_json, occurred_at FROM management_audit_events WHERE occurred_at >= $1 AND occurred_at < $2 AND ($3::text IS NULL OR event_type = $3) AND ($4::text IS NULL OR outcome = $4) AND ($5::uuid IS NULL OR job_id = $5) AND ($6::uuid IS NULL OR session_id = $6) AND ($7::timestamptz IS NULL OR (occurred_at, id) < ($7, $8)) ORDER BY occurred_at DESC, id DESC LIMIT $9",
        )
        .bind(query.from)
        .bind(query.to)
        .bind(query.event_type.map(event_type))
        .bind(query.outcome.map(outcome))
        .bind(query.job_id)
        .bind(query.session_id)
        .bind(cursor.map(|cursor| cursor.occurred_at))
        .bind(cursor.map(|cursor| cursor.id))
        .bind(limit + 1)
        .fetch_all(&self.pool)
        .await?;

        let has_more = rows.len() as i64 > limit;
        let rows = rows.into_iter().take(limit as usize).collect::<Vec<_>>();
        let next_cursor = has_more.then(|| {
            let row = rows.last().expect("a page with another row is non-empty");
            encode_cursor(Cursor {
                occurred_at: row.occurred_at,
                id: row.id,
            })
        });
        let items = rows
            .into_iter()
            .map(AuditEventResponse::try_from)
            .collect::<Result<_, _>>()?;
        Ok(AuditListResponse { items, next_cursor })
    }
}

#[derive(FromRow)]
struct AuditEventRow {
    id: Uuid,
    job_id: Option<Uuid>,
    session_id: Option<Uuid>,
    aggregate_type: String,
    event_type: String,
    outcome: String,
    summary_json: serde_json::Value,
    sanitized_error_json: Option<serde_json::Value>,
    occurred_at: DateTime<Utc>,
}

impl TryFrom<AuditEventRow> for AuditEventResponse {
    type Error = anyhow::Error;

    fn try_from(row: AuditEventRow) -> Result<Self> {
        Ok(Self {
            id: row.id,
            job_id: row.job_id,
            session_id: row.session_id,
            aggregate_type: aggregate_type(&row.aggregate_type)?,
            event_type: parse_event_type(&row.event_type)?,
            outcome: parse_outcome(&row.outcome)?,
            summary: serde_json::from_value::<AuditSummary>(row.summary_json)?,
            sanitized_error: row
                .sanitized_error_json
                .map(serde_json::from_value::<SanitizedError>)
                .transpose()?,
            occurred_at: row.occurred_at,
        })
    }
}

#[derive(Clone, Copy)]
struct Cursor {
    occurred_at: DateTime<Utc>,
    id: Uuid,
}

fn encode_cursor(cursor: Cursor) -> String {
    hex::encode(format!("{}|{}", cursor.occurred_at.to_rfc3339(), cursor.id))
}

fn decode_cursor(cursor: &OpaqueCursor) -> Result<Cursor, AuditLookupError> {
    let value = hex::decode(cursor.as_str()).map_err(|_| AuditLookupError::InvalidCursor)?;
    let value = String::from_utf8(value).map_err(|_| AuditLookupError::InvalidCursor)?;
    let (occurred_at, id) = value
        .split_once('|')
        .ok_or(AuditLookupError::InvalidCursor)?;
    Ok(Cursor {
        occurred_at: DateTime::parse_from_rfc3339(occurred_at)
            .map_err(|_| AuditLookupError::InvalidCursor)?
            .with_timezone(&Utc),
        id: Uuid::parse_str(id).map_err(|_| AuditLookupError::InvalidCursor)?,
    })
}

fn aggregate_type(value: &str) -> Result<AuditAggregateTypeResponse> {
    match value {
        "chat_job" => Ok(AuditAggregateTypeResponse::ChatJob),
        "chat_session" => Ok(AuditAggregateTypeResponse::ChatSession),
        "management" => Ok(AuditAggregateTypeResponse::Management),
        _ => Err(anyhow!("unknown audit aggregate type")),
    }
}
fn event_type(value: AuditEventType) -> &'static str {
    match value {
        AuditEventType::ChatJobCreated => ModelEventType::ChatJobCreated.as_str(),
        AuditEventType::KnowledgeRetrievalCompleted => {
            ModelEventType::KnowledgeRetrievalCompleted.as_str()
        }
        AuditEventType::ContextAssembled => ModelEventType::ContextAssembled.as_str(),
        AuditEventType::PolicyEvaluated => ModelEventType::PolicyEvaluated.as_str(),
        AuditEventType::ExecutionAuthorized => ModelEventType::ExecutionAuthorized.as_str(),
        AuditEventType::ExecutionBlocked => ModelEventType::ExecutionBlocked.as_str(),
        AuditEventType::ExecutionCompleted => ModelEventType::ExecutionCompleted.as_str(),
        AuditEventType::ChatClarificationRequested => {
            ModelEventType::ChatClarificationRequested.as_str()
        }
        AuditEventType::ChatClarificationReceived => {
            ModelEventType::ChatClarificationReceived.as_str()
        }
        AuditEventType::ChatJobCompleted => ModelEventType::ChatJobCompleted.as_str(),
        AuditEventType::ChatJobFailed => ModelEventType::ChatJobFailed.as_str(),
        AuditEventType::ChatSessionArchived => ModelEventType::ChatSessionArchived.as_str(),
        AuditEventType::ChatSessionDeleted => ModelEventType::ChatSessionDeleted.as_str(),
        AuditEventType::BusinessDateFallbackUsed => ModelEventType::BusinessDateFallback.as_str(),
        AuditEventType::ExecutionResultTruncated => {
            ModelEventType::ExecutionResultTruncated.as_str()
        }
        AuditEventType::ExecutionTimedOut => ModelEventType::ExecutionTimedOut.as_str(),
    }
}
fn outcome(value: AuditOutcome) -> &'static str {
    match value {
        AuditOutcome::Success => ModelOutcome::Success.as_str(),
        AuditOutcome::Blocked => ModelOutcome::Blocked.as_str(),
        AuditOutcome::Clarification => ModelOutcome::Clarification.as_str(),
        AuditOutcome::Unsupported => ModelOutcome::Unsupported.as_str(),
        AuditOutcome::Failed => ModelOutcome::Failed.as_str(),
    }
}

fn parse_event_type(value: &str) -> Result<AuditEventType> {
    serde_json::from_str(&format!("\"{}\"", value.replace('.', "_"))).map_err(Into::into)
}
fn parse_outcome(value: &str) -> Result<AuditOutcome> {
    serde_json::from_str(&format!("\"{value}\"")).map_err(Into::into)
}

#[derive(Debug)]
pub enum AuditLookupError {
    InvalidCursor,
    Internal(anyhow::Error),
}
impl From<sqlx::Error> for AuditLookupError {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(error.into())
    }
}
impl From<anyhow::Error> for AuditLookupError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}
