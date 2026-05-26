//! Repository detection — parse git remote URLs and detect the current repository.

use std::collections::HashMap;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use regex::Regex;

/// Parsed repository information.
#[derive(Debug, Clone)]
pub struct ParsedRepository {
    pub host: String,
    pub owner: String,
    pub name: String,
}

/// Cache for repository detection results.
static REPOSITORY_CACHE: Lazy<Mutex<HashMap<String, Option<ParsedRepository>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Clear all repository caches.
pub fn clear_repository_caches() {
    let mut cache = REPOSITORY_CACHE.lock().unwrap();
    cache.clear();
}

/// Detect the current repository (github.com only, returns "owner/name").
pub async fn detect_current_repository(
    cwd: &str,
    get_remote_url: impl std::future::Future<Output = Option<String>>,
) -> Option<String> {
    let result = detect_current_repository_with_host(cwd, get_remote_url).await;
    match result {
        Some(ref parsed) if parsed.host == "github.com" => {
            Some(format!("{}/{}", parsed.owner, parsed.name))
        }
        _ => None,
    }
}

/// Like detect_current_repository, but also returns the host.
pub async fn detect_current_repository_with_host(
    cwd: &str,
    get_remote_url: impl std::future::Future<Output = Option<String>>,
) -> Option<ParsedRepository> {
    {
        let cache = REPOSITORY_CACHE.lock().unwrap();
        if let Some(cached) = cache.get(cwd) {
            return cached.clone();
        }
    }

    let remote_url = get_remote_url.await;
    let parsed = match remote_url {
        Some(ref url) => {
            tracing::debug!("Git remote URL: {}", url);
            parse_git_remote(url)
        }
        None => {
            tracing::debug!("No git remote URL found");
            None
        }
    };

    let mut cache = REPOSITORY_CACHE.lock().unwrap();
    cache.insert(cwd.to_string(), parsed.clone());
    parsed
}

/// Synchronously returns the cached github.com repository for a cwd.
pub fn get_cached_repository(cwd: &str) -> Option<String> {
    let cache = REPOSITORY_CACHE.lock().unwrap();
    match cache.get(cwd) {
        Some(Some(parsed)) if parsed.host == "github.com" => {
            Some(format!("{}/{}", parsed.owner, parsed.name))
        }
        _ => None,
    }
}

/// Parses a git remote URL into host, owner, and name components.
pub fn parse_git_remote(input: &str) -> Option<ParsedRepository> {
    let trimmed = input.trim();

    // SSH format: git@host:owner/repo.git
    static SSH_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^git@([^:]+):([^/]+)/([^/]+?)(?:\.git)?$").unwrap());
    if let Some(caps) = SSH_RE.captures(trimmed) {
        let host = caps.get(1)?.as_str();
        if !looks_like_real_hostname(host) {
            return None;
        }
        return Some(ParsedRepository {
            host: host.to_string(),
            owner: caps.get(2)?.as_str().to_string(),
            name: caps.get(3)?.as_str().to_string(),
        });
    }

    // URL format
    static URL_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^(https?|ssh|git)://(?:[^@]+@)?([^/:]+(?::\d+)?)/([^/]+)/([^/]+?)(?:\.git)?$")
            .unwrap()
    });
    if let Some(caps) = URL_RE.captures(trimmed) {
        let protocol = caps.get(1)?.as_str();
        let host_with_port = caps.get(2)?.as_str();
        let host_without_port = host_with_port.split(':').next().unwrap_or("");
        if !looks_like_real_hostname(host_without_port) {
            return None;
        }
        let host = if protocol == "https" || protocol == "http" {
            host_with_port.to_string()
        } else {
            host_without_port.to_string()
        };
        return Some(ParsedRepository {
            host,
            owner: caps.get(3)?.as_str().to_string(),
            name: caps.get(4)?.as_str().to_string(),
        });
    }

    None
}

/// Parses a git remote URL or "owner/repo" string and returns "owner/repo".
/// Only returns results for github.com hosts.
pub fn parse_github_repository(input: &str) -> Option<String> {
    let trimmed = input.trim();

    // Try parsing as a full remote URL first
    if let Some(parsed) = parse_git_remote(trimmed) {
        if parsed.host != "github.com" {
            return None;
        }
        return Some(format!("{}/{}", parsed.owner, parsed.name));
    }

    // Check if it's already in owner/repo format
    if !trimmed.contains("://") && !trimmed.contains('@') && trimmed.contains('/') {
        let parts: Vec<&str> = trimmed.split('/').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            let repo = parts[1].trim_end_matches(".git");
            return Some(format!("{}/{}", parts[0], repo));
        }
    }

    tracing::debug!("Could not parse repository from: {}", trimmed);
    None
}

/// Checks whether a hostname looks like a real domain name.
fn looks_like_real_hostname(host: &str) -> bool {
    if !host.contains('.') {
        return false;
    }
    let last_segment = host.split('.').next_back().unwrap_or("");
    if last_segment.is_empty() {
        return false;
    }
    // Real TLDs are purely alphabetic
    last_segment.chars().all(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ssh_remote() {
        let parsed = parse_git_remote("git@github.com:owner/repo.git").unwrap();
        assert_eq!(parsed.host, "github.com");
        assert_eq!(parsed.owner, "owner");
        assert_eq!(parsed.name, "repo");
    }

    #[test]
    fn test_parse_https_remote() {
        let parsed = parse_git_remote("https://github.com/owner/repo.git").unwrap();
        assert_eq!(parsed.host, "github.com");
        assert_eq!(parsed.owner, "owner");
        assert_eq!(parsed.name, "repo");
    }

    #[test]
    fn test_parse_github_repository_owner_repo() {
        assert_eq!(
            parse_github_repository("owner/repo"),
            Some("owner/repo".to_string())
        );
    }

    #[test]
    fn test_looks_like_real_hostname() {
        assert!(looks_like_real_hostname("github.com"));
        assert!(!looks_like_real_hostname("github.com-work"));
        assert!(!looks_like_real_hostname("localhost"));
    }
}
