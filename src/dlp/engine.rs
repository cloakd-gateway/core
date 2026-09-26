use crate::dlp::rules::{build_rules, default_rules, DlpRule};
use crate::dlp::types::MatchSpan;
use crate::tenant::TenantConfig;
use crate::vault::SessionVault;
use serde_json::Value;
use tracing::debug;

pub trait DlpPipeline: Send + Sync {
    /// Masks PII entities within a text string, updating the session vault.
    #[allow(dead_code)]
    fn mask_text(&self, text: &str, vault: &mut SessionVault) -> String;

    /// Traverses and masks sensitive data in an OpenAI-compatible chat completion payload.
    /// Returns the total number of masked entities.
    #[allow(dead_code)]
    fn mask_chat_payload(&self, payload: &mut Value, vault: &mut SessionVault) -> usize;

    /// Masks sensitive entities within a text string using only the rules enabled for the given tenant.
    fn mask_text_for_tenant(
        &self,
        text: &str,
        vault: &mut SessionVault,
        tenant: &TenantConfig,
    ) -> String;

    /// Traverses and masks sensitive data in a chat payload using the tenant's enabled rule profile.
    fn mask_chat_payload_for_tenant(
        &self,
        payload: &mut Value,
        vault: &mut SessionVault,
        tenant: &TenantConfig,
    ) -> usize;
}

pub struct DlpEngine {
    rules: Vec<Box<dyn DlpRule>>,
}

impl DlpEngine {
    pub fn new() -> Self {
        Self {
            rules: default_rules(),
        }
    }

    pub fn from_rules_str(enabled_rules: &str) -> Self {
        Self {
            rules: build_rules(enabled_rules),
        }
    }

    #[allow(dead_code)]
    pub fn with_rules(rules: Vec<Box<dyn DlpRule>>) -> Self {
        Self { rules }
    }

    /// Finds all non-overlapping PII matches across all registered rules.
    #[allow(dead_code)]
    fn extract_matches(&self, text: &str) -> Vec<MatchSpan> {
        let mut candidates = Vec::new();

        for rule in &self.rules {
            candidates.extend(rule.find_matches(text));
        }

        if candidates.is_empty() {
            return candidates;
        }

        // Sort by start index; for same start, pick the longest match
        candidates.sort_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
        });

        // Filter out overlapping spans
        let mut filtered = Vec::with_capacity(candidates.len());
        let mut last_end = 0;

        for candidate in candidates {
            if candidate.start >= last_end {
                last_end = candidate.end;
                filtered.push(candidate);
            }
        }

        filtered
    }

    /// Finds non-overlapping PII matches considering only the rules enabled for the given tenant.
    fn extract_matches_for_tenant(&self, text: &str, tenant: &TenantConfig) -> Vec<MatchSpan> {
        let mut candidates = Vec::new();

        for rule in &self.rules {
            let rule_name = rule.entity_type().rule_name();
            if tenant.is_rule_enabled(rule_name) {
                candidates.extend(rule.find_matches(text));
            }
        }

        if candidates.is_empty() {
            return candidates;
        }

        candidates.sort_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
        });

        let mut filtered = Vec::with_capacity(candidates.len());
        let mut last_end = 0;

        for candidate in candidates {
            if candidate.start >= last_end {
                last_end = candidate.end;
                filtered.push(candidate);
            }
        }

        filtered
    }
}

impl Default for DlpEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DlpPipeline for DlpEngine {
    fn mask_text(&self, text: &str, vault: &mut SessionVault) -> String {
        let spans = self.extract_matches(text);
        if spans.is_empty() {
            return text.to_string();
        }

        let mut output = String::with_capacity(text.len());
        let mut cursor = 0;

        for span in spans {
            if span.start > cursor {
                output.push_str(&text[cursor..span.start]);
            }

            let token = vault.get_or_create_token(span.entity_type.code(), &span.text);
            output.push_str(&token);

            cursor = span.end;
        }

        if cursor < text.len() {
            output.push_str(&text[cursor..]);
        }

        output
    }

    fn mask_chat_payload(&self, payload: &mut Value, vault: &mut SessionVault) -> usize {
        let initial_count = vault.token_count();

        // Standard OpenAI Chat: inspect "messages" array
        if let Some(messages) = payload.get_mut("messages").and_then(Value::as_array_mut) {
            for message in messages {
                if let Some(content) = message.get_mut("content") {
                    if let Some(text) = content.as_str() {
                        let masked = self.mask_text(text, vault);
                        *content = Value::String(masked);
                    } else if let Some(parts) = content.as_array_mut() {
                        // Multimodal content parts: [{"type": "text", "text": "..."}]
                        for part in parts {
                            if let Some(text_val) = part.get_mut("text") {
                                if let Some(text) = text_val.as_str() {
                                    let masked = self.mask_text(text, vault);
                                    *text_val = Value::String(masked);
                                }
                            }
                        }
                    }
                }
            }
        }

        let tokens_added = vault.token_count() - initial_count;
        if tokens_added > 0 {
            debug!(
                tokens_masked = tokens_added,
                "Ingress DLP pseudonymized sensitive entities"
            );
        }

        tokens_added
    }

    fn mask_text_for_tenant(
        &self,
        text: &str,
        vault: &mut SessionVault,
        tenant: &TenantConfig,
    ) -> String {
        let spans = self.extract_matches_for_tenant(text, tenant);
        if spans.is_empty() {
            return text.to_string();
        }

        let mut output = String::with_capacity(text.len());
        let mut cursor = 0;

        for span in spans {
            if span.start > cursor {
                output.push_str(&text[cursor..span.start]);
            }

            let token = vault.get_or_create_token(span.entity_type.code(), &span.text);
            output.push_str(&token);

            cursor = span.end;
        }

        if cursor < text.len() {
            output.push_str(&text[cursor..]);
        }

        output
    }

    fn mask_chat_payload_for_tenant(
        &self,
        payload: &mut Value,
        vault: &mut SessionVault,
        tenant: &TenantConfig,
    ) -> usize {
        let initial_count = vault.token_count();

        if let Some(messages) = payload.get_mut("messages").and_then(Value::as_array_mut) {
            for message in messages {
                if let Some(content) = message.get_mut("content") {
                    if let Some(text) = content.as_str() {
                        let masked = self.mask_text_for_tenant(text, vault, tenant);
                        *content = Value::String(masked);
                    } else if let Some(parts) = content.as_array_mut() {
                        for part in parts {
                            if let Some(text_val) = part.get_mut("text") {
                                if let Some(text) = text_val.as_str() {
                                    let masked = self.mask_text_for_tenant(text, vault, tenant);
                                    *text_val = Value::String(masked);
                                }
                            }
                        }
                    }
                }
            }
        }

        let tokens_added = vault.token_count() - initial_count;
        if tokens_added > 0 {
            debug!(
                tenant_id = %tenant.id,
                tokens_masked = tokens_added,
                "Tenant Ingress DLP pseudonymized sensitive entities"
            );
        }

        tokens_added
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_mask_text_with_multiple_entities() {
        let engine = DlpEngine::from_rules_str("all");
        let mut vault = SessionVault::new();

        let input = "Client contact: alice@corp.com, NIR: 1 85 12 75 108 042 79, Card: 4532-0150-1808-1114.";
        let masked = engine.mask_text(input, &mut vault);

        assert!(masked.contains("{{__VAR_EMAIL_1__}}"));
        assert!(masked.contains("{{__VAR_NIR_1__}}"));
        assert!(masked.contains("{{__VAR_CARD_1__}}"));
        assert!(!masked.contains("alice@corp.com"));
        assert!(!masked.contains("4532-0150-1808-1114"));

        // Test roundtrip restoration
        let restored = vault.restore_in_text(&masked);
        assert_eq!(restored, input);
    }

    #[test]
    fn test_mask_text_international_pack() {
        let engine = DlpEngine::from_rules_str("all");
        let mut vault = SessionVault::new();

        let input = "IBAN: DE89 3704 0044 0532 0130 00, Secret: AKIAIOSFODNN7EXAMPLE, SSN: 219-45-7819, DNI: 12345678Z, IP: 192.168.1.50";
        let masked = engine.mask_text(input, &mut vault);

        assert!(masked.contains("{{__VAR_IBAN_1__}}"));
        assert!(masked.contains("{{__VAR_SECRET_1__}}"));
        assert!(masked.contains("{{__VAR_US_SSN_1__}}"));
        assert!(masked.contains("{{__VAR_ES_DNI_1__}}"));
        assert!(masked.contains("{{__VAR_IP_1__}}"));
        assert!(!masked.contains("DE89 3704 0044 0532 0130 00"));
        assert!(!masked.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(!masked.contains("219-45-7819"));
        assert!(!masked.contains("12345678Z"));
        assert!(!masked.contains("192.168.1.50"));

        let restored = vault.restore_in_text(&masked);
        assert_eq!(restored, input);
    }

    #[test]
    fn test_mask_chat_payload() {
        let engine = DlpEngine::new();
        let mut vault = SessionVault::new();

        let mut payload = json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "system", "content": "You are a helpful assistant."},
                {"role": "user", "content": "Please verify user bob@domain.com."}
            ]
        });

        let count = engine.mask_chat_payload(&mut payload, &mut vault);
        assert_eq!(count, 1);

        let user_content = payload["messages"][1]["content"].as_str().unwrap();
        assert_eq!(user_content, "Please verify user {{__VAR_EMAIL_1__}}.");
    }

    #[test]
    fn test_mask_chat_payload_for_tenant() {
        use crate::tenant::context::{TenantConfig, TenantId};
        use std::collections::{HashMap, HashSet};

        let engine = DlpEngine::from_rules_str("all");
        let mut vault_a = SessionVault::new();
        let mut vault_b = SessionVault::new();

        let mut rules_banking = HashSet::new();
        rules_banking.insert("card".to_string());
        rules_banking.insert("iban".to_string());

        let tenant_banking = TenantConfig {
            id: TenantId::from("tenant-bank"),
            organization_name: "FinTech SA".to_string(),
            user_id: None,
            user_name: None,
            role: None,
            default_model: None,
            allowed_models: None,
            fallback_models: None,
            enabled_rules: rules_banking,
            provider_keys: HashMap::new(),
            cache_enabled: true,
            rate_limit_rpm: None,
        };

        let mut rules_pii = HashSet::new();
        rules_pii.insert("email".to_string());

        let tenant_pii = TenantConfig {
            id: TenantId::from("tenant-pii"),
            organization_name: "CRM Corp".to_string(),
            user_id: None,
            user_name: None,
            role: None,
            default_model: None,
            allowed_models: None,
            fallback_models: None,
            enabled_rules: rules_pii,
            provider_keys: HashMap::new(),
            cache_enabled: true,
            rate_limit_rpm: None,
        };

        let mut payload_a = json!({
            "messages": [
                {"role": "user", "content": "Client email is user@corp.com, card is 4532-0150-1808-1114."}
            ]
        });
        let mut payload_b = payload_a.clone();

        // Tenant Banking only masks the card, NOT the email
        let count_a = engine.mask_chat_payload_for_tenant(&mut payload_a, &mut vault_a, &tenant_banking);
        assert_eq!(count_a, 1);
        let content_a = payload_a["messages"][0]["content"].as_str().unwrap();
        assert!(content_a.contains("user@corp.com")); // Not masked!
        assert!(content_a.contains("{{__VAR_CARD_1__}}")); // Masked!

        // Tenant PII only masks the email, NOT the card
        let count_b = engine.mask_chat_payload_for_tenant(&mut payload_b, &mut vault_b, &tenant_pii);
        assert_eq!(count_b, 1);
        let content_b = payload_b["messages"][0]["content"].as_str().unwrap();
        assert!(content_b.contains("{{__VAR_EMAIL_1__}}")); // Masked!
        assert!(content_b.contains("4532-0150-1808-1114")); // Not masked!
    }
}
