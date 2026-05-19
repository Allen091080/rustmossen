use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use regex::Regex;
use tokio::fs;

use super::schemas::MarketplaceSource;

static SSH_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^([a-zA-Z0-9._-]+@[^:]+:.+?(?:\.git)?)(#(.+))?$").unwrap());

/// Parses a marketplace input string and returns the appropriate marketplace source type.
/// Handles:
/// - Git SSH URLs
/// - HTTP/HTTPS URLs
/// - GitHub shorthand (owner/repo)
/// - Local file paths (.json files)
/// - Local directory paths
pub async fn parse_marketplace_input(input: &str) -> Result<Option<MarketplaceSource>, String> {
    let trimmed = input.trim();

    // Handle git SSH URLs with any valid username
    if let Some(caps) = SSH_RE.captures(trimmed) {
        let url = caps.get(1).unwrap().as_str().to_string();
        let ref_val = caps.get(3).map(|m| m.as_str().to_string());
        return Ok(Some(MarketplaceSource::Git {
            url,
            git_ref: ref_val,
            path: None,
            sparse_paths: None,
        }));
    }

    // Handle URLs
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let fragment_re = Regex::new(r"^([^#]+)(#(.+))?$").unwrap();
        let (url_without_fragment, ref_val) = if let Some(caps) = fragment_re.captures(trimmed) {
            let url = caps.get(1).map(|m| m.as_str()).unwrap_or(trimmed);
            let ref_v = caps.get(3).map(|m| m.as_str().to_string());
            (url.to_string(), ref_v)
        } else {
            (trimmed.to_string(), None)
        };

        // Check if it looks like a git repo
        if url_without_fragment.ends_with(".git") || url_without_fragment.contains("/_git/") {
            return Ok(Some(MarketplaceSource::Git {
                url: url_without_fragment,
                git_ref: ref_val,
                path: None,
                sparse_paths: None,
            }));
        }

        // Parse URL to check hostname
        if let Ok(parsed_url) = url::Url::parse(&url_without_fragment) {
            let hostname = parsed_url.host_str().unwrap_or("");
            if hostname == "github.com" || hostname == "www.github.com" {
                let path_re = Regex::new(r"^/([^/]+/[^/]+?)(/|\.git|$)").unwrap();
                if let Some(caps) = path_re.captures(parsed_url.path()) {
                    if caps.get(1).is_some() {
                        let git_url = if url_without_fragment.ends_with(".git") {
                            url_without_fragment.clone()
                        } else {
                            format!("{}.git", url_without_fragment)
                        };
                        return Ok(Some(MarketplaceSource::Git {
                            url: git_url,
                            git_ref: ref_val,
                            path: None,
                            sparse_paths: None,
                        }));
                    }
                }
            }
        }

        return Ok(Some(MarketplaceSource::Url {
            url: url_without_fragment,
            headers: None,
        }));
    }

    // Handle local paths
    let is_local_path = trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.starts_with('/')
        || trimmed.starts_with('~');

    if is_local_path {
        let resolved_path = if trimmed.starts_with('~') {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
            let rest = trimmed.strip_prefix('~').unwrap_or("");
            home.join(rest.strip_prefix('/').unwrap_or(rest))
        } else {
            PathBuf::from(trimmed)
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(trimmed))
        };

        let resolved_str = resolved_path.to_string_lossy().to_string();

        match fs::metadata(&resolved_path).await {
            Ok(meta) => {
                if meta.is_file() {
                    if resolved_str.ends_with(".json") {
                        return Ok(Some(MarketplaceSource::File {
                            path: resolved_str,
                        }));
                    } else {
                        return Err(format!(
                            "File path must point to a .json file (marketplace.json), but got: {}",
                            resolved_str
                        ));
                    }
                } else if meta.is_dir() {
                    return Ok(Some(MarketplaceSource::Directory {
                        path: resolved_str,
                    }));
                } else {
                    return Err(format!(
                        "Path is neither a file nor a directory: {}",
                        resolved_str
                    ));
                }
            }
            Err(e) => {
                let msg = if e.kind() == std::io::ErrorKind::NotFound {
                    format!("Path does not exist: {}", resolved_str)
                } else {
                    format!("Cannot access path: {} ({})", resolved_str, e)
                };
                return Err(msg);
            }
        }
    }

    // Handle GitHub shorthand (owner/repo, owner/repo#ref, or owner/repo@ref)
    if trimmed.contains('/') && !trimmed.starts_with('@') {
        if trimmed.contains(':') {
            return Ok(None);
        }
        let fragment_re = Regex::new(r"^([^#@]+)(?:[#@](.+))?$").unwrap();
        let (repo, ref_val) = if let Some(caps) = fragment_re.captures(trimmed) {
            let repo = caps.get(1).map(|m| m.as_str()).unwrap_or(trimmed);
            let ref_v = caps.get(2).map(|m| m.as_str().to_string());
            (repo.to_string(), ref_v)
        } else {
            (trimmed.to_string(), None)
        };
        return Ok(Some(MarketplaceSource::GitHub {
            repo,
            git_ref: ref_val,
            path: None,
            sparse_paths: None,
        }));
    }

    // Unrecognized input
    Ok(None)
}
