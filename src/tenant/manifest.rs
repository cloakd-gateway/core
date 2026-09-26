use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level declarative manifest with `kind` discriminator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum Manifest {
    Tenant(TenantManifest),
    Role(RoleManifest),
    User(UserManifest),
}

/// Declares an organization / tenant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TenantManifest {
    pub id: String,
    pub name: Option<String>,
    pub default_model: Option<String>,
    pub allowed_models: Option<Vec<String>>,
    pub fallback_models: Option<Vec<String>>,
    pub enabled_rules: Option<Vec<String>>,
    #[serde(default)]
    pub provider_keys: Option<HashMap<String, String>>,
    pub rate_limit_rpm: Option<u32>,
    pub cache_enabled: Option<bool>,
}

/// Declares a role scoped to a specific tenant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoleManifest {
    pub id: String,
    pub tenant_id: String,
    pub name: Option<String>,
    pub default_model: Option<String>,
    pub allowed_models: Option<Vec<String>>,
    pub fallback_models: Option<Vec<String>>,
    pub enabled_rules: Option<Vec<String>>,
    pub rate_limit_rpm: Option<u32>,
    pub cache_enabled: Option<bool>,
}

/// Declares a user / service API key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserManifest {
    pub id: String,
    pub tenant_id: String,
    pub role_id: String,
    pub key: String,
    pub name: Option<String>,
    pub default_model: Option<String>,
    pub allowed_models: Option<Vec<String>>,
    pub fallback_models: Option<Vec<String>>,
    pub enabled_rules: Option<Vec<String>>,
    pub rate_limit_rpm: Option<u32>,
    pub cache_enabled: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_manifests_yaml() {
        let yaml = r#"
kind: Tenant
id: tenant-bank
name: Bank Corp
default_model: gemini-3.5-flash-lite
provider_keys:
  openai: sk-proj-123
---
kind: Role
id: developer
tenant_id: tenant-bank
name: Software Developer
allowed_models:
  - gemini-3.5-flash-lite
  - gpt-4o-mini
rate_limit_rpm: 60
---
kind: User
id: usr_alice
tenant_id: tenant-bank
role_id: developer
name: Alice Martin
key: sk-cloakd-alice-999
"#;

        let mut manifests = Vec::new();
        for doc in serde_yaml::Deserializer::from_str(yaml) {
            let m = Manifest::deserialize(doc).unwrap();
            manifests.push(m);
        }

        assert_eq!(manifests.len(), 3);
        match &manifests[0] {
            Manifest::Tenant(t) => {
                assert_eq!(t.id, "tenant-bank");
                assert_eq!(t.name.as_deref(), Some("Bank Corp"));
                assert_eq!(t.default_model.as_deref(), Some("gemini-3.5-flash-lite"));
            }
            _ => panic!("Expected Tenant manifest"),
        }
        match &manifests[1] {
            Manifest::Role(r) => {
                assert_eq!(r.id, "developer");
                assert_eq!(r.tenant_id, "tenant-bank");
                assert_eq!(r.rate_limit_rpm, Some(60));
            }
            _ => panic!("Expected Role manifest"),
        }
        match &manifests[2] {
            Manifest::User(u) => {
                assert_eq!(u.id, "usr_alice");
                assert_eq!(u.tenant_id, "tenant-bank");
                assert_eq!(u.role_id, "developer");
                assert_eq!(u.key, "sk-cloakd-alice-999");
            }
            _ => panic!("Expected User manifest"),
        }
    }
}
