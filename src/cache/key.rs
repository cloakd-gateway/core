use serde_json::Value;
use sha2::{Digest, Sha256};

/// Computes a tenant-scoped, deterministic SHA-256 cache key.
///
/// The cache is shared between users of the SAME tenant (FinOps savings), but never between
/// tenants: the tenant identifier and the resolved upstream provider are mixed into the key.
/// This prevents one tenant from reading or poisoning another tenant's cached responses.
///
/// The payload MUST already be pseudonymized by Ingress DLP.
pub fn compute_scoped_cache_key(tenant_id: &str, provider_id: &str, payload: &Value) -> String {
    let mut hasher = Sha256::new();
    // Length-prefixed fields so that ("a", "bc") and ("ab", "c") can never collide.
    hasher.update(b"tenant:");
    hasher.update(tenant_id.len().to_string().as_bytes());
    hasher.update(b":");
    hasher.update(tenant_id.as_bytes());
    hasher.update(b"\nprovider:");
    hasher.update(provider_id.len().to_string().as_bytes());
    hasher.update(b":");
    hasher.update(provider_id.as_bytes());
    hasher.update(b"\npayload:");
    hasher.update(compute_cache_key(payload).as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Computes a deterministic SHA-256 hash of an OpenAI-compatible completion payload.
///
/// This is the payload-only part of the cache key. Callers should use
/// [`compute_scoped_cache_key`] so that entries are isolated per tenant.
///
/// The payload passed to this function MUST already be pseudonymized by Ingress DLP.
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

    #[test]
    fn test_scoped_key_isolates_tenants() {
        let p = json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "Hello {{__VAR_EMAIL_1__}}"}]
        });

        let a = compute_scoped_cache_key("tenant-a", "openai", &p);
        let b = compute_scoped_cache_key("tenant-b", "openai", &p);
        assert_ne!(a, b, "same prompt must not share cache across tenants");
        assert_eq!(a, compute_scoped_cache_key("tenant-a", "openai", &p));
    }

    #[test]
    fn test_scoped_key_isolates_providers() {
        let p = json!({"model": "m", "messages": [{"role": "user", "content": "x"}]});
        assert_ne!(
            compute_scoped_cache_key("t", "openai", &p),
            compute_scoped_cache_key("t", "custom", &p)
        );
    }

    #[test]
    fn test_scoped_key_has_no_boundary_collision() {
        let p = json!({"model": "m", "messages": [{"role": "user", "content": "x"}]});
        assert_ne!(
            compute_scoped_cache_key("a", "bc", &p),
            compute_scoped_cache_key("ab", "c", &p)
        );
    }
}
