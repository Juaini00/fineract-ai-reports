use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::{
    api::{
        AppState,
        dto::auth::{
            CreateApiKeyRequest, CreateApiKeyResponse, LoginRequest, LoginResponse, LogoutResponse,
            RefreshResponse,
        },
        error::ApiError,
        extractors::{authenticated_user::AuthenticatedUser, validated_json::ValidatedJson},
        response,
    },
    auth::model::{CreateApiKeyInput, LoginInput},
};

pub(crate) async fn create_api_key(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    ValidatedJson(request): ValidatedJson<CreateApiKeyRequest>,
) -> Result<Response, ApiError> {
    if user.role != "admin" {
        return Err(ApiError::forbidden("admin role is required"));
    }
    let profile = state
        .auth_service
        .get_user(user.user_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::unauthorized("invalid access token"))?;

    let created = state
        .auth_service
        .create_api_key(CreateApiKeyInput {
            name: request.name,
            owner: profile.username,
            expires_at: request.expires_at,
            allowed_office_ids: request.allowed_office_ids,
            allowed_capabilities: request.allowed_capabilities,
            allow_all_offices: request.allow_all_offices,
            allow_all_capabilities: request.allow_all_capabilities,
            can_view_pii: request.can_view_pii,
            user_id: Some(user.user_id),
        })
        .await
        .map_err(ApiError::internal)?;

    Ok(response::success(
        StatusCode::CREATED,
        CreateApiKeyResponse {
            id: created.id,
            api_key: created.raw_key,
            message: "Store this API key securely. It will not be shown again.",
        },
    )
    .into_response())
}

pub(crate) async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    ValidatedJson(request): ValidatedJson<LoginRequest>,
) -> Result<Response, ApiError> {
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string);
    let (login, refresh_token) = state
        .auth_service
        .login(LoginInput {
            username: request.username,
            password: request.password,
            user_agent,
            ip_address: None,
        })
        .await
        .map_err(|_| ApiError::unauthorized("invalid username or password"))?;

    let mut res = response::success(
        StatusCode::OK,
        LoginResponse {
            access_token: login.access_token,
            token_type: login.token_type,
            expires_in: login.expires_in,
            user: login.user,
        },
    )
    .into_response();
    res.headers_mut()
        .insert(header::SET_COOKIE, refresh_cookie(&state, refresh_token)?);
    Ok(res)
}

pub(crate) async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let token = extract_refresh_cookie(&state, &headers)
        .ok_or_else(|| ApiError::unauthorized("missing refresh token"))?;
    let refreshed = state
        .auth_service
        .refresh(&token)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::unauthorized("invalid refresh token"))?;

    Ok(response::success(
        StatusCode::OK,
        RefreshResponse {
            access_token: refreshed.access_token,
            token_type: refreshed.token_type,
            expires_in: refreshed.expires_in,
        },
    )
    .into_response())
}

pub(crate) async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(token) = extract_refresh_cookie(&state, &headers) {
        state
            .auth_service
            .logout(&token)
            .await
            .map_err(ApiError::internal)?;
    }

    let mut res = response::success(
        StatusCode::OK,
        LogoutResponse {
            message: "logged out",
        },
    )
    .into_response();
    res.headers_mut()
        .insert(header::SET_COOKIE, clear_refresh_cookie(&state)?);
    Ok(res)
}

pub(crate) async fn get_user_me(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Response, ApiError> {
    let profile = state
        .auth_service
        .get_user(user.user_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::unauthorized("invalid access token"))?;
    Ok(response::success(StatusCode::OK, profile).into_response())
}

pub fn authorize_bootstrap_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let token = extract_bearer_token(headers)
        .ok_or_else(|| ApiError::unauthorized("missing Authorization: Bearer <bootstrap_token>"))?;

    if token == state.config.auth.bootstrap_admin_token {
        Ok(())
    } else {
        Err(ApiError::forbidden("invalid bootstrap admin token"))
    }
}

fn refresh_cookie(state: &AppState, raw_token: String) -> Result<HeaderValue, ApiError> {
    let same_site = match state.config.auth.refresh_cookie_same_site.as_str() {
        "lax" => cookie::SameSite::Lax,
        "none" => cookie::SameSite::None,
        _ => cookie::SameSite::Strict,
    };
    let cookie = cookie::Cookie::build((state.config.auth.refresh_cookie_name.clone(), raw_token))
        .http_only(true)
        .secure(state.config.auth.refresh_cookie_secure)
        .same_site(same_site)
        .path(state.config.auth.refresh_cookie_path.clone())
        .max_age(time::Duration::seconds(
            state.config.auth.jwt_refresh_token_expiry_seconds,
        ))
        .build();
    HeaderValue::from_str(&cookie.to_string()).map_err(|err| ApiError::internal(err.into()))
}

fn clear_refresh_cookie(state: &AppState) -> Result<HeaderValue, ApiError> {
    let cookie = cookie::Cookie::build((state.config.auth.refresh_cookie_name.clone(), ""))
        .http_only(true)
        .secure(state.config.auth.refresh_cookie_secure)
        .path(state.config.auth.refresh_cookie_path.clone())
        .max_age(time::Duration::seconds(0))
        .build();
    HeaderValue::from_str(&cookie.to_string()).map_err(|err| ApiError::internal(err.into()))
}

fn extract_refresh_cookie(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let prefix = format!("{}=", state.config.auth.refresh_cookie_name);
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(&prefix))
        .map(ToString::to_string)
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value.strip_prefix("Bearer ")?.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}
