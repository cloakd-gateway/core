use crate::provider::types::ProviderId;

/// Core trait representing an LLM Provider backend.
pub trait LlmProvider: Send + Sync {
    /// Identifier for this provider (e.g. Gemini, OpenAI, Anthropic).
    fn id(&self) -> ProviderId;

    /// Base URL for the OpenAI-compatible endpoint.
    fn base_url(&self) -> &str;

    /// Provider secret API key (if configured in Cloakd).
    fn api_key(&self) -> Option<&str>;

    /// Model name prefixes that route to this provider.
    fn model_prefixes(&self) -> &[&str];

    /// Default model name for this provider.
    fn default_model(&self) -> &str;

    /// Determines if a model name belongs to this provider.
    fn matches_model(&self, model: &str) -> bool {
        self.model_prefixes()
            .iter()
            .any(|prefix| model.starts_with(prefix))
    }

    /// Normalizes or rewrites requested model (e.g. handling aliases or deprecations).
    fn normalize_model(&self, requested: Option<&str>) -> String {
        requested
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| self.default_model())
            .to_string()
    }

    /// Full endpoint URL for chat completions.
    fn endpoint_url(&self) -> String {
        format!("{}/chat/completions", self.base_url().trim_end_matches('/'))
    }
}
