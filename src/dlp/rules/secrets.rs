use crate::dlp::rules::DlpRule;
use crate::dlp::types::{EntityType, MatchSpan};
use regex::Regex;
use std::sync::LazyLock;

/// Regexes targeting critical secrets and credentials according to CRA / NIS 2 requirements.
static SECRET_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // 1. AWS Access Key ID
        Regex::new(r"\b(AKIA[0-9A-Z]{16})\b").expect("AWS regex must compile"),
        // 2. GitHub Personal Access Tokens (Classic & Fine-Grained)
        Regex::new(r"\b((?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9_]{36})\b")
            .expect("GitHub classic token regex must compile"),
        Regex::new(r"\b(github_pat_[A-Za-z0-9_]{82})\b")
            .expect("GitHub PAT regex must compile"),
        // 3. Google API Key (AIzaSy...)
        Regex::new(r"\b(AIzaSy[A-Za-z0-9_-]{33})\b").expect("Google API key regex must compile"),
        // 4. OpenAI, Anthropic & Generic sk- API Keys
        Regex::new(r"\b(sk-(?:proj-|ant-)?[A-Za-z0-9_-]{32,})\b")
            .expect("LLM API key regex must compile"),
        // 5. JSON Web Tokens (JWT)
        Regex::new(r"\b(eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,})\b")
            .expect("JWT regex must compile"),
        // 6. PEM Private Key Blocks
        Regex::new(r"-----BEGIN (?:[A-Z ]* )?PRIVATE KEY-----[\s\S]*?-----END (?:[A-Z ]* )?PRIVATE KEY-----")
            .expect("PEM private key regex must compile"),
        // 7. Database connection URLs with passwords
        Regex::new(r"\b(?:postgres|postgresql|mysql|mongodb|redis)://[^\s:]+:[^\s@]+@[^\s]+\b")
            .expect("DB URL regex must compile"),
    ]
});

pub struct SecretsRule;

impl SecretsRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SecretsRule {
    fn default() -> Self {
        Self::new()
    }
}

impl DlpRule for SecretsRule {
    fn entity_type(&self) -> EntityType {
        EntityType::Secret
    }

    fn find_matches(&self, text: &str) -> Vec<MatchSpan> {
        let mut spans = Vec::new();

        for re in SECRET_PATTERNS.iter() {
            for mat in re.find_iter(text) {
                spans.push(MatchSpan {
                    start: mat.start(),
                    end: mat.end(),
                    entity_type: EntityType::Secret,
                    text: mat.as_str().to_string(),
                });
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
    fn test_detect_aws_key() {
        let rule = SecretsRule::new();
        let text = "Deploy using key AKIAIOSFODNN7EXAMPLE today.";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "AKIAIOSFODNN7EXAMPLE");
    }

    #[test]
    fn test_detect_github_token() {
        let rule = SecretsRule::new();
        let text = "ghp_1234567890abcdefghijklmnopqrstuvwxyz";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, text);
    }

    #[test]
    fn test_detect_google_api_key() {
        let rule = SecretsRule::new();
        let text = "API key: AIzaSyA1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6Q";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "AIzaSyA1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6Q");
    }

    #[test]
    fn test_detect_openai_and_anthropic_keys() {
        let rule = SecretsRule::new();
        let text = "Keys: sk-proj-abc123456789012345678901234567890 and sk-ant-api03-abcdefghijklmnopqrstuvwxyz1234567890";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_detect_jwt() {
        let rule = SecretsRule::new();
        let text = "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_detect_pem_private_key() {
        let rule = SecretsRule::new();
        let text = "Here is the key:\n-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA0\n-----END RSA PRIVATE KEY-----\nDone.";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].text.contains("BEGIN RSA PRIVATE KEY"));
    }

    #[test]
    fn test_detect_db_url() {
        let rule = SecretsRule::new();
        let text = "Database uri: postgres://admin:super_secret_pw@db.internal.net:5432/production";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "postgres://admin:super_secret_pw@db.internal.net:5432/production");
    }
}
