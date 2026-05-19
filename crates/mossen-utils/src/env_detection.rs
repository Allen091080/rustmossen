//! Environment detection utilities.
//!
//! Detects terminal type, deployment platform, WSL environment,
//! package managers, runtimes, and other environment properties.

use once_cell::sync::Lazy;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::OnceCell;

/// Supported platform types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Win32,
    Darwin,
    Linux,
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Win32 => "win32",
            Platform::Darwin => "darwin",
            Platform::Linux => "linux",
        }
    }
}

/// JetBrains IDE identifiers.
pub const JETBRAINS_IDES: &[&str] = &[
    "pycharm",
    "intellij",
    "webstorm",
    "phpstorm",
    "rubymine",
    "clion",
    "goland",
    "rider",
    "datagrip",
    "appcode",
    "dataspell",
    "aqua",
    "gateway",
    "fleet",
    "jetbrains",
    "androidstudio",
];

/// Returns the global mossen config file path.
pub fn get_global_mossen_file(config_home_dir: &str, file_suffix: &str) -> String {
    let filename = format!(".mossen{}.json", file_suffix);
    let path = Path::new(config_home_dir).join(&filename);
    path.to_string_lossy().to_string()
}

/// Check if an environment variable is truthy ("1", "true", "yes").
fn is_env_truthy(val: Option<&str>) -> bool {
    match val {
        Some(v) => matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"),
        None => false,
    }
}

/// Check if a command is available on the system PATH.
async fn is_command_available(command: &str) -> bool {
    which::which(command).is_ok()
}

/// Detect available package managers.
pub async fn detect_package_managers() -> Vec<String> {
    let mut managers = Vec::new();
    if is_command_available("npm").await {
        managers.push("npm".to_string());
    }
    if is_command_available("yarn").await {
        managers.push("yarn".to_string());
    }
    if is_command_available("pnpm").await {
        managers.push("pnpm".to_string());
    }
    managers
}

/// Detect available runtimes.
pub async fn detect_runtimes() -> Vec<String> {
    let mut runtimes = Vec::new();
    if is_command_available("bun").await {
        runtimes.push("bun".to_string());
    }
    if is_command_available("deno").await {
        runtimes.push("deno".to_string());
    }
    if is_command_available("node").await {
        runtimes.push("node".to_string());
    }
    runtimes
}

/// Check if running in a WSL environment.
pub fn is_wsl_environment() -> bool {
    Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists()
}

/// Check if npm executable is from Windows path in WSL.
pub fn is_npm_from_windows_path() -> bool {
    if !is_wsl_environment() {
        return false;
    }
    match which::which("npm") {
        Ok(path) => path.to_string_lossy().starts_with("/mnt/c/"),
        Err(_) => false,
    }
}

/// Check if running via Conductor.
pub fn is_conductor() -> bool {
    std::env::var("__CFBundleIdentifier")
        .map(|v| v == "com.conductor.app")
        .unwrap_or(false)
}

/// Check if this is an SSH session.
pub fn is_ssh_session() -> bool {
    std::env::var("SSH_CONNECTION").is_ok()
        || std::env::var("SSH_CLIENT").is_ok()
        || std::env::var("SSH_TTY").is_ok()
}

/// Check internet access by attempting a HEAD request.
pub async fn has_internet_access() -> bool {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    client
        .head("http://1.1.1.1")
        .send()
        .await
        .is_ok()
}

/// Detect terminal type from environment variables.
pub fn detect_terminal() -> Option<String> {
    let env_var = |name: &str| std::env::var(name).ok();

    if env_var("CURSOR_TRACE_ID").is_some() {
        return Some("cursor".to_string());
    }
    if let Some(askpass) = env_var("VSCODE_GIT_ASKPASS_MAIN") {
        if askpass.contains("cursor") {
            return Some("cursor".to_string());
        }
        if askpass.contains("windsurf") {
            return Some("windsurf".to_string());
        }
        if askpass.contains("antigravity") {
            return Some("antigravity".to_string());
        }
    }

    let bundle_id = env_var("__CFBundleIdentifier").map(|s| s.to_lowercase());
    if let Some(ref bid) = bundle_id {
        if bid.contains("vscodium") {
            return Some("codium".to_string());
        }
        if bid.contains("windsurf") {
            return Some("windsurf".to_string());
        }
        if bid.contains("com.google.android.studio") {
            return Some("androidstudio".to_string());
        }
        for ide in JETBRAINS_IDES {
            if bid.contains(ide) {
                return Some(ide.to_string());
            }
        }
    }

    if env_var("VisualStudioVersion").is_some() {
        return Some("visualstudio".to_string());
    }

    if env_var("TERMINAL_EMULATOR").as_deref() == Some("JetBrains-JediTerm") {
        return Some("pycharm".to_string());
    }

    if env_var("TERM").as_deref() == Some("xterm-ghostty") {
        return Some("ghostty".to_string());
    }
    if let Some(ref term) = env_var("TERM") {
        if term.contains("kitty") {
            return Some("kitty".to_string());
        }
    }

    if let Some(term_prog) = env_var("TERM_PROGRAM") {
        return Some(term_prog);
    }

    if env_var("TMUX").is_some() {
        return Some("tmux".to_string());
    }
    if env_var("STY").is_some() {
        return Some("screen".to_string());
    }

    if env_var("KONSOLE_VERSION").is_some() {
        return Some("konsole".to_string());
    }
    if env_var("GNOME_TERMINAL_SERVICE").is_some() {
        return Some("gnome-terminal".to_string());
    }
    if env_var("XTERM_VERSION").is_some() {
        return Some("xterm".to_string());
    }
    if env_var("VTE_VERSION").is_some() {
        return Some("vte-based".to_string());
    }
    if env_var("TERMINATOR_UUID").is_some() {
        return Some("terminator".to_string());
    }
    if env_var("KITTY_WINDOW_ID").is_some() {
        return Some("kitty".to_string());
    }
    if env_var("ALACRITTY_LOG").is_some() {
        return Some("alacritty".to_string());
    }
    if env_var("TILIX_ID").is_some() {
        return Some("tilix".to_string());
    }

    // Windows-specific
    if env_var("WT_SESSION").is_some() {
        return Some("windows-terminal".to_string());
    }
    if env_var("SESSIONNAME").is_some() && env_var("TERM").as_deref() == Some("cygwin") {
        return Some("cygwin".to_string());
    }
    if let Some(msystem) = env_var("MSYSTEM") {
        return Some(msystem.to_lowercase());
    }
    if env_var("ConEmuANSI").is_some()
        || env_var("ConEmuPID").is_some()
        || env_var("ConEmuTask").is_some()
    {
        return Some("conemu".to_string());
    }

    // WSL detection
    if let Some(distro) = env_var("WSL_DISTRO_NAME") {
        return Some(format!("wsl-{}", distro));
    }

    // SSH session
    if is_ssh_session() {
        return Some("ssh-session".to_string());
    }

    // Fall back to TERM
    if let Some(term) = env_var("TERM") {
        if term.contains("alacritty") {
            return Some("alacritty".to_string());
        }
        if term.contains("rxvt") {
            return Some("rxvt".to_string());
        }
        if term.contains("termite") {
            return Some("termite".to_string());
        }
        return Some(term);
    }

    None
}

/// Detect the deployment environment/platform.
pub fn detect_deployment_environment() -> String {
    let env_var = |name: &str| std::env::var(name).ok();
    let env_truthy = |name: &str| is_env_truthy(std::env::var(name).ok().as_deref());

    // Cloud development environments
    if env_truthy("CODESPACES") {
        return "codespaces".to_string();
    }
    if env_var("GITPOD_WORKSPACE_ID").is_some() {
        return "gitpod".to_string();
    }
    if env_var("REPL_ID").is_some() || env_var("REPL_SLUG").is_some() {
        return "replit".to_string();
    }
    if env_var("PROJECT_DOMAIN").is_some() {
        return "glitch".to_string();
    }

    // Cloud platforms
    if env_truthy("VERCEL") {
        return "vercel".to_string();
    }
    if env_var("RAILWAY_ENVIRONMENT_NAME").is_some()
        || env_var("RAILWAY_SERVICE_NAME").is_some()
    {
        return "railway".to_string();
    }
    if env_truthy("RENDER") {
        return "render".to_string();
    }
    if env_truthy("NETLIFY") {
        return "netlify".to_string();
    }
    if env_var("DYNO").is_some() {
        return "heroku".to_string();
    }
    if env_var("FLY_APP_NAME").is_some() || env_var("FLY_MACHINE_ID").is_some() {
        return "fly.io".to_string();
    }
    if env_truthy("CF_PAGES") {
        return "cloudflare-pages".to_string();
    }
    if env_var("DENO_DEPLOYMENT_ID").is_some() {
        return "deno-deploy".to_string();
    }
    if env_var("AWS_LAMBDA_FUNCTION_NAME").is_some() {
        return "aws-lambda".to_string();
    }
    if env_var("AWS_EXECUTION_ENV").as_deref() == Some("AWS_ECS_FARGATE") {
        return "aws-fargate".to_string();
    }
    if env_var("AWS_EXECUTION_ENV").as_deref() == Some("AWS_ECS_EC2") {
        return "aws-ecs".to_string();
    }
    // Check for EC2 via hypervisor UUID
    if let Ok(uuid) = std::fs::read_to_string("/sys/hypervisor/uuid") {
        if uuid.trim().to_lowercase().starts_with("ec2") {
            return "aws-ec2".to_string();
        }
    }
    if env_var("K_SERVICE").is_some() {
        return "gcp-cloud-run".to_string();
    }
    if env_var("GOOGLE_CLOUD_PROJECT").is_some() {
        return "gcp".to_string();
    }
    if env_var("WEBSITE_SITE_NAME").is_some() || env_var("WEBSITE_SKU").is_some() {
        return "azure-app-service".to_string();
    }
    if env_var("AZURE_FUNCTIONS_ENVIRONMENT").is_some() {
        return "azure-functions".to_string();
    }
    if let Some(app_url) = env_var("APP_URL") {
        if app_url.contains("ondigitalocean.app") {
            return "digitalocean-app-platform".to_string();
        }
    }
    if env_var("SPACE_CREATOR_USER_ID").is_some() {
        return "huggingface-spaces".to_string();
    }

    // CI/CD platforms
    if env_truthy("GITHUB_ACTIONS") {
        return "github-actions".to_string();
    }
    if env_truthy("GITLAB_CI") {
        return "gitlab-ci".to_string();
    }
    if env_var("CIRCLECI").is_some() {
        return "circleci".to_string();
    }
    if env_var("BUILDKITE").is_some() {
        return "buildkite".to_string();
    }
    if env_truthy("CI") {
        return "ci".to_string();
    }

    // Container orchestration
    if env_var("KUBERNETES_SERVICE_HOST").is_some() {
        return "kubernetes".to_string();
    }
    if Path::new("/.dockerenv").exists() {
        return "docker".to_string();
    }

    // Platform-specific fallback
    let platform = current_platform();
    match platform {
        Platform::Darwin => "unknown-darwin".to_string(),
        Platform::Linux => "unknown-linux".to_string(),
        Platform::Win32 => "unknown-win32".to_string(),
    }
}

/// Returns the current platform.
pub fn current_platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::Darwin
    } else if cfg!(target_os = "windows") {
        Platform::Win32
    } else {
        Platform::Linux
    }
}

/// Returns the host platform for analytics reporting.
/// If MOSSEN_CODE_HOST_PLATFORM is set to a valid platform value, that overrides
/// the detected platform.
pub fn get_host_platform_for_analytics() -> Platform {
    if let Ok(override_val) = std::env::var("MOSSEN_CODE_HOST_PLATFORM") {
        match override_val.as_str() {
            "win32" => return Platform::Win32,
            "darwin" => return Platform::Darwin,
            "linux" => return Platform::Linux,
            _ => {}
        }
    }
    current_platform()
}

/// Environment info struct similar to the TS `env` export.
pub struct EnvInfo {
    pub is_ci: bool,
    pub platform: Platform,
    pub arch: &'static str,
    pub terminal: Option<String>,
}

/// Get static environment info.
pub fn get_env_info() -> EnvInfo {
    let is_ci = is_env_truthy(std::env::var("CI").ok().as_deref());
    let platform = current_platform();
    let arch = std::env::consts::ARCH;
    let terminal = detect_terminal();

    EnvInfo {
        is_ci,
        platform,
        arch,
        terminal,
    }
}
