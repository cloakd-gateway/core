mod api;
mod cache;
mod config;
mod dlp;
mod error;
mod provider;
mod stream;
mod upstream;
mod vault;

use crate::api::{create_router, AppState};
use crate::cache::PromptCache;
use crate::config::AppConfig;
use crate::dlp::DlpEngine;
use crate::provider::ProviderRegistry;
use crate::upstream::UpstreamClient;
use std::sync::Arc;
use tokio::signal;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Load configuration
    let config = AppConfig::from_env().map_err(|e| anyhow::anyhow!("Configuration error: {e}"))?;

    // 2. Initialize tracing with env filter
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().json().flatten_event(true))
        .init();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        default_model = %config.default_model,
        cache_enabled = config.cache_enabled,
        failover_enabled = config.failover_enabled,
        "Starting Cloakd Reverse Proxy Gateway"
    );

    let config_arc = Arc::new(config.clone());
    let upstream_client = Arc::new(UpstreamClient::new(config_arc.clone()));
    let dlp_engine = Arc::new(DlpEngine::from_rules_str(&config.enabled_rules));
    let provider_registry = Arc::new(ProviderRegistry::from_config(&config));
    let prompt_cache = Arc::new(PromptCache::from_config(&config));

    let state = AppState {
        config: config_arc,
        upstream_client,
        dlp_engine,
        provider_registry,
        prompt_cache,
    };

    let app = create_router(state);

    let socket_addr = config.socket_addr()?;
    let listener = tokio::net::TcpListener::bind(socket_addr).await?;

    info!(
        listening_on = %socket_addr,
        "Cloakd gateway is listening for requests"
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Cloakd gateway has terminated gracefully");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C signal handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, initiating graceful shutdown");
        },
        _ = terminate => {
            info!("Received SIGTERM, initiating graceful shutdown");
        },
    }
}
