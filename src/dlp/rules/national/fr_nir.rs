use crate::dlp::rules::DlpRule;
use crate::dlp::types::{EntityType, MatchSpan};
use regex::Regex;
use std::sync::LazyLock;

static NIR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // Matches French NIR (Sécurité Sociale): starts with 1 or 2, followed by 12 or 14 digits with optional spaces/dashes
    Regex::new(r"\b[12](?:[\s.-]*\d){12}(?:(?:[\s.-]*\d){2})?\b")
        .expect("Valid NIR regex pattern")
});

pub fn is_valid_nir(candidate: &str) -> bool {
    let digits: Vec<u32> = candidate
        .chars()
        .filter(|c| c.is_ascii_digit())
        .filter_map(|c| c.to_digit(10))
        .collect();

    if digits.len() != 13 && digits.len() != 15 {
        return false;
    }

    // First digit must be 1 (Male) or 2 (Female)
    if digits[0] != 1 && digits[0] != 2 {
        return false;
    }

    // Month of birth: digits[3] and digits[4] (01 to 12, or 20-42 for special cases, or 99)
    let month = digits[3] * 10 + digits[4];
    if month == 0 || (month > 12 && !(20..=42).contains(&month) && month != 99) {
        return false;
    }

    // If 15 digits are present, validate Modulo 97 control key
    if digits.len() == 15 {
        let mut base_num: u64 = 0;
        for &d in &digits[..13] {
            base_num = base_num * 10 + d as u64;
        }
        let key = (digits[13] * 10 + digits[14]) as u64;
        let expected_key = 97 - (base_num % 97);
        if key != expected_key {
            return false;
        }
    }

    true
}

#[derive(Default, Debug, Clone)]
pub struct FrNirRule;

impl FrNirRule {
    pub fn new() -> Self {
        Self
    }
}

impl DlpRule for FrNirRule {
    fn entity_type(&self) -> EntityType {
        EntityType::Nir
    }

    fn find_matches(&self, text: &str) -> Vec<MatchSpan> {
        NIR_REGEX
            .find_iter(text)
            .filter(|m| is_valid_nir(m.as_str()))
            .map(|m| MatchSpan {
                start: m.start(),
                end: m.end(),
                entity_type: EntityType::Nir,
                text: m.as_str().to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nir_rule() {
        let rule = FrNirRule::new();
        // 1 85 12 75 108 042 79 -> 1851275108042 % 97 = 18 -> 97 - 18 = 79 (exact match!)
        let valid_nir_1 = "Secu: 1 85 12 75 108 042 79";
        let matches = rule.find_matches(valid_nir_1);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "1 85 12 75 108 042 79");

        let invalid = "Num: 3 85 12 75 108 042 12";
        assert!(rule.find_matches(invalid).is_empty());

        let invalid_key = "Secu: 1 85 12 75 108 042 99";
        assert!(rule.find_matches(invalid_key).is_empty());
    }
}
