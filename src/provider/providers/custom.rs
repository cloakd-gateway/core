use crate::provider::traits::LlmProvider;
use crate::provider::types::ProviderId;

const CUSTOM_PREFIXES: &[&str] = &[
    "custom-", "llama-", "mistral-", "qwen-", "deepseek-", "phi-",
];

pub struct CustomProvider {
    name: String,
    base_url: String,
    api_key: Option<String>,
    default_model: String,
}

impl CustomProvider {
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: Option<String>,
        default_model: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            api_key,
            default_model: default_model.into(),
        }
    }
}

impl LlmProvider for CustomProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Custom(self.name.clone())
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    fn model_prefixes(&self) -> &[&str] {
        CUSTOM_PREFIXES
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
    fn test_custom_provider() {
        let provider = CustomProvider::new(
            "vllm",
            "http://localhost:8000/v1",
            None,
            "mistral-7b-instruct",
        );
        assert_eq!(provider.id(), ProviderId::Custom("vllm".to_string()));
        assert!(provider.matches_model("mistral-7b-instruct"));
        assert!(provider.matches_model("llama-3-8b"));
        assert_eq!(
            provider.endpoint_url(),
            "http://localhost:8000/v1/chat/completions"
        );
    }
}
