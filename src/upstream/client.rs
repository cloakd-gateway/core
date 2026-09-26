use crate::config::AppConfig;
use crate::error::CloakdError;
use crate::provider::ResolvedTarget;
use crate::tenant::TenantConfig;
use crate::upstream::failover::{is_failover_status, ForwardResult};
use axum::http::HeaderMap;
use reqwest::{header, Client, Response, StatusCode};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

#[derive(Clone)]
pub struct UpstreamClient {
    client: Client,
    config: Arc<AppConfig>,
}

impl UpstreamClient {
    pub fn new(config: Arc<AppConfig>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300)) // LLM reasoning / long generation support
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .expect("Failed to initialize reqwest HTTP client");

        Self { client, config }
    }

    /// Forwards the pseudonymized request body to a single resolved upstream LLM provider.
    pub async fn forward_chat_completion(
        &self,
        target: &ResolvedTarget,
        headers: &HeaderMap,
        body: &Value,
        tenant: Option<&TenantConfig>,
    ) -> Result<Response, CloakdError> {
        let mut request_builder = self.client.post(&target.target_url).json(body);

        // Resolve Authorization: Bearer token
        // Priority:
        // 1. Tenant BYOK key (if configured for this provider in TenantConfig)
        // 2. Secret API key configured in Cloakd for this provider
        // 3. Client incoming Authorization header (excluding cloakd tenant bearer tokens)
        let tenant_key = tenant.and_then(|t| t.get_provider_key(target.provider_id.as_str()));

        let auth_header = tenant_key
            .map(|key| format!("Bearer {key}"))
            .or_else(|| {
                target
                    .api_key
                    .as_ref()
                    .map(|key| format!("Bearer {key}"))
            })
            .or_else(|| {
                headers
                    .get(header::AUTHORIZATION)
                    .and_then(|h| h.to_str().ok())
                    .filter(|s| !s.starts_with("Bearer sk-cloakd-"))
                    .map(|s| s.to_string())
            });

        if let Some(auth) = auth_header {
            request_builder = request_builder.header(header::AUTHORIZATION, auth);
        }

        // Forward optional Organization or Project headers (OpenAI specific)
        if let Some(org) = headers.get("openai-organization") {
            request_builder = request_builder.header("openai-organization", org);
        }
        if let Some(proj) = headers.get("openai-project") {
            request_builder = request_builder.header("openai-project", proj);
        }

        debug!(
            provider = %target.provider_id,
            target_url = %target.target_url,
            model = %target.model,
            "Forwarding chat completion to resolved upstream provider"
        );

        let response = request_builder.send().await.map_err(|e| {
            error!(error = %e, "Network failure reaching upstream LLM");
            CloakdError::Upstream {
                status: StatusCode::BAD_GATEWAY,
                message: format!("Failed to reach upstream LLM at {}: {e}", target.target_url),
            }
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown upstream error".to_string());
            error!(
                status = %status,
                provider = %target.provider_id,
                body = %error_body,
                "Upstream LLM returned error"
            );

            return Err(CloakdError::Upstream {
                status: StatusCode::from_u16(status.as_u16())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                message: error_body,
            });
        }

        Ok(response)
    }

    /// Forwards a request with automatic failover across alternative providers
    /// if the primary provider encounters a retryable failure (HTTP 429, 5xx, or network drop).
    pub async fn forward_with_failover(
        &self,
        primary_target: &ResolvedTarget,
        fallback_targets: &[ResolvedTarget],
        headers: &HeaderMap,
        payload: &mut Value,
        tenant: Option<&TenantConfig>,
    ) -> Result<ForwardResult, CloakdError> {
        // 1. Attempt primary target
        match self.forward_chat_completion(primary_target, headers, payload, tenant).await {
            Ok(response) => Ok(ForwardResult {
                response,
                effective_target: primary_target.clone(),
                fallback_occurred: false,
                original_provider: None,
            }),
            Err(err) => {
                // If failover is disabled or no fallbacks available, return error immediately
                if !self.config.failover_enabled || fallback_targets.is_empty() {
                    return Err(err);
                }

                // Check if error is eligible for failover (429, 5xx, or network 502)
                let is_eligible = match &err {
                    CloakdError::Upstream { status, .. } => is_failover_status(*status),
                    _ => false,
                };

                if !is_eligible {
                    return Err(err);
                }

                warn!(
                    primary_provider = %primary_target.provider_id,
                    primary_model = %primary_target.model,
                    error = %err,
                    "Primary upstream returned retryable failure; activating failover chain"
                );

                // 2. Cascade through fallback targets in order
                let mut last_error = err;
                for fallback in fallback_targets {
                    warn!(
                        fallback_provider = %fallback.provider_id,
                        fallback_model = %fallback.model,
                        "Attempting fallback upstream target"
                    );

                    // Update model name in payload for the fallback provider
                    payload["model"] = Value::String(fallback.model.clone());

                    match self.forward_chat_completion(fallback, headers, payload, tenant).await {
                        Ok(response) => {
                            info!(
                                fallback_provider = %fallback.provider_id,
                                fallback_model = %fallback.model,
                                original_provider = %primary_target.provider_id,
                                "Failover execution succeeded"
                            );
                            return Ok(ForwardResult {
                                response,
                                effective_target: fallback.clone(),
                                fallback_occurred: true,
                                original_provider: Some(primary_target.provider_id.as_str().to_string()),
                            });
                        }
                        Err(fallback_err) => {
                            warn!(
                                fallback_provider = %fallback.provider_id,
                                error = %fallback_err,
                                "Fallback upstream failed"
                            );
                            last_error = fallback_err;
                        }
                    }
                }

                Err(last_error)
            }
        }
    }
}
