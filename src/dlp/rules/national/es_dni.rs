use crate::dlp::rules::DlpRule;
use crate::dlp::types::{EntityType, MatchSpan};
use regex::Regex;
use std::sync::LazyLock;

static DNI_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b([XYZxyz\d])(\d{7})[- ]?([A-Za-z])\b").expect("Spanish DNI/NIE regex must compile")
});

const DNI_LETTERS: &[u8] = b"TRWAGMYFPDXBNJZSQVHLCKE";

pub struct EsDniRule;

impl EsDniRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EsDniRule {
    fn default() -> Self {
        Self::new()
    }
}

/// Validates Spanish DNI and NIE using Modulo 23 checksum.
pub fn is_valid_dni_nie(first_char: char, digits_str: &str, control_char: char) -> bool {
    let mut num_str = String::with_capacity(8);

    match first_char.to_ascii_uppercase() {
        'X' => num_str.push('0'),
        'Y' => num_str.push('1'),
        'Z' => num_str.push('2'),
        c if c.is_ascii_digit() => num_str.push(c),
        _ => return false,
    }

    num_str.push_str(digits_str);

    let num: u64 = match num_str.parse() {
        Ok(n) => n,
        Err(_) => return false,
    };

    let idx = (num % 23) as usize;
    let expected = DNI_LETTERS[idx] as char;

    control_char.to_ascii_uppercase() == expected
}

impl DlpRule for EsDniRule {
    fn entity_type(&self) -> EntityType {
        EntityType::EsDni
    }

    fn find_matches(&self, text: &str) -> Vec<MatchSpan> {
        let mut spans = Vec::new();

        for cap in DNI_REGEX.captures_iter(text) {
            let full_match = cap.get(0).unwrap();
            let first_char = cap[1].chars().next().unwrap();
            let digits = &cap[2];
            let control = cap[3].chars().next().unwrap();

            if is_valid_dni_nie(first_char, digits, control) {
                spans.push(MatchSpan {
                    start: full_match.start(),
                    end: full_match.end(),
                    entity_type: EntityType::EsDni,
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
    fn test_valid_dni() {
        let rule = EsDniRule::new();
        // 12345678 % 23 = 14 -> 'Z'
        let text = "DNI del cliente: 12345678Z o 12345678-Z.";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_valid_nie() {
        let rule = EsDniRule::new();
        // X1234567 -> 01234567 % 23 = 19 -> 'L'
        let text = "NIE residente: X1234567L";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "X1234567L");
    }

    #[test]
    fn test_invalid_dni() {
        let rule = EsDniRule::new();
        // 12345678 with wrong letter
        assert!(rule.find_matches("12345678A").is_empty());
    }
}
