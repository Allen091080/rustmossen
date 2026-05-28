use std::collections::HashMap;
use std::env;
use std::fs;
use std::sync::Mutex;

use once_cell::sync::Lazy;

/// Cached session ingress token state
static SESSION_INGRESS_TOKEN: Lazy<Mutex<Option<Option<String>>>> = Lazy::new(|| Mutex::new(None));

/// Default path for CCR session ingress token
pub const CCR_SESSION_INGRESS_TOKEN_PATH: &str =
    "/home/mossen/.mossen/remote/.session_ingress_token";

fn get_session_ingress_token() -> Option<Option<String>> {
    SESSION_INGRESS_TOKEN.lock().unwrap().clone()
}

fn set_session_ingress_token(token: Option<String>) {
    *SESSION_INGRESS_TOKEN.lock().unwrap() = Some(token);
}

/// Read token from a well-known file path
fn read_token_from_well_known_file(path: &str) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(content) => {
            let trimmed = content.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        Err(_) => None,
    }
}

/// Read token via file descriptor, falling back to well-known file.
/// Uses global state to cache the result since file descriptors can only be read once.
fn get_token_from_file_descriptor() -> Option<String> {
    // Check if we've already attempted to read the token
    if let Some(cached) = get_session_ingress_token() {
        return cached;
    }

    let fd_env = env::var("MOSSEN_CODE_WEBSOCKET_AUTH_FILE_DESCRIPTOR").ok();
    if fd_env.is_none() {
        // No FD env var — try the well-known file
        let path = env::var("MOSSEN_SESSION_INGRESS_TOKEN_FILE")
            .unwrap_or_else(|_| CCR_SESSION_INGRESS_TOKEN_PATH.to_string());
        let from_file = read_token_from_well_known_file(&path);
        set_session_ingress_token(from_file.clone());
        return from_file;
    }

    let fd_str = fd_env.unwrap();
    let fd: i32 = match fd_str.parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!(
                "MOSSEN_CODE_WEBSOCKET_AUTH_FILE_DESCRIPTOR must be a valid file descriptor number, got: {}",
                fd_str
            );
            set_session_ingress_token(None);
            return None;
        }
    };

    // Read from the file descriptor
    let fd_path = if cfg!(target_os = "macos") || cfg!(target_os = "freebsd") {
        format!("/dev/fd/{}", fd)
    } else {
        format!("/proc/self/fd/{}", fd)
    };

    match fs::read_to_string(&fd_path) {
        Ok(content) => {
            let token = content.trim().to_string();
            if token.is_empty() {
                eprintln!("File descriptor contained empty token");
                set_session_ingress_token(None);
                None
            } else {
                set_session_ingress_token(Some(token.clone()));
                // Persist for subprocesses
                let _ = fs::write(CCR_SESSION_INGRESS_TOKEN_PATH, &token);
                Some(token)
            }
        }
        Err(err) => {
            eprintln!("Failed to read token from file descriptor {}: {}", fd, err);
            // FD env var was set but read failed — try the well-known file
            let path = env::var("MOSSEN_SESSION_INGRESS_TOKEN_FILE")
                .unwrap_or_else(|_| CCR_SESSION_INGRESS_TOKEN_PATH.to_string());
            let from_file = read_token_from_well_known_file(&path);
            set_session_ingress_token(from_file.clone());
            from_file
        }
    }
}

/// Get session ingress authentication token.
///
/// Priority order:
///  1. Environment variable (MOSSEN_CODE_SESSION_ACCESS_TOKEN)
///  2. File descriptor (legacy path) — MOSSEN_CODE_WEBSOCKET_AUTH_FILE_DESCRIPTOR
///  3. Well-known file — MOSSEN_SESSION_INGRESS_TOKEN_FILE or default path
pub fn get_session_ingress_auth_token() -> Option<String> {
    // 1. Check environment variable
    if let Ok(env_token) = env::var("MOSSEN_CODE_SESSION_ACCESS_TOKEN") {
        if !env_token.is_empty() {
            return Some(env_token);
        }
    }

    // 2. Check file descriptor (legacy path), with file fallback
    get_token_from_file_descriptor()
}

/// Build auth headers for the current session token.
/// Session keys (sk-mossen-sid) use Cookie auth + X-Organization-Uuid;
/// JWTs use Bearer auth.
pub fn get_session_ingress_auth_headers() -> HashMap<String, String> {
    let token = match get_session_ingress_auth_token() {
        Some(t) => t,
        None => return HashMap::new(),
    };

    if token.starts_with("sk-mossen-sid") {
        let mut headers = HashMap::new();
        headers.insert("Cookie".to_string(), format!("sessionKey={}", token));
        if let Ok(org_uuid) = env::var("MOSSEN_CODE_ORGANIZATION_UUID") {
            headers.insert("X-Organization-Uuid".to_string(), org_uuid);
        }
        headers
    } else {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), format!("Bearer {}", token));
        headers
    }
}

/// Update the session ingress auth token in-process by setting the env var.
pub fn update_session_ingress_auth_token(token: &str) {
    env::set_var("MOSSEN_CODE_SESSION_ACCESS_TOKEN", token);
}
