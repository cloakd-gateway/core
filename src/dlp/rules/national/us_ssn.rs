use crate::dlp::rules::DlpRule;
use crate::dlp::types::{EntityType, MatchSpan};
use regex::Regex;
use std::sync::LazyLock;

static SSN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(\d{3})[-\s](\d{2})[-\s](\d{4})\b").expect("SSN regex must compile")
});

pub struct UsSsnRule;

impl UsSsnRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UsSsnRule {
    fn default() -> Self {
        Self::new()
    }
}

/// Validates US Social Security Number per SSA guidelines.
pub fn is_valid_ssn(area_str: &str, group_str: &str, serial_str: &str) -> bool {
    let area = match area_str.parse::<u32>() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let group = match group_str.parse::<u32>() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let serial = match serial_str.parse::<u32>() {
        Ok(v) => v,
        Err(_) => return false,
    };

    // SSA Area restrictions:
    // - Not 000
    // - Not 666
    // - Not 900-999
    if area == 0 || area == 666 || area >= 900 {
        return false;
    }

    // SSA Group restrictions:
    // - Not 00
    if group == 0 {
        return false;
    }

    // SSA Serial restrictions:
    // - Not 0000
    if serial == 0 {
        return false;
    }

    true
}

impl DlpRule for UsSsnRule {
    fn entity_type(&self) -> EntityType {
        EntityType::UsSsn
    }

    fn find_matches(&self, text: &str) -> Vec<MatchSpan> {
        let mut spans = Vec::new();

        for cap in SSN_REGEX.captures_iter(text) {
            let full_match = cap.get(0).unwrap();
            let area = &cap[1];
            let group = &cap[2];
            let serial = &cap[3];

            if is_valid_ssn(area, group, serial) {
                spans.push(MatchSpan {
                    start: full_match.start(),
                    end: full_match.end(),
                    entity_type: EntityType::UsSsn,
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
    fn test_valid_ssn() {
        let rule = UsSsnRule::new();
        let text = "Employee SSN: 219-45-7819 in record.";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "219-45-7819");
    }

    #[test]
    fn test_invalid_ssn() {
        let rule = UsSsnRule::new();
        // 000 area
        assert!(rule.find_matches("000-45-7819").is_empty());
        // 666 area
        assert!(rule.find_matches("666-45-7819").is_empty());
        // 900+ area
        assert!(rule.find_matches("912-45-7819").is_empty());
        // 00 group
        assert!(rule.find_matches("219-00-7819").is_empty());
        // 0000 serial
        assert!(rule.find_matches("219-45-0000").is_empty());
    }
}
