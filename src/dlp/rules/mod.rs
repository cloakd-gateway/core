pub mod card;
pub mod email;
pub mod iban;
pub mod ip;
pub mod national;
pub mod nir;
pub mod phone;
pub mod secrets;

use crate::dlp::types::{EntityType, MatchSpan};
use std::collections::HashSet;
use tracing::info;

#[allow(unused_imports)]
pub use card::CardRule;
#[allow(unused_imports)]
pub use email::EmailRule;
#[allow(unused_imports)]
pub use iban::IbanRule;
#[allow(unused_imports)]
pub use ip::IpRule;
#[allow(unused_imports)]
pub use national::{EsDniRule, FrNirRule, UkNinoRule, UsSsnRule};
#[allow(unused_imports)]
pub use nir::NirRule;
#[allow(unused_imports)]
pub use phone::PhoneRule;
#[allow(unused_imports)]
pub use secrets::SecretsRule;

/// Core trait representing an autonomous DLP inspection rule.
pub trait DlpRule: Send + Sync {
    /// Identifies the sensitive entity type handled by this rule.
    #[allow(dead_code)]
    fn entity_type(&self) -> EntityType;

    /// Scans text and extracts non-overlapping or candidate match spans.
    fn find_matches(&self, text: &str) -> Vec<MatchSpan>;
}

/// Expands packs and tokens into a set of unique rule names.
fn expand_rule_names(input: &str) -> HashSet<String> {
    let mut resolved = HashSet::new();

    for raw_token in input.split(|c: char| c == ',' || c.is_whitespace()) {
        let token = raw_token.trim().to_lowercase();
        if token.is_empty() {
            continue;
        }

        match token.as_str() {
            // Pack: default (Universal rules)
            "default" => {
                resolved.insert("email".to_string());
                resolved.insert("card".to_string());
                resolved.insert("iban".to_string());
                resolved.insert("phone".to_string());
                resolved.insert("secrets".to_string());
                resolved.insert("ip".to_string());
            }
            // Pack: all (Universal + all national rules)
            "all" => {
                resolved.insert("email".to_string());
                resolved.insert("card".to_string());
                resolved.insert("iban".to_string());
                resolved.insert("phone".to_string());
                resolved.insert("secrets".to_string());
                resolved.insert("ip".to_string());
                resolved.insert("fr_nir".to_string());
                resolved.insert("us_ssn".to_string());
                resolved.insert("uk_nino".to_string());
                resolved.insert("es_dni".to_string());
            }
            // Pack: banking
            "banking" | "finance" => {
                resolved.insert("card".to_string());
                resolved.insert("iban".to_string());
            }
            // Pack: secrets (Cyber Resilience Act / NIS 2)
            "secrets_pack" | "cra" | "nis2" => {
                resolved.insert("secrets".to_string());
                resolved.insert("ip".to_string());
            }
            // Pack: pii
            "pii" => {
                resolved.insert("email".to_string());
                resolved.insert("phone".to_string());
            }
            // Country packs
            "fr" | "france" => {
                resolved.insert("fr_nir".to_string());
            }
            "us" | "usa" => {
                resolved.insert("us_ssn".to_string());
            }
            "uk" | "gb" => {
                resolved.insert("uk_nino".to_string());
            }
            "es" | "spain" => {
                resolved.insert("es_dni".to_string());
            }
            "eu" => {
                resolved.insert("iban".to_string());
                resolved.insert("fr_nir".to_string());
                resolved.insert("uk_nino".to_string());
                resolved.insert("es_dni".to_string());
            }
            // Unit rule aliases
            "email" => {
                resolved.insert("email".to_string());
            }
            "card" | "creditcard" | "credit_card" => {
                resolved.insert("card".to_string());
            }
            "iban" => {
                resolved.insert("iban".to_string());
            }
            "phone" => {
                resolved.insert("phone".to_string());
            }
            "secret" | "secrets" => {
                resolved.insert("secrets".to_string());
            }
            "ip" | "ipaddress" | "ip_address" => {
                resolved.insert("ip".to_string());
            }
            "fr_nir" | "nir" => {
                resolved.insert("fr_nir".to_string());
            }
            "us_ssn" | "ssn" => {
                resolved.insert("us_ssn".to_string());
            }
            "uk_nino" | "nino" => {
                resolved.insert("uk_nino".to_string());
            }
            "es_dni" | "dni" | "nie" => {
                resolved.insert("es_dni".to_string());
            }
            unknown => {
                tracing::warn!(rule = unknown, "Unknown DLP rule or pack specified, ignoring");
            }
        }
    }

    if resolved.is_empty() {
        // Fallback to default pack if nothing valid was specified
        return expand_rule_names("default");
    }

    resolved
}

/// Builds active DLP rule instances based on a configuration string.
pub fn build_rules(enabled_names: &str) -> Vec<Box<dyn DlpRule>> {
    let resolved_set = expand_rule_names(enabled_names);
    let mut rules: Vec<Box<dyn DlpRule>> = Vec::with_capacity(resolved_set.len());
    let mut active_names = Vec::new();

    // Instantiate in predictable order: universal first, national second
    if resolved_set.contains("email") {
        rules.push(Box::new(EmailRule::new()));
        active_names.push("email");
    }
    if resolved_set.contains("card") {
        rules.push(Box::new(CardRule::new()));
        active_names.push("card");
    }
    if resolved_set.contains("iban") {
        rules.push(Box::new(IbanRule::new()));
        active_names.push("iban");
    }
    if resolved_set.contains("phone") {
        rules.push(Box::new(PhoneRule::new()));
        active_names.push("phone");
    }
    if resolved_set.contains("secrets") {
        rules.push(Box::new(SecretsRule::new()));
        active_names.push("secrets");
    }
    if resolved_set.contains("ip") {
        rules.push(Box::new(IpRule::new()));
        active_names.push("ip");
    }
    if resolved_set.contains("fr_nir") {
        rules.push(Box::new(FrNirRule::new()));
        active_names.push("fr_nir");
    }
    if resolved_set.contains("us_ssn") {
        rules.push(Box::new(UsSsnRule::new()));
        active_names.push("us_ssn");
    }
    if resolved_set.contains("uk_nino") {
        rules.push(Box::new(UkNinoRule::new()));
        active_names.push("uk_nino");
    }
    if resolved_set.contains("es_dni") {
        rules.push(Box::new(EsDniRule::new()));
        active_names.push("es_dni");
    }

    info!(
        active_rules_count = rules.len(),
        rules = ?active_names,
        "Configured DLP inspection rules"
    );

    rules
}

/// Constructs the default suite of production DLP rules.
pub fn default_rules() -> Vec<Box<dyn DlpRule>> {
    build_rules("default")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_default_pack() {
        let rules = build_rules("default");
        assert_eq!(rules.len(), 6);
    }

    #[test]
    fn test_expand_all_pack() {
        let rules = build_rules("all");
        assert_eq!(rules.len(), 10);
    }

    #[test]
    fn test_granular_selection() {
        let rules = build_rules("email, iban, us_ssn");
        assert_eq!(rules.len(), 3);
    }

    #[test]
    fn test_combine_default_and_country_packs() {
        let rules = build_rules("default, us, fr");
        // default (6) + us_ssn (1) + fr_nir (1) = 8
        assert_eq!(rules.len(), 8);
    }
}
