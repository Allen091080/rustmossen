//! Auth file descriptor credential reader for CCR (Cloud Code Remote).
//!
//! Reads credentials from file descriptors or well-known file paths,
//! with caching and fallback between the two mechanisms.

use std::sync::Mutex;

use once_cell::sync::Lazy;

/// Well-known token file locations in CCR.
const CCR_TOKEN_DIR: &str = "/home/mossen/.mossen/remote";
pub const CCR_AUTH_TOKEN_PATH: &str = "/home/mossen/.mossen/remote/.auth_token";
pub const CCR_API_KEY_PATH: &str = "/home/mossen/.mossen/remote/.api_key";
pub const CCR_SESSION_INGRESS_TOKEN_PATH: &str =
    "/home/mossen/.mossen/remote/.session_ingress_token";

/// Cached credential values.
static OAUTH_TOKEN_FROM_FD: Lazy<Mutex<Option<Option<String>>>> = Lazy::new(|| Mutex::new(None));
static API_KEY_FROM_FD: Lazy<Mutex<Option<Option<String>>>> = Lazy::new(|| Mutex::new(None));

fn is_env_truthy(val: &str) -> bool {
    matches!(val, "1" | "true" | "yes" | "on")
}

/// Best-effort write of the token to a well-known location for subprocess access.
/// CCR-gated: outside CCR there's no /home/mossen/ and no reason to persist.
pub fn maybe_persist_token_for_subprocesses(path: &str, token: &str, token_name: &str) {
    let is_remote = std::env::var("MOSSEN_CODE_REMOTE")
        .map(|v| is_env_truthy(&v))
        .unwrap_or(false);

    if !is_remote {
        return;
    }

    if let Err(e) = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(CCR_TOKEN_DIR)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(CCR_TOKEN_DIR, std::fs::Permissions::from_mode(0o700));
        }
        std::fs::write(path, token)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        tracing::debug!("Persisted {} to {} for subprocess access", token_name, path);
        Ok(())
    })() {
        tracing::error!(
            "Failed to persist {} to disk (non-fatal): {}",
            token_name,
            e
        );
    }
}

/// Fallback read from a well-known file.
/// Returns None if the file doesn't exist (expected outside CCR).
pub fn read_token_from_well_known_file(path: &str, token_name: &str) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let token = content.trim().to_string();
            if token.is_empty() {
                return None;
            }
            tracing::debug!("Read {} from well-known file {}", token_name, path);
            Some(token)
        }
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::debug!("Failed to read {} from {}: {}", token_name, path, e);
            }
            None
        }
    }
}

/// Configuration for a credential read from FD.
struct CredentialConfig {
    env_var: &'static str,
    well_known_path: &'static str,
    label: &'static str,
}

/// Shared FD-or-well-known-file credential reader.
///
/// Priority order:
///  1. File descriptor (legacy path) — env var points at a pipe FD
///  2. Well-known file — written on successful FD read
///
/// Returns None if neither source has a credential. Result is cached.
fn get_credential_from_fd(
    config: &CredentialConfig,
    cache: &Lazy<Mutex<Option<Option<String>>>>,
) -> Option<String> {
    // Check cache first
    {
        let guard = cache.lock().unwrap();
        if let Some(ref cached) = *guard {
            return cached.clone();
        }
    }

    let fd_env = std::env::var(config.env_var).ok();

    if fd_env.is_none() {
        // No FD env var — try well-known file
        let from_file = read_token_from_well_known_file(config.well_known_path, config.label);
        *cache.lock().unwrap() = Some(from_file.clone());
        return from_file;
    }

    let fd_str = fd_env.unwrap();
    let fd: i32 = match fd_str.parse() {
        Ok(f) => f,
        Err(_) => {
            tracing::error!(
                "{} must be a valid file descriptor number, got: {}",
                config.env_var,
                fd_str
            );
            *cache.lock().unwrap() = Some(None);
            return None;
        }
    };

    // Construct the FD path based on platform
    let fd_path = if cfg!(target_os = "macos") || cfg!(target_os = "freebsd") {
        format!("/dev/fd/{fd}")
    } else {
        format!("/proc/self/fd/{fd}")
    };

    match std::fs::read_to_string(&fd_path) {
        Ok(content) => {
            let token = content.trim().to_string();
            if token.is_empty() {
                tracing::error!("File descriptor contained empty {}", config.label);
                *cache.lock().unwrap() = Some(None);
                return None;
            }
            tracing::debug!(
                "Successfully read {} from file descriptor {}",
                config.label,
                fd
            );
            *cache.lock().unwrap() = Some(Some(token.clone()));
            maybe_persist_token_for_subprocesses(config.well_known_path, &token, config.label);
            Some(token)
        }
        Err(e) => {
            tracing::error!(
                "Failed to read {} from file descriptor {}: {}",
                config.label,
                fd,
                e
            );
            // FD read failed — try well-known file
            let from_file = read_token_from_well_known_file(config.well_known_path, config.label);
            *cache.lock().unwrap() = Some(from_file.clone());
            from_file
        }
    }
}

/// Get the CCR-injected hosted adapter token (OAuth).
/// Env var: MOSSEN_CODE_AUTH_TOKEN_FILE_DESCRIPTOR.
/// Well-known file: /home/mossen/.mossen/remote/.auth_token.
pub fn get_oauth_token_from_file_descriptor() -> Option<String> {
    get_credential_from_fd(
        &CredentialConfig {
            env_var: "MOSSEN_CODE_AUTH_TOKEN_FILE_DESCRIPTOR",
            well_known_path: CCR_AUTH_TOKEN_PATH,
            label: "hosted adapter token",
        },
        &OAUTH_TOKEN_FROM_FD,
    )
}

/// Get the CCR-injected API key.
/// Env var: MOSSEN_CODE_API_KEY_FILE_DESCRIPTOR.
/// Well-known file: /home/mossen/.mossen/remote/.api_key.
pub fn get_api_key_from_file_descriptor() -> Option<String> {
    get_credential_from_fd(
        &CredentialConfig {
            env_var: "MOSSEN_CODE_API_KEY_FILE_DESCRIPTOR",
            well_known_path: CCR_API_KEY_PATH,
            label: "API key",
        },
        &API_KEY_FROM_FD,
    )
}
