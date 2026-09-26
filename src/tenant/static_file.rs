use super::context::TenantConfig;
use super::manifest::{Manifest, RoleManifest, TenantManifest, UserManifest};
use crate::config::AppConfig;
use crate::error::CloakdError;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct LegacyTenantEntry {
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
struct LegacyTenantsFile {
    pub tenants: Vec<LegacyTenantEntry>,
}

/// Recursively scans a directory for `.yaml`, `.yml`, and `.json` configuration files.
fn collect_manifest_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                collect_manifest_files(&path, files)?;
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if ext == "yaml" || ext == "yml" || ext == "json" {
                        files.push(path);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Loads tenant configurations from a file or directory.
/// Supports declarative manifests (Tenant, Role, User) and legacy tenants.yaml files.
/// Returns a map of API Key -> TenantConfig.
pub fn load_tenants_from_path<P: AsRef<Path>>(
    app_config: &AppConfig,
    path: P,
) -> Result<HashMap<String, TenantConfig>, CloakdError> {
    let path_ref = path.as_ref();
    let mut files = Vec::new();

    if path_ref.is_dir() {
        collect_manifest_files(path_ref, &mut files).map_err(|e| {
            CloakdError::Config(format!(
                "Failed to scan directory {}: {e}",
                path_ref.display()
            ))
        })?;
        files.sort();
    } else if path_ref.is_file() {
        files.push(path_ref.to_path_buf());
    } else {
        return Err(CloakdError::Config(format!(
            "Configuration path does not exist: {}",
            path_ref.display()
        )));
    }

    let mut tenants: HashMap<String, TenantManifest> = HashMap::new();
    let mut roles: HashMap<(String, String), RoleManifest> = HashMap::new();
    let mut users: Vec<UserManifest> = Vec::new();

    for file_path in &files {
        let content = fs::read_to_string(file_path).map_err(|e| {
            CloakdError::Config(format!(
                "Failed to read configuration file at {}: {e}",
                file_path.display()
            ))
        })?;

        // 1. Check for legacy v1.1.0 format ("tenants:" root key without "kind:")
        if content.contains("tenants:") && !content.contains("kind:") {
            let legacy_file: LegacyTenantsFile = if file_path
                .extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
            {
                serde_json::from_str(&content).map_err(|e| {
                    CloakdError::Config(format!(
                        "Failed to parse legacy JSON tenants file at {}: {e}",
                        file_path.display()
                    ))
                })?
            } else {
                serde_yaml::from_str(&content).map_err(|e| {
                    CloakdError::Config(format!(
                        "Failed to parse legacy YAML tenants file at {}: {e}",
                        file_path.display()
                    ))
                })?
            };

            for entry in legacy_file.tenants {
                let tenant_id = entry.id.clone();
                tenants.insert(
                    tenant_id.clone(),
                    TenantManifest {
                        id: tenant_id.clone(),
                        name: entry.organization_name,
                        default_model: None,
                        allowed_models: None,
                        fallback_models: entry.fallback_providers,
                        enabled_rules: entry.enabled_rules,
                        provider_keys: entry.provider_keys,
                        rate_limit_rpm: entry.rate_limit_rpm,
                        cache_enabled: entry.cache_enabled,
                    },
                );

                let default_role_id = "default".to_string();
                roles.insert(
                    (tenant_id.clone(), default_role_id.clone()),
                    RoleManifest {
                        id: default_role_id.clone(),
                        tenant_id: tenant_id.clone(),
                        name: Some("Default Role".to_string()),
                        default_model: None,
                        allowed_models: None,
                        fallback_models: None,
                        enabled_rules: None,
                        rate_limit_rpm: None,
                        cache_enabled: None,
                    },
                );

                users.push(UserManifest {
                    id: format!("{}-user", tenant_id),
                    tenant_id: tenant_id.clone(),
                    role_id: default_role_id,
                    key: entry.api_key,
                    name: None,
                    default_model: None,
                    allowed_models: None,
                    fallback_models: None,
                    enabled_rules: None,
                    rate_limit_rpm: None,
                    cache_enabled: None,
                });
            }
            continue;
        }

        // 2. Parse declarative manifests (kind: Tenant / Role / User)
        let is_json = file_path
            .extension()
            .map(|ext| ext == "json")
            .unwrap_or(false);

        if is_json {
            if let Ok(manifest_list) = serde_json::from_str::<Vec<Manifest>>(&content) {
                for m in manifest_list {
                    match m {
                        Manifest::Tenant(t) => {
                            tenants.insert(t.id.clone(), t);
                        }
                        Manifest::Role(r) => {
                            roles.insert((r.tenant_id.clone(), r.id.clone()), r);
                        }
                        Manifest::User(u) => {
                            users.push(u);
                        }
                    }
                }
            } else if let Ok(m) = serde_json::from_str::<Manifest>(&content) {
                match m {
                    Manifest::Tenant(t) => {
                        tenants.insert(t.id.clone(), t);
                    }
                    Manifest::Role(r) => {
                        roles.insert((r.tenant_id.clone(), r.id.clone()), r);
                    }
                    Manifest::User(u) => {
                        users.push(u);
                    }
                }
            }
        } else {
            // Multi-document YAML
            for doc in serde_yaml::Deserializer::from_str(&content) {
                let m = Manifest::deserialize(doc).map_err(|e| {
                    CloakdError::Config(format!(
                        "Failed to parse declarative YAML manifest in {}: {e}",
                        file_path.display()
                    ))
                })?;
                match m {
                    Manifest::Tenant(t) => {
                        tenants.insert(t.id.clone(), t);
                    }
                    Manifest::Role(r) => {
                        roles.insert((r.tenant_id.clone(), r.id.clone()), r);
                    }
                    Manifest::User(u) => {
                        users.push(u);
                    }
                }
            }
        }
    }

    // 3. Validation: check that foreign keys (tenant_id, role) exist
    for (tenant_id, role_id) in roles.keys() {
        if !tenants.contains_key(tenant_id) {
            return Err(CloakdError::Config(format!(
                "Role '{}' references unknown tenant '{}'",
                role_id, tenant_id
            )));
        }
    }

    for user in &users {
        if !tenants.contains_key(&user.tenant_id) {
            return Err(CloakdError::Config(format!(
                "User '{}' references unknown tenant '{}'",
                user.id, user.tenant_id
            )));
        }
        if !roles.contains_key(&(user.tenant_id.clone(), user.role_id.clone())) {
            return Err(CloakdError::Config(format!(
                "User '{}' references unknown role '{}' in tenant '{}'",
                user.id, user.role_id, user.tenant_id
            )));
        }
    }

    // 4. Merge hierarchy and compile into Key -> TenantConfig map
    let mut result_map = HashMap::new();
    for user in users {
        let tenant = tenants.get(&user.tenant_id).ok_or_else(|| {
            CloakdError::Config(format!("Tenant '{}' not found", user.tenant_id))
        })?;
        let role = roles.get(&(user.tenant_id.clone(), user.role_id.clone()));
        let merged = TenantConfig::merge_hierarchy(tenant, role, Some(&user), app_config);
        result_map.insert(user.key, merged);
    }

    Ok(result_map)
}

/// Backward compatibility wrapper for loading from a single file path with default config.
#[allow(dead_code)]
pub fn load_tenants_from_file<P: AsRef<Path>>(
    path: P,
) -> Result<HashMap<String, TenantConfig>, CloakdError> {
    let dummy_config = AppConfig {
        host: "0.0.0.0".to_string(),
        port: 8080,
        default_model: "gemini-3.5-flash-lite".to_string(),
        gemini_api_key: None,
        openai_api_key: None,
        anthropic_api_key: None,
        upstream_base_url: "https://api.openai.com".to_string(),
        upstream_api_key: None,
        enabled_rules: "default".to_string(),
        cache_enabled: true,
        cache_ttl_secs: 3600,
        cache_max_capacity: 10000,
        failover_enabled: true,
        fallback_providers: vec!["gemini".to_string(), "openai".to_string()],
        log_level: "info".to_string(),
        tenants_file: None,
        allow_anonymous: true,
    };
    load_tenants_from_path(&dummy_config, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn test_load_legacy_tenants_yaml() {
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
        assert_eq!(tenant.rate_limit_rpm, Some(300));
    }

    #[test]
    fn test_load_declarative_manifests_directory() {
        let temp_dir = TempDir::new().unwrap();
        let bank_dir = temp_dir.path().join("bank-corp");
        fs::create_dir(&bank_dir).unwrap();

        // 1. Tenant manifest
        let tenant_yaml = r#"
kind: Tenant
id: bank-corp
name: Bank Corp Global
default_model: gemini-3.5-flash-lite
enabled_rules:
  - banking
provider_keys:
  openai: sk-proj-bank-private
"#;
        fs::write(bank_dir.join("tenant.yaml"), tenant_yaml).unwrap();

        // 2. Roles manifest
        let roles_yaml = r#"
kind: Role
id: developer
tenant_id: bank-corp
name: Software Engineer
default_model: gpt-4o-mini
allowed_models:
  - gpt-4o-mini
rate_limit_rpm: 60
"#;
        fs::write(bank_dir.join("roles.yaml"), roles_yaml).unwrap();

        // 3. Users manifest
        let users_yaml = r#"
kind: User
id: usr_alice
tenant_id: bank-corp
role_id: developer
name: Alice Martin
key: sk-cloakd-alice-7788
"#;
        fs::write(bank_dir.join("users.yaml"), users_yaml).unwrap();

        let dummy_config = AppConfig {
            host: "0.0.0.0".to_string(),
            port: 8080,
            default_model: "gemini-3.5-flash-lite".to_string(),
            gemini_api_key: None,
            openai_api_key: None,
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

        let loaded = load_tenants_from_path(&dummy_config, temp_dir.path()).unwrap();
        assert_eq!(loaded.len(), 1);

        let user_ctx = loaded.get("sk-cloakd-alice-7788").unwrap();
        assert_eq!(user_ctx.id.as_str(), "bank-corp");
        assert_eq!(user_ctx.user_id.as_deref(), Some("usr_alice"));
        assert_eq!(user_ctx.role.as_deref(), Some("developer"));
        assert_eq!(user_ctx.default_model.as_deref(), Some("gpt-4o-mini")); // inherited from role!
        assert!(user_ctx.is_model_allowed("gpt-4o-mini"));
        assert!(!user_ctx.is_model_allowed("gemini-3.5-flash-lite")); // restricted by role!
        assert_eq!(user_ctx.get_provider_key("openai"), Some("sk-proj-bank-private"));
    }
}
