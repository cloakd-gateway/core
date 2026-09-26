use crate::dlp::rules::DlpRule;
use crate::dlp::types::{EntityType, MatchSpan};
use regex::Regex;
use std::sync::LazyLock;

static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b")
        .expect("Valid EMAIL regex pattern")
});

#[derive(Default, Debug, Clone)]
pub struct EmailRule;

impl EmailRule {
    pub fn new() -> Self {
        Self
    }
}

impl DlpRule for EmailRule {
    fn entity_type(&self) -> EntityType {
        EntityType::Email
    }

    fn find_matches(&self, text: &str) -> Vec<MatchSpan> {
        EMAIL_REGEX
            .find_iter(text)
            .map(|m| MatchSpan {
                start: m.start(),
                end: m.end(),
                entity_type: EntityType::Email,
                text: m.as_str().to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_rule() {
        let rule = EmailRule::new();
        let text = "Contact support@example.com or admin@corp.co.uk";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].text, "support@example.com");
        assert_eq!(matches[1].text, "admin@corp.co.uk");
    }
}
