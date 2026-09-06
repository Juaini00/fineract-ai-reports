use app_core::api::{
    error::ApiError, extractors::authenticated_management_admin::AuthenticatedManagementAdmin,
    response,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use validator::Validate;

use crate::api::ChatAppState;
use crate::api::dto::management::{
    AuditQuery, AuditStatusResponse, CatalogStatusResponse, DashboardQuery, IndexStatusResponse,
    KnowledgeListResponse, KnowledgePath, KnowledgeQuery, LlmUsageQuery,
    ManagementFeaturesResponse, ManagementStatusResponse, ProviderStatusResponse,
    TelemetryStatusResponse,
};
use crate::management::audit::{AuditLookupError, ManagementAuditRepository};
use crate::management::dashboard::DashboardService;
use crate::management::knowledge::{KnowledgeLookupError, KnowledgeService};
use crate::management::repository::outbox_health;
use crate::management::usage::LlmUsageRepository;

pub async fn status(
    AuthenticatedManagementAdmin { .. }: AuthenticatedManagementAdmin,
    State(state): State<ChatAppState>,
) -> Result<Response, ApiError> {
    let knowledge = KnowledgeService::new(state.catalog);
    let health = outbox_health(&state.core.pools.app)
        .await
        .map_err(ApiError::internal)?;
    let decision_audit_status = if health.exhausted > 0 || health.pending > 0 {
        "delayed"
    } else {
        "healthy"
    };
    Ok(response::success(
        StatusCode::OK,
        ManagementStatusResponse {
            provider: ProviderStatusResponse {
                name: state.core.config.llm.provider,
                model: state.core.config.llm.model,
            },
            catalog: CatalogStatusResponse {
                content_hash: knowledge.catalog_version(),
                validation_status: "valid",
            },
            // This slice does not read an index repository. An in-memory catalog
            // is usable even before an index has been created.
            index: IndexStatusResponse {
                status: "unavailable",
                version_id: None,
            },
            audit: AuditStatusResponse {
                decision_audit_status,
                telemetry: TelemetryStatusResponse {
                    dropped_events: 0,
                    last_persisted_at: None,
                },
            },
            features: ManagementFeaturesResponse {
                reference_knowledge: false,
                cost_warnings: true,
            },
        },
    )
    .into_response())
}

pub async fn audit(
    AuthenticatedManagementAdmin { .. }: AuthenticatedManagementAdmin,
    State(state): State<ChatAppState>,
    Query(query): Query<AuditQuery>,
) -> Result<Response, ApiError> {
    query.validate().map_err(ApiError::validation)?;
    query.validate_time_range().map_err(ApiError::validation)?;
    audit_response(&state, &query).await
}

pub async fn audit_job(
    AuthenticatedManagementAdmin { .. }: AuthenticatedManagementAdmin,
    State(state): State<ChatAppState>,
    Path(job_id): Path<uuid::Uuid>,
    Query(query): Query<AuditQuery>,
) -> Result<Response, ApiError> {
    query.validate().map_err(ApiError::validation)?;
    let query = query.with_job_id(job_id);
    query.validate_time_range().map_err(ApiError::validation)?;
    audit_response(&state, &query).await
}

async fn audit_response(state: &ChatAppState, query: &AuditQuery) -> Result<Response, ApiError> {
    let list = ManagementAuditRepository::new(state.core.pools.app.clone())
        .list(query)
        .await
        .map_err(|error| match error {
            AuditLookupError::InvalidCursor => {
                ApiError::bad_request_with_code("invalid_cursor", "Audit cursor is invalid.", None)
            }
            AuditLookupError::Internal(error) => ApiError::internal(error),
        })?;
    Ok(response::success(StatusCode::OK, list).into_response())
}

pub async fn llm_usage(
    AuthenticatedManagementAdmin { .. }: AuthenticatedManagementAdmin,
    State(state): State<ChatAppState>,
    Query(query): Query<LlmUsageQuery>,
) -> Result<Response, ApiError> {
    query.validate().map_err(ApiError::validation)?;
    query.validate_time_range().map_err(ApiError::validation)?;
    let usage = LlmUsageRepository::new(state.core.pools.app.clone())
        .aggregate(&query)
        .await
        .map_err(ApiError::internal)?;
    Ok(response::success(StatusCode::OK, usage).into_response())
}

pub async fn knowledge(
    AuthenticatedManagementAdmin { .. }: AuthenticatedManagementAdmin,
    State(state): State<ChatAppState>,
    Query(query): Query<KnowledgeQuery>,
) -> Result<Response, ApiError> {
    query.validate().map_err(ApiError::validation)?;
    let list = KnowledgeService::new(state.catalog)
        .list(&query)
        .map_err(|error| match error {
            KnowledgeLookupError::InvalidCursor => ApiError::bad_request_with_code(
                "invalid_cursor",
                "Knowledge cursor is invalid.",
                None,
            ),
        })?;

    Ok(response::success(
        StatusCode::OK,
        KnowledgeListResponse {
            items: list.items,
            next_cursor: list.next_cursor,
            catalog_version: list.catalog_version,
            index_version: list.index_version,
            reference_knowledge_status: list.reference_knowledge_status,
        },
    )
    .into_response())
}

pub async fn knowledge_detail(
    AuthenticatedManagementAdmin { .. }: AuthenticatedManagementAdmin,
    State(state): State<ChatAppState>,
    Path(path): Path<KnowledgePath>,
) -> Result<Response, ApiError> {
    path.validate().map_err(ApiError::validation)?;
    let detail = KnowledgeService::new(state.catalog)
        .detail(&path.id)
        .ok_or_else(|| ApiError::not_found("Knowledge item was not found."))?;
    Ok(response::success(StatusCode::OK, detail).into_response())
}

pub async fn dashboard(
    AuthenticatedManagementAdmin { .. }: AuthenticatedManagementAdmin,
    State(state): State<ChatAppState>,
    Query(query): Query<DashboardQuery>,
) -> Result<Response, ApiError> {
    query.validate().map_err(ApiError::validation)?;
    query.validate_time_range().map_err(ApiError::validation)?;
    let service = DashboardService::new(
        state.core.pools.app.clone(),
        state.catalog.clone(),
        state.core.config.llm.provider.clone(),
        state.core.config.llm.model.clone(),
    );
    let snapshot = service
        .snapshot(query.from, query.to)
        .await
        .map_err(ApiError::internal)?;
    Ok(response::success(StatusCode::OK, snapshot).into_response())
}
