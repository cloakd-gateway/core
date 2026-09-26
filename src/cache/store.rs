use crate::config::AppConfig;
use moka::future::Cache;
use serde_json::Value;
use std::time::Duration;
use tracing::debug;

#[derive(Clone)]
pub struct PromptCache {
    enabled: bool,
    store: Cache<String, Value>,
}

impl PromptCache {
    pub fn new(enabled: bool, ttl_secs: u64, max_capacity: u64) -> Self {
        let store = Cache::builder()
            .max_capacity(max_capacity)
            .time_to_live(Duration::from_secs(ttl_secs))
            .build();

        Self { enabled, store }
    }

    pub fn from_config(config: &AppConfig) -> Self {
        Self::new(
            config.cache_enabled,
            config.cache_ttl_secs,
            config.cache_max_capacity,
        )
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Retrieves an anonymized response from cache if it exists.
    pub async fn get(&self, key: &str) -> Option<Value> {
        if !self.enabled {
            return None;
        }

        let entry = self.store.get(key).await;
        if entry.is_some() {
            debug!(cache_key = %key, "Prompt cache HIT");
        } else {
            debug!(cache_key = %key, "Prompt cache MISS");
        }
        entry
    }

    /// Stores an anonymized LLM response in cache.
    pub async fn insert(&self, key: String, value: Value) {
        if !self.enabled {
            return;
        }

        debug!(cache_key = %key, "Caching anonymized prompt response");
        self.store.insert(key, value).await;
    }

    #[allow(dead_code)]
    pub fn entry_count(&self) -> u64 {
        self.store.entry_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_cache_insert_and_get() {
        let cache = PromptCache::new(true, 60, 100);
        let key = "test-hash-key".to_string();
        let value = json!({
            "id": "chatcmpl-123",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello {{__VAR_EMAIL_1__}}"
                }
            }]
        });

        assert!(cache.get(&key).await.is_none());

        cache.insert(key.clone(), value.clone()).await;

        let retrieved = cache.get(&key).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), value);
    }

    #[tokio::test]
    async fn test_cache_disabled() {
        let cache = PromptCache::new(false, 60, 100);
        let key = "test-hash-key".to_string();
        let value = json!({"result": "ok"});

        cache.insert(key.clone(), value).await;
        assert!(cache.get(&key).await.is_none());
    }
}
