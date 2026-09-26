use crate::provider::traits::LlmProvider;
use crate::provider::types::ProviderId;

// Note: Anthropic native API uses /v1/messages, but many enterprise gateways or proxies
// provide an OpenAI-compatible /chat/completions facade or Anthropic-OpenAI bridge.
const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_PREFIXES: &[&str] = &["claude-"];

pub struct AnthropicProvider {
    api_key: Option<String>,
    default_model: String,
}

impl AnthropicProvider {
    pub fn new(api_key: Option<String>, default_model: Option<String>) -> Self {
        Self {
            api_key,
            default_model: default_model.unwrap_or_else(|| "claude-3-7-sonnet".to_string()),
        }
    }
}

impl LlmProvider for AnthropicProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Anthropic
    }

    fn base_url(&self) -> &str {
        ANTHROPIC_BASE_URL
    }

    fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    fn model_prefixes(&self) -> &[&str] {
        ANTHROPIC_PREFIXES
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn normalize_model(&self, requested: Option<&str>) -> String {
        requested
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| self.default_model())
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_provider() {
        let provider = AnthropicProvider::new(Some("sk-ant-test".to_string()), None);
        assert_eq!(provider.id(), ProviderId::Anthropic);
        assert!(provider.matches_model("claude-3-7-sonnet"));
        assert!(!provider.matches_model("gpt-4o"));
        assert_eq!(provider.normalize_model(None), "claude-3-7-sonnet");
    }
}
