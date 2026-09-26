use crate::api::handlers;
use crate::cache::PromptCache;
use crate::config::AppConfig;
use crate::dlp::DlpPipeline;
use crate::provider::ProviderRegistry;
use crate::upstream::UpstreamClient;
use crate::tenant::TenantResolver;
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub upstream_client: Arc<UpstreamClient>,
    pub dlp_engine: Arc<dyn DlpPipeline>,
    pub provider_registry: Arc<ProviderRegistry>,
    pub prompt_cache: Arc<PromptCache>,
    pub tenant_resolver: Arc<TenantResolver>,
}

pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(handlers::health))
        .route("/v1/chat/completions", post(handlers::chat_completions))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
