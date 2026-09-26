use crate::config::AppConfig;
use crate::error::CloakdError;
use crate::provider::providers::default_providers;
use crate::provider::traits::LlmProvider;
use crate::provider::types::{ProviderId, ResolvedTarget};
use std::sync::Arc;
use tracing::{debug, info};

pub struct ProviderRegistry {
    providers: Vec<Arc<dyn LlmProvider>>,
    default_provider_id: ProviderId,
    #[allow(dead_code)]
    default_model: String,
}

impl ProviderRegistry {
    pub fn from_config(config: &AppConfig) -> Self {
        let providers = default_providers(config);

        // Infer default provider dynamically from the configured default_model
        let default_provider_id = providers
            .iter()
            .find(|p| p.matches_model(&config.default_model))
            .map(|p| p.id())
            .unwrap_or(ProviderId::Gemini);

        info!(
            default_model = %config.default_model,
            resolved_default_provider = %default_provider_id,
            providers_count = providers.len(),
            "Initialized Multi-Provider Model Registry"
        );

        Self {
            providers,
            default_provider_id,
            default_model: config.default_model.clone(),
        }
    }

    #[allow(dead_code)]
    pub fn with_providers(
        providers: Vec<Arc<dyn LlmProvider>>,
        default_provider_id: ProviderId,
        default_model: String,
    ) -> Self {
        Self {
            providers,
            default_provider_id,
            default_model,
        }
    }

    /// Resolves the upstream endpoint, API key, and model name based on requested model.
    pub fn resolve(&self, requested_model: Option<&str>) -> Result<ResolvedTarget, CloakdError> {
        let trimmed_model = requested_model.map(str::trim).filter(|m| !m.is_empty());

        let provider = match trimmed_model {
            Some(model_name) => {
                if let Some(p) = self.providers.iter().find(|p| p.matches_model(model_name)) {
                    debug!(model = model_name, provider = %p.id(), "Resolved provider by model prefix");
                    p.clone()
                } else {
                    let def = self.get_default_provider();
                    debug!(
                        model = model_name,
                        default_provider = %def.id(),
                        "No specific prefix match, routing to default provider"
                    );
                    def
                }
            }
            None => self.get_default_provider(),
        };

        let effective_model = provider.normalize_model(trimmed_model);
        let target_url = provider.endpoint_url();
        let api_key = provider.api_key().map(|s| s.to_string());

        Ok(ResolvedTarget {
            provider_id: provider.id(),
            target_url,
            api_key,
            model: effective_model,
        })
    }

    /// Resolves available fallback targets excluding the primary provider, sorted by preferred order.
    pub fn resolve_fallbacks(
        &self,
        primary_id: &ProviderId,
        preferred_order: &[String],
    ) -> Vec<ResolvedTarget> {
        let mut candidates = Vec::new();

        for provider in &self.providers {
            if provider.id() == *primary_id {
                continue;
            }
            // A provider is an eligible fallback if credentials are configured
            if provider.api_key().is_some() || matches!(provider.id(), ProviderId::Custom(_)) {
                candidates.push(ResolvedTarget {
                    provider_id: provider.id(),
                    target_url: provider.endpoint_url(),
                    api_key: provider.api_key().map(|s| s.to_string()),
                    model: provider.default_model().to_string(),
                });
            }
        }

        // Sort candidates based on configured preferred_order
        candidates.sort_by_key(|target| {
            preferred_order
                .iter()
                .position(|name| name.eq_ignore_ascii_case(target.provider_id.as_str()))
                .unwrap_or(usize::MAX)
        });

        candidates
    }

    fn get_default_provider(&self) -> Arc<dyn LlmProvider> {
        self.providers
            .iter()
            .find(|p| p.id() == self.default_provider_id)
            .cloned()
            .unwrap_or_else(|| self.providers[0].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_config(default_model: &str) -> AppConfig {
        AppConfig {
            host: "0.0.0.0".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            upstream_base_url: "https://api.openai.com".to_string(),
            upstream_api_key: None,
            gemini_api_key: Some("gemini-secret-key-123".to_string()),
            openai_api_key: Some("openai-secret-key-456".to_string()),
            anthropic_api_key: Some("anthropic-secret-789".to_string()),
            default_model: default_model.to_string(),
            enabled_rules: "default".to_string(),
            cache_enabled: true,
            cache_ttl_secs: 3600,
            cache_max_capacity: 1000,
            failover_enabled: true,
            fallback_providers: vec!["gemini".to_string(), "openai".to_string()],
            tenants_file: None,
            allow_anonymous: true,
        }
    }

    #[test]
    fn test_resolve_gemini_model() {
        let config = mock_config("gemini-3.8-flash");
        let registry = ProviderRegistry::from_config(&config);

        let target = registry.resolve(Some("gemini-3.8-flash")).unwrap();
        assert_eq!(target.provider_id, ProviderId::Gemini);
        assert_eq!(
            target.target_url,
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        );
        assert_eq!(target.api_key.as_deref(), Some("gemini-secret-key-123"));
        assert_eq!(target.model, "gemini-3.8-flash");
    }

    #[test]
    fn test_resolve_deprecated_gemini_2_remaps_to_active_default() {
        let config = mock_config("gemini-3.8-flash");
        let registry = ProviderRegistry::from_config(&config);

        // Asking for deprecated gemini-2.0-flash automatically aliases to gemini-3.8-flash
        let target = registry.resolve(Some("gemini-2.0-flash")).unwrap();
        assert_eq!(target.provider_id, ProviderId::Gemini);
        assert_eq!(target.model, "gemini-3.8-flash");
    }

    #[test]
    fn test_resolve_openai_model() {
        let config = mock_config("gemini-3.8-flash");
        let registry = ProviderRegistry::from_config(&config);

        let target = registry.resolve(Some("gpt-4o")).unwrap();
        assert_eq!(target.provider_id, ProviderId::OpenAI);
        assert_eq!(
            target.target_url,
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(target.api_key.as_deref(), Some("openai-secret-key-456"));
        assert_eq!(target.model, "gpt-4o");
    }

    #[test]
    fn test_resolve_anthropic_model() {
        let config = mock_config("gemini-3.8-flash");
        let registry = ProviderRegistry::from_config(&config);

        let target = registry.resolve(Some("claude-3-7-sonnet")).unwrap();
        assert_eq!(target.provider_id, ProviderId::Anthropic);
        assert_eq!(
            target.target_url,
            "https://api.anthropic.com/v1/chat/completions"
        );
        assert_eq!(target.api_key.as_deref(), Some("anthropic-secret-789"));
        assert_eq!(target.model, "claude-3-7-sonnet");
    }

    #[test]
    fn test_resolve_omitted_model_uses_defaults() {
        let config = mock_config("gemini-3.8-flash");
        let registry = ProviderRegistry::from_config(&config);

        let target = registry.resolve(None).unwrap();
        assert_eq!(target.provider_id, ProviderId::Gemini);
        assert_eq!(target.model, "gemini-3.8-flash");
        assert_eq!(target.api_key.as_deref(), Some("gemini-secret-key-123"));
    }

    #[test]
    fn test_infer_default_provider_from_openai_model() {
        let config = mock_config("gpt-4o-mini");
        let registry = ProviderRegistry::from_config(&config);

        // When no model is provided, it defaults to OpenAI because default_model is gpt-4o-mini
        let target = registry.resolve(None).unwrap();
        assert_eq!(target.provider_id, ProviderId::OpenAI);
        assert_eq!(target.model, "gpt-4o-mini");
    }

    #[test]
    fn test_resolve_fallbacks_order() {
        let config = mock_config("gemini-3.8-flash");
        let registry = ProviderRegistry::from_config(&config);

        let preferred = vec!["openai".to_string(), "anthropic".to_string()];
        let fallbacks = registry.resolve_fallbacks(&ProviderId::Gemini, &preferred);

        // Fallbacks should exclude Gemini and contain OpenAI first, then Anthropic
        assert_eq!(fallbacks.len(), 2);
        assert_eq!(fallbacks[0].provider_id, ProviderId::OpenAI);
        assert_eq!(fallbacks[1].provider_id, ProviderId::Anthropic);
    }
}
