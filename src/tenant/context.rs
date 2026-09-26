use crate::config::AppConfig;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Unique identifier for a tenant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub String);

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for TenantId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for TenantId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Tenant-specific configuration for DLP, upstream keys (BYOK), and caching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantConfig {
    /// Identifier of the tenant (e.g. "tenant-alpha")
    pub id: TenantId,
    /// Human-readable organization name
    pub organization_name: String,
    /// Set of active DLP rules for this tenant (e.g. "email", "card", "iban", "fr_nir")
    pub enabled_rules: HashSet<String>,
    /// BYOK (Bring-Your-Own-Key) credentials mapped by provider_id ("gemini", "openai", "anthropic", "custom")
    pub provider_keys: HashMap<String, String>,
    /// Custom fallback ordering for this tenant (e.g. ["gemini", "openai"])
    pub fallback_providers: Option<Vec<String>>,
    /// Whether FinOps prompt cache is active for this tenant
    pub cache_enabled: bool,
    /// Optional rate limit in requests per minute
    pub rate_limit_rpm: Option<u32>,
}

impl TenantConfig {
    /// Creates a default TenantConfig using the global AppConfig.
    /// Used for backward compatibility with v1.0.0 when no tenant key is provided.
    pub fn from_app_config(config: &AppConfig) -> Self {
        let mut provider_keys = HashMap::new();
        if let Some(ref k) = config.gemini_api_key {
            provider_keys.insert("gemini".to_string(), k.clone());
        }
        if let Some(ref k) = config.openai_api_key {
            provider_keys.insert("openai".to_string(), k.clone());
        }
        if let Some(ref k) = config.anthropic_api_key {
            provider_keys.insert("anthropic".to_string(), k.clone());
        }
        if let Some(ref k) = config.upstream_api_key {
            provider_keys.insert("custom".to_string(), k.clone());
        }

        let enabled_rules = crate::dlp::rules::expand_rule_names(&config.enabled_rules);

        Self {
            id: TenantId::from("default"),
            organization_name: "Default (Standalone)".to_string(),
            enabled_rules,
            provider_keys,
            fallback_providers: Some(config.fallback_providers.clone()),
            cache_enabled: config.cache_enabled,
            rate_limit_rpm: None,
        }
    }

    /// Checks if a specific rule name is enabled for this tenant.
    pub fn is_rule_enabled(&self, rule_name: &str) -> bool {
        self.enabled_rules.contains("all") || self.enabled_rules.contains(rule_name)
    }

    /// Returns the tenant's custom key for a provider, if configured.
    pub fn get_provider_key(&self, provider_id: &str) -> Option<&str> {
        self.provider_keys.get(provider_id).map(|s| s.as_str())
    }
}

/// Request-scoped context identifying the active tenant.
#[derive(Debug, Clone)]
pub struct TenantContext {
    pub config: TenantConfig,
}

impl TenantContext {
    pub fn new(config: TenantConfig) -> Self {
        Self { config }
    }

    pub fn id(&self) -> &TenantId {
        &self.config.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_rule_filtering() {
        let mut rules = HashSet::new();
        rules.insert("email".to_string());
        rules.insert("card".to_string());

        let tenant = TenantConfig {
            id: TenantId::from("tenant-1"),
            organization_name: "Acme Corp".to_string(),
            enabled_rules: rules,
            provider_keys: HashMap::new(),
            fallback_providers: None,
            cache_enabled: true,
            rate_limit_rpm: Some(100),
        };

        assert!(tenant.is_rule_enabled("email"));
        assert!(tenant.is_rule_enabled("card"));
        assert!(!tenant.is_rule_enabled("iban"));
    }

    #[test]
    fn test_tenant_byok_key_lookup() {
        let mut keys = HashMap::new();
        keys.insert("openai".to_string(), "sk-custom-openai-key".to_string());

        let tenant = TenantConfig {
            id: TenantId::from("tenant-byok"),
            organization_name: "Byok Inc".to_string(),
            enabled_rules: HashSet::new(),
            provider_keys: keys,
            fallback_providers: None,
            cache_enabled: true,
            rate_limit_rpm: None,
        };

        assert_eq!(tenant.get_provider_key("openai"), Some("sk-custom-openai-key"));
        assert_eq!(tenant.get_provider_key("gemini"), None);
    }
}
