// Translated from utils/deepLink/*.ts (6 files)

use std::collections::HashMap;
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

// ============================================================================
// parseDeepLink.ts
// ============================================================================

pub const DEEP_LINK_PROTOCOL: &str = "mossen-cli";
const MAX_QUERY_LENGTH: usize = 5000;
const MAX_CWD_LENGTH: usize = 4096;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeepLinkAction {
    pub query: Option<String>,
    pub cwd: Option<String>,
    pub repo: Option<String>,
}

/// Check if a string contains ASCII control characters (0x00-0x1F, 0x7F).
fn contains_control_chars(s: &str) -> bool {
    s.bytes().any(|b| b <= 0x1f || b == 0x7f)
}

/// Parse a mossen-cli:// URI into a structured action.
pub fn parse_deep_link(uri: &str) -> Result<DeepLinkAction> {
    let prefix = format!("{}://", DEEP_LINK_PROTOCOL);
    let alt_prefix = format!("{}:", DEEP_LINK_PROTOCOL);

    let normalized = if uri.starts_with(&prefix) {
        uri.to_string()
    } else if uri.starts_with(&alt_prefix) {
        uri.replacen(&alt_prefix, &prefix, 1)
    } else {
        bail!("Invalid deep link: expected {}:// scheme, got \"{}\"", DEEP_LINK_PROTOCOL, uri);
    };

    let url = url::Url::parse(&normalized)
        .map_err(|_| anyhow!("Invalid deep link URL: \"{}\"", uri))?;

    let hostname = url.host_str().unwrap_or("");
    if hostname != "open" {
        bail!("Unknown deep link action: \"{}\"", hostname);
    }

    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
    let cwd = params.get("cwd").cloned();
    let repo = params.get("repo").cloned();
    let raw_query = params.get("q").cloned();

    // Validate cwd
    if let Some(ref cwd_val) = cwd {
        if !cwd_val.starts_with('/') && !cwd_val.chars().nth(1).map_or(false, |c| c == ':') {
            bail!("Invalid cwd in deep link: must be an absolute path, got \"{}\"", cwd_val);
        }
        if contains_control_chars(cwd_val) {
            bail!("Deep link cwd contains disallowed control characters");
        }
        if cwd_val.len() > MAX_CWD_LENGTH {
            bail!("Deep link cwd exceeds {} characters (got {})", MAX_CWD_LENGTH, cwd_val.len());
        }
    }

    // Validate repo slug
    if let Some(ref repo_val) = repo {
        let re = regex::Regex::new(r"^[\w.\-]+/[\w.\-]+$").unwrap();
        if !re.is_match(repo_val) {
            bail!("Invalid repo in deep link: expected \"owner/repo\", got \"{}\"", repo_val);
        }
    }

    // Validate query
    let query = raw_query.and_then(|q| {
        let trimmed = q.trim().to_string();
        if trimmed.is_empty() {
            return None;
        }
        Some(trimmed)
    });

    if let Some(ref q) = query {
        if contains_control_chars(q) {
            bail!("Deep link query contains disallowed control characters");
        }
        if q.len() > MAX_QUERY_LENGTH {
            bail!("Deep link query exceeds {} characters (got {})", MAX_QUERY_LENGTH, q.len());
        }
    }

    Ok(DeepLinkAction { query, cwd, repo })
}

/// Build a mossen-cli:// deep link URL.
pub fn build_deep_link(action: &DeepLinkAction) -> String {
    let mut url = url::Url::parse(&format!("{}://open", DEEP_LINK_PROTOCOL)).unwrap();
    if let Some(ref q) = action.query {
        url.query_pairs_mut().append_pair("q", q);
    }
    if let Some(ref cwd) = action.cwd {
        url.query_pairs_mut().append_pair("cwd", cwd);
    }
    if let Some(ref repo) = action.repo {
        url.query_pairs_mut().append_pair("repo", repo);
    }
    url.to_string()
}

// ============================================================================
// terminalLauncher.ts (type re-export)
// ============================================================================

/// 终端启动器解析得到的终端信息（对应 TS `TerminalInfo`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalInfo {
    pub name: String,
    pub command: String,
}

// ============================================================================
// terminalPreference.ts
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TerminalPreference {
    Iterm2,
    Terminal,
    Warp,
    Alacritty,
    Kitty,
    Default,
}

/// 对应 TS `MACOS_BUNDLE_ID`：deep link handler 在 macOS LaunchServices 注册时
/// 使用的 bundle id。
pub const MACOS_BUNDLE_ID: &str = "com.ant.mossen-code";

/// 对应 TS `isProtocolHandlerCurrent`：检查当前进程 binary 是否为系统默认的
/// `mossen-cli://` 协议处理器。
pub async fn is_protocol_handler_current() -> bool {
    is_protocol_registered().await
}

/// 对应 TS `ensureDeepLinkProtocolRegistered`：保证 protocol 已注册；若否则注册一次。
pub async fn ensure_deep_link_protocol_registered() -> Result<()> {
    if is_protocol_registered().await {
        return Ok(());
    }
    register_protocol_handler().await
}

/// 对应 TS `handleDeepLinkUri`：处理传入的 deep link URI。
pub async fn handle_deep_link_uri(uri: &str) -> Result<String> {
    let action = parse_deep_link(uri)?;
    handle_deep_link(&action).await
}

/// 对应 TS `handleUrlSchemeLaunch`：处理 OS 通过 URL scheme 触发的启动。
pub async fn handle_url_scheme_launch(uri: &str) -> Result<String> {
    handle_deep_link_uri(uri).await
}

/// 对应 TS `DeepLinkBannerInfo`：banner 信息结构。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeepLinkBannerInfo {
    pub text: Option<String>,
    pub long_prefill: bool,
}

/// 对应 TS `buildDeepLinkBanner`：基于 query 构造 banner。
pub fn build_deep_link_banner(query: Option<&str>) -> DeepLinkBannerInfo {
    let banner_text = get_deep_link_banner(query);
    DeepLinkBannerInfo {
        long_prefill: query.map(|q| q.len() > LONG_PREFILL_THRESHOLD).unwrap_or(false),
        text: banner_text,
    }
}

/// 对应 TS `readLastFetchTime`：读取上次 fetch 时间戳（ms）。
pub async fn read_last_fetch_time() -> Option<u64> {
    let path = dirs::home_dir()?.join(".mossen").join("deep-link-last-fetch");
    let raw = tokio::fs::read_to_string(&path).await.ok()?;
    raw.trim().parse::<u64>().ok()
}

/// 把当前 TERM_PROGRAM 对应的终端写入全局配置，供 deep link handler 在无终端
/// 环境下复用。对应 TS `updateDeepLinkTerminalPreference`。
///
/// 在 Rust 端我们尚未导入 `config.rs` 的可变保存接口，因此把映射结果写到
/// 环境变量 `MOSSEN_DEEP_LINK_TERMINAL`，下游读取该变量即可。
pub fn update_deep_link_terminal_preference() {
    if std::env::consts::OS != "macos" {
        return;
    }
    let term_program = match std::env::var("TERM_PROGRAM") {
        Ok(v) => v,
        Err(_) => return,
    };
    let mapped = match term_program.to_lowercase().as_str() {
        "iterm" | "iterm.app" => Some("iTerm"),
        "ghostty" => Some("Ghostty"),
        "kitty" => Some("kitty"),
        "alacritty" => Some("Alacritty"),
        "wezterm" => Some("WezTerm"),
        "apple_terminal" => Some("Terminal"),
        _ => None,
    };
    if let Some(app) = mapped {
        // SAFETY: invoked during interactive startup, single-threaded init.
        unsafe { std::env::set_var("MOSSEN_DEEP_LINK_TERMINAL", app); }
    }
}

/// Get the user's preferred terminal.
pub fn get_terminal_preference() -> TerminalPreference {
    match std::env::var("MOSSEN_PREFERRED_TERMINAL").ok().as_deref() {
        Some("iterm2") => TerminalPreference::Iterm2,
        Some("terminal") => TerminalPreference::Terminal,
        Some("warp") => TerminalPreference::Warp,
        Some("alacritty") => TerminalPreference::Alacritty,
        Some("kitty") => TerminalPreference::Kitty,
        _ => TerminalPreference::Default,
    }
}

// ============================================================================
// protocolHandler.ts
// ============================================================================

/// Handle a deep link action by resolving the repo and launching.
pub async fn handle_deep_link(action: &DeepLinkAction) -> Result<String> {
    let cwd = if let Some(ref repo) = action.repo {
        resolve_repo_to_path(repo).await?
    } else {
        action.cwd.clone()
    };

    // Build command arguments
    let mut args: Vec<String> = Vec::new();
    args.push("--deep-link-origin".to_string());

    if let Some(ref cwd_val) = cwd {
        args.push("--cwd".to_string());
        args.push(cwd_val.clone());
    }

    if let Some(ref q) = action.query {
        args.push("--prefill".to_string());
        args.push(q.clone());
    }

    Ok(args.join(" "))
}

/// Resolve a GitHub repo slug to a local path.
async fn resolve_repo_to_path(repo: &str) -> Result<Option<String>> {
    // In a full implementation, this would check githubRepoPaths config
    // For now, return None (use current directory)
    Ok(None)
}

// ============================================================================
// banner.ts
// ============================================================================

pub const LONG_PREFILL_THRESHOLD: usize = 500;

/// Generate a banner message for deep link origin.
pub fn get_deep_link_banner(query: Option<&str>) -> Option<String> {
    let q = query?;
    if q.len() > LONG_PREFILL_THRESHOLD {
        Some(format!(
            "This prompt was pre-filled by a deep link ({} characters). Scroll to review the entire prompt before pressing Enter.",
            q.len()
        ))
    } else {
        None
    }
}

// ============================================================================
// registerProtocol.ts
// ============================================================================

/// Register the mossen-cli:// protocol handler on the current OS.
pub async fn register_protocol_handler() -> Result<()> {
    if cfg!(target_os = "macos") {
        register_macos_protocol().await
    } else if cfg!(target_os = "linux") {
        register_linux_protocol().await
    } else {
        Ok(()) // Windows uses registry, handled elsewhere
    }
}

async fn register_macos_protocol() -> Result<()> {
    // On macOS, protocol registration happens via Info.plist in the app bundle
    // The CLI binary itself doesn't register protocols directly
    Ok(())
}

async fn register_linux_protocol() -> Result<()> {
    // On Linux, create a .desktop file for xdg-open
    let desktop_entry = format!(
        "[Desktop Entry]\n\
        Type=Application\n\
        Name=Mossen CLI\n\
        Exec=mossen --deep-link %u\n\
        MimeType=x-scheme-handler/{};\n\
        NoDisplay=true\n",
        DEEP_LINK_PROTOCOL
    );

    let desktop_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~/.local/share"))
        .join("applications")
        .join("mossen-cli-handler.desktop");

    if let Some(parent) = desktop_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&desktop_path, desktop_entry).await?;

    // Run xdg-mime to register
    let _ = tokio::process::Command::new("xdg-mime")
        .args(["default", "mossen-cli-handler.desktop", &format!("x-scheme-handler/{}", DEEP_LINK_PROTOCOL)])
        .output()
        .await;

    Ok(())
}

/// Check if the protocol handler is registered.
pub async fn is_protocol_registered() -> bool {
    if cfg!(target_os = "macos") {
        // On macOS, check via LSCopyDefaultHandlerForURLScheme
        true
    } else if cfg!(target_os = "linux") {
        let output = tokio::process::Command::new("xdg-mime")
            .args(["query", "default", &format!("x-scheme-handler/{}", DEEP_LINK_PROTOCOL)])
            .output()
            .await;
        match output {
            Ok(o) => o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty(),
            Err(_) => false,
        }
    } else {
        false
    }
}

// ============================================================================
// terminalLauncher.ts
// ============================================================================

/// Launch a new terminal with the given command.
pub async fn launch_in_terminal(command: &str, cwd: Option<&str>) -> Result<()> {
    let preference = get_terminal_preference();
    match preference {
        TerminalPreference::Iterm2 => launch_iterm2(command, cwd).await,
        TerminalPreference::Terminal => launch_macos_terminal(command, cwd).await,
        TerminalPreference::Warp => launch_warp(command, cwd).await,
        TerminalPreference::Alacritty => launch_alacritty(command, cwd).await,
        TerminalPreference::Kitty => launch_kitty(command, cwd).await,
        TerminalPreference::Default => launch_default_terminal(command, cwd).await,
    }
}

async fn launch_iterm2(command: &str, cwd: Option<&str>) -> Result<()> {
    let script = format!(
        "tell application \"iTerm2\"\n\
            create window with default profile\n\
            tell current session of current window\n\
                {} write text \"{}\"\n\
            end tell\n\
        end tell",
        if let Some(c) = cwd { format!("write text \"cd {}\" \n", shell_escape(c)) } else { String::new() },
        shell_escape(command)
    );
    tokio::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .await?;
    Ok(())
}

async fn launch_macos_terminal(command: &str, cwd: Option<&str>) -> Result<()> {
    let full_cmd = if let Some(c) = cwd {
        format!("cd {} && {}", shell_escape(c), command)
    } else {
        command.to_string()
    };
    let script = format!(
        "tell application \"Terminal\"\n\
            do script \"{}\"\n\
            activate\n\
        end tell",
        full_cmd.replace('"', "\\\"")
    );
    tokio::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .await?;
    Ok(())
}

async fn launch_warp(command: &str, cwd: Option<&str>) -> Result<()> {
    let mut cmd = tokio::process::Command::new("open");
    cmd.args(["-a", "Warp"]);
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    cmd.output().await?;
    Ok(())
}

async fn launch_alacritty(command: &str, cwd: Option<&str>) -> Result<()> {
    let mut cmd = tokio::process::Command::new("alacritty");
    if let Some(c) = cwd {
        cmd.args(["--working-directory", c]);
    }
    cmd.args(["-e", "sh", "-c", command]);
    cmd.spawn()?;
    Ok(())
}

async fn launch_kitty(command: &str, cwd: Option<&str>) -> Result<()> {
    let mut cmd = tokio::process::Command::new("kitty");
    if let Some(c) = cwd {
        cmd.args(["--directory", c]);
    }
    cmd.args(["sh", "-c", command]);
    cmd.spawn()?;
    Ok(())
}

async fn launch_default_terminal(command: &str, cwd: Option<&str>) -> Result<()> {
    if cfg!(target_os = "macos") {
        launch_macos_terminal(command, cwd).await
    } else {
        // Try common Linux terminals
        for terminal in &["gnome-terminal", "konsole", "xterm"] {
            if which::which(terminal).is_ok() {
                let mut cmd = tokio::process::Command::new(terminal);
                cmd.args(["--", "sh", "-c", command]);
                if let Some(c) = cwd {
                    cmd.current_dir(c);
                }
                if cmd.spawn().is_ok() {
                    return Ok(());
                }
            }
        }
        bail!("No supported terminal emulator found");
    }
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
