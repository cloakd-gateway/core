use crate::dlp::rules::DlpRule;
use crate::dlp::types::{EntityType, MatchSpan};
use regex::Regex;
use std::sync::LazyLock;

static NINO_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b([A-Z]{2})\s*(\d{2})\s*(\d{2})\s*(\d{2})\s*([A-D])\b")
        .expect("UK NINO regex must compile")
});

pub struct UkNinoRule;

impl UkNinoRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UkNinoRule {
    fn default() -> Self {
        Self::new()
    }
}

/// Validates UK National Insurance Number according to HMRC rules.
pub fn is_valid_nino(prefix: &str, suffix: &str) -> bool {
    if prefix.len() != 2 || suffix.len() != 1 {
        return false;
    }

    let p_bytes = prefix.as_bytes();
    let c1 = p_bytes[0] as char;
    let c2 = p_bytes[1] as char;

    // HMRC Disallowed letters in 1st and 2nd positions: D, F, I, Q, U, V
    const DISALLOWED: [char; 6] = ['D', 'F', 'I', 'Q', 'U', 'V'];
    if DISALLOWED.contains(&c1) || DISALLOWED.contains(&c2) {
        return false;
    }

    // Letter 'O' cannot be used as 2nd letter
    if c2 == 'O' {
        return false;
    }

    // Disallowed prefix combinations
    const INVALID_PREFIXES: [&str; 7] = ["BG", "GB", "KN", "NK", "NT", "TN", "ZZ"];
    if INVALID_PREFIXES.contains(&prefix) {
        return false;
    }

    // Suffix must be A, B, C, or D
    let s_char = suffix.chars().next().unwrap();
    if !matches!(s_char, 'A' | 'B' | 'C' | 'D') {
        return false;
    }

    true
}

impl DlpRule for UkNinoRule {
    fn entity_type(&self) -> EntityType {
        EntityType::UkNino
    }

    fn find_matches(&self, text: &str) -> Vec<MatchSpan> {
        let mut spans = Vec::new();

        for cap in NINO_REGEX.captures_iter(text) {
            let full_match = cap.get(0).unwrap();
            let prefix = &cap[1];
            let suffix = &cap[5];

            if is_valid_nino(prefix, suffix) {
                spans.push(MatchSpan {
                    start: full_match.start(),
                    end: full_match.end(),
                    entity_type: EntityType::UkNino,
                    text: full_match.as_str().to_string(),
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
    fn test_valid_uk_nino() {
        let rule = UkNinoRule::new();
        let text = "HMRC reference: QQ 12 34 56 A or JH218765C.";
        let matches = rule.find_matches(text);
        // QQ has Q which is disallowed!
        // JH 21 87 65 C is valid!
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "JH218765C");
    }

    #[test]
    fn test_invalid_uk_nino() {
        let rule = UkNinoRule::new();
        // GB prefix is disallowed
        assert!(rule.find_matches("GB 12 34 56 A").is_empty());
        // Second letter O is disallowed
        assert!(rule.find_matches("AO 12 34 56 B").is_empty());
        // Disallowed letter D
        assert!(rule.find_matches("DA 12 34 56 C").is_empty());
    }
}
