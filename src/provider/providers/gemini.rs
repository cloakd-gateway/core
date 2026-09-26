use crate::provider::traits::LlmProvider;
use crate::provider::types::ProviderId;
use tracing::info;

const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";
const GEMINI_PREFIXES: &[&str] = &["gemini-", "models/gemini-", "learnlm-"];

pub struct GeminiProvider {
    api_key: Option<String>,
    default_model: String,
}

impl GeminiProvider {
    pub fn new(api_key: Option<String>, default_model: Option<String>) -> Self {
        Self {
            api_key,
            default_model: default_model.unwrap_or_else(|| "gemini-3.8-flash".to_string()),
        }
    }
}

impl LlmProvider for GeminiProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Gemini
    }

    fn base_url(&self) -> &str {
        GEMINI_BASE_URL
    }

    fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    fn model_prefixes(&self) -> &[&str] {
        GEMINI_PREFIXES
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn normalize_model(&self, requested: Option<&str>) -> String {
        let raw = requested.map(str::trim).filter(|m| !m.is_empty());
        match raw {
            Some("gemini-2.0-flash" | "models/gemini-2.0-flash" | "gemini-1.5-flash" | "models/gemini-1.5-flash" | "gemini" | "default") => {
                info!(
                    requested = raw,
                    aliased_to = %self.default_model,
                    "Gemini provider: Remapping deprecated/alias model to active default model"
                );
                self.default_model.clone()
            }
            Some(model) => model.to_string(),
            None => self.default_model.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_provider() {
        let provider = GeminiProvider::new(Some("test-key".to_string()), None);
        assert_eq!(provider.id(), ProviderId::Gemini);
        assert!(provider.matches_model("gemini-3.8-flash"));
        assert!(provider.matches_model("models/gemini-3.5-flash-lite"));
        assert!(!provider.matches_model("gpt-4o"));

        // Test alias remapping
        assert_eq!(
            provider.normalize_model(Some("gemini-2.0-flash")),
            "gemini-3.8-flash"
        );
        assert_eq!(
            provider.normalize_model(Some("gemini-3.8-flash")),
            "gemini-3.8-flash"
        );
        assert_eq!(provider.normalize_model(None), "gemini-3.8-flash");
    }
}
