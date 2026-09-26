use crate::error::CloakdError;
use std::env;
use std::net::SocketAddr;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub log_level: String,
    pub upstream_base_url: String,
    pub upstream_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub default_model: String,
    pub enabled_rules: String,
    pub cache_enabled: bool,
    pub cache_ttl_secs: u64,
    pub cache_max_capacity: u64,
    pub failover_enabled: bool,
    pub fallback_models: Vec<String>,
    pub tenants_file: Option<String>,
    pub allow_anonymous: bool,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, CloakdError> {
        let _ = dotenvy::dotenv();

        let host = env::var("CLOAKD_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("CLOAKD_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()
            .map_err(|e| CloakdError::Config(format!("Invalid CLOAKD_PORT: {e}")))?;

        let log_level = env::var("CLOAKD_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
        let upstream_base_url = env::var("UPSTREAM_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com".to_string())
            .trim_end_matches('/')
            .to_string();

        let upstream_api_key = env::var("UPSTREAM_API_KEY")
            .ok()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty());

        let gemini_api_key = env::var("GEMINI_API_KEY")
            .ok()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty());

        let openai_api_key = env::var("OPENAI_API_KEY")
            .ok()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty());

        let anthropic_api_key = env::var("ANTHROPIC_API_KEY")
            .ok()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty());

        let default_model = env::var("CLOAKD_DEFAULT_MODEL")
            .unwrap_or_else(|_| "gemini-3.8-flash".to_string());

        let enabled_rules = env::var("CLOAKD_ENABLED_RULES")
            .unwrap_or_else(|_| "default".to_string());

        let cache_enabled = env::var("CLOAKD_CACHE_ENABLED")
            .map(|v| v.to_lowercase() != "false" && v != "0")
            .unwrap_or(true);

        let cache_ttl_secs = env::var("CLOAKD_CACHE_TTL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3600);

        let cache_max_capacity = env::var("CLOAKD_CACHE_MAX_CAPACITY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10_000);

        let failover_enabled = env::var("CLOAKD_FAILOVER_ENABLED")
            .map(|v| v.to_lowercase() != "false" && v != "0")
            .unwrap_or(true);

        let fallback_models = env::var("CLOAKD_FALLBACK_MODELS")
            .unwrap_or_else(|_| "gpt-4o-mini,claude-3-5-haiku".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let tenants_file = env::var("CLOAKD_TENANTS_FILE")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let allow_anonymous = env::var("CLOAKD_ALLOW_ANONYMOUS")
            .map(|v| v.to_lowercase() != "false" && v != "0")
            .unwrap_or(true);

        Ok(Self {
            host,
            port,
            log_level,
            upstream_base_url,
            upstream_api_key,
            gemini_api_key,
            openai_api_key,
            anthropic_api_key,
            default_model,
            enabled_rules,
            cache_enabled,
            cache_ttl_secs,
            cache_max_capacity,
            failover_enabled,
            fallback_models,
            tenants_file,
            allow_anonymous,
        })
    }

    pub fn socket_addr(&self) -> Result<SocketAddr, CloakdError> {
        let addr_str = format!("{}:{}", self.host, self.port);
        addr_str
            .parse::<SocketAddr>()
            .map_err(|e| CloakdError::Config(format!("Invalid socket address {addr_str}: {e}")))
    }
}
