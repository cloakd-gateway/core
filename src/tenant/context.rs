use crate::config::AppConfig;
use crate::tenant::manifest::{RoleManifest, TenantManifest, UserManifest};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Unique identifier for a tenant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub String);

impl TenantId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

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

/// Tenant & user specific configuration for DLP, upstream keys (BYOK), models, and caching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantConfig {
    /// Identifier of the tenant (e.g. "tenant-alpha")
    pub id: TenantId,
    /// Human-readable organization name
    pub organization_name: String,
    /// Identifier of the specific user/service (e.g. "usr_alice", "srv_crm")
    pub user_id: Option<String>,
    /// Human-readable user name
    pub user_name: Option<String>,
    /// Role assigned to this user/key (e.g. "developer", "admin")
    pub role: Option<String>,
    /// Resolved default model for this user (User > Role > Tenant > Global)
    pub default_model: Option<String>,
    /// Allowed models allowlist supporting wildcards (e.g. ["gemini-*", "gpt-4o-mini"])
    pub allowed_models: Option<Vec<String>>,
    /// Fallback models sequence (e.g. ["gpt-4o-mini", "claude-3-5-haiku"])
    pub fallback_models: Option<Vec<String>>,
    /// Custom fallback ordering for providers (kept for backward compatibility)
    pub fallback_providers: Option<Vec<String>>,
    /// Set of active DLP rules (e.g. "email", "card", "iban", "fr_nir")
    pub enabled_rules: HashSet<String>,
    /// BYOK (Bring-Your-Own-Key) credentials mapped by provider_id
    pub provider_keys: HashMap<String, String>,
    /// Whether FinOps prompt cache is active
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
            user_id: None,
            user_name: None,
            role: None,
            default_model: Some(config.default_model.clone()),
            allowed_models: None,
            fallback_models: None,
            fallback_providers: Some(config.fallback_providers.clone()),
            enabled_rules,
            provider_keys,
            cache_enabled: config.cache_enabled,
            rate_limit_rpm: None,
        }
    }

    /// Merges declarative manifests using the cascading priority: User > Role > Tenant > Global.
    pub fn merge_hierarchy(
        tenant: &TenantManifest,
        role: Option<&RoleManifest>,
        user: Option<&UserManifest>,
        app_config: &AppConfig,
    ) -> Self {
        let user_id = user.map(|u| u.id.clone());
        let user_name = user.and_then(|u| u.name.clone());
        let role_name = role.map(|r| r.id.clone());

        // 1. default_model: User > Role > Tenant > AppConfig
        let default_model = user
            .and_then(|u| u.default_model.clone())
            .or_else(|| role.and_then(|r| r.default_model.clone()))
            .or_else(|| tenant.default_model.clone())
            .or_else(|| Some(app_config.default_model.clone()));

        // 2. allowed_models: User > Role > Tenant
        let allowed_models = user
            .and_then(|u| u.allowed_models.clone())
            .or_else(|| role.and_then(|r| r.allowed_models.clone()))
            .or_else(|| tenant.allowed_models.clone());

        // 3. fallback_models: User > Role > Tenant
        let fallback_models = user
            .and_then(|u| u.fallback_models.clone())
            .or_else(|| role.and_then(|r| r.fallback_models.clone()))
            .or_else(|| tenant.fallback_models.clone());

        // 4. rate_limit_rpm: User > Role > Tenant
        let rate_limit_rpm = user
            .and_then(|u| u.rate_limit_rpm)
            .or_else(|| role.and_then(|r| r.rate_limit_rpm))
            .or(tenant.rate_limit_rpm);

        // 5. cache_enabled: User > Role > Tenant > AppConfig
        let cache_enabled = user
            .and_then(|u| u.cache_enabled)
            .or_else(|| role.and_then(|r| r.cache_enabled))
            .or(tenant.cache_enabled)
            .unwrap_or(app_config.cache_enabled);

        // 6. enabled_rules: Union of Tenant U Role U User
        let mut rules_parts = Vec::new();
        if let Some(ref tr) = tenant.enabled_rules {
            rules_parts.extend(tr.iter().cloned());
        }
        if let Some(r) = role {
            if let Some(ref rr) = r.enabled_rules {
                rules_parts.extend(rr.iter().cloned());
            }
        }
        if let Some(u) = user {
            if let Some(ref ur) = u.enabled_rules {
                rules_parts.extend(ur.iter().cloned());
            }
        }
        let rules_str = rules_parts.join(",");
        let enabled_rules = if rules_str.is_empty() {
            crate::dlp::rules::expand_rule_names(&app_config.enabled_rules)
        } else {
            crate::dlp::rules::expand_rule_names(&rules_str)
        };

        // 7. provider_keys: Tenant BYOK overrides AppConfig
        let mut provider_keys = HashMap::new();
        if let Some(ref k) = app_config.gemini_api_key {
            provider_keys.insert("gemini".to_string(), k.clone());
        }
        if let Some(ref k) = app_config.openai_api_key {
            provider_keys.insert("openai".to_string(), k.clone());
        }
        if let Some(ref k) = app_config.anthropic_api_key {
            provider_keys.insert("anthropic".to_string(), k.clone());
        }
        if let Some(ref k) = app_config.upstream_api_key {
            provider_keys.insert("custom".to_string(), k.clone());
        }
        if let Some(ref pk) = tenant.provider_keys {
            for (prov, key) in pk {
                provider_keys.insert(prov.clone(), key.clone());
            }
        }

        Self {
            id: TenantId::from(tenant.id.clone()),
            organization_name: tenant.name.clone().unwrap_or_else(|| tenant.id.clone()),
            user_id,
            user_name,
            role: role_name,
            default_model,
            allowed_models,
            fallback_models,
            fallback_providers: Some(app_config.fallback_providers.clone()),
            enabled_rules,
            provider_keys,
            cache_enabled,
            rate_limit_rpm,
        }
    }

    /// Checks if a requested model name matches the allowed model patterns.
    /// Supports exact matching and wildcards (e.g. "gemini-*", "*").
    #[allow(dead_code)]
    pub fn is_model_allowed(&self, model: &str) -> bool {
        let patterns = match &self.allowed_models {
            None => return true,
            Some(p) => p,
        };

        if patterns.is_empty() {
            return true;
        }

        for pattern in patterns {
            if pattern == "*" {
                return true;
            }
            if let Some(prefix) = pattern.strip_suffix('*') {
                if model.starts_with(prefix) {
                    return true;
                }
            } else if pattern == model {
                return true;
            }
        }

        false
    }

    /// Returns the effective default model for this user/tenant, or the server fallback.
    #[allow(dead_code)]
    pub fn effective_default_model<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.default_model.as_deref().unwrap_or(fallback)
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

/// Request-scoped context identifying the active tenant and caller identity.
#[derive(Debug, Clone)]
pub struct TenantContext {
    pub config: TenantConfig,
}

impl TenantContext {
    pub fn new(config: TenantConfig) -> Self {
        Self { config }
    }

    #[allow(dead_code)]
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
            user_id: None,
            user_name: None,
            role: None,
            default_model: None,
            allowed_models: None,
            fallback_models: None,
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
            user_id: None,
            user_name: None,
            role: None,
            default_model: None,
            allowed_models: None,
            fallback_models: None,
            enabled_rules: HashSet::new(),
            provider_keys: keys,
            fallback_providers: None,
            cache_enabled: true,
            rate_limit_rpm: None,
        };

        assert_eq!(tenant.get_provider_key("openai"), Some("sk-custom-openai-key"));
        assert_eq!(tenant.get_provider_key("gemini"), None);
    }

    #[test]
    fn test_is_model_allowed_with_wildcards() {
        let config = TenantConfig {
            id: TenantId::from("t1"),
            organization_name: "Org".to_string(),
            user_id: None,
            user_name: None,
            role: None,
            default_model: None,
            allowed_models: Some(vec!["gemini-*".to_string(), "gpt-4o-mini".to_string()]),
            fallback_models: None,
            enabled_rules: HashSet::new(),
            provider_keys: HashMap::new(),
            fallback_providers: None,
            cache_enabled: true,
            rate_limit_rpm: None,
        };

        assert!(config.is_model_allowed("gemini-3.5-flash-lite"));
        assert!(config.is_model_allowed("gemini-2.5-pro"));
        assert!(config.is_model_allowed("gpt-4o-mini"));
        assert!(!config.is_model_allowed("gpt-4o"));
        assert!(!config.is_model_allowed("o1"));
    }

    #[test]
    fn test_merge_hierarchy_cascading() {
        let app_config = AppConfig {
            host: "0.0.0.0".to_string(),
            port: 8080,
            default_model: "gemini-3.5-flash-lite".to_string(),
            gemini_api_key: Some("gemini-global".to_string()),
            openai_api_key: Some("openai-global".to_string()),
            anthropic_api_key: None,
            upstream_base_url: "https://api.openai.com".to_string(),
            upstream_api_key: None,
            enabled_rules: "default".to_string(),
            cache_enabled: true,
            cache_ttl_secs: 3600,
            cache_max_capacity: 10000,
            failover_enabled: true,
            fallback_providers: vec!["gemini".to_string()],
            log_level: "info".to_string(),
            tenants_file: None,
            allow_anonymous: true,
        };

        let tenant = TenantManifest {
            id: "bank-corp".to_string(),
            name: Some("Bank Corp".to_string()),
            default_model: Some("gemini-3.5-flash-lite".to_string()),
            allowed_models: Some(vec!["*".to_string()]),
            fallback_models: Some(vec!["gpt-4o-mini".to_string()]),
            enabled_rules: Some(vec!["banking".to_string()]),
            provider_keys: Some({
                let mut m = HashMap::new();
                m.insert("openai".to_string(), "openai-byok".to_string());
                m
            }),
            rate_limit_rpm: Some(2000),
            cache_enabled: Some(true),
        };

        let role = RoleManifest {
            id: "developer".to_string(),
            tenant_id: "bank-corp".to_string(),
            name: Some("Developer".to_string()),
            default_model: Some("gpt-4o-mini".to_string()),
            allowed_models: Some(vec!["gpt-4o-mini".to_string()]),
            fallback_models: None,
            enabled_rules: Some(vec!["secrets".to_string()]),
            rate_limit_rpm: Some(60),
            cache_enabled: None,
        };

        let user = UserManifest {
            id: "usr_alice".to_string(),
            tenant_id: "bank-corp".to_string(),
            role_id: "developer".to_string(),
            key: "sk-cloakd-alice".to_string(),
            name: Some("Alice".to_string()),
            default_model: None, // inherits role default_model: gpt-4o-mini
            allowed_models: None, // inherits role allowed_models
            fallback_models: None, // inherits tenant fallback_models: gpt-4o-mini
            enabled_rules: None, // union of banking + secrets
            rate_limit_rpm: Some(120), // overrides role rate_limit_rpm 60!
            cache_enabled: None,
        };

        let merged = TenantConfig::merge_hierarchy(&tenant, Some(&role), Some(&user), &app_config);

        assert_eq!(merged.id, TenantId::from("bank-corp"));
        assert_eq!(merged.user_id.as_deref(), Some("usr_alice"));
        assert_eq!(merged.role.as_deref(), Some("developer"));
        assert_eq!(merged.default_model.as_deref(), Some("gpt-4o-mini")); // from role
        assert_eq!(merged.fallback_models, Some(vec!["gpt-4o-mini".to_string()])); // from tenant
        assert_eq!(merged.rate_limit_rpm, Some(120)); // from user override!
        assert!(merged.is_rule_enabled("card")); // from tenant banking pack
        assert!(merged.is_rule_enabled("secrets")); // from role secrets pack
        assert_eq!(merged.get_provider_key("openai"), Some("openai-byok")); // tenant BYOK override
        assert_eq!(merged.get_provider_key("gemini"), Some("gemini-global")); // app_config fallback
    }
}
