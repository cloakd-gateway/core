use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

/// Prefix and suffix defining Cloakd token boundaries.
pub const TOKEN_PREFIX: &str = "{{__VAR_";
pub const TOKEN_SUFFIX: &str = "__}}";

/// Ephemeral in-memory vault tied to the lifecycle of a single request/response cycle.
///
/// Features:
/// - Bidirectional mapping: token <-> original PII
/// - Consistent pseudonymization: same PII in the request gets the exact same token
/// - Zero disk persistence, destroyed immediately on Drop
/// - Sanitized Debug implementation to prevent sensitive data leakage in logs/traces
pub struct SessionVault {
    token_to_original: HashMap<String, String>,
    original_to_token: HashMap<String, String>,
    counters: HashMap<String, usize>,
}

impl SessionVault {
    pub fn new() -> Self {
        Self {
            token_to_original: HashMap::new(),
            original_to_token: HashMap::new(),
            counters: HashMap::new(),
        }
    }

    /// Registers an entity value or retrieves its existing token if already encountered.
    pub fn get_or_create_token(&mut self, entity_type: &str, original_value: &str) -> String {
        if let Some(token) = self.original_to_token.get(original_value) {
            return token.clone();
        }

        let counter = self.counters.entry(entity_type.to_string()).or_insert(0);
        *counter += 1;
        let token = format!("{TOKEN_PREFIX}{entity_type}_{counter}{TOKEN_SUFFIX}");

        self.token_to_original
            .insert(token.clone(), original_value.to_string());
        self.original_to_token
            .insert(original_value.to_string(), token.clone());

        token
    }

    /// Direct lookup of an original value from a token.
    pub fn restore_token(&self, token: &str) -> Option<&str> {
        self.token_to_original.get(token).map(|s| s.as_str())
    }

    /// Replaces all tokens present in the input text with their original values.
    /// Uses Cow to avoid allocation if no tokens are present.
    pub fn restore_in_text<'a>(&self, text: &'a str) -> Cow<'a, str> {
        if self.token_to_original.is_empty() || !text.contains(TOKEN_PREFIX) {
            return Cow::Borrowed(text);
        }

        let mut result = text.to_string();
        for (token, original) in &self.token_to_original {
            result = result.replace(token, original);
        }
        Cow::Owned(result)
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.token_to_original.is_empty()
    }

    pub fn token_count(&self) -> usize {
        self.token_to_original.len()
    }
}

impl Default for SessionVault {
    fn default() -> Self {
        Self::new()
    }
}

/// Secure Debug implementation: prevents PII from appearing in logs/traces.
impl fmt::Debug for SessionVault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionVault")
            .field("registered_tokens_count", &self.token_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_consistency() {
        let mut vault = SessionVault::new();
        let email = "john.doe@example.com";

        let t1 = vault.get_or_create_token("EMAIL", email);
        let t2 = vault.get_or_create_token("EMAIL", email);
        assert_eq!(t1, t2);
        assert_eq!(t1, "{{__VAR_EMAIL_1__}}");

        let email2 = "jane.doe@example.com";
        let t3 = vault.get_or_create_token("EMAIL", email2);
        assert_eq!(t3, "{{__VAR_EMAIL_2__}}");
        assert_ne!(t1, t3);
    }

    #[test]
    fn test_vault_restoration() {
        let mut vault = SessionVault::new();
        let token = vault.get_or_create_token("EMAIL", "john@example.com");

        let masked = format!("Hello {token}, welcome!");
        let unmasked = vault.restore_in_text(&masked);
        assert_eq!(unmasked, "Hello john@example.com, welcome!");
    }

    #[test]
    fn test_vault_debug_sanitization() {
        let mut vault = SessionVault::new();
        vault.get_or_create_token("EMAIL", "secret@bank.com");

        let debug_str = format!("{vault:?}");
        assert!(!debug_str.contains("secret@bank.com"));
        assert!(debug_str.contains("registered_tokens_count: 1"));
    }
}
