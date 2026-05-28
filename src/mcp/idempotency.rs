//! AUDIT-12: Idempotency cache for MCP write tools.
//!
//! Harnesses retry tool calls when the network blips or a tool response gets
//! lost. Without deduplication the retry would re-append messages, re-write
//! files, double-update state. This cache lets clients send the same
//! `idempotency_key` on retry and receive the original result rather than
//! triggering the side effect twice.
//!
//! Design:
//! - Keyed by `(namespace, key)` where namespace is typically the tool name.
//! - Bounded to 1000 entries via FIFO eviction (oldest insertion wins).
//! - Entries expire after 24 hours.
//! - Stored result is the full response JSON string the tool would have
//!   returned, so a hit short-circuits side effects entirely.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_ENTRIES: usize = 1000;
const TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone)]
struct Entry {
    result: String,
    inserted_at: Instant,
}

/// Bounded LRU-ish cache keyed by `(namespace, idempotency_key)`.
///
/// Not a strict LRU — we evict by insertion order, not access order, since
/// idempotency keys are expected to be one-shot (write once, retry within
/// minutes). True LRU would add complexity for no practical benefit here.
#[derive(Debug, Default)]
pub struct IdempotencyCache {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    map: HashMap<(String, String), Entry>,
    /// Insertion order for FIFO eviction.
    order: VecDeque<(String, String)>,
}

impl IdempotencyCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Wrap in `Arc` for sharing across tool handlers.
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// Look up a previously stored result. Returns `Some(result)` if a
    /// non-expired entry exists; `None` otherwise. Expired entries are
    /// not removed lazily here — `store` handles eviction.
    pub fn get(&self, namespace: &str, key: &str) -> Option<String> {
        let inner = self.inner.lock().ok()?;
        let entry = inner.map.get(&(namespace.to_string(), key.to_string()))?;
        if entry.inserted_at.elapsed() > TTL {
            return None;
        }
        Some(entry.result.clone())
    }

    /// Store a result against `(namespace, key)`. Evicts the oldest entry if
    /// the cache is at capacity. Also opportunistically drops the head if
    /// it has expired.
    pub fn store(&self, namespace: &str, key: &str, result: String) {
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let composite = (namespace.to_string(), key.to_string());

        // Drop expired head entries before checking capacity.
        while let Some(head) = inner.order.front() {
            let expired = inner
                .map
                .get(head)
                .map(|e| e.inserted_at.elapsed() > TTL)
                .unwrap_or(true);
            if !expired {
                break;
            }
            let head = head.clone();
            inner.order.pop_front();
            inner.map.remove(&head);
        }

        // If the key already exists, refresh it without disturbing order.
        if let std::collections::hash_map::Entry::Occupied(mut e) = inner.map.entry(composite.clone()) {
            e.insert(Entry {
                result,
                inserted_at: Instant::now(),
            });
            return;
        }

        // Evict oldest if at capacity.
        if inner.order.len() >= MAX_ENTRIES {
            if let Some(oldest) = inner.order.pop_front() {
                inner.map.remove(&oldest);
            }
        }

        inner.order.push_back(composite.clone());
        inner.map.insert(
            composite,
            Entry {
                result,
                inserted_at: Instant::now(),
            },
        );
    }

    /// Current number of stored entries (test helper).
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.map.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_12_get_miss_returns_none() {
        let c = IdempotencyCache::new();
        assert!(c.get("append_message", "key1").is_none());
    }

    #[test]
    fn audit_12_store_then_get_returns_value() {
        let c = IdempotencyCache::new();
        c.store("append_message", "key1", "result A".to_string());
        assert_eq!(c.get("append_message", "key1").as_deref(), Some("result A"));
    }

    #[test]
    fn audit_12_namespace_isolated() {
        let c = IdempotencyCache::new();
        c.store("tool_a", "shared_key", "result A".to_string());
        c.store("tool_b", "shared_key", "result B".to_string());
        assert_eq!(c.get("tool_a", "shared_key").as_deref(), Some("result A"));
        assert_eq!(c.get("tool_b", "shared_key").as_deref(), Some("result B"));
    }

    #[test]
    fn audit_12_store_refresh_does_not_duplicate_order() {
        // Storing same key twice should update the value without growing the
        // entry count.
        let c = IdempotencyCache::new();
        c.store("ns", "key", "v1".to_string());
        c.store("ns", "key", "v2".to_string());
        assert_eq!(c.len(), 1);
        assert_eq!(c.get("ns", "key").as_deref(), Some("v2"));
    }

    #[test]
    fn audit_12_eviction_at_capacity() {
        let c = IdempotencyCache::new();
        // Fill to capacity + a few extras
        for i in 0..(MAX_ENTRIES + 5) {
            c.store("ns", &format!("k{}", i), format!("v{}", i));
        }
        assert_eq!(c.len(), MAX_ENTRIES);
        // Oldest entries should be evicted
        assert!(c.get("ns", "k0").is_none());
        assert!(c.get("ns", "k4").is_none());
        // Newest still present
        assert!(c.get("ns", &format!("k{}", MAX_ENTRIES + 4)).is_some());
    }

    #[test]
    fn audit_12_empty_namespace_or_key_works() {
        // Edge case: empty strings are valid keys.
        let c = IdempotencyCache::new();
        c.store("", "", "result".to_string());
        assert_eq!(c.get("", "").as_deref(), Some("result"));
    }
}
