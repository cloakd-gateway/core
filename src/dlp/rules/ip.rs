use crate::dlp::rules::DlpRule;
use crate::dlp::types::{EntityType, MatchSpan};
use regex::Regex;
use std::net::Ipv4Addr;
use std::sync::LazyLock;

static IP_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b((?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(?:\.(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3})\b")
        .expect("IPv4 regex must compile")
});

pub struct IpRule;

impl IpRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for IpRule {
    fn default() -> Self {
        Self::new()
    }
}

fn is_private_ip(candidate: &str) -> bool {
    if let Ok(ipv4) = candidate.parse::<Ipv4Addr>() {
        ipv4.is_private() || ipv4.is_loopback() || ipv4.is_link_local()
    } else {
        false
    }
}

impl DlpRule for IpRule {
    fn entity_type(&self) -> EntityType {
        EntityType::IpAddress
    }

    fn find_matches(&self, text: &str) -> Vec<MatchSpan> {
        let mut spans = Vec::new();

        for mat in IP_REGEX.find_iter(text) {
            let candidate = mat.as_str();
            if is_private_ip(candidate) {
                spans.push(MatchSpan {
                    start: mat.start(),
                    end: mat.end(),
                    entity_type: EntityType::IpAddress,
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
    fn test_private_ips_detected() {
        let rule = IpRule::new();
        let text = "Cluster internal hosts: 10.244.0.15, 172.20.10.4 and 192.168.1.100 or localhost 127.0.0.1.";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 4);
    }

    #[test]
    fn test_public_ips_ignored() {
        let rule = IpRule::new();
        let text = "DNS servers: 8.8.8.8 and 1.1.1.1 should not be masked.";
        let matches = rule.find_matches(text);
        assert_eq!(matches.len(), 0);
    }
}
