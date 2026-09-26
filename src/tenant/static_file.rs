use super::context::{TenantConfig, TenantId};
use crate::error::CloakdError;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct StaticTenantEntry {
    pub id: String,
    pub api_key: String,
    pub organization_name: Option<String>,
    pub enabled_rules: Option<Vec<String>>,
    pub provider_keys: Option<HashMap<String, String>>,
    pub fallback_providers: Option<Vec<String>>,
    pub cache_enabled: Option<bool>,
    pub rate_limit_rpm: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct StaticTenantsFile {
    pub tenants: Vec<StaticTenantEntry>,
}

/// Loads tenant configurations from a YAML or JSON file.
/// Returns a map of API Key -> TenantConfig.
pub fn load_tenants_from_file<P: AsRef<Path>>(
    path: P,
) -> Result<HashMap<String, TenantConfig>, CloakdError> {
    let path_ref = path.as_ref();
    let content = fs::read_to_string(path_ref).map_err(|e| {
        CloakdError::Config(format!(
            "Failed to read tenants configuration file at {}: {e}",
            path_ref.display()
        ))
    })?;

    let parsed_file: StaticTenantsFile = if path_ref
        .extension()
        .map(|ext| ext == "json")
        .unwrap_or(false)
    {
        serde_json::from_str(&content).map_err(|e| {
            CloakdError::Config(format!(
                "Failed to parse JSON tenants file at {}: {e}",
                path_ref.display()
            ))
        })?
    } else {
        serde_yaml::from_str(&content).map_err(|e| {
            CloakdError::Config(format!(
                "Failed to parse YAML tenants file at {}: {e}",
                path_ref.display()
            ))
        })?
    };

    let mut map = HashMap::new();
    for entry in parsed_file.tenants {
        let rules_str = entry.enabled_rules.unwrap_or_default().join(",");
        let enabled_rules = crate::dlp::rules::expand_rule_names(&rules_str);

        let config = TenantConfig {
            id: TenantId::from(entry.id),
            organization_name: entry
                .organization_name
                .unwrap_or_else(|| "Unknown Organization".to_string()),
            enabled_rules,
            provider_keys: entry.provider_keys.unwrap_or_default(),
            fallback_providers: entry.fallback_providers,
            cache_enabled: entry.cache_enabled.unwrap_or(true),
            rate_limit_rpm: entry.rate_limit_rpm,
        };

        map.insert(entry.api_key, config);
    }

    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_tenants_yaml() {
        let yaml_content = r#"
tenants:
  - id: tenant-alpha
    api_key: sk-cloakd-tenant-alpha-secret
    organization_name: Alpha Corp
    enabled_rules:
      - banking
      - eu
    provider_keys:
      openai: sk-proj-alpha-123
    fallback_providers:
      - openai
      - gemini
    cache_enabled: true
    rate_limit_rpm: 300
"#;
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(yaml_content.as_bytes()).unwrap();

        let loaded = load_tenants_from_file(tmp.path()).unwrap();
        assert_eq!(loaded.len(), 1);

        let tenant = loaded.get("sk-cloakd-tenant-alpha-secret").unwrap();
        assert!(tenant.is_rule_enabled("card"));
        assert!(tenant.is_rule_enabled("iban"));
        assert!(tenant.is_rule_enabled("fr_nir"));
        assert!(!tenant.is_rule_enabled("email"));
        assert_eq!(tenant.get_provider_key("openai"), Some("sk-proj-alpha-123"));
        assert_eq!(tenant.fallback_providers, Some(vec!["openai".to_string(), "gemini".to_string()]));
        assert_eq!(tenant.rate_limit_rpm, Some(300));
    }
}
