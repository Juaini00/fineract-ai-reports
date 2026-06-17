pub mod api;
pub mod auth;
pub mod config;
pub mod db;
pub mod telemetry;

use std::net::SocketAddr;

use anyhow::Context;
use tokio::net::TcpListener;
use tracing::{info, warn};

pub async fn run() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    telemetry::init();

    let config = config::AppConfig::from_env()?;
    let pools = db::DatabasePools::connect(&config).await?;

    if config.app_database_migrate_on_startup {
        info!("running app database migrations");
        pools.run_app_migrations().await?;
        info!("app database migrations completed");
    }

    let readiness = pools.readiness().await;

    let state = api::AppState::new(config.clone(), pools);
    let router = api::router(state);
    let addr: SocketAddr = format!("{}:{}", config.app.host, config.app.port)
        .parse()
        .context("invalid APP_HOST or APP_PORT")?;
    let listener = TcpListener::bind(addr).await?;

    log_startup_status(&config, addr, &readiness);
    axum::serve(listener, router).await?;

    Ok(())
}

fn log_startup_status(
    config: &config::AppConfig,
    addr: SocketAddr,
    readiness: &db::ReadinessChecks,
) {
    let ready = readiness.is_ready();

    info!(
        app_env = %config.app.env,
        address = %addr,
        health_url = %format!("http://{addr}/health"),
        ready_url = %format!("http://{addr}/ready"),
        "AI Reporting Service starting"
    );

    info!(
        app_database = %readiness.app_database.label(),
        fineract_database = %readiness.fineract_database.label(),
        pgvector = %readiness.pgvector.label(),
        redis = %readiness.redis.label(),
        "dependency readiness"
    );

    if ready {
        info!("AI Reporting Service is ready to accept requests");
    } else {
        warn!("AI Reporting Service started but one or more dependencies are not ready");
    }
}
