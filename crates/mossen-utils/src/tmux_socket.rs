//! TMUX Socket Isolation
//!
//! Manages an isolated tmux socket for Mossen's operations.
//! Without isolation, Mossen could accidentally affect the user's tmux sessions.
//! Mossen creates its own tmux socket: `mossen-<PID>` and ALL commands use this socket.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tracing::debug;

const TMUX_COMMAND: &str = "tmux";
const MOSSEN_SOCKET_PREFIX: &str = "mossen";

/// Result of executing a tmux command.
struct TmuxExecResult {
    stdout: String,
    stderr: String,
    code: i32,
}

/// Executes a tmux command, routing through WSL on Windows.
async fn exec_tmux(args: &[&str], _use_cwd: bool) -> TmuxExecResult {
    let output = if cfg!(target_os = "windows") {
        let mut cmd_args = vec!["-e", TMUX_COMMAND];
        cmd_args.extend(args);
        Command::new("wsl").args(&cmd_args).output()
    } else {
        Command::new(TMUX_COMMAND).args(args).output()
    };

    match output {
        Ok(out) => TmuxExecResult {
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
            code: out.status.code().unwrap_or(1),
        },
        Err(_) => TmuxExecResult {
            stdout: String::new(),
            stderr: String::from("Failed to execute tmux"),
            code: 1,
        },
    }
}

/// Socket state — initialized lazily when Tmux tool is first used.
#[derive(Default)]
struct SocketState {
    socket_name: Option<String>,
    socket_path: Option<String>,
    server_pid: Option<u32>,
    is_initializing: bool,
}

static SOCKET_STATE: Lazy<Mutex<SocketState>> = Lazy::new(|| Mutex::new(SocketState::default()));
static TMUX_AVAILABILITY_CHECKED: AtomicBool = AtomicBool::new(false);
static TMUX_AVAILABLE: AtomicBool = AtomicBool::new(false);
static TMUX_TOOL_USED: AtomicBool = AtomicBool::new(false);

/// Gets the socket name for Mossen's isolated tmux session.
/// Format: mossen-<PID>
pub fn get_mossen_socket_name() -> String {
    let mut state = SOCKET_STATE.lock();
    if let Some(ref name) = state.socket_name {
        return name.clone();
    }
    let name = format!("{}-{}", MOSSEN_SOCKET_PREFIX, std::process::id());
    state.socket_name = Some(name.clone());
    name
}

/// Gets the socket path if the socket has been initialized.
/// Returns None if not yet initialized.
pub fn get_mossen_socket_path() -> Option<String> {
    let state = SOCKET_STATE.lock();
    state.socket_path.clone()
}

/// Sets socket info after initialization.
pub fn set_mossen_socket_info(path: String, pid: u32) {
    let mut state = SOCKET_STATE.lock();
    state.socket_path = Some(path);
    state.server_pid = Some(pid);
}

/// Returns whether the socket has been initialized.
pub fn is_socket_initialized() -> bool {
    let state = SOCKET_STATE.lock();
    state.socket_path.is_some() && state.server_pid.is_some()
}

/// Gets the TMUX environment variable value for Mossen's isolated socket.
///
/// Format: "socket_path,server_pid,pane_index" (matches tmux's TMUX env var)
/// Returns None if socket is not yet initialized.
pub fn get_mossen_tmux_env() -> Option<String> {
    let state = SOCKET_STATE.lock();
    match (&state.socket_path, state.server_pid) {
        (Some(path), Some(pid)) => Some(format!("{},{},0", path, pid)),
        _ => None,
    }
}

/// Checks if tmux is available on this system.
/// Checked once and cached for the lifetime of the process.
pub async fn check_tmux_available() -> bool {
    if TMUX_AVAILABILITY_CHECKED.load(Ordering::Relaxed) {
        return TMUX_AVAILABLE.load(Ordering::Relaxed);
    }

    let result = if cfg!(target_os = "windows") {
        exec_tmux(&["-V"], false).await
    } else {
        let output = Command::new("which").arg(TMUX_COMMAND).output();
        match output {
            Ok(out) => TmuxExecResult {
                stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                code: out.status.code().unwrap_or(1),
            },
            Err(_) => TmuxExecResult {
                stdout: String::new(),
                stderr: String::new(),
                code: 1,
            },
        }
    };

    let available = result.code == 0;
    if !available {
        debug!("[Socket] tmux is not installed. The Tmux tool and Teammate tool will not be available.");
    }
    TMUX_AVAILABLE.store(available, Ordering::Relaxed);
    TMUX_AVAILABILITY_CHECKED.store(true, Ordering::Relaxed);
    available
}

/// Returns the cached tmux availability status.
pub fn is_tmux_available() -> bool {
    TMUX_AVAILABILITY_CHECKED.load(Ordering::Relaxed) && TMUX_AVAILABLE.load(Ordering::Relaxed)
}

/// Marks that the Tmux tool has been used at least once.
pub fn mark_tmux_tool_used() {
    TMUX_TOOL_USED.store(true, Ordering::Relaxed);
}

/// Returns whether the Tmux tool has been used at least once.
pub fn has_tmux_tool_been_used() -> bool {
    TMUX_TOOL_USED.load(Ordering::Relaxed)
}

/// Ensures the socket is initialized with a tmux session.
/// Safe to call multiple times; will only initialize once.
pub async fn ensure_socket_initialized() -> anyhow::Result<()> {
    if is_socket_initialized() {
        return Ok(());
    }

    let available = check_tmux_available().await;
    if !available {
        return Ok(());
    }

    {
        let state = SOCKET_STATE.lock();
        if state.is_initializing {
            return Ok(());
        }
    }

    {
        let mut state = SOCKET_STATE.lock();
        state.is_initializing = true;
    }

    let result = do_initialize().await;

    {
        let mut state = SOCKET_STATE.lock();
        state.is_initializing = false;
    }

    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            debug!(
                "[Socket] Failed to initialize tmux socket: {}. Tmux isolation will be disabled.",
                e
            );
            Ok(())
        }
    }
}

/// Kills the tmux server for Mossen's isolated socket.
async fn kill_tmux_server() {
    let socket = get_mossen_socket_name();
    debug!("[Socket] Killing tmux server for socket: {}", socket);

    let result = exec_tmux(&["-L", &socket, "kill-server"], false).await;

    if result.code == 0 {
        debug!("[Socket] Successfully killed tmux server");
    } else {
        debug!(
            "[Socket] Failed to kill tmux server (exit {}): {}",
            result.code, result.stderr
        );
    }
}

async fn do_initialize() -> anyhow::Result<()> {
    let socket = get_mossen_socket_name();

    // Create a new session with our custom socket
    let mut args: Vec<&str> = vec![
        "-L",
        &socket,
        "new-session",
        "-d",
        "-s",
        "base",
        "-e",
        "MOSSEN_CODE_SKIP_PROMPT_HISTORY=true",
    ];

    if cfg!(target_os = "windows") {
        args.extend(&["-e", "WSL_INTEROP=/run/WSL/1_interop"]);
    }

    let result = exec_tmux(&args, false).await;

    if result.code != 0 {
        // Session might already exist — check
        let check_result = exec_tmux(&["-L", &socket, "has-session", "-t", "base"], false).await;
        if check_result.code != 0 {
            anyhow::bail!(
                "Failed to create tmux session on socket {}: {}",
                socket,
                result.stderr
            );
        }
    }

    // Set MOSSEN_CODE_SKIP_PROMPT_HISTORY in the tmux GLOBAL environment
    exec_tmux(
        &[
            "-L",
            &socket,
            "set-environment",
            "-g",
            "MOSSEN_CODE_SKIP_PROMPT_HISTORY",
            "true",
        ],
        false,
    )
    .await;

    // WSL_INTEROP pin for Windows
    if cfg!(target_os = "windows") {
        exec_tmux(
            &[
                "-L",
                &socket,
                "set-environment",
                "-g",
                "WSL_INTEROP",
                "/run/WSL/1_interop",
            ],
            false,
        )
        .await;
    }

    // Get the socket path and server PID
    let info_result = exec_tmux(
        &[
            "-L",
            &socket,
            "display-message",
            "-p",
            "#{socket_path},#{pid}",
        ],
        false,
    )
    .await;

    if info_result.code == 0 {
        let trimmed = info_result.stdout.trim();
        let parts: Vec<&str> = trimmed.splitn(2, ',').collect();
        if parts.len() == 2 {
            if let Ok(pid) = parts[1].parse::<u32>() {
                set_mossen_socket_info(parts[0].to_string(), pid);
                return Ok(());
            }
        }
        debug!(
            "[Socket] Failed to parse socket info from tmux output: \"{}\". Using fallback path.",
            trimmed
        );
    } else {
        debug!(
            "[Socket] Failed to get socket info via display-message (exit {}): {}. Using fallback path.",
            info_result.code, info_result.stderr
        );
    }

    // Fallback: construct the socket path from standard tmux location
    let uid = unsafe { nix::unistd::getuid() }.as_raw();
    let base_tmp_dir = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
    let fallback_path = PathBuf::from(&base_tmp_dir)
        .join(format!("tmux-{}", uid))
        .join(&socket);
    let fallback_path_str = fallback_path.to_string_lossy().to_string();

    // Get server PID separately
    let pid_result = exec_tmux(&["-L", &socket, "display-message", "-p", "#{pid}"], false).await;

    if pid_result.code == 0 {
        if let Ok(pid) = pid_result.stdout.trim().parse::<u32>() {
            debug!(
                "[Socket] Using fallback socket path: {} (server PID: {})",
                fallback_path_str, pid
            );
            set_mossen_socket_info(fallback_path_str, pid);
            return Ok(());
        }
        debug!(
            "[Socket] Failed to parse server PID from tmux output: \"{}\"",
            pid_result.stdout.trim()
        );
    } else {
        debug!(
            "[Socket] Failed to get server PID (exit {}): {}",
            pid_result.code, pid_result.stderr
        );
    }

    anyhow::bail!(
        "Failed to get socket info for {}: primary=\"{}\", fallback=\"{}\"",
        socket,
        info_result.stderr,
        pid_result.stderr
    )
}

/// For testing purposes — resets all socket state.
pub fn reset_socket_state() {
    let mut state = SOCKET_STATE.lock();
    state.socket_name = None;
    state.socket_path = None;
    state.server_pid = None;
    state.is_initializing = false;
    TMUX_AVAILABILITY_CHECKED.store(false, Ordering::Relaxed);
    TMUX_AVAILABLE.store(false, Ordering::Relaxed);
    TMUX_TOOL_USED.store(false, Ordering::Relaxed);
}
