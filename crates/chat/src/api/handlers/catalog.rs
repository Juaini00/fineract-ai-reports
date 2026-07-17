use app_core::api::{
    error::ApiError, extractors::authenticated_chat_client::AuthenticatedChatClient, response,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::api::ChatAppState;
use crate::api::dto::catalog::{
    CatalogCapabilitiesResponse, CatalogCapabilityItem, ValidateCatalogResponse,
};
use crate::knowledge::catalog::loader::KnowledgeLoader;
use crate::knowledge::catalog::validator::validate_runtime;
use crate::knowledge::embedding::VoyageEmbeddingClient;
use crate::knowledge::index::sync::KnowledgeSyncService;

pub async fn validate(
    AuthenticatedChatClient(_client): AuthenticatedChatClient,
    State(state): State<ChatAppState>,
) -> Result<Response, ApiError> {
    validate_runtime(&state.catalog, &state.core.pools.fineract)
        .await
        .map_err(ApiError::internal)?;

    let data = ValidateCatalogResponse {
        valid: true,
        data_areas: state.catalog.data_areas.len(),
        domains: state.catalog.domains.len(),
        capabilities: state.catalog.capabilities.len(),
        queries: state.catalog.queries.len(),
    };

    Ok(response::success(StatusCode::OK, data).into_response())
}

pub async fn capabilities(
    State(state): State<ChatAppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    app_core::api::handlers::auth::authorize_bootstrap_admin(&state.core, &headers)?;

    let approved: Vec<_> = state
        .catalog
        .capabilities
        .iter()
        .filter(|capability| capability.status == "approved_mvp")
        .collect();

    let allowed_capabilities = approved.iter().map(|c| c.id.clone()).collect();
    let capabilities = approved
        .iter()
        .map(|c| CatalogCapabilityItem {
            id: c.id.clone(),
            status: c.status.clone(),
            domain: c.domain.clone(),
            display_name: c.display_name.clone(),
            description: c.description.clone(),
            data_areas: c.data_areas.clone(),
            required_parameters: c.required_parameters.clone(),
            optional_parameters: c.optional_parameters.clone(),
        })
        .collect();

    Ok(response::success(
        StatusCode::OK,
        CatalogCapabilitiesResponse {
            allowed_capabilities,
            capabilities,
        },
    )
    .into_response())
}

pub async fn vector_index_rebuild(
    AuthenticatedChatClient(_client): AuthenticatedChatClient,
    State(state): State<ChatAppState>,
) -> Result<Response, ApiError> {
    let core = &state.core;
    let loader = KnowledgeLoader::new(&core.config.catalog.path, &core.config.catalog.query_path);
    let embedding_client =
        VoyageEmbeddingClient::new(&core.config.voyage_ai).map_err(ApiError::internal)?;

    let summary = KnowledgeSyncService::with_embeddings(
        loader,
        core.pools.app.clone(),
        embedding_client,
        core.config.voyage_ai.embedding_model.clone(),
        core.config.voyage_ai.embedding_dimensions,
    )
    .sync()
    .await
    .map_err(ApiError::internal)?;

    let body = json!({
        "catalog_version_id": summary.catalog_version_id,
        "content_hash": summary.content_hash,
        "document_count": summary.document_count,
        "embedding_model": summary.embedding_model,
    });
    Ok(response::success(StatusCode::OK, body).into_response())
}

pub async fn vector_index_status(
    AuthenticatedChatClient(_client): AuthenticatedChatClient,
    State(state): State<ChatAppState>,
) -> Result<Response, ApiError> {
    let row: Option<(
        uuid::Uuid,
        String,
        String,
        String,
        i32,
        Option<String>,
        Option<i32>,
        Option<chrono::DateTime<chrono::Utc>>,
        chrono::DateTime<chrono::Utc>,
    )> = sqlx::query_as(
        r#"
        SELECT id, version, content_hash, status, document_count,
               embedding_model, embedding_dimensions, synced_at, created_at
        FROM knowledge_catalog_versions
        ORDER BY synced_at DESC NULLS LAST, created_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(&state.core.pools.app)
    .await
    .map_err(|err| ApiError::internal(anyhow::Error::from(err)))?;

    let body = match row {
        Some((id, version, hash, status, count, model, dims, synced, created)) => json!({
            "catalog_version_id": id,
            "version": version,
            "content_hash": hash,
            "status": status,
            "document_count": count,
            "embedding_model": model,
            "embedding_dimensions": dims,
            "synced_at": synced,
            "created_at": created,
        }),
        None => json!({ "status": "empty" }),
    };
    Ok(response::success(StatusCode::OK, body).into_response())
}
