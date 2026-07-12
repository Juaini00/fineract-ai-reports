#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use std::net::SocketAddr;

    use axum::http::{HeaderValue, Method};
    use tokio::net::TcpListener;
    use tower_http::{cors::CorsLayer, trace::TraceLayer};

    let (config, pools) = app_core::bootstrap().await?;
    let readiness = pools.readiness().await;

    let core_state = app_core::api::AppState::new(config.clone(), pools);
    core_state.auth_service.bootstrap_admin().await?;
    let chat_state = chat::api::ChatAppState::new(core_state.clone()).await?;

    let cors = CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("http://localhost:5173"),
            HeaderValue::from_static("http://127.0.0.1:5173"),
        ])
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::ACCEPT,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderName::from_static("x-api-key"),
        ])
        .allow_credentials(true);

    let router = app_core::api::router(core_state)
        .merge(chat::api::router(chat_state))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = format!("{}:{}", config.app.host, config.app.port).parse()?;
    let listener = TcpListener::bind(addr).await?;

    app_core::log_startup_status(&config, addr, &readiness);

    axum::serve(listener, router).await?;

    Ok(())
}
