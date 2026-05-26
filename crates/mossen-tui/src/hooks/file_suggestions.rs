//! File suggestions engine (fileSuggestions.ts).
//!
//! Provides file path suggestions for autocomplete based on fuzzy matching.

/// A file suggestion result.
#[derive(Debug, Clone)]
pub struct FileSuggestion {
    pub path: String,
    pub display: String,
    pub score: f64,
    pub is_directory: bool,
}

/// State for the file suggestion engine.
#[derive(Debug, Clone)]
pub struct FileSuggesterState {
    pub suggestions: Vec<FileSuggestion>,
    pub query: String,
    pub max_results: usize,
    pub all_files: Vec<String>,
    pub selected_index: Option<usize>,
}

impl FileSuggesterState {
    pub fn new(max_results: usize) -> Self {
        Self {
            suggestions: Vec::new(),
            query: String::new(),
            max_results,
            all_files: Vec::new(),
            selected_index: None,
        }
    }

    /// Set the full list of available files.
    pub fn set_files(&mut self, files: Vec<String>) {
        self.all_files = files;
        if !self.query.is_empty() {
            self.recompute();
        }
    }

    /// Update the search query and recompute suggestions.
    pub fn update_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.selected_index = None;
        self.recompute();
    }

    /// Recompute suggestions based on current query.
    fn recompute(&mut self) {
        if self.query.is_empty() {
            self.suggestions.clear();
            return;
        }

        let query_lower = self.query.to_lowercase();
        let mut scored: Vec<(f64, &String)> = self
            .all_files
            .iter()
            .filter_map(|path| {
                let score = fuzzy_score(&query_lower, &path.to_lowercase());
                if score > 0.0 {
                    Some((score, path))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(self.max_results);

        self.suggestions = scored
            .into_iter()
            .map(|(score, path)| {
                let display = path.rsplit('/').next().unwrap_or(path).to_string();
                FileSuggestion {
                    path: path.clone(),
                    display,
                    score,
                    is_directory: path.ends_with('/'),
                }
            })
            .collect();
    }

    /// Select the next suggestion.
    pub fn select_next(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.selected_index = Some(match self.selected_index {
            Some(i) => (i + 1) % self.suggestions.len(),
            None => 0,
        });
    }

    /// Select the previous suggestion.
    pub fn select_prev(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.selected_index = Some(match self.selected_index {
            Some(0) => self.suggestions.len() - 1,
            Some(i) => i - 1,
            None => self.suggestions.len() - 1,
        });
    }

    /// Get the currently selected suggestion.
    pub fn selected(&self) -> Option<&FileSuggestion> {
        self.selected_index.and_then(|i| self.suggestions.get(i))
    }

    /// Clear suggestions.
    pub fn clear(&mut self) {
        self.suggestions.clear();
        self.query.clear();
        self.selected_index = None;
    }
}

impl Default for FileSuggesterState {
    fn default() -> Self {
        Self::new(10)
    }
}

// ============================================================================
// Module-level cache state and helpers translated from fileSuggestions.ts
// ============================================================================

use std::collections::HashSet;
use std::path::MAIN_SEPARATOR;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Maximum suggestions returned by `generate_file_suggestions`.
pub const MAX_SUGGESTIONS: usize = 15;

/// Background-refresh throttle (matches REFRESH_THROTTLE_MS in TS).
const REFRESH_THROTTLE_MS: u64 = 5_000;

/// Wrapper around a `SuggestionItem`-like file suggestion to avoid coupling
/// to the prompt input component's type. Same shape as TS `SuggestionItem`
/// for files (`id`, `displayText`, optional `score`).
#[derive(Debug, Clone)]
pub struct FileSuggestionItem {
    pub id: String,
    pub display_text: String,
    pub score: Option<f64>,
}

/// Subscriber callback fired when a background index build completes.
pub type IndexBuildCompleteCallback = Box<dyn Fn() + Send + Sync>;

/// Internal cache state — mirrors module-level `let` variables in
/// fileSuggestions.ts. All access is mutex-guarded so this is safe to call
/// from multiple async tasks.
struct FileSuggestionCache {
    cache_generation: u64,
    cached_tracked_files: Vec<String>,
    cached_config_files: Vec<String>,
    cached_tracked_dirs: Vec<String>,
    last_refresh_ms: u128,
    last_git_index_mtime: Option<u128>,
    loaded_tracked_signature: Option<String>,
    loaded_merged_signature: Option<String>,
    index_build_subscribers: Vec<IndexBuildCompleteCallback>,
    refresh_in_progress: bool,
}

impl FileSuggestionCache {
    const fn empty() -> Self {
        Self {
            cache_generation: 0,
            cached_tracked_files: Vec::new(),
            cached_config_files: Vec::new(),
            cached_tracked_dirs: Vec::new(),
            last_refresh_ms: 0,
            last_git_index_mtime: None,
            loaded_tracked_signature: None,
            loaded_merged_signature: None,
            index_build_subscribers: Vec::new(),
            refresh_in_progress: false,
        }
    }
}

static FILE_SUGGESTION_CACHE: Mutex<FileSuggestionCache> = Mutex::new(FileSuggestionCache::empty());

/// Subscribe a callback to fire when an in-progress index build completes.
///
/// TS source: `export const onIndexBuildComplete = indexBuildComplete.subscribe`.
pub fn on_index_build_complete(callback: IndexBuildCompleteCallback) {
    if let Ok(mut g) = FILE_SUGGESTION_CACHE.lock() {
        g.index_build_subscribers.push(callback);
    }
}

/// Emit the `indexBuildComplete` signal — fires all subscribed callbacks.
/// Mirrors `indexBuildComplete.emit()` from TS.
pub fn emit_index_build_complete() {
    // Take ownership of subscribers under the lock so callbacks can re-enter
    // the cache without deadlocking.
    let subs: Vec<IndexBuildCompleteCallback> = {
        let Ok(mut g) = FILE_SUGGESTION_CACHE.lock() else {
            return;
        };
        std::mem::take(&mut g.index_build_subscribers)
    };
    for cb in &subs {
        cb();
    }
    // Re-insert subscribers (TS signal is persistent: subscribers keep
    // receiving subsequent emits).
    if let Ok(mut g) = FILE_SUGGESTION_CACHE.lock() {
        g.index_build_subscribers.extend(subs);
    }
}

/// Clear all file suggestion caches. Call this when resuming a session to
/// ensure fresh file discovery.
///
/// TS source: `clearFileSuggestionCaches()`.
pub fn clear_file_suggestion_caches() {
    if let Ok(mut g) = FILE_SUGGESTION_CACHE.lock() {
        g.cache_generation = g.cache_generation.wrapping_add(1);
        g.cached_tracked_files.clear();
        g.cached_config_files.clear();
        g.cached_tracked_dirs.clear();
        g.last_refresh_ms = 0;
        g.last_git_index_mtime = None;
        g.loaded_tracked_signature = None;
        g.loaded_merged_signature = None;
        g.index_build_subscribers.clear();
        g.refresh_in_progress = false;
    }
}

/// Content hash of a path list — a length+stride-sampled FNV-1a variant.
///
/// TS source: `pathListSignature(paths)`. Returns `"<n>:<hex_hash>"`.
pub fn path_list_signature(paths: &[String]) -> String {
    let n = paths.len();
    let stride = std::cmp::max(1, n / 500);
    let mut h: u32 = 0x811c9dc5;
    let mut i = 0;
    while i < n {
        let p = paths[i].as_bytes();
        for &b in p {
            h ^= b as u32;
            h = h.wrapping_mul(0x01000193);
        }
        h = h.wrapping_mul(0x01000193);
        i += stride;
    }
    // Include last path so single-file add/rm at the tail is caught.
    if n > 0 {
        let last = paths[n - 1].as_bytes();
        for &b in last {
            h ^= b as u32;
            h = h.wrapping_mul(0x01000193);
        }
    }
    format!("{}:{:x}", n, h)
}

/// Collect all unique parent directories for a slice of file paths, in the
/// range `start..end`. Mirrors `collectDirectoryNames` from the TS module.
fn collect_directory_names(files: &[String], start: usize, end: usize, out: &mut HashSet<String>) {
    for i in start..end {
        let Some(file) = files.get(i) else { continue };
        let mut current_dir = parent_dir(file);
        // Walk up parent chain. Stop at "." or fixed point.
        loop {
            if current_dir == "." || current_dir.is_empty() {
                break;
            }
            if out.contains(&current_dir) {
                break;
            }
            let parent = parent_dir(&current_dir);
            if parent == current_dir {
                break;
            }
            out.insert(current_dir);
            current_dir = parent;
        }
    }
}

/// Equivalent to Node `path.dirname` for the cases this module uses.
fn parent_dir(p: &str) -> String {
    let trimmed = p.trim_end_matches(MAIN_SEPARATOR);
    let trimmed = if MAIN_SEPARATOR != '/' {
        trimmed.trim_end_matches('/')
    } else {
        trimmed
    };
    let sep_idx = trimmed.rfind(MAIN_SEPARATOR).or_else(|| {
        if MAIN_SEPARATOR != '/' {
            trimmed.rfind('/')
        } else {
            None
        }
    });
    match sep_idx {
        Some(i) if i == 0 => "/".to_string(),
        Some(i) => trimmed[..i].to_string(),
        None => ".".to_string(),
    }
}

/// Collect all parent directories for each file path and return a sorted
/// list of unique directory names with a trailing separator.
///
/// TS source: `getDirectoryNames(files)`.
pub fn get_directory_names(files: &[String]) -> Vec<String> {
    let mut set: HashSet<String> = HashSet::new();
    collect_directory_names(files, 0, files.len(), &mut set);
    let mut v: Vec<String> = set
        .into_iter()
        .map(|d| format!("{}{}", d, MAIN_SEPARATOR))
        .collect();
    v.sort();
    v
}

/// Async-yielding variant of `get_directory_names` — yields every ~256
/// entries to keep the main task responsive on huge file lists.
///
/// TS source: `getDirectoryNamesAsync(files)`.
pub async fn get_directory_names_async(files: &[String]) -> Vec<String> {
    let mut set: HashSet<String> = HashSet::new();
    for i in 0..files.len() {
        collect_directory_names(files, i, i + 1, &mut set);
        if (i & 0xff) == 0xff {
            // Yield to executor periodically.
            tokio_yield().await;
        }
    }
    let mut v: Vec<String> = set
        .into_iter()
        .map(|d| format!("{}{}", d, MAIN_SEPARATOR))
        .collect();
    v.sort();
    v
}

#[cfg(feature = "tokio")]
async fn tokio_yield() {
    tokio::task::yield_now().await;
}

#[cfg(not(feature = "tokio"))]
async fn tokio_yield() {
    // No-op; future is immediately ready.
}

/// Provider trait for path-list sources. The TS module pulls paths from
/// `git ls-files` (fast) or `ripgrep` (fallback), plus Mossen config dirs.
/// We accept a generic provider so callers can plug in whatever discovery
/// strategy fits the environment.
pub trait PathProvider {
    fn list_project_files(&self) -> Vec<String>;
    fn list_config_files(&self) -> Vec<String> {
        Vec::new()
    }
}

/// In-memory provider used in tests and as the no-op fallback. Returns a
/// fixed list.
pub struct StaticPathProvider {
    pub project: Vec<String>,
    pub config: Vec<String>,
}

impl PathProvider for StaticPathProvider {
    fn list_project_files(&self) -> Vec<String> {
        self.project.clone()
    }
    fn list_config_files(&self) -> Vec<String> {
        self.config.clone()
    }
}

/// Rebuild the file index from the given provider, returning the merged
/// path list. Mirrors the body of `getPathsForSuggestions()` in TS, minus
/// the JS-side `FileIndex` (the Rust port uses `FileSuggesterState`
/// directly so we just return the path list).
///
/// TS source: `getPathsForSuggestions()`.
pub async fn get_paths_for_suggestions<P: PathProvider>(provider: &P) -> Vec<String> {
    let project_files = provider.list_project_files();
    let config_files = provider.list_config_files();

    // Update cached config files; mirrors `cachedConfigFiles = configFiles`.
    if let Ok(mut g) = FILE_SUGGESTION_CACHE.lock() {
        g.cached_config_files = config_files.clone();
    }

    let mut all_files = project_files;
    all_files.extend(config_files);

    let directories = get_directory_names_async(&all_files).await;
    if let Ok(mut g) = FILE_SUGGESTION_CACHE.lock() {
        g.cached_tracked_dirs = directories.clone();
    }

    let mut all_paths = directories;
    all_paths.extend(all_files);

    // Skip rebuild when the signature hasn't changed.
    let sig = path_list_signature(&all_paths);
    let should_rebuild = {
        let Ok(mut g) = FILE_SUGGESTION_CACHE.lock() else {
            return all_paths;
        };
        let changed = g.loaded_tracked_signature.as_deref() != Some(sig.as_str());
        if changed {
            g.loaded_tracked_signature = Some(sig.clone());
            g.loaded_merged_signature = None;
        }
        changed
    };
    let _ = should_rebuild; // caller decides whether to push into a search index
    all_paths
}

/// Finds the longest common prefix among an array of suggestion items.
///
/// TS source: `findLongestCommonPrefix(suggestions)`.
pub fn find_longest_common_prefix(suggestions: &[FileSuggestionItem]) -> String {
    if suggestions.is_empty() {
        return String::new();
    }
    let mut prefix = suggestions[0].display_text.clone();
    for s in suggestions.iter().skip(1) {
        prefix = find_common_prefix(&prefix, &s.display_text);
        if prefix.is_empty() {
            return prefix;
        }
    }
    prefix
}

fn find_common_prefix(a: &str, b: &str) -> String {
    let mut out = String::new();
    for (ca, cb) in a.chars().zip(b.chars()) {
        if ca == cb {
            out.push(ca);
        } else {
            break;
        }
    }
    out
}

/// Kick a background cache refresh if not already in progress. Throttled
/// to once per `REFRESH_THROTTLE_MS` unless git state changed.
///
/// TS source: `startBackgroundCacheRefresh()`. The Rust port is
/// synchronous-effect-only: we record bookkeeping (so subsequent calls
/// honor the throttle) but rely on a caller-supplied async runtime for
/// the actual rebuild. Use `get_paths_for_suggestions` when the throttle
/// returns true.
///
/// Returns true if the caller should perform a refresh now.
pub fn start_background_cache_refresh() -> bool {
    let now_ms = current_millis();
    let Ok(mut g) = FILE_SUGGESTION_CACHE.lock() else {
        return false;
    };
    if g.refresh_in_progress {
        return false;
    }
    let has_cache = !g.cached_tracked_files.is_empty() || !g.cached_config_files.is_empty();
    if has_cache {
        let elapsed_ms = now_ms.saturating_sub(g.last_refresh_ms);
        if elapsed_ms < REFRESH_THROTTLE_MS as u128 {
            return false;
        }
    }
    g.refresh_in_progress = true;
    g.last_refresh_ms = now_ms;
    true
}

/// Mark the in-progress refresh as completed. Caller should invoke
/// `emit_index_build_complete()` afterwards to wake subscribed UIs.
pub fn finish_background_cache_refresh() {
    if let Ok(mut g) = FILE_SUGGESTION_CACHE.lock() {
        g.refresh_in_progress = false;
        g.last_refresh_ms = current_millis();
    }
}

fn current_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
}

/// Generate file suggestions for the current input and cursor position.
/// Performs the same special-cases as the TS body: empty path returns
/// nothing (unless `show_on_empty`), `./` returns top-level paths via the
/// provider, otherwise the active `FileSuggesterState` is queried.
///
/// TS source: `generateFileSuggestions(partialPath, showOnEmpty)`.
pub async fn generate_file_suggestions<P: PathProvider>(
    partial_path: &str,
    show_on_empty: bool,
    provider: &P,
    state: &mut FileSuggesterState,
) -> Vec<FileSuggestionItem> {
    if partial_path.is_empty() && !show_on_empty {
        return Vec::new();
    }

    if partial_path.is_empty() || partial_path == "." || partial_path == "./" {
        let top = provider.list_project_files();
        if start_background_cache_refresh() {
            finish_background_cache_refresh();
        }
        return top
            .into_iter()
            .take(MAX_SUGGESTIONS)
            .map(|p| FileSuggestionItem {
                id: format!("file-{}", p),
                display_text: p,
                score: None,
            })
            .collect();
    }

    // Refresh cache opportunistically.
    let was_building = !start_background_cache_refresh();
    let _ = was_building;

    // Normalize ./ prefix.
    let mut normalized = partial_path.to_string();
    let cur_prefix = format!(".{}", MAIN_SEPARATOR);
    if normalized.starts_with(&cur_prefix) {
        normalized = normalized[2..].to_string();
    }

    state.update_query(&normalized);
    let now = Instant::now();
    let mut results: Vec<FileSuggestionItem> = state
        .suggestions
        .iter()
        .map(|s| FileSuggestionItem {
            id: format!("file-{}", s.path),
            display_text: s.path.clone(),
            score: Some(s.score),
        })
        .collect();
    results.truncate(MAX_SUGGESTIONS);
    finish_background_cache_refresh();
    let _ = now.elapsed();
    results
}

/// Apply a file suggestion to the input. Replaces the partial path
/// starting at `start_pos` with `suggestion.display_text`, returning the
/// new input and cursor position.
///
/// TS source: `applyFileSuggestion(...)`.
pub fn apply_file_suggestion(
    suggestion: &FileSuggestionItem,
    input: &str,
    partial_path: &str,
    start_pos: usize,
) -> (String, usize) {
    let suggestion_text = &suggestion.display_text;
    let end_pos = (start_pos + partial_path.len()).min(input.len());
    let start_pos = start_pos.min(input.len());
    let mut out = String::with_capacity(input.len() + suggestion_text.len());
    out.push_str(&input[..start_pos]);
    out.push_str(suggestion_text);
    out.push_str(&input[end_pos..]);
    let new_cursor = start_pos + suggestion_text.len();
    (out, new_cursor)
}

/// Simple fuzzy matching score.
fn fuzzy_score(query: &str, target: &str) -> f64 {
    if query.is_empty() {
        return 0.0;
    }
    if target.contains(query) {
        return 1.0 + (query.len() as f64 / target.len() as f64);
    }
    let mut qi = 0;
    let query_chars: Vec<char> = query.chars().collect();
    let mut score = 0.0;
    let mut consecutive = 0.0;

    for ch in target.chars() {
        if qi < query_chars.len() && ch == query_chars[qi] {
            qi += 1;
            consecutive += 1.0;
            score += consecutive;
        } else {
            consecutive = 0.0;
        }
    }

    if qi == query_chars.len() {
        score / target.len() as f64
    } else {
        0.0
    }
}
