//! Ripgrep integration for file search operations.
//!
//! Provides functions to run ripgrep searches, count files, and stream results.

use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

/// Maximum buffer size for ripgrep output (20MB).
const MAX_BUFFER_SIZE: usize = 20_000_000;

/// Ripgrep configuration mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RipgrepMode {
    System,
    Builtin,
    Embedded,
}

/// Ripgrep configuration.
#[derive(Debug, Clone)]
pub struct RipgrepConfig {
    pub mode: RipgrepMode,
    pub command: String,
    pub args: Vec<String>,
    pub argv0: Option<String>,
}

/// Ripgrep status information.
#[derive(Debug, Clone)]
pub struct RipgrepStatus {
    pub mode: RipgrepMode,
    pub path: String,
    pub working: Option<bool>,
}

/// Error for ripgrep timeouts with partial results.
#[derive(Debug, thiserror::Error)]
#[error("Ripgrep search timed out: {message}")]
pub struct RipgrepTimeoutError {
    pub message: String,
    pub partial_results: Vec<String>,
}

/// Global ripgrep configuration cache.
static RIPGREP_CONFIG: OnceLock<RipgrepConfig> = OnceLock::new();

/// Whether the first-use test has run.
static FIRST_USE_TESTED: AtomicBool = AtomicBool::new(false);

/// Whether codesign check has been done (macOS only).
static CODESIGN_CHECKED: AtomicBool = AtomicBool::new(false);

/// Get the ripgrep configuration, determining which binary to use.
pub fn get_ripgrep_config() -> &'static RipgrepConfig {
    RIPGREP_CONFIG.get_or_init(|| {
        let user_wants_system = std::env::var("USE_BUILTIN_RIPGREP")
            .map(|v| v == "0" || v.eq_ignore_ascii_case("false"))
            .unwrap_or(false);

        // Try system ripgrep if user wants it
        if user_wants_system && which::which("rg").is_ok() {
            return RipgrepConfig {
                mode: RipgrepMode::System,
                command: "rg".to_string(),
                args: vec![],
                argv0: None,
            };
        }

        // Check for bundled ripgrep
        let exe_path = std::env::current_exe().unwrap_or_default();
        let vendor_rg = exe_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("vendor")
            .join("ripgrep")
            .join(format!(
                "{}-{}",
                std::env::consts::ARCH,
                std::env::consts::OS
            ))
            .join(if cfg!(windows) { "rg.exe" } else { "rg" });

        if vendor_rg.exists() {
            return RipgrepConfig {
                mode: RipgrepMode::Builtin,
                command: vendor_rg.to_string_lossy().to_string(),
                args: vec![],
                argv0: None,
            };
        }

        // Fallback to system rg
        if which::which("rg").is_ok() {
            return RipgrepConfig {
                mode: RipgrepMode::System,
                command: "rg".to_string(),
                args: vec![],
                argv0: None,
            };
        }

        // Last resort: try rg anyway
        RipgrepConfig {
            mode: RipgrepMode::System,
            command: "rg".to_string(),
            args: vec![],
            argv0: None,
        }
    })
}

/// Get the ripgrep command path and base arguments.
pub fn ripgrep_command() -> (&'static str, &'static [String], Option<&'static str>) {
    let config = get_ripgrep_config();
    (&config.command, &config.args, config.argv0.as_deref())
}

/// Check if an error string indicates an EAGAIN resource error.
fn is_eagain_error(stderr: &str) -> bool {
    stderr.contains("os error 11") || stderr.contains("Resource temporarily unavailable")
}

/// Get the default timeout for ripgrep based on platform.
fn get_default_timeout() -> Duration {
    // WSL has severe performance penalty for file reads
    let is_wsl = std::env::var("WSL_DISTRO_NAME").is_ok();
    if is_wsl {
        Duration::from_secs(60)
    } else {
        Duration::from_secs(20)
    }
}

/// Get the configured timeout for ripgrep.
fn get_timeout() -> Duration {
    if let Ok(seconds_str) = std::env::var("MOSSEN_CODE_GLOB_TIMEOUT_SECONDS") {
        if let Ok(seconds) = seconds_str.parse::<u64>() {
            if seconds > 0 {
                return Duration::from_secs(seconds);
            }
        }
    }
    get_default_timeout()
}

/// Run ripgrep and return matching lines.
///
/// Returns a list of matched lines. Exit code 1 (no matches) returns an empty vec.
/// Retries with single-threaded mode on EAGAIN errors.
pub async fn rip_grep(
    args: &[&str],
    target: &str,
    cancel: &CancellationToken,
) -> Result<Vec<String>> {
    codesign_ripgrep_if_necessary().await;
    test_ripgrep_on_first_use().await;

    rip_grep_impl(args, target, cancel, false).await
}

fn rip_grep_impl<'a>(
    args: &'a [&'a str],
    target: &'a str,
    cancel: &'a CancellationToken,
    is_retry: bool,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>>> + Send + 'a>> {
    Box::pin(async move {
        let config = get_ripgrep_config();
        let rg_timeout = get_timeout();

        let mut cmd_args: Vec<&str> = Vec::new();
        for a in &config.args {
            cmd_args.push(a.as_str());
        }
        if is_retry {
            cmd_args.push("-j");
            cmd_args.push("1");
        }
        cmd_args.extend_from_slice(args);
        cmd_args.push(target);

        let mut cmd = Command::new(&config.command);
        cmd.args(&cmd_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().context("Failed to spawn ripgrep")?;

        let stdout = child.stdout.take().context("No stdout from ripgrep")?;
        let stderr_handle = child.stderr.take().context("No stderr from ripgrep")?;

        let stdout_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut output = String::new();
            let mut buf = vec![0u8; 65536];
            loop {
                use tokio::io::AsyncReadExt;
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if output.len() + n <= MAX_BUFFER_SIZE {
                            output.push_str(&String::from_utf8_lossy(&buf[..n]));
                        }
                    }
                    Err(_) => break,
                }
            }
            output
        });

        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr_handle);
            let mut output = String::new();
            let mut buf = vec![0u8; 8192];
            loop {
                use tokio::io::AsyncReadExt;
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if output.len() + n <= MAX_BUFFER_SIZE {
                            output.push_str(&String::from_utf8_lossy(&buf[..n]));
                        }
                    }
                    Err(_) => break,
                }
            }
            output
        });

        let result = tokio::select! {
            _ = cancel.cancelled() => {
                child.kill().await.ok();
                return Ok(vec![]);
            }
            result = timeout(rg_timeout, child.wait()) => {
                match result {
                    Ok(Ok(status)) => {
                        let stdout_str = stdout_task.await.unwrap_or_default();
                        let stderr_str = stderr_task.await.unwrap_or_default();
                        (status.code(), stdout_str, stderr_str)
                    }
                    Ok(Err(e)) => {
                        child.kill().await.ok();
                        return Err(e.into());
                    }
                    Err(_) => {
                        // Timeout
                        child.kill().await.ok();
                        let stdout_str = stdout_task.await.unwrap_or_default();
                        let lines = parse_lines(&stdout_str);
                        if lines.is_empty() {
                            return Err(RipgrepTimeoutError {
                                message: format!(
                                    "Ripgrep search timed out after {} seconds.",
                                    rg_timeout.as_secs()
                                ),
                                partial_results: vec![],
                            }.into());
                        }
                        // Return partial results (drop last potentially incomplete line)
                        let mut lines = lines;
                        lines.pop();
                        return Ok(lines);
                    }
                }
            }
        };

        let (exit_code, stdout_str, stderr_str) = result;

        match exit_code {
            Some(0) | Some(1) => {
                // 0 = matches found, 1 = no matches
                Ok(parse_lines(&stdout_str))
            }
            Some(code) => {
                // Check for EAGAIN retry
                if !is_retry && is_eagain_error(&stderr_str) {
                    debug!("rg EAGAIN error detected, retrying with single-threaded mode");
                    return rip_grep_impl(args, target, cancel, true).await;
                }

                debug!(
                    "rg error (code={}, stderr={}), returning partial results",
                    code, stderr_str
                );

                let lines = parse_lines(&stdout_str);
                if lines.is_empty() && code != 2 {
                    error!("ripgrep exited with code {}: {}", code, stderr_str);
                }
                Ok(lines)
            }
            None => {
                // Process was killed by signal
                let lines = parse_lines(&stdout_str);
                if lines.is_empty() {
                    return Err(RipgrepTimeoutError {
                        message: "Ripgrep was killed by signal".to_string(),
                        partial_results: vec![],
                    }
                    .into());
                }
                Ok(lines)
            }
        }
    }) // end Box::pin
}

/// Parse stdout into lines, filtering empty lines and stripping CR.
fn parse_lines(stdout: &str) -> Vec<String> {
    stdout
        .trim()
        .split('\n')
        .map(|line| line.trim_end_matches('\r').to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Stream ripgrep results line-by-line as they arrive.
pub async fn rip_grep_stream<F>(
    args: &[&str],
    target: &str,
    cancel: &CancellationToken,
    mut on_lines: F,
) -> Result<()>
where
    F: FnMut(Vec<String>),
{
    codesign_ripgrep_if_necessary().await;
    let config = get_ripgrep_config();

    let mut cmd_args: Vec<&str> = Vec::new();
    for a in &config.args {
        cmd_args.push(a.as_str());
    }
    cmd_args.extend_from_slice(args);
    cmd_args.push(target);

    let mut cmd = Command::new(&config.command);
    cmd.args(&cmd_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    let mut child = cmd
        .spawn()
        .context("Failed to spawn ripgrep for streaming")?;
    let stdout = child.stdout.take().context("No stdout from ripgrep")?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                child.kill().await.ok();
                return Ok(());
            }
            result = reader.read_line(&mut line) => {
                match result {
                    Ok(0) => break,
                    Ok(_) => {
                        let stripped = line.trim_end_matches('\n').trim_end_matches('\r').to_string();
                        if !stripped.is_empty() {
                            on_lines(vec![stripped]);
                        }
                        line.clear();
                    }
                    Err(e) => {
                        debug!("Error reading ripgrep stream: {}", e);
                        break;
                    }
                }
            }
        }
    }

    let status = child.wait().await?;
    match status.code() {
        Some(0) | Some(1) => Ok(()),
        Some(code) => Err(anyhow::anyhow!("ripgrep exited with code {}", code)),
        None => Ok(()),
    }
}

/// Count files using ripgrep's `--files` mode, rounded to nearest power of 10 for privacy.
pub async fn count_files_rounded_rg(
    dir_path: &Path,
    cancel: &CancellationToken,
    ignore_patterns: &[&str],
) -> Option<u64> {
    // Skip if we're in the home directory to avoid permission dialogs
    if let Some(home) = dirs::home_dir() {
        if dir_path == home {
            return None;
        }
    }

    codesign_ripgrep_if_necessary().await;
    let config = get_ripgrep_config();

    let mut cmd_args: Vec<String> = config.args.clone();
    cmd_args.push("--files".to_string());
    cmd_args.push("--hidden".to_string());

    for pattern in ignore_patterns {
        cmd_args.push("--glob".to_string());
        cmd_args.push(format!("!{}", pattern));
    }
    cmd_args.push(dir_path.to_string_lossy().to_string());

    let mut cmd = Command::new(&config.command);
    cmd.args(&cmd_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    let child_result = cmd.spawn();
    let mut child = match child_result {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to spawn ripgrep for file count: {}", e);
            return None;
        }
    };

    let stdout = child.stdout.take()?;
    let count_task = tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut count: u64 = 0;
        while let Ok(Some(_)) = lines.next_line().await {
            count += 1;
        }
        count
    });

    let result = tokio::select! {
        _ = cancel.cancelled() => {
            child.kill().await.ok();
            return None;
        }
        result = timeout(Duration::from_secs(30), count_task) => {
            match result {
                Ok(Ok(count)) => count,
                _ => {
                    child.kill().await.ok();
                    return None;
                }
            }
        }
    };

    child.wait().await.ok();

    if result == 0 {
        return Some(0);
    }

    // Round to nearest power of 10 for privacy
    let magnitude = (result as f64).log10().floor() as u32;
    let power = 10u64.pow(magnitude);
    Some(((result as f64 / power as f64).round() as u64) * power)
}

/// Get ripgrep status information.
pub fn get_ripgrep_status() -> RipgrepStatus {
    let config = get_ripgrep_config();
    RipgrepStatus {
        mode: config.mode,
        path: config.command.clone(),
        working: if FIRST_USE_TESTED.load(Ordering::Relaxed) {
            Some(true) // Simplified: if tested, assume working
        } else {
            None
        },
    }
}

/// Test ripgrep availability on first use (fire-and-forget).
async fn test_ripgrep_on_first_use() {
    if FIRST_USE_TESTED.swap(true, Ordering::Relaxed) {
        return;
    }

    let config = get_ripgrep_config();
    let result = Command::new(&config.command)
        .args(&config.args)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let working = output.status.success() && stdout.starts_with("ripgrep ");
            debug!(
                "Ripgrep first use test: {} (mode={:?}, path={})",
                if working { "PASSED" } else { "FAILED" },
                config.mode,
                config.command
            );
        }
        Err(e) => {
            error!("Ripgrep first use test failed: {}", e);
        }
    }
}

/// On macOS, codesign the builtin ripgrep binary if necessary.
async fn codesign_ripgrep_if_necessary() {
    if cfg!(not(target_os = "macos")) {
        return;
    }
    if CODESIGN_CHECKED.swap(true, Ordering::Relaxed) {
        return;
    }

    let config = get_ripgrep_config();
    if config.mode != RipgrepMode::Builtin {
        return;
    }

    let builtin_path = &config.command;

    // Check if already properly signed
    let check_result = Command::new("codesign")
        .args(["-vv", "-d", builtin_path])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    let needs_signing = match check_result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.contains("linker-signed")
        }
        Err(_) => false,
    };

    if !needs_signing {
        return;
    }

    // Sign the binary
    let sign_result = Command::new("codesign")
        .args([
            "--sign",
            "-",
            "--force",
            "--preserve-metadata=entitlements,requirements,flags,runtime",
            builtin_path,
        ])
        .output()
        .await;

    if let Err(e) = sign_result {
        error!("Failed to sign ripgrep: {}", e);
        return;
    }

    // Remove quarantine attribute
    let _ = Command::new("xattr")
        .args(["-d", "com.apple.quarantine", builtin_path])
        .output()
        .await;
}
