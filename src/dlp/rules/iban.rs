use crate::dlp::rules::DlpRule;
use crate::dlp::types::{EntityType, MatchSpan};
use regex::Regex;
use std::sync::LazyLock;

static IBAN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b([A-Z]{2}\d{2}(?:[0-9A-Z]{11,30}|(?:\s[0-9A-Z]{2,4}){3,8}))\b")
        .expect("IBAN regex must compile")
});

pub struct IbanRule;

impl IbanRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for IbanRule {
    fn default() -> Self {
        Self::new()
    }
}

/// Known country IBAN lengths according to ISO 13616.
fn expected_country_length(country: &str) -> Option<usize> {
    match country {
        "AL" => Some(28),
        "AD" => Some(24),
        "AT" => Some(20),
        "AZ" => Some(28),
        "BH" => Some(22),
        "BY" => Some(28),
        "BE" => Some(16),
        "BA" => Some(20),
        "BR" => Some(29),
        "BG" => Some(22),
        "CR" => Some(22),
        "HR" => Some(21),
        "CY" => Some(28),
        "CZ" => Some(24),
        "DK" => Some(18),
        "DO" => Some(28),
        "EE" => Some(20),
        "FO" => Some(18),
        "FI" => Some(18),
        "FR" => Some(27),
        "GE" => Some(22),
        "DE" => Some(22),
        "GI" => Some(23),
        "GR" => Some(27),
        "GL" => Some(18),
        "GT" => Some(28),
        "HU" => Some(28),
        "IS" => Some(26),
        "IE" => Some(22),
        "IL" => Some(23),
        "IT" => Some(27),
        "JO" => Some(30),
        "KZ" => Some(20),
        "XK" => Some(20),
        "KW" => Some(30),
        "LV" => Some(21),
        "LB" => Some(28),
        "LI" => Some(21),
        "LT" => Some(20),
        "LU" => Some(20),
        "MK" => Some(19),
        "MT" => Some(31),
        "MR" => Some(27),
        "MU" => Some(30),
        "MC" => Some(27),
        "MD" => Some(24),
        "ME" => Some(22),
        "NL" => Some(18),
        "NO" => Some(15),
        "PK" => Some(24),
        "PS" => Some(29),
        "PL" => Some(28),
        "PT" => Some(25),
        "QA" => Some(29),
        "RO" => Some(24),
        "LC" => Some(32),
        "SM" => Some(27),
        "ST" => Some(25),
        "SA" => Some(24),
        "RS" => Some(22),
        "SC" => Some(31),
        "SK" => Some(24),
        "SI" => Some(19),
        "ES" => Some(24),
        "SE" => Some(24),
        "CH" => Some(21),
        "TL" => Some(23),
        "TN" => Some(24),
        "TR" => Some(26),
        "UA" => Some(29),
        "AE" => Some(23),
        "GB" => Some(22),
        "VA" => Some(22),
        "VG" => Some(24),
        _ => None,
    }
}

/// Validates an IBAN using ISO 7064 Modulo 97-10 check.
pub fn is_valid_iban(raw: &str) -> bool {
    let clean: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();

    let len = clean.len();
    if !(15..=34).contains(&len) {
        return false;
    }

    let country = &clean[..2];
    if let Some(expected_len) = expected_country_length(country) {
        if len != expected_len {
            return false;
        }
    }

    // Rearrange: move first 4 characters to the end
    let rearranged = format!("{}{}", &clean[4..], &clean[..4]);

    // Stream digits and compute modulo 97
    let mut remainder: u64 = 0;
    for ch in rearranged.chars() {
        if ch.is_ascii_digit() {
            let digit = ch.to_digit(10).unwrap() as u64;
            remainder = (remainder * 10 + digit) % 97;
        } else if ch.is_ascii_uppercase() {
            let val = (ch as u64) - ('A' as u64) + 10;
            // val is a two digit number between 10 and 35
            remainder = (remainder * 100 + val) % 97;
        } else {
            return false;
        }
    }

    remainder == 1
}

impl DlpRule for IbanRule {
    fn entity_type(&self) -> EntityType {
        EntityType::Iban
    }

    fn find_matches(&self, text: &str) -> Vec<MatchSpan> {
        let mut spans = Vec::new();

        for mat in IBAN_REGEX.find_iter(text) {
            let candidate = mat.as_str();
            if is_valid_iban(candidate) {
                spans.push(MatchSpan {
                    start: mat.start(),
                    end: mat.end(),
                    entity_type: EntityType::Iban,
                    text: candidate.to_string(),
                });
            }
        }

        spans
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_ibans() {
        // Valid French IBAN
        assert!(is_valid_iban("FR1420041010050500013M02606"));
        assert!(is_valid_iban("FR14 2004 1010 0505 0001 3M02 606"));
        // Valid German IBAN
        assert!(is_valid_iban("DE89370400440532013000"));
        assert!(is_valid_iban("DE89 3704 0044 0532 0130 00"));
        // Valid UK IBAN
        assert!(is_valid_iban("GB29NWBK60161331926819"));
        // Valid Spanish IBAN
        assert!(is_valid_iban("ES9121000418450200051332"));
    }

    #[test]
    fn test_invalid_ibans() {
        // Incorrect checksum
        assert!(!is_valid_iban("FR1420041010050500013M02607"));
        assert!(!is_valid_iban("DE89370400440532013001"));
        // Too short
        assert!(!is_valid_iban("FR14200410"));
        // Invalid characters
        assert!(!is_valid_iban("FR1420041010050500013M026@@"));
    }

    #[test]
    fn test_rule_find_matches() {
        let rule = IbanRule::new();
        let text = "Virement vers DE89 3704 0044 0532 0130 00 et non valide FR1420041010050500013M02607.";
        let matches = rule.find_matches(text);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "DE89 3704 0044 0532 0130 00");
    }
}
