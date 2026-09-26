use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProviderId {
    Gemini,
    OpenAI,
    Anthropic,
    Custom(String),
}

impl ProviderId {
    pub fn as_str(&self) -> &str {
        match self {
            ProviderId::Gemini => "gemini",
            ProviderId::OpenAI => "openai",
            ProviderId::Anthropic => "anthropic",
            ProviderId::Custom(name) => name.as_str(),
        }
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedTarget {
    pub provider_id: ProviderId,
    pub target_url: String,
    pub api_key: Option<String>,
    pub model: String,
}
