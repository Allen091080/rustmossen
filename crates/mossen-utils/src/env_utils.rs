//! Environment detection utilities.
//!
//! Detects platform, terminal, deployment environment, package managers,
//! runtimes, and other environment characteristics.

use once_cell::sync::Lazy;
use std::path::PathBuf;
use tokio::process::Command;

/// Platform type.
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
    "pycharm", "intellij", "webstorm", "phpstorm", "rubymine", "clion",
    "goland", "rider", "datagrip", "appcode", "dataspell", "aqua",
    "gateway", "fleet", "jetbrains", "androidstudio",
];

/// Get the global mossen config file path.
pub fn get_global_mossen_file(config_home: &str, suffix: &str) -> PathBuf {
    let filename = format!(".mossen{}.json", suffix);
    PathBuf::from(config_home).join(filename)
}

/// Check if a command is available on the system.
async fn is_command_available(command: &str) -> bool {
    which::which(command).is_ok()
}

/// Check internet access by attempting to connect to 1.1.1.1.
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

/// Check if running in WSL environment.
pub fn is_wsl_environment() -> bool {
    std::path::Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists()
}

/// Check if npm is from Windows path (in WSL).
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

/// Check if running in an SSH session.
pub fn is_ssh_session() -> bool {
    std::env::var("SSH_CONNECTION").is_ok()
        || std::env::var("SSH_CLIENT").is_ok()
        || std::env::var("SSH_TTY").is_ok()
}

/// Detect the terminal type.
pub fn detect_terminal() -> Option<String> {
    if std::env::var("CURSOR_TRACE_ID").is_ok() {
        return Some("cursor".to_string());
    }
    if let Ok(val) = std::env::var("VSCODE_GIT_ASKPASS_MAIN") {
        if val.contains("cursor") { return Some("cursor".to_string()); }
        if val.contains("windsurf") { return Some("windsurf".to_string()); }
        if val.contains("antigravity") { return Some("antigravity".to_string()); }
    }

    if let Ok(bundle_id) = std::env::var("__CFBundleIdentifier") {
        let lower = bundle_id.to_lowercase();
        if lower.contains("vscodium") { return Some("codium".to_string()); }
        if lower.contains("windsurf") { return Some("windsurf".to_string()); }
        if lower.contains("com.google.android.studio") { return Some("androidstudio".to_string()); }
        for ide in JETBRAINS_IDES {
            if lower.contains(ide) { return Some(ide.to_string()); }
        }
    }

    if std::env::var("VisualStudioVersion").is_ok() {
        return Some("visualstudio".to_string());
    }

    if std::env::var("TERMINAL_EMULATOR").map(|v| v == "JetBrains-JediTerm").unwrap_or(false) {
        return Some("pycharm".to_string());
    }

    if let Ok(term) = std::env::var("TERM") {
        if term == "xterm-ghostty" { return Some("ghostty".to_string()); }
        if term.contains("kitty") { return Some("kitty".to_string()); }
    }

    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        return Some(term_program);
    }

    if std::env::var("TMUX").is_ok() { return Some("tmux".to_string()); }
    if std::env::var("STY").is_ok() { return Some("screen".to_string()); }

    if std::env::var("KONSOLE_VERSION").is_ok() { return Some("konsole".to_string()); }
    if std::env::var("GNOME_TERMINAL_SERVICE").is_ok() { return Some("gnome-terminal".to_string()); }
    if std::env::var("XTERM_VERSION").is_ok() { return Some("xterm".to_string()); }
    if std::env::var("VTE_VERSION").is_ok() { return Some("vte-based".to_string()); }
    if std::env::var("TERMINATOR_UUID").is_ok() { return Some("terminator".to_string()); }
    if std::env::var("KITTY_WINDOW_ID").is_ok() { return Some("kitty".to_string()); }
    if std::env::var("ALACRITTY_LOG").is_ok() { return Some("alacritty".to_string()); }
    if std::env::var("TILIX_ID").is_ok() { return Some("tilix".to_string()); }

    if std::env::var("WT_SESSION").is_ok() { return Some("windows-terminal".to_string()); }
    if std::env::var("SESSIONNAME").is_ok() && std::env::var("TERM").map(|v| v == "cygwin").unwrap_or(false) {
        return Some("cygwin".to_string());
    }
    if let Ok(msystem) = std::env::var("MSYSTEM") {
        return Some(msystem.to_lowercase());
    }
    if std::env::var("ConEmuANSI").is_ok()
        || std::env::var("ConEmuPID").is_ok()
        || std::env::var("ConEmuTask").is_ok()
    {
        return Some("conemu".to_string());
    }

    if let Ok(distro) = std::env::var("WSL_DISTRO_NAME") {
        return Some(format!("wsl-{}", distro));
    }

    if is_ssh_session() {
        return Some("ssh-session".to_string());
    }

    if let Ok(term) = std::env::var("TERM") {
        if term.contains("alacritty") { return Some("alacritty".to_string()); }
        if term.contains("rxvt") { return Some("rxvt".to_string()); }
        if term.contains("termite") { return Some("termite".to_string()); }
        return Some(term);
    }

    None
}

/// Helper to check if env var is truthy.
fn is_env_truthy_val(key: &str) -> bool {
    std::env::var(key)
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Detect the deployment environment.
pub fn detect_deployment_environment() -> String {
    // Cloud development environments
    if is_env_truthy_val("CODESPACES") { return "codespaces".to_string(); }
    if std::env::var("GITPOD_WORKSPACE_ID").is_ok() { return "gitpod".to_string(); }
    if std::env::var("REPL_ID").is_ok() || std::env::var("REPL_SLUG").is_ok() { return "replit".to_string(); }
    if std::env::var("PROJECT_DOMAIN").is_ok() { return "glitch".to_string(); }

    // Cloud platforms
    if is_env_truthy_val("VERCEL") { return "vercel".to_string(); }
    if std::env::var("RAILWAY_ENVIRONMENT_NAME").is_ok() || std::env::var("RAILWAY_SERVICE_NAME").is_ok() {
        return "railway".to_string();
    }
    if is_env_truthy_val("RENDER") { return "render".to_string(); }
    if is_env_truthy_val("NETLIFY") { return "netlify".to_string(); }
    if std::env::var("DYNO").is_ok() { return "heroku".to_string(); }
    if std::env::var("FLY_APP_NAME").is_ok() || std::env::var("FLY_MACHINE_ID").is_ok() { return "fly.io".to_string(); }
    if is_env_truthy_val("CF_PAGES") { return "cloudflare-pages".to_string(); }
    if std::env::var("DENO_DEPLOYMENT_ID").is_ok() { return "deno-deploy".to_string(); }
    if std::env::var("AWS_LAMBDA_FUNCTION_NAME").is_ok() { return "aws-lambda".to_string(); }
    if std::env::var("AWS_EXECUTION_ENV").map(|v| v == "AWS_ECS_FARGATE").unwrap_or(false) { return "aws-fargate".to_string(); }
    if std::env::var("AWS_EXECUTION_ENV").map(|v| v == "AWS_ECS_EC2").unwrap_or(false) { return "aws-ecs".to_string(); }

    // Check for EC2 via hypervisor UUID
    if let Ok(uuid) = std::fs::read_to_string("/sys/hypervisor/uuid") {
        if uuid.trim().to_lowercase().starts_with("ec2") {
            return "aws-ec2".to_string();
        }
    }

    if std::env::var("K_SERVICE").is_ok() { return "gcp-cloud-run".to_string(); }
    if std::env::var("GOOGLE_CLOUD_PROJECT").is_ok() { return "gcp".to_string(); }
    if std::env::var("WEBSITE_SITE_NAME").is_ok() || std::env::var("WEBSITE_SKU").is_ok() { return "azure-app-service".to_string(); }
    if std::env::var("AZURE_FUNCTIONS_ENVIRONMENT").is_ok() { return "azure-functions".to_string(); }
    if std::env::var("APP_URL").map(|v| v.contains("ondigitalocean.app")).unwrap_or(false) {
        return "digitalocean-app-platform".to_string();
    }
    if std::env::var("SPACE_CREATOR_USER_ID").is_ok() { return "huggingface-spaces".to_string(); }

    // CI/CD platforms
    if is_env_truthy_val("GITHUB_ACTIONS") { return "github-actions".to_string(); }
    if is_env_truthy_val("GITLAB_CI") { return "gitlab-ci".to_string(); }
    if std::env::var("CIRCLECI").is_ok() { return "circleci".to_string(); }
    if std::env::var("BUILDKITE").is_ok() { return "buildkite".to_string(); }
    if is_env_truthy_val("CI") { return "ci".to_string(); }

    // Container orchestration
    if std::env::var("KUBERNETES_SERVICE_HOST").is_ok() { return "kubernetes".to_string(); }
    if std::path::Path::new("/.dockerenv").exists() { return "docker".to_string(); }

    // Platform-specific fallback
    let platform = get_platform();
    match platform {
        Platform::Darwin => "unknown-darwin".to_string(),
        Platform::Linux => "unknown-linux".to_string(),
        Platform::Win32 => "unknown-win32".to_string(),
    }
}

/// Get the current platform.
pub fn get_platform() -> Platform {
    if cfg!(target_os = "windows") {
        Platform::Win32
    } else if cfg!(target_os = "macos") {
        Platform::Darwin
    } else {
        Platform::Linux
    }
}

/// Check if CI environment.
pub fn is_ci() -> bool {
    is_env_truthy_val("CI")
}

/// Environment info struct.
pub struct EnvInfo {
    pub platform: Platform,
    pub arch: &'static str,
    pub terminal: Option<String>,
    pub is_ci: bool,
}

/// Get environment info.
pub fn get_env_info() -> EnvInfo {
    EnvInfo {
        platform: get_platform(),
        arch: std::env::consts::ARCH,
        terminal: detect_terminal(),
        is_ci: is_ci(),
    }
}

/// Returns the host platform for analytics reporting.
pub fn get_host_platform_for_analytics() -> Platform {
    if let Ok(override_val) = std::env::var("MOSSEN_CODE_HOST_PLATFORM") {
        match override_val.as_str() {
            "win32" => return Platform::Win32,
            "darwin" => return Platform::Darwin,
            "linux" => return Platform::Linux,
            _ => {}
        }
    }
    get_platform()
}

/// 对应 TS `hasNodeOption`：检查 NODE_OPTIONS 环境变量是否包含给定选项。
pub fn has_node_option(option: &str) -> bool {
    std::env::var("NODE_OPTIONS")
        .map(|s| s.split_whitespace().any(|p| p == option))
        .unwrap_or(false)
}

/// 对应 TS `shouldMaintainProjectWorkingDir`：判断是否应保持项目工作目录不变。
pub fn should_maintain_project_working_dir() -> bool {
    matches!(
        std::env::var("MOSSEN_KEEP_PROJECT_CWD").as_deref(),
        Ok("1") | Ok("true")
    )
}

/// 对应 TS `isInProtectedNamespace`：路径是否位于受保护命名空间。
pub fn is_in_protected_namespace(path: &str) -> bool {
    let protected = ["/.mossen/", "/.mossen.json", "/.mossenrc"];
    protected.iter().any(|p| path.contains(p))
}

/// 对应 TS `getVertexRegionForModel`：根据模型名返回 Vertex 区域。
pub fn get_vertex_region_for_model(model: &str) -> String {
    let lower = model.to_lowercase();
    if lower.contains("opus") {
        "us-east5".to_string()
    } else if lower.contains("sonnet") {
        "us-central1".to_string()
    } else {
        std::env::var("MOSSEN_VERTEX_REGION").unwrap_or_else(|_| "us-central1".to_string())
    }
}
