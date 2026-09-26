use crate::dlp::rules::DlpRule;
use crate::dlp::types::{EntityType, MatchSpan};
use regex::Regex;
use std::sync::LazyLock;

static PHONE_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // 1. E.164 International Phone format: +<country_code> ...
        Regex::new(r"\+(?:[1-9]\d{0,2})[\s.-]?(?:\(?\d{1,4}\)?[\s.-]?)?\d{1,4}[\s.-]?\d{1,4}(?:[\s.-]?\d{1,6})\b")
            .expect("E.164 phone regex must compile"),
        // 2. North America (NANP) national format: (xxx) xxx-xxxx or xxx-xxx-xxxx
        Regex::new(r"\b(?:\+?1[\s.-]?)?\(?[2-9]\d{2}\)?[\s.-][2-9]\d{2}[\s.-]\d{4}\b")
            .expect("US phone regex must compile"),
        // 3. France national format: 01 xx xx xx xx to 09 xx xx xx xx
        Regex::new(r"\b0[1-9](?:[\s.-]?\d{2}){4}\b").expect("FR phone regex must compile"),
    ]
});

pub struct PhoneRule;

impl PhoneRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PhoneRule {
    fn default() -> Self {
        Self::new()
    }
}

fn is_plausible_phone(raw: &str) -> bool {
    let digit_count = raw.chars().filter(|c| c.is_ascii_digit()).count();
    // E.164 max digits is 15, standard phones have at least 8 digits
    (8..=15).contains(&digit_count)
}

impl DlpRule for PhoneRule {
    fn entity_type(&self) -> EntityType {
        EntityType::Phone
    }

    fn find_matches(&self, text: &str) -> Vec<MatchSpan> {
        let mut spans = Vec::new();

        for re in PHONE_PATTERNS.iter() {
            for mat in re.find_iter(text) {
                let candidate = mat.as_str();
                if is_plausible_phone(candidate) {
                    spans.push(MatchSpan {
                        start: mat.start(),
                        end: mat.end(),
                        entity_type: EntityType::Phone,
                        text: candidate.to_string(),
                    });
                }
            }
        }

        if spans.len() <= 1 {
            return spans;
        }

        spans.sort_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
        });

        let mut filtered = Vec::with_capacity(spans.len());
        let mut last_end = 0;
        for span in spans {
            if span.start >= last_end {
                last_end = span.end;
                filtered.push(span);
            }
        }
        filtered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_international_e164() {
        let rule = PhoneRule::new();
        let text = "Call me at +33 6 12 34 56 78 or +1 (415) 555-2671 or +49 30 1234567.";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn test_french_national() {
        let rule = PhoneRule::new();
        let text = "Support standard: 01.42.68.55.00 ou 06 98 76 54 32.";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_us_national() {
        let rule = PhoneRule::new();
        let text = "Office line: (212) 555-0199 or 415-555-0132.";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 2);
    }
}
