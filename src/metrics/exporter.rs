use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// Thread-safe in-memory metrics collector producing Prometheus text format.
#[derive(Default)]
pub struct MetricsCollector {
    requests: RwLock<HashMap<(String, u16, String), Arc<AtomicU64>>>,
    dlp_entities: RwLock<HashMap<(String, String), Arc<AtomicU64>>>,
    cache_hits: RwLock<HashMap<String, Arc<AtomicU64>>>,
    cache_misses: RwLock<HashMap<String, Arc<AtomicU64>>>,
    failovers: RwLock<HashMap<(String, String), Arc<AtomicU64>>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an HTTP request completion.
    pub fn record_request(&self, tenant: &str, status: u16, cache: &str) {
        let key = (tenant.to_string(), status, cache.to_string());
        self.increment_counter(&self.requests, key, 1);
    }

    /// Records DLP masked entities by type.
    pub fn record_masked_entity(&self, tenant: &str, entity_type: &str, count: u64) {
        if count == 0 {
            return;
        }
        let key = (tenant.to_string(), entity_type.to_string());
        self.increment_counter(&self.dlp_entities, key, count);
    }

    /// Records a FinOps prompt cache hit.
    pub fn record_cache_hit(&self, tenant: &str) {
        self.increment_counter(&self.cache_hits, tenant.to_string(), 1);
    }

    /// Records a FinOps prompt cache miss.
    pub fn record_cache_miss(&self, tenant: &str) {
        self.increment_counter(&self.cache_misses, tenant.to_string(), 1);
    }

    /// Records an upstream failover event.
    pub fn record_failover(&self, from: &str, to: &str) {
        let key = (from.to_string(), to.to_string());
        self.increment_counter(&self.failovers, key, 1);
    }

    fn increment_counter<K: std::hash::Hash + Eq + Clone>(
        &self,
        map_lock: &RwLock<HashMap<K, Arc<AtomicU64>>>,
        key: K,
        val: u64,
    ) {
        // Fast read lock path
        {
            let read_guard = map_lock.read().unwrap();
            if let Some(counter) = read_guard.get(&key) {
                counter.fetch_add(val, Ordering::Relaxed);
                return;
            }
        }

        // Write lock path to insert new counter
        let mut write_guard = map_lock.write().unwrap();
        let counter = write_guard
            .entry(key)
            .or_insert_with(|| Arc::new(AtomicU64::new(0)));
        counter.fetch_add(val, Ordering::Relaxed);
    }

    /// Renders all metrics in Prometheus text exposition format (version 0.0.4).
    pub fn render_prometheus(&self) -> String {
        let mut out = String::with_capacity(2048);

        // 1. HTTP Requests
        out.push_str("# HELP cloakd_http_requests_total Total number of HTTP requests processed by Cloakd\n");
        out.push_str("# TYPE cloakd_http_requests_total counter\n");
        {
            let guard = self.requests.read().unwrap();
            for ((tenant, status, cache), counter) in guard.iter() {
                let val = counter.load(Ordering::Relaxed);
                out.push_str(&format!(
                    "cloakd_http_requests_total{{tenant=\"{tenant}\",status=\"{status}\",cache=\"{cache}\"}} {val}\n"
                ));
            }
        }
        out.push('\n');

        // 2. DLP Masked Entities
        out.push_str("# HELP cloakd_dlp_masked_entities_total Total sensitive PII entities masked by DLP\n");
        out.push_str("# TYPE cloakd_dlp_masked_entities_total counter\n");
        {
            let guard = self.dlp_entities.read().unwrap();
            for ((tenant, entity), counter) in guard.iter() {
                let val = counter.load(Ordering::Relaxed);
                out.push_str(&format!(
                    "cloakd_dlp_masked_entities_total{{tenant=\"{tenant}\",entity=\"{entity}\"}} {val}\n"
                ));
            }
        }
        out.push('\n');

        // 3. Cache Hits
        out.push_str("# HELP cloakd_cache_hits_total Total FinOps prompt cache hits\n");
        out.push_str("# TYPE cloakd_cache_hits_total counter\n");
        {
            let guard = self.cache_hits.read().unwrap();
            for (tenant, counter) in guard.iter() {
                let val = counter.load(Ordering::Relaxed);
                out.push_str(&format!(
                    "cloakd_cache_hits_total{{tenant=\"{tenant}\"}} {val}\n"
                ));
            }
        }
        out.push('\n');

        // 4. Cache Misses
        out.push_str("# HELP cloakd_cache_misses_total Total FinOps prompt cache misses\n");
        out.push_str("# TYPE cloakd_cache_misses_total counter\n");
        {
            let guard = self.cache_misses.read().unwrap();
            for (tenant, counter) in guard.iter() {
                let val = counter.load(Ordering::Relaxed);
                out.push_str(&format!(
                    "cloakd_cache_misses_total{{tenant=\"{tenant}\"}} {val}\n"
                ));
            }
        }
        out.push('\n');

        // 5. Failovers
        out.push_str("# HELP cloakd_upstream_failover_total Total automatic upstream failover cascades executed\n");
        out.push_str("# TYPE cloakd_upstream_failover_total counter\n");
        {
            let guard = self.failovers.read().unwrap();
            for ((from, to), counter) in guard.iter() {
                let val = counter.load(Ordering::Relaxed);
                out.push_str(&format!(
                    "cloakd_upstream_failover_total{{from=\"{from}\",to=\"{to}\"}} {val}\n"
                ));
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new();

        collector.record_request("tenant-1", 200, "HIT");
        collector.record_request("tenant-1", 200, "HIT");
        collector.record_request("tenant-2", 500, "MISS");
        collector.record_masked_entity("tenant-1", "EMAIL", 3);
        collector.record_cache_hit("tenant-1");
        collector.record_cache_miss("tenant-2");
        collector.record_failover("gemini", "openai");

        let rendered = collector.render_prometheus();
        assert!(rendered.contains("cloakd_http_requests_total{tenant=\"tenant-1\",status=\"200\",cache=\"HIT\"} 2"));
        assert!(rendered.contains("cloakd_http_requests_total{tenant=\"tenant-2\",status=\"500\",cache=\"MISS\"} 1"));
        assert!(rendered.contains("cloakd_dlp_masked_entities_total{tenant=\"tenant-1\",entity=\"EMAIL\"} 3"));
        assert!(rendered.contains("cloakd_cache_hits_total{tenant=\"tenant-1\"} 1"));
        assert!(rendered.contains("cloakd_cache_misses_total{tenant=\"tenant-2\"} 1"));
        assert!(rendered.contains("cloakd_upstream_failover_total{from=\"gemini\",to=\"openai\"} 1"));
    }
}
