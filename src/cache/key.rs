use serde_json::Value;
use sha2::{Digest, Sha256};

/// Computes a deterministic SHA-256 cache key for an OpenAI-compatible completion payload.
///
/// The payload passed to this function MUST already be pseudonymized by Ingress DLP.
/// This enables cache hits across different users while preserving total privacy isolation.
pub fn compute_cache_key(payload: &Value) -> String {
    let mut hasher = Sha256::new();

    // 1. Model identifier
    if let Some(model) = payload.get("model").and_then(Value::as_str) {
        hasher.update(b"model:");
        hasher.update(model.as_bytes());
        hasher.update(b"\n");
    }

    // 2. Ordered sequence of messages (role + content)
    if let Some(messages) = payload.get("messages").and_then(Value::as_array) {
        hasher.update(b"messages_count:");
        hasher.update(messages.len().to_string().as_bytes());
        hasher.update(b"\n");

        for (i, msg) in messages.iter().enumerate() {
            let role = msg.get("role").and_then(Value::as_str).unwrap_or("");
            hasher.update(format!("msg_{i}_role:{role}\n").as_bytes());

            if let Some(content) = msg.get("content") {
                hasher.update(b"content:");
                if let Some(text) = content.as_str() {
                    hasher.update(text.as_bytes());
                } else {
                    hasher.update(content.to_string().as_bytes());
                }
                hasher.update(b"\n");
            }
        }
    }

    // 3. Generation hyper-parameters affecting determinism
    const DETERMINISTIC_KEYS: &[&str] = &[
        "temperature",
        "top_p",
        "max_tokens",
        "max_completion_tokens",
        "response_format",
        "tools",
        "tool_choice",
    ];

    for &param in DETERMINISTIC_KEYS {
        if let Some(val) = payload.get(param) {
            hasher.update(param.as_bytes());
            hasher.update(b":");
            hasher.update(val.to_string().as_bytes());
            hasher.update(b"\n");
        }
    }

    let result = hasher.finalize();
    format!("{:x}", result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_identical_payloads_generate_same_key() {
        let p1 = json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "user", "content": "Hello {{__VAR_EMAIL_1__}}"}
            ],
            "temperature": 0.7
        });

        let p2 = json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "user", "content": "Hello {{__VAR_EMAIL_1__}}"}
            ],
            "temperature": 0.7
        });

        assert_eq!(compute_cache_key(&p1), compute_cache_key(&p2));
    }

    #[test]
    fn test_different_prompts_generate_different_keys() {
        let p1 = json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "user", "content": "Hello {{__VAR_EMAIL_1__}}"}
            ]
        });

        let p2 = json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "user", "content": "Goodbye {{__VAR_EMAIL_1__}}"}
            ]
        });

        assert_ne!(compute_cache_key(&p1), compute_cache_key(&p2));
    }

    #[test]
    fn test_different_models_generate_different_keys() {
        let p1 = json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "test"}]
        });

        let p2 = json!({
            "model": "gemini-3.5-flash-lite",
            "messages": [{"role": "user", "content": "test"}]
        });

        assert_ne!(compute_cache_key(&p1), compute_cache_key(&p2));
    }
}
