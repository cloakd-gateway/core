use crate::api::models::HealthResponse;
use crate::api::routes::AppState;
use crate::cache::compute_cache_key;
use crate::error::CloakdError;
use crate::stream::SseStreamTransformer;
use crate::vault::SessionVault;
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, instrument};

/// Health check endpoint for Kubernetes liveness/readiness probes.
pub async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "healthy",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Helper to restore PII placeholders across choice messages in a chat completion JSON response.
fn restore_pii_in_response(response_json: &mut Value, vault: &SessionVault) {
    if let Some(choices) = response_json.get_mut("choices").and_then(Value::as_array_mut) {
        for choice in choices {
            if let Some(message) = choice.get_mut("message") {
                if let Some(content) = message.get_mut("content") {
                    if let Some(text) = content.as_str() {
                        let unmasked = vault.restore_in_text(text);
                        *content = Value::String(unmasked.into_owned());
                    }
                }
                if let Some(reasoning) = message.get_mut("reasoning_content") {
                    if let Some(text) = reasoning.as_str() {
                        let unmasked = vault.restore_in_text(text);
                        *reasoning = Value::String(unmasked.into_owned());
                    }
                }
            }
        }
    }
}

/// Core OpenAI-compatible proxy handler for `/v1/chat/completions`.
///
/// V2 Lifecycle:
/// 1. Inspects requested model & resolves primary Provider and Model.
/// 2. Ingress DLP: masks sensitive data with typed tokens, storing raw values in ephemeral `SessionVault`.
/// 3. FinOps Cache Lookup:
///    - For non-streaming requests, calculates SHA-256 of the pseudonymized prompt.
///    - If CACHE HIT: detokenizes response with current vault and returns in < 1ms ($0 API cost).
/// 4. Upstream & Failover (on Cache MISS):
///    - Dispatches to primary target.
///    - If 429/5xx occurs, automatically cascades through eligible fallback providers.
/// 5. Egress Processing:
///    - Stores fresh anonymized response in prompt cache.
///    - Non-streaming: De-tokenizes entire JSON body with vault.
///    - Streaming: Pipes through sliding-window `SseStreamTransformer` FSM.
/// 6. Immediate drop of `SessionVault`.
#[instrument(skip(state, headers, payload), fields(stream = false))]
pub async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut payload): Json<Value>,
) -> Result<Response, CloakdError> {
    // 1. Resolve Provider and Model
    let requested_model = payload.get("model").and_then(Value::as_str);
    let resolved_target = state.provider_registry.resolve(requested_model)?;

    // Ensure payload model reflects the resolved model (handling defaults and alias migrations)
    payload["model"] = Value::String(resolved_target.model.clone());

    let mut vault = SessionVault::new();

    let is_streaming = payload
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // 2. Ingress DLP: Mask sensitive entities
    let masked_count = state.dlp_engine.mask_chat_payload(&mut payload, &mut vault);
    debug!(
        is_streaming,
        provider = %resolved_target.provider_id,
        model = %resolved_target.model,
        masked_entities = masked_count,
        "Ingress DLP processing completed"
    );

    // 3. FinOps Cache Lookup (Non-streaming mode)
    let cache_key = if state.prompt_cache.is_enabled() {
        Some(compute_cache_key(&payload))
    } else {
        None
    };

    if !is_streaming {
        if let Some(ref key) = cache_key {
            if let Some(mut cached_val) = state.prompt_cache.get(key).await {
                // Restore PII using the current session's vault
                restore_pii_in_response(&mut cached_val, &vault);

                let serialized = serde_json::to_vec(&cached_val)?;
                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-cloakd-cache", "HIT")
                    .header("x-cloakd-fallback", "false")
                    .header("x-cloakd-masked-count", masked_count.to_string())
                    .header("x-cloakd-provider", resolved_target.provider_id.as_str())
                    .header("x-cloakd-model", &resolved_target.model)
                    .body(Body::from(serialized))
                    .map_err(|e| CloakdError::Internal(anyhow::anyhow!("Failed to build cached response: {e}")));
            }
        }
    }

    // 4. Upstream call with Failover
    let fallback_targets = state.provider_registry.resolve_fallbacks(
        &resolved_target.provider_id,
        &state.config.fallback_providers,
    );

    let forward_result = state
        .upstream_client
        .forward_with_failover(&resolved_target, &fallback_targets, &headers, &mut payload)
        .await?;

    let effective_target = forward_result.effective_target;
    let fallback_occurred = forward_result.fallback_occurred;
    let original_provider = forward_result.original_provider;

    // 5. Egress Processing
    if is_streaming {
        // Mode SSE Streaming
        let vault_arc = Arc::new(vault);
        let stream = forward_result.response.bytes_stream();
        let transformed_stream = SseStreamTransformer::new(stream, vault_arc);

        let body = Body::from_stream(transformed_stream);

        let mut builder = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "keep-alive")
            .header("x-cloakd-cache", "BYPASS")
            .header("x-cloakd-fallback", fallback_occurred.to_string())
            .header("x-cloakd-masked-count", masked_count.to_string())
            .header("x-cloakd-provider", effective_target.provider_id.as_str())
            .header("x-cloakd-model", &effective_target.model);

        if let Some(ref orig) = original_provider {
            builder = builder.header("x-cloakd-fallback-from", orig.as_str());
        }

        builder
            .body(body)
            .map_err(|e| CloakdError::Internal(anyhow::anyhow!("Failed to build SSE response: {e}")))
    } else {
        // Mode JSON Standard
        let mut response_json: Value = forward_result
            .response
            .json()
            .await
            .map_err(|e| CloakdError::Upstream {
                status: StatusCode::BAD_GATEWAY,
                message: format!("Failed to parse upstream response JSON: {e}"),
            })?;

        // Cache the anonymized response before PII restoration
        if let Some(ref key) = cache_key {
            state.prompt_cache.insert(key.clone(), response_json.clone()).await;
        }

        // De-tokenize response
        restore_pii_in_response(&mut response_json, &vault);

        let serialized = serde_json::to_vec(&response_json)?;

        let mut builder = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-cloakd-cache", "MISS")
            .header("x-cloakd-fallback", fallback_occurred.to_string())
            .header("x-cloakd-masked-count", masked_count.to_string())
            .header("x-cloakd-provider", effective_target.provider_id.as_str())
            .header("x-cloakd-model", &effective_target.model);

        if let Some(ref orig) = original_provider {
            builder = builder.header("x-cloakd-fallback-from", orig.as_str());
        }

        builder
            .body(Body::from(serialized))
            .map_err(|e| CloakdError::Internal(anyhow::anyhow!("Failed to build JSON response: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dlp::{DlpEngine, DlpPipeline};
    use serde_json::json;

    #[test]
    fn test_restore_pii_in_response() {
        let mut vault = SessionVault::new();
        let token = vault.get_or_create_token("EMAIL", "alice@corp.com");

        let mut response = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": format!("Invoice sent to {token}.")
                }
            }]
        });

        restore_pii_in_response(&mut response, &vault);

        let content = response["choices"][0]["message"]["content"]
            .as_str()
            .unwrap();
        assert_eq!(content, "Invoice sent to alice@corp.com.");
    }

    #[test]
    fn test_cache_cross_user_isolation() {
        let engine = DlpEngine::new();

        // User A request
        let mut vault_a = SessionVault::new();
        let mut payload_a = json!({
            "model": "gemini-3.5-flash-lite",
            "messages": [
                {"role": "user", "content": "What is the status of alice@corp.com?"}
            ]
        });
        engine.mask_chat_payload(&mut payload_a, &mut vault_a);
        let key_a = compute_cache_key(&payload_a);

        // User B request (different sensitive email, same intent)
        let mut vault_b = SessionVault::new();
        let mut payload_b = json!({
            "model": "gemini-3.5-flash-lite",
            "messages": [
                {"role": "user", "content": "What is the status of bob@partner.fr?"}
            ]
        });
        engine.mask_chat_payload(&mut payload_b, &mut vault_b);
        let key_b = compute_cache_key(&payload_b);

        // 1. Both produce the identical anonymized cache key!
        assert_eq!(key_a, key_b);

        // 2. Simulated cached completion stored for key_a
        let cached_completion = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Account {{__VAR_EMAIL_1__}} is active."
                }
            }]
        });

        // 3. User B retrieves the cached completion and detokenizes with User B's vault
        let mut user_b_response = cached_completion.clone();
        restore_pii_in_response(&mut user_b_response, &vault_b);

        let user_b_content = user_b_response["choices"][0]["message"]["content"]
            .as_str()
            .unwrap();

        // User B sees their own email, and zero trace of Alice's email!
        assert_eq!(user_b_content, "Account bob@partner.fr is active.");
        assert!(!user_b_content.contains("alice@corp.com"));
    }
}
