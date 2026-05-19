use std::collections::linked_list::LinkedList;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// File state entry in the cache
#[derive(Debug, Clone)]
pub struct FileState {
    pub content: String,
    pub timestamp: u64,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
    /// True when this entry was populated by auto-injection and the injected
    /// content did not match disk. The model has only seen a partial view.
    pub is_partial_view: bool,
}

/// Default max entries for read file state caches
pub const READ_FILE_STATE_CACHE_SIZE: usize = 100;

/// Default size limit for file state caches (25MB)
const DEFAULT_MAX_CACHE_SIZE_BYTES: usize = 25 * 1024 * 1024;

/// A file state cache that normalizes all path keys before access.
/// Uses LRU eviction based on both entry count and total byte size.
pub struct FileStateCache {
    entries: LinkedList<(String, FileState)>,
    index: HashMap<String, usize>,
    max_entries: usize,
    max_size_bytes: usize,
    current_size_bytes: usize,
}

impl FileStateCache {
    pub fn new(max_entries: usize, max_size_bytes: usize) -> Self {
        Self {
            entries: LinkedList::new(),
            index: HashMap::new(),
            max_entries,
            max_size_bytes,
            current_size_bytes: 0,
        }
    }

    fn normalize_key(key: &str) -> String {
        let path = Path::new(key);
        match path.canonicalize() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => {
                // Fallback: just normalize the path components
                let normalized: PathBuf = path.components().collect();
                normalized.to_string_lossy().to_string()
            }
        }
    }

    fn entry_size(state: &FileState) -> usize {
        std::cmp::max(1, state.content.len())
    }

    fn evict_if_needed(&mut self) {
        while self.entries.len() > self.max_entries || self.current_size_bytes > self.max_size_bytes
        {
            if let Some((key, state)) = self.entries.pop_back() {
                self.current_size_bytes -= Self::entry_size(&state);
                self.index.remove(&key);
            } else {
                break;
            }
        }
        self.rebuild_index();
    }

    fn rebuild_index(&mut self) {
        self.index.clear();
        for (i, (key, _)) in self.entries.iter().enumerate() {
            self.index.insert(key.clone(), i);
        }
    }

    pub fn get(&self, key: &str) -> Option<&FileState> {
        let normalized = Self::normalize_key(key);
        // Linear search since LinkedList doesn't support index access
        for (k, v) in self.entries.iter() {
            if *k == normalized {
                return Some(v);
            }
        }
        None
    }

    pub fn set(&mut self, key: &str, value: FileState) {
        let normalized = Self::normalize_key(key);

        // Remove existing entry if present
        self.delete(&normalized);

        let size = Self::entry_size(&value);
        self.current_size_bytes += size;
        self.entries.push_front((normalized.clone(), value));
        self.rebuild_index();
        self.evict_if_needed();
    }

    pub fn has(&self, key: &str) -> bool {
        let normalized = Self::normalize_key(key);
        self.index.contains_key(&normalized)
    }

    pub fn delete(&mut self, key: &str) -> bool {
        let normalized = Self::normalize_key(key);
        let mut new_list = LinkedList::new();
        let mut found = false;
        while let Some((k, v)) = self.entries.pop_front() {
            if k == normalized && !found {
                self.current_size_bytes -= Self::entry_size(&v);
                found = true;
            } else {
                new_list.push_back((k, v));
            }
        }
        self.entries = new_list;
        if found {
            self.rebuild_index();
        }
        found
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.index.clear();
        self.current_size_bytes = 0;
    }

    pub fn size(&self) -> usize {
        self.entries.len()
    }

    pub fn max(&self) -> usize {
        self.max_entries
    }

    pub fn max_size(&self) -> usize {
        self.max_size_bytes
    }

    pub fn calculated_size(&self) -> usize {
        self.current_size_bytes
    }

    pub fn keys(&self) -> Vec<String> {
        self.entries.iter().map(|(k, _)| k.clone()).collect()
    }

    pub fn entries(&self) -> Vec<(String, FileState)> {
        self.entries.iter().cloned().collect()
    }
}

/// Factory function to create a size-limited FileStateCache
pub fn create_file_state_cache_with_size_limit(
    max_entries: usize,
    max_size_bytes: Option<usize>,
) -> FileStateCache {
    FileStateCache::new(max_entries, max_size_bytes.unwrap_or(DEFAULT_MAX_CACHE_SIZE_BYTES))
}

/// Convert cache to a HashMap (used by compact)
pub fn cache_to_object(cache: &FileStateCache) -> HashMap<String, FileState> {
    cache.entries().into_iter().collect()
}

/// Get all keys from cache
pub fn cache_keys(cache: &FileStateCache) -> Vec<String> {
    cache.keys()
}

/// Clone a FileStateCache preserving size limit configuration
pub fn clone_file_state_cache(cache: &FileStateCache) -> FileStateCache {
    let mut cloned = create_file_state_cache_with_size_limit(
        cache.max(),
        Some(cache.max_size()),
    );
    for (key, state) in cache.entries().into_iter().rev() {
        cloned.set(&key, state);
    }
    cloned
}

/// Merge two file state caches, with more recent entries (by timestamp) overriding older ones
pub fn merge_file_state_caches(
    first: &FileStateCache,
    second: &FileStateCache,
) -> FileStateCache {
    let mut merged = clone_file_state_cache(first);
    for (file_path, file_state) in second.entries() {
        let existing = merged.get(&file_path);
        if existing.is_none() || file_state.timestamp > existing.unwrap().timestamp {
            merged.set(&file_path, file_state);
        }
    }
    merged
}
