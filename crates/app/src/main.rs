#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use std::net::SocketAddr;

    use tokio::net::TcpListener;
    use tower_http::trace::TraceLayer;

    let (config, pools) = app_core::bootstrap().await?;
    let readiness = pools.readiness().await;

    let core_state = app_core::api::AppState::new(config.clone(), pools);
    let chat_state = chat::api::ChatAppState::new(core_state.clone()).await?;

    let router = app_core::api::router(core_state)
        .merge(chat::api::router(chat_state))
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = format!("{}:{}", config.app.host, config.app.port).parse()?;
    let listener = TcpListener::bind(addr).await?;

    app_core::log_startup_status(&config, addr, &readiness);

    axum::serve(listener, router).await?;

    Ok(())
}
