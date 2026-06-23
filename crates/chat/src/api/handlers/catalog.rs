use app_core::api::{
    error::ApiError, extractors::authenticated_client::AuthenticatedClient, response,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::api::ChatAppState;
use crate::api::dto::catalog::ValidateCatalogResponse;

pub async fn validate(
    AuthenticatedClient(_client): AuthenticatedClient,
    State(state): State<ChatAppState>,
) -> Result<Response, ApiError> {
    let data = ValidateCatalogResponse {
        valid: true,
        data_areas: state.catalog.data_areas.len(),
        domains: state.catalog.domains.len(),
        capabilities: state.catalog.capabilities.len(),
        queries: state.catalog.queries.len(),
    };

    Ok(response::success(StatusCode::OK, data).into_response())
}
