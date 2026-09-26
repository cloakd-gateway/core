use super::cache::TenantCache;
use super::context::{TenantConfig, TenantContext};
use super::static_file::load_tenants_from_file;
use crate::config::AppConfig;
use crate::error::CloakdError;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::debug;

/// Source of tenant configurations.
#[derive(Clone)]
pub enum TenantSource {
    /// In-memory static map (loaded from file or test fixtures)
    Static(Arc<HashMap<String, TenantConfig>>),
}

/// Resolves tenant configurations and credentials from API keys with in-memory LRU caching.
#[derive(Clone)]
pub struct TenantResolver {
    default_config: TenantConfig,
    cache: TenantCache,
    source: Option<TenantSource>,
    allow_anonymous: bool,
}

impl TenantResolver {
    pub fn new(
        default_config: TenantConfig,
        cache: TenantCache,
        source: Option<TenantSource>,
        allow_anonymous: bool,
    ) -> Self {
        Self {
            default_config,
            cache,
            source,
            allow_anonymous,
        }
    }

    /// Initializes a TenantResolver in standalone mode using the global AppConfig.
    /// Backward-compatible with v1.0.0.
    pub fn from_app_config(config: &AppConfig) -> Self {
        let default_config = TenantConfig::from_app_config(config);
        let cache = TenantCache::default();
        Self::new(default_config, cache, None, true)
    }

    /// Initializes a TenantResolver with an in-memory map of tenants.
    pub fn from_static_map(
        config: &AppConfig,
        map: HashMap<String, TenantConfig>,
        allow_anonymous: bool,
    ) -> Self {
        let default_config = TenantConfig::from_app_config(config);
        let cache = TenantCache::default();
        Self::new(
            default_config,
            cache,
            Some(TenantSource::Static(Arc::new(map))),
            allow_anonymous,
        )
    }

    /// Loads a TenantResolver from a local YAML or JSON file.
    pub fn from_file<P: AsRef<Path>>(
        config: &AppConfig,
        path: P,
        allow_anonymous: bool,
    ) -> Result<Self, CloakdError> {
        let map = load_tenants_from_file(path)?;
        Ok(Self::from_static_map(config, map, allow_anonymous))
    }

    /// Resolves an incoming API key into an active `TenantContext`.
    ///
    /// Resolution logic:
    /// 1. If no key is provided and anonymous requests are allowed, returns the default config.
    /// 2. If a key is provided:
    ///    a. Checks the LRU memory cache (< 10 µs latency).
    ///    b. On cache miss, checks the registered tenant source.
    ///    c. If found, populates the LRU cache and returns.
    ///    d. If not found, returns `CloakdError::Auth`.
    pub async fn resolve(&self, api_key: Option<&str>) -> Result<TenantContext, CloakdError> {
        match api_key {
            None => {
                if self.allow_anonymous {
                    debug!("No tenant API key provided; applying default standalone configuration");
                    Ok(TenantContext::new(self.default_config.clone()))
                } else {
                    Err(CloakdError::Auth(
                        "Missing Authorization Bearer token or x-cloakd-tenant-key header".to_string(),
                    ))
                }
            }
            Some(raw_key) => {
                let key = raw_key.trim();
                if key.is_empty() {
                    return if self.allow_anonymous {
                        Ok(TenantContext::new(self.default_config.clone()))
                    } else {
                        Err(CloakdError::Auth("Empty tenant API key provided".to_string()))
                    };
                }

                // 1. Check in-memory LRU cache
                if let Some(cached) = self.cache.get(key).await {
                    debug!(tenant_id = %cached.id, "TenantContext LRU cache HIT");
                    return Ok(TenantContext::new(cached));
                }

                // 2. Query source
                if let Some(TenantSource::Static(ref map)) = self.source {
                    if let Some(found) = map.get(key) {
                        debug!(tenant_id = %found.id, "TenantContext found in static source; caching");
                        self.cache.insert(key.to_string(), found.clone()).await;
                        return Ok(TenantContext::new(found.clone()));
                    }
                }

                // 3. Fallback: if key matches default or anonymous allowed with invalid key
                if self.allow_anonymous && key == "default" {
                    return Ok(TenantContext::new(self.default_config.clone()));
                }

                Err(CloakdError::Auth(format!(
                    "Invalid or unauthorized tenant API key: '{key}'"
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tenant::context::TenantId;
    use std::collections::HashSet;

    fn dummy_config() -> AppConfig {
        AppConfig {
            host: "0.0.0.0".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            upstream_base_url: "https://api.openai.com".to_string(),
            upstream_api_key: None,
            gemini_api_key: Some("AIza-gemini-test".to_string()),
            openai_api_key: None,
            anthropic_api_key: None,
            default_model: "gemini-3.5-flash-lite".to_string(),
            enabled_rules: "email,card".to_string(),
            cache_enabled: true,
            cache_ttl_secs: 3600,
            cache_max_capacity: 1000,
            failover_enabled: true,
            fallback_providers: vec!["gemini".to_string()],
            tenants_file: None,
            allow_anonymous: true,
        }
    }

    #[tokio::test]
    async fn test_anonymous_resolution_returns_default() {
        let app_cfg = dummy_config();
        let resolver = TenantResolver::from_app_config(&app_cfg);

        let ctx = resolver.resolve(None).await.unwrap();
        assert_eq!(ctx.id(), &TenantId::from("default"));
        assert!(ctx.config.is_rule_enabled("email"));
        assert!(ctx.config.is_rule_enabled("card"));
        assert!(!ctx.config.is_rule_enabled("iban"));
    }

    #[tokio::test]
    async fn test_static_map_resolution() {
        let app_cfg = dummy_config();
        let mut map = HashMap::new();

        let mut tenant_rules = HashSet::new();
        tenant_rules.insert("iban".to_string());

        let tenant_alpha = TenantConfig {
            id: TenantId::from("tenant-alpha"),
            organization_name: "Alpha Corp".to_string(),
            enabled_rules: tenant_rules,
            provider_keys: HashMap::new(),
            fallback_providers: None,
            cache_enabled: true,
            rate_limit_rpm: None,
        };
        map.insert("sk-cloakd-tenant-alpha-key".to_string(), tenant_alpha);

        let resolver = TenantResolver::from_static_map(&app_cfg, map, false);

        // Missing key fails when anonymous is false
        assert!(resolver.resolve(None).await.is_err());

        // Invalid key fails
        assert!(resolver.resolve(Some("sk-invalid")).await.is_err());

        // Valid key succeeds and caches
        let ctx = resolver.resolve(Some("sk-cloakd-tenant-alpha-key")).await.unwrap();
        assert_eq!(ctx.id(), &TenantId::from("tenant-alpha"));
        assert!(ctx.config.is_rule_enabled("iban"));
        assert!(!ctx.config.is_rule_enabled("email"));

        // Second lookup is a cache hit
        let cached_ctx = resolver.resolve(Some("sk-cloakd-tenant-alpha-key")).await.unwrap();
        assert_eq!(cached_ctx.id(), &TenantId::from("tenant-alpha"));
    }
}
