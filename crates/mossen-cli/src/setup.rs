//! 初始化序列 — 对应 TS 的 setup.ts + entrypoints/init.ts。
//!
//! 执行启动时的配置加载、日志初始化、认证检查等步骤。

use anyhow::{Context, Result};
use std::path::Path;
use tracing::{info, warn};

use crate::bootstrap::SharedBootstrapState;

/// 配置环境 — 对应 TS 的 enableConfigs() + applyConfigEnvironmentVariables()。
///
/// 加载 .mossensrc 配置、环境变量和 feature flags。
pub async fn configure_environment() -> Result<()> {
    info!("configure_environment: loading .env files");

    // 加载 .mossensrc/feature-flags.env（如果存在）
    let env_path = Path::new(".mossensrc/feature-flags.env");
    if env_path.exists() {
        dotenvy::from_path(env_path).ok();
        info!("configure_environment: loaded feature-flags.env");
    }

    // 加载项目 .env（如果存在）
    dotenvy::dotenv().ok();

    info!("configure_environment: environment configured");
    Ok(())
}

/// 初始化日志系统 — 对应 TS 的 tracing 初始化。
///
/// `file_sink`: when true (TUI or terminal frontend launch path), tracing logs
/// go to a per-pid file under `~/.cache/mossen/logs/` instead of stderr.
/// Writing to stderr while ratatui owns the alternate screen corrupts the
/// display, and writing diagnostics into `--emit terminal` pollutes the
/// rendered UI. Other non-interactive paths keep the stderr destination so log
/// lines remain visible inline.
pub fn initialize_logging(verbose: bool, file_sink: bool, announce_file_sink: bool) {
    use tracing_subscriber::EnvFilter;

    let filter = if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::from_default_env().add_directive("mossen=info".parse().unwrap())
    };

    if file_sink {
        // Per-pid log file so concurrent mossen processes don't clobber
        // each other. The directory is best-effort: if creation fails we
        // fall back to a stderr writer to preserve a debug trail.
        let log_dir = dirs::cache_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("mossen")
            .join("logs");
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path = log_dir.join(format!("mossen-{}.log", std::process::id()));
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            Ok(file) => {
                let writer = std::sync::Mutex::new(file);
                tracing_subscriber::fmt()
                    .with_env_filter(filter)
                    .with_target(true)
                    .with_thread_ids(false)
                    .with_file(false)
                    .with_line_number(false)
                    .with_ansi(false)
                    .with_writer(writer)
                    .init();
                if announce_file_sink {
                    eprintln!("mossen: TUI logs → {}", log_path.display());
                }
                info!("logging initialized (file sink: {})", log_path.display());
                return;
            }
            Err(e) => {
                eprintln!(
                    "mossen: failed to open log file at {} ({}); falling back to stderr",
                    log_path.display(),
                    e
                );
            }
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    info!("logging initialized");
}

/// 完整初始化序列 — 对应 TS 的 init() + setup()。
///
/// 执行顺序：
/// 1. 加载配置
/// 2. 应用环境变量
/// 3. 设置 graceful shutdown
/// 4. 初始化网络（代理、TLS）
/// 5. 检测平台信息
pub async fn run_init_sequence(state: &SharedBootstrapState) -> Result<()> {
    let _span = tracing::info_span!("init_sequence").entered();
    info!("init_sequence: starting");

    // 1. 加载环境配置
    configure_environment().await?;
    info!("init_sequence: environment configured");

    // 2. 读取启动状态并设置
    {
        let state = state
            .read()
            .map_err(|e| anyhow::anyhow!("failed to read state: {}", e))?;
        info!(
            session_id = %state.session_id,
            cwd = %state.cwd.display(),
            "init_sequence: session initialized"
        );
    }

    // 3. 检测平台信息
    detect_platform().await;

    info!("init_sequence: completed");
    Ok(())
}

/// 执行完整 setup 流程 — 对应 TS 的 setup()。
///
/// 在 init 之后、REPL 启动之前执行：
/// - 捕获 hooks 配置快照
/// - 初始化文件变更监听
/// - 处理 worktree 创建
/// - 启动后台预取任务
pub async fn run_setup(state: &SharedBootstrapState, bare_mode: bool) -> Result<()> {
    let _span = tracing::info_span!("setup").entered();
    info!("setup: starting");

    // 设置 bare mode
    {
        let mut state = state
            .write()
            .map_err(|e| anyhow::anyhow!("failed to write state: {}", e))?;
        state.bare_mode = bare_mode;
    }

    mossen_utils::hooks_dir::capture_hooks_config_snapshot();
    info!("setup: hooks config snapshot captured");

    let is_non_interactive = state
        .read()
        .map(|state| !state.is_interactive)
        .unwrap_or(false);
    let setup_hook_messages = crate::session_hooks::run_setup_hooks(
        state,
        mossen_utils::session_start::SetupTrigger::Init,
        is_non_interactive,
    )
    .await;
    if !setup_hook_messages.is_empty() {
        info!(
            target: "mossen_agent::hooks",
            count = setup_hook_messages.len(),
            "Setup hook messages produced during setup"
        );
    }

    // 后台预取（非 bare 模式）
    if !bare_mode {
        info!("setup: launching background prefetch tasks");
        launch_background_prefetch(state).await;
    }

    info!("setup: completed");
    Ok(())
}

/// 启动后台预取任务 — 对应 TS 的 startDeferredPrefetches()。
///
/// 在首次渲染后异步执行，不阻塞主流程。
async fn launch_background_prefetch(_state: &SharedBootstrapState) {
    // 使用 tokio::spawn 启动后台任务，不阻塞主流程

    // 预取用户上下文（对应 TS initUser / getUserContext）
    tokio::spawn(async {
        info!("background: prefetching user context");
        // 1. 预热邮箱缓存（通过 git user.email / 环境变量）
        mossen_utils::user::init_user().await;
        // 2. 预取 OAuth tokens / API key 状态（不会写入磁盘，仅刷新进程内缓存）
        mossen_utils::auth::prefetch_api_key_from_api_key_helper_if_safe(false);
        if mossen_utils::auth::is_aws_credential_export_from_project_settings() {
            mossen_utils::auth::prefetch_aws_credentials_and_bedrock_info_if_safe();
        }
        if mossen_utils::auth::is_gcp_auth_refresh_from_project_settings() {
            mossen_utils::auth::prefetch_gcp_credentials_if_safe();
        }
        info!("background: user context prefetch complete");
    });

    // 预取 MCP 官方注册表 URL（对应 TS prefetchOfficialMcpUrls）
    tokio::spawn(async {
        info!("background: prefetching MCP registry URLs");
        let registry = crate::repl_mcp::get_or_init_official_registry();
        registry.prefetch().await;
        info!("background: MCP registry prefetch complete");
    });

    // 预取设置变更检测器（对应 TS settingsChangeDetector.initialize()）
    tokio::spawn(async {
        info!("background: initializing settings change detector");
        mossen_utils::change_detector::initialize().await;
        info!("background: settings change detector initialized");
    });
}

/// 平台检测 — 对应 TS 的 getPlatform()。
async fn detect_platform() {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    info!(os, arch, "platform detected");

    // macOS 特定检测
    #[cfg(target_os = "macos")]
    {
        info!("platform: macOS detected, Apple Silicon features available");
    }

    // Linux 特定检测
    #[cfg(target_os = "linux")]
    {
        info!("platform: Linux detected");
    }

    // Windows 特定检测
    #[cfg(target_os = "windows")]
    {
        info!("platform: Windows detected");
    }
}

/// 版本检查与更新提示 — 对应 TS 的 cli/update.ts。
pub async fn check_for_updates() -> Result<()> {
    use mossen_utils::auto_updater::{get_latest_version_from_gcs, version_lt, ReleaseChannel};

    info!("update_check: checking for updates");

    // 1. 读取当前版本（从 Cargo 元数据）
    let current = env!("CARGO_PKG_VERSION");

    // 2. 查询最新版本（GCS dist tags）
    let channel = std::env::var("MOSSEN_RELEASE_CHANNEL")
        .ok()
        .and_then(|s| match s.as_str() {
            "latest" => Some(ReleaseChannel::Latest),
            "stable" => Some(ReleaseChannel::Stable),
            // 历史上曾有 beta/alpha 通道；当前 ReleaseChannel 仅支持 Latest/Stable，
            // 因此将 beta/alpha 映射到 Latest（rolling release）。
            "beta" | "alpha" => Some(ReleaseChannel::Latest),
            _ => None,
        })
        .unwrap_or(ReleaseChannel::Stable);

    match get_latest_version_from_gcs(channel).await {
        Some(latest) => {
            if version_lt(current, &latest) {
                info!(current, latest, "update available");
                // 仅打印；不强制升级（与 TS 行为一致）
                eprintln!(
                    "A newer mossen version is available: {} (current: {}). Run `mossen evolve` to upgrade.",
                    latest, current
                );
            } else {
                info!(current, latest, "already up to date");
            }
        }
        None => {
            warn!("update_check: failed to fetch latest version from registry");
        }
    }
    info!("update_check: completed");
    Ok(())
}

/// 验证权限模式是否安全 — 对应 TS setup.ts 中的权限安全检查。
pub fn validate_permission_safety(unrestricted: bool, skip_permissions: bool) -> Result<()> {
    if !unrestricted && !skip_permissions {
        return Ok(());
    }

    // 检查是否在 root 下运行
    #[cfg(unix)]
    {
        // 检查是否为 root 用户（通过环境变量和 euid 检测）
        let is_root = std::env::var("USER").map(|u| u == "root").unwrap_or(false)
            || std::env::var("EUID").map(|e| e == "0").unwrap_or(false);
        if is_root {
            let is_sandbox = std::env::var("IS_SANDBOX")
                .map(|v| v == "1")
                .unwrap_or(false);
            if !is_sandbox {
                anyhow::bail!(
                    "--dangerously-skip-permissions cannot be used with root/sudo privileges \
                     for security reasons"
                );
            }
        }
    }

    Ok(())
}
