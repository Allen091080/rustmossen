//! OAuth redirect port helpers — extracted from auth to break circular dependencies.

use std::env;
use std::net::TcpListener;

use rand::Rng;

/// Port range configuration by platform.
struct PortRange {
    min: u16,
    max: u16,
}

/// Get the redirect port range based on platform.
fn get_redirect_port_range() -> PortRange {
    if cfg!(target_os = "windows") {
        PortRange { min: 39152, max: 49151 }
    } else {
        PortRange { min: 49152, max: 65535 }
    }
}

/// Fallback port when random selection fails.
const REDIRECT_PORT_FALLBACK: u16 = 3118;

/// Builds a redirect URI on localhost with the given port and a fixed `/callback` path.
///
/// RFC 8252 Section 7.3 (OAuth for Native Apps): loopback redirect URIs match any
/// port as long as the path matches.
pub fn build_redirect_uri(port: u16) -> String {
    format!("http://localhost:{}/callback", port)
}

/// Get the configured MCP OAuth callback port from environment.
fn get_mcp_oauth_callback_port() -> Option<u16> {
    env::var("MCP_OAUTH_CALLBACK_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|&p| p > 0)
}

/// Check if a port is available by trying to bind to it.
fn is_port_available(port: u16) -> bool {
    TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok()
}

/// Finds an available port in the specified range for OAuth redirect.
/// Uses random selection for better security.
pub async fn find_available_port() -> anyhow::Result<u16> {
    // First, try the configured port if specified
    if let Some(configured_port) = get_mcp_oauth_callback_port() {
        return Ok(configured_port);
    }

    let range = get_redirect_port_range();
    let port_range = range.max - range.min + 1;
    let max_attempts = std::cmp::min(port_range as usize, 100);
    let mut rng = rand::thread_rng();

    for _ in 0..max_attempts {
        let port = range.min + (rng.gen::<u16>() % port_range);
        if is_port_available(port) {
            return Ok(port);
        }
    }

    // If random selection failed, try the fallback port
    if is_port_available(REDIRECT_PORT_FALLBACK) {
        return Ok(REDIRECT_PORT_FALLBACK);
    }

    Err(anyhow::anyhow!("No available ports for OAuth redirect"))
}
