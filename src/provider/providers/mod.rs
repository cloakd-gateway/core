pub mod anthropic;
pub mod custom;
pub mod gemini;
pub mod openai;

pub use anthropic::AnthropicProvider;
pub use custom::CustomProvider;
pub use gemini::GeminiProvider;
pub use openai::OpenAiProvider;

use crate::config::AppConfig;
use crate::provider::traits::LlmProvider;
use std::sync::Arc;

/// Builds the default suite of enabled LLM providers from application configuration.
pub fn default_providers(config: &AppConfig) -> Vec<Arc<dyn LlmProvider>> {
    let mut providers: Vec<Arc<dyn LlmProvider>> = Vec::new();

    // 1. Google Gemini
    let gemini_key = config
        .gemini_api_key
        .clone()
        .or_else(|| config.upstream_api_key.clone());
    let gemini_default = if config.default_model.starts_with("gemini-")
        || config.default_model.starts_with("models/gemini-")
    {
        Some(config.default_model.clone())
    } else {
        None
    };
    providers.push(Arc::new(GeminiProvider::new(gemini_key, gemini_default)));

    // 2. OpenAI
    let openai_key = config.openai_api_key.clone();
    let openai_default = if config.default_model.starts_with("gpt-")
        || config.default_model.starts_with("o1-")
        || config.default_model.starts_with("o3-")
    {
        Some(config.default_model.clone())
    } else {
        None
    };
    providers.push(Arc::new(OpenAiProvider::new(openai_key, openai_default)));

    // 3. Anthropic
    let anthropic_key = config.anthropic_api_key.clone();
    let anthropic_default = if config.default_model.starts_with("claude-") {
        Some(config.default_model.clone())
    } else {
        None
    };
    providers.push(Arc::new(AnthropicProvider::new(
        anthropic_key,
        anthropic_default,
    )));

    // 4. Custom upstream (e.g. on-premise vLLM, Ollama) if custom base URL configured
    if config.upstream_base_url != "https://api.openai.com"
        && !config.upstream_base_url.contains("generativelanguage.googleapis.com")
        && !config.upstream_base_url.contains("api.anthropic.com")
    {
        providers.push(Arc::new(CustomProvider::new(
            "custom",
            config.upstream_base_url.clone(),
            config.upstream_api_key.clone(),
            config.default_model.clone(),
        )));
    }

    providers
}
