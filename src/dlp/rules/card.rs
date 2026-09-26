use crate::dlp::rules::DlpRule;
use crate::dlp::types::{EntityType, MatchSpan};
use regex::Regex;
use std::sync::LazyLock;

static CARD_CANDIDATE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // 13 to 19 digits with optional spaces or dashes
    Regex::new(r"\b(?:\d[\s-]*?){13,19}\b")
        .expect("Valid CARD candidate regex pattern")
});

pub fn is_valid_luhn(candidate: &str) -> bool {
    let digits: Vec<u32> = candidate
        .chars()
        .filter(|c| c.is_ascii_digit())
        .filter_map(|c| c.to_digit(10))
        .collect();

    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }

    let mut sum = 0;
    let mut double = false;

    for &digit in digits.iter().rev() {
        if double {
            let doubled = digit * 2;
            sum += if doubled > 9 { doubled - 9 } else { doubled };
        } else {
            sum += digit;
        }
        double = !double;
    }

    sum % 10 == 0
}

#[derive(Default, Debug, Clone)]
pub struct CardRule;

impl CardRule {
    pub fn new() -> Self {
        Self
    }
}

impl DlpRule for CardRule {
    fn entity_type(&self) -> EntityType {
        EntityType::CreditCard
    }

    fn find_matches(&self, text: &str) -> Vec<MatchSpan> {
        CARD_CANDIDATE_REGEX
            .find_iter(text)
            .filter(|m| is_valid_luhn(m.as_str()))
            .map(|m| MatchSpan {
                start: m.start(),
                end: m.end(),
                entity_type: EntityType::CreditCard,
                text: m.as_str().to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_rule() {
        let rule = CardRule::new();
        let text = "Visa: 4532-0150-1808-1114, Amex: 3782-822463-10005, Fake: 12345678901234";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].text, "4532-0150-1808-1114");
        assert_eq!(matches[1].text, "3782-822463-10005");
    }
}
