use crate::provider::traits::LlmProvider;
use crate::provider::types::ProviderId;

const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const OPENAI_PREFIXES: &[&str] = &["gpt-", "o1-", "o3-", "chatgpt-"];

pub struct OpenAiProvider {
    api_key: Option<String>,
    default_model: String,
}

impl OpenAiProvider {
    pub fn new(api_key: Option<String>, default_model: Option<String>) -> Self {
        Self {
            api_key,
            default_model: default_model.unwrap_or_else(|| "gpt-4o-mini".to_string()),
        }
    }
}

impl LlmProvider for OpenAiProvider {
    fn id(&self) -> ProviderId {
        ProviderId::OpenAI
    }

    fn base_url(&self) -> &str {
        OPENAI_BASE_URL
    }

    fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    fn model_prefixes(&self) -> &[&str] {
        OPENAI_PREFIXES
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
    fn test_openai_provider() {
        let provider = OpenAiProvider::new(Some("sk-test".to_string()), None);
        assert_eq!(provider.id(), ProviderId::OpenAI);
        assert!(provider.matches_model("gpt-4o"));
        assert!(provider.matches_model("o1-preview"));
        assert!(!provider.matches_model("gemini-3.8-flash"));
        assert_eq!(provider.normalize_model(None), "gpt-4o-mini");
    }
}
