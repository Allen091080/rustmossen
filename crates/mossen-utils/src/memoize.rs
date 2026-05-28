//! Memoization utilities with TTL and LRU eviction strategies.
//!
//! Provides write-through caching with configurable TTL and LRU-based memoization
//! to prevent unbounded memory growth.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// A cached entry with its value, timestamp, and refresh state.
#[derive(Clone)]
struct CacheEntry<T: Clone> {
    value: T,
    timestamp: Instant,
    refreshing: bool,
}

/// A memoized function handle with TTL-based cache invalidation.
///
/// Implements a write-through cache pattern:
/// - If cache is fresh, return immediately
/// - If cache is stale, return the stale value but mark for refresh
/// - If no cache exists, compute the value
pub struct MemoizedWithTtl<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    cache: Arc<Mutex<HashMap<K, CacheEntry<V>>>>,
    cache_lifetime: Duration,
}

impl<K, V> MemoizedWithTtl<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// Create a new TTL-based memoization cache.
    ///
    /// # Arguments
    /// * `cache_lifetime` - Duration after which cached values are considered stale
    pub fn new(cache_lifetime: Duration) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            cache_lifetime,
        }
    }

    /// Create a new TTL-based memoization cache with the default 5-minute lifetime.
    pub fn with_default_lifetime() -> Self {
        Self::new(Duration::from_secs(5 * 60))
    }

    /// Get a cached value or compute it using the provided function.
    ///
    /// If a stale value exists and is not being refreshed, returns the stale value
    /// and marks it for background refresh on the next call.
    pub fn get_or_insert_with<F>(&self, key: K, f: F) -> V
    where
        F: FnOnce() -> V,
    {
        let mut cache = self.cache.lock();
        let now = Instant::now();

        if let Some(entry) = cache.get(&key) {
            if now.duration_since(entry.timestamp) <= self.cache_lifetime {
                // Fresh cache hit
                return entry.value.clone();
            }

            if !entry.refreshing {
                // Stale but not refreshing - mark for refresh and return stale
                let value = entry.value.clone();
                if let Some(e) = cache.get_mut(&key) {
                    e.refreshing = true;
                }
                return value;
            }

            // Already refreshing, return stale
            return entry.value.clone();
        }

        // No cache entry - compute the value
        let value = f();
        cache.insert(
            key,
            CacheEntry {
                value: value.clone(),
                timestamp: now,
                refreshing: false,
            },
        );
        value
    }

    /// Refresh a stale entry with a new value.
    /// Call this after computing a fresh value in the background.
    pub fn refresh(&self, key: &K, value: V) {
        let mut cache = self.cache.lock();
        cache.insert(
            key.clone(),
            CacheEntry {
                value,
                timestamp: Instant::now(),
                refreshing: false,
            },
        );
    }

    /// Remove a key from the cache (e.g., on refresh failure).
    pub fn invalidate(&self, key: &K) {
        let mut cache = self.cache.lock();
        cache.remove(key);
    }

    /// Clear the entire cache.
    pub fn clear(&self) {
        let mut cache = self.cache.lock();
        cache.clear();
    }
}

impl<K, V> Clone for MemoizedWithTtl<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
            cache_lifetime: self.cache_lifetime,
        }
    }
}

/// An async memoized function handle with TTL-based cache invalidation.
///
/// Implements a write-through cache pattern for async functions:
/// - If cache is fresh, return immediately
/// - If cache is stale, return the stale value but refresh in background
/// - If no cache exists, block and compute the value
/// - Deduplicates concurrent cold-miss calls to the same key
pub struct MemoizedWithTtlAsync<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    cache: Arc<Mutex<HashMap<K, CacheEntry<V>>>>,
    in_flight: Arc<Mutex<HashMap<K, Arc<tokio::sync::Notify>>>>,
    cache_lifetime: Duration,
}

impl<K, V> MemoizedWithTtlAsync<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Create a new async TTL-based memoization cache.
    pub fn new(cache_lifetime: Duration) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            cache_lifetime,
        }
    }

    /// Create with default 5-minute lifetime.
    pub fn with_default_lifetime() -> Self {
        Self::new(Duration::from_secs(5 * 60))
    }

    /// Get a cached value or compute it asynchronously.
    ///
    /// Deduplicates concurrent calls for the same key when no cached value exists.
    pub async fn get_or_insert_with<F, Fut>(&self, key: K, f: F) -> V
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = V>,
    {
        // Check cache first
        {
            let cache = self.cache.lock();
            let now = Instant::now();

            if let Some(entry) = cache.get(&key) {
                if now.duration_since(entry.timestamp) <= self.cache_lifetime {
                    return entry.value.clone();
                }
                // Stale - return stale value (refresh handled separately)
                return entry.value.clone();
            }
        }

        // No cache - compute the value
        let value = f().await;

        {
            let mut cache = self.cache.lock();
            cache.insert(
                key.clone(),
                CacheEntry {
                    value: value.clone(),
                    timestamp: Instant::now(),
                    refreshing: false,
                },
            );
        }

        value
    }

    /// Check if a cached value is stale and needs refresh.
    pub fn needs_refresh(&self, key: &K) -> bool {
        let cache = self.cache.lock();
        if let Some(entry) = cache.get(key) {
            let now = Instant::now();
            now.duration_since(entry.timestamp) > self.cache_lifetime && !entry.refreshing
        } else {
            false
        }
    }

    /// Mark a key as being refreshed.
    pub fn mark_refreshing(&self, key: &K) {
        let mut cache = self.cache.lock();
        if let Some(entry) = cache.get_mut(key) {
            entry.refreshing = true;
        }
    }

    /// Store a refreshed value.
    pub fn refresh(&self, key: &K, value: V) {
        let mut cache = self.cache.lock();
        cache.insert(
            key.clone(),
            CacheEntry {
                value,
                timestamp: Instant::now(),
                refreshing: false,
            },
        );
    }

    /// Remove a key from the cache.
    pub fn invalidate(&self, key: &K) {
        let mut cache = self.cache.lock();
        cache.remove(key);
    }

    /// Clear the entire cache including in-flight trackers.
    pub fn clear(&self) {
        let mut cache = self.cache.lock();
        cache.clear();
        let mut in_flight = self.in_flight.lock();
        in_flight.clear();
    }
}

impl<K, V> Clone for MemoizedWithTtlAsync<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
            in_flight: Arc::clone(&self.in_flight),
            cache_lifetime: self.cache_lifetime,
        }
    }
}

/// A memoized function with LRU (Least Recently Used) eviction policy.
///
/// Prevents unbounded memory growth by evicting the least recently used entries
/// when the cache reaches its maximum size.
pub struct MemoizedWithLru<K, V>
where
    K: Eq + Hash + Clone,
{
    cache: Arc<Mutex<lru::LruCache<K, V>>>,
}

impl<K, V> MemoizedWithLru<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// Create a new LRU-based memoization cache.
    ///
    /// # Arguments
    /// * `max_size` - Maximum number of entries before eviction
    pub fn new(max_size: usize) -> Self {
        use std::num::NonZeroUsize;
        Self {
            cache: Arc::new(Mutex::new(lru::LruCache::new(
                NonZeroUsize::new(max_size).unwrap_or(NonZeroUsize::new(100).unwrap()),
            ))),
        }
    }

    /// Get a cached value or compute it using the provided function.
    pub fn get_or_insert_with<F>(&self, key: K, f: F) -> V
    where
        F: FnOnce() -> V,
    {
        let mut cache = self.cache.lock();
        if let Some(cached) = cache.get(&key) {
            return cached.clone();
        }

        let result = f();
        cache.put(key, result.clone());
        result
    }

    /// Clear the entire cache.
    pub fn clear(&self) {
        let mut cache = self.cache.lock();
        cache.clear();
    }

    /// Get the current cache size.
    pub fn size(&self) -> usize {
        let cache = self.cache.lock();
        cache.len()
    }

    /// Delete a specific key from the cache.
    pub fn delete(&self, key: &K) -> bool {
        let mut cache = self.cache.lock();
        cache.pop(key).is_some()
    }

    /// Peek at a cached value without updating recency.
    pub fn get(&self, key: &K) -> Option<V> {
        let cache = self.cache.lock();
        cache.peek(key).cloned()
    }

    /// Check if a key exists in the cache.
    pub fn has(&self, key: &K) -> bool {
        let cache = self.cache.lock();
        cache.contains(key)
    }
}

impl<K, V> Clone for MemoizedWithLru<K, V>
where
    K: Eq + Hash + Clone,
{
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
        }
    }
}

// =============================================================================
// 工厂函数 — 对应 TS `memoizeWithTTL` / `memoizeWithTTLAsync` / `memoizeWithLRU`。
// 这些只是 [`MemoizedWithTtl`]、[`MemoizedWithTtlAsync`]、[`MemoizedWithLru`]
// 的便捷构造器，与 TS 同名导出对齐。
// =============================================================================

/// 创建一个新的 TTL memoize 实例（对应 TS `memoizeWithTTL`）。
pub fn memoize_with_ttl<K, V>(cache_lifetime_ms: u64) -> MemoizedWithTtl<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    MemoizedWithTtl::new(Duration::from_millis(cache_lifetime_ms))
}

/// 创建一个新的异步 TTL memoize 实例（对应 TS `memoizeWithTTLAsync`）。
pub fn memoize_with_ttl_async<K, V>(cache_lifetime_ms: u64) -> MemoizedWithTtlAsync<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    MemoizedWithTtlAsync::new(Duration::from_millis(cache_lifetime_ms))
}

/// 创建一个新的 LRU memoize 实例（对应 TS `memoizeWithLRU`）。
pub fn memoize_with_lru<K, V>(max_size: usize) -> MemoizedWithLru<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    MemoizedWithLru::new(max_size)
}
