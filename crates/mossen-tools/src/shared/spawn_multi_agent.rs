//! Shared spawn module for teammate creation.
//!
//! Extracted from TeammateTool to allow reuse by AgentTool. Handles three spawn
//! modes: split-pane (tmux/iTerm2), separate window (tmux), and in-process.

use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::process::Command;

// ── Constants ───────────────────────────────────────────────────────────────

pub const SWARM_SESSION_NAME: &str = "mossen-swarm";
pub const TEAM_LEAD_NAME: &str = "team-lead";
pub const TEAMMATE_COMMAND_ENV_VAR: &str = "MOSSEN_TEAMMATE_COMMAND";
pub const TMUX_COMMAND: &str = "tmux";

// ── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SpawnOutput {
    pub teammate_id: String,
    pub agent_id: String,
    pub agent_type: Option<String>,
    pub model: Option<String>,
    pub name: String,
    pub color: Option<String>,
    pub tmux_session_name: String,
    pub tmux_window_name: String,
    pub tmux_pane_id: String,
    pub team_name: Option<String>,
    pub is_splitpane: Option<bool>,
    pub plan_mode_required: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct SpawnTeammateConfig {
    pub name: String,
    pub prompt: String,
    pub team_name: Option<String>,
    pub cwd: Option<String>,
    pub use_splitpane: Option<bool>,
    pub plan_mode_required: Option<bool>,
    pub model: Option<String>,
    pub agent_type: Option<String>,
    pub description: Option<String>,
    pub invoking_request_id: Option<String>,
}

/// Permission mode for teammates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    Default,
    BypassPermissions,
    AcceptEdits,
    Auto,
    Plan,
}

/// Backend type for teammate spawning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    Tmux,
    ITerm2,
    InProcess,
}

/// Team member information stored in team files.
#[derive(Debug, Clone)]
pub struct TeamMember {
    pub agent_id: String,
    pub name: String,
    pub agent_type: Option<String>,
    pub model: Option<String>,
    pub prompt: String,
    pub color: String,
    pub plan_mode_required: Option<bool>,
    pub joined_at: u64,
    pub tmux_pane_id: String,
    pub cwd: String,
    pub subscriptions: Vec<String>,
    pub backend_type: BackendType,
}

/// Team file structure.
#[derive(Debug, Clone)]
pub struct TeamFile {
    pub team_name: String,
    pub members: Vec<TeamMember>,
}

/// Teammate tracking info in app state.
#[derive(Debug, Clone)]
pub struct TeammateInfo {
    pub name: String,
    pub agent_type: Option<String>,
    pub color: String,
    pub tmux_session_name: String,
    pub tmux_pane_id: String,
    pub cwd: String,
    pub spawned_at: u64,
}

/// Team context in app state.
#[derive(Debug, Clone, Default)]
pub struct TeamContext {
    pub team_name: String,
    pub team_file_path: String,
    pub lead_agent_id: String,
    pub teammates: HashMap<String, TeammateInfo>,
}

// ── Color assignment ────────────────────────────────────────────────────────

static TEAMMATE_COLORS: &[&str] = &[
    "#FF6B6B", "#4ECDC4", "#45B7D1", "#96CEB4", "#FFEAA7", "#DDA0DD", "#98D8C8", "#F7DC6F",
    "#BB8FCE", "#85C1E9", "#F8C471", "#82E0AA", "#F1948A", "#AED6F1", "#D7BDE2",
];

/// Assign a unique color to a teammate based on their ID hash.
pub fn assign_teammate_color(teammate_id: &str) -> String {
    let hash: u64 = teammate_id
        .bytes()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
    let idx = (hash as usize) % TEAMMATE_COLORS.len();
    TEAMMATE_COLORS[idx].to_string()
}

// ── Name utilities ──────────────────────────────────────────────────────────

/// Sanitize a name for use in tmux window names.
pub fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Sanitize agent name - prevent @ in agent IDs (would break agentName@teamName format).
pub fn sanitize_agent_name(name: &str) -> String {
    sanitize_name(name).replace('@', "-")
}

/// Format a deterministic agent ID from name and team.
pub fn format_agent_id(name: &str, team_name: &str) -> String {
    format!("{}@{}", sanitize_agent_name(name), sanitize_name(team_name))
}

/// Generate a unique teammate name by checking existing team members.
/// If the name already exists, appends a numeric suffix (e.g., tester-2, tester-3).
pub async fn generate_unique_teammate_name(base_name: &str, team_name: Option<&str>) -> String {
    let Some(t_name) = team_name else {
        return base_name.to_string();
    };

    let team_file = match read_team_file_async(t_name).await {
        Some(tf) => tf,
        None => return base_name.to_string(),
    };

    let existing_names: std::collections::HashSet<String> = team_file
        .members
        .iter()
        .map(|m| m.name.to_lowercase())
        .collect();

    if !existing_names.contains(&base_name.to_lowercase()) {
        return base_name.to_string();
    }

    let mut suffix = 2u32;
    loop {
        let candidate = format!("{}-{}", base_name, suffix);
        if !existing_names.contains(&candidate.to_lowercase()) {
            return candidate;
        }
        suffix += 1;
    }
}

// ── Model resolution ────────────────────────────────────────────────────────

const HARDCODED_TEAMMATE_MODEL_FALLBACK: &str = "balanced";

/// Get the default teammate model.
fn get_default_teammate_model(leader_model: Option<&str>) -> String {
    // Check configured global teammate default model
    if let Ok(configured) = std::env::var("MOSSEN_TEAMMATE_DEFAULT_MODEL") {
        if configured == "null" || configured.is_empty() {
            // User picked "Default" — follow the leader
            return leader_model
                .unwrap_or(HARDCODED_TEAMMATE_MODEL_FALLBACK)
                .to_string();
        }
        return configured;
    }
    leader_model
        .unwrap_or(HARDCODED_TEAMMATE_MODEL_FALLBACK)
        .to_string()
}

/// Resolve a teammate model value. Handles the 'inherit' alias by substituting
/// the leader's model. If leader model is None, falls through to the default.
pub fn resolve_teammate_model(input_model: Option<&str>, leader_model: Option<&str>) -> String {
    match input_model {
        Some("inherit") => leader_model
            .map(|m| m.to_string())
            .unwrap_or_else(|| get_default_teammate_model(leader_model)),
        Some(model) => model.to_string(),
        None => get_default_teammate_model(leader_model),
    }
}

// ── Tmux helpers ────────────────────────────────────────────────────────────

/// Check if a tmux session exists.
async fn has_session(session_name: &str) -> bool {
    let output = Command::new(TMUX_COMMAND)
        .args(["has-session", "-t", session_name])
        .output()
        .await;
    match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

/// Creates a new tmux session if it doesn't exist.
async fn ensure_session(session_name: &str) -> Result<(), String> {
    if has_session(session_name).await {
        return Ok(());
    }
    let output = Command::new(TMUX_COMMAND)
        .args(["new-session", "-d", "-s", session_name])
        .output()
        .await
        .map_err(|e| format!("Failed to run tmux: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Failed to create tmux session '{session_name}': {stderr}"
        ));
    }
    Ok(())
}

/// Check if we're inside a tmux session.
pub async fn is_inside_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

/// Check if tmux is available on the system.
pub async fn is_tmux_available() -> bool {
    Command::new("which")
        .arg(TMUX_COMMAND)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get the command to spawn a teammate.
fn get_teammate_command() -> String {
    if let Ok(cmd) = std::env::var(TEAMMATE_COMMAND_ENV_VAR) {
        return cmd;
    }
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "mossen".to_string())
}

// ── CLI flag building ───────────────────────────────────────────────────────

/// Shell-quote a single argument.
fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    if s.chars()
        .all(|c| c.is_alphanumeric() || "-_./=:@".contains(c))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Builds CLI flags to propagate from the current session to spawned teammates.
pub fn build_inherited_cli_flags(
    plan_mode_required: bool,
    permission_mode: PermissionMode,
) -> String {
    let mut flags: Vec<String> = Vec::new();

    // Propagate permission mode (not if plan mode is required for safety)
    if plan_mode_required {
        // Don't inherit bypass permissions when plan mode is required
    } else {
        match permission_mode {
            PermissionMode::BypassPermissions => {
                flags.push("--dangerously-skip-permissions".to_string());
            }
            PermissionMode::AcceptEdits => {
                flags.push("--permission-mode acceptEdits".to_string());
            }
            PermissionMode::Auto => {
                flags.push("--permission-mode auto".to_string());
            }
            _ => {}
        }
    }

    // Propagate --model if explicitly set via CLI
    if let Ok(model_override) = std::env::var("MOSSEN_MODEL_OVERRIDE") {
        if !model_override.is_empty() {
            flags.push(format!("--model {}", shell_quote(&model_override)));
        }
    }

    // Propagate --settings if set via CLI
    if let Ok(settings_path) = std::env::var("MOSSEN_SETTINGS_PATH") {
        if !settings_path.is_empty() {
            flags.push(format!("--settings {}", shell_quote(&settings_path)));
        }
    }

    // Propagate --chrome / --no-chrome if explicitly set
    if let Ok(chrome) = std::env::var("MOSSEN_CHROME_FLAG") {
        match chrome.as_str() {
            "true" => flags.push("--chrome".to_string()),
            "false" => flags.push("--no-chrome".to_string()),
            _ => {}
        }
    }

    flags.join(" ")
}

/// Build environment variables string that teammates need.
pub fn build_inherited_env_vars() -> String {
    let mut env_parts: Vec<String> = Vec::new();

    // Session marker
    if let Ok(val) = std::env::var("MOSSEN_SESSION_MARKER") {
        env_parts.push(format!("MOSSEN_SESSION_MARKER={}", shell_quote(&val)));
    }

    // Experimental agent teams flag
    if let Ok(val) = std::env::var("MOSSEN_CODE_EXPERIMENTAL_AGENT_TEAMS") {
        env_parts.push(format!(
            "MOSSEN_CODE_EXPERIMENTAL_AGENT_TEAMS={}",
            shell_quote(&val)
        ));
    }

    // API provider vars
    for key in &["OPENAI_API_KEY", "MOSSEN_API_KEY", "MOSSEN_API_BASE_URL"] {
        if let Ok(val) = std::env::var(key) {
            env_parts.push(format!("{}={}", key, shell_quote(&val)));
        }
    }

    env_parts.join(" ")
}

// ── Team file I/O ───────────────────────────────────────────────────────────

/// Get the path for a team file.
fn get_team_file_path(team_name: &str) -> std::path::PathBuf {
    let base = std::env::var("MOSSEN_TEAMS_DIR").unwrap_or_else(|_| {
        let data_dir = if cfg!(target_os = "macos") {
            std::env::var("HOME")
                .map(|h| format!("{}/Library/Application Support", h))
                .unwrap_or_else(|_| ".".to_string())
        } else if cfg!(target_os = "windows") {
            std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string())
        } else {
            std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
                std::env::var("HOME")
                    .map(|h| format!("{}/.local/share", h))
                    .unwrap_or_else(|_| ".".to_string())
            })
        };
        format!("{}/mossen/teams", data_dir)
    });
    std::path::PathBuf::from(base).join(format!("{}.json", sanitize_name(team_name)))
}

/// Read team file asynchronously.
pub async fn read_team_file_async(team_name: &str) -> Option<TeamFile> {
    let path = get_team_file_path(team_name);
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    // Parse JSON team file
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let members = json.get("members")?.as_array()?;
    let parsed_members: Vec<TeamMember> = members
        .iter()
        .filter_map(|m| {
            Some(TeamMember {
                agent_id: m.get("agentId")?.as_str()?.to_string(),
                name: m.get("name")?.as_str()?.to_string(),
                agent_type: m
                    .get("agentType")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                model: m
                    .get("model")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                prompt: m
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                color: m
                    .get("color")
                    .and_then(|v| v.as_str())
                    .unwrap_or("#FFFFFF")
                    .to_string(),
                plan_mode_required: m.get("planModeRequired").and_then(|v| v.as_bool()),
                joined_at: m.get("joinedAt").and_then(|v| v.as_u64()).unwrap_or(0),
                tmux_pane_id: m
                    .get("tmuxPaneId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                cwd: m
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".")
                    .to_string(),
                subscriptions: m
                    .get("subscriptions")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|s| s.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
                backend_type: match m.get("backendType").and_then(|v| v.as_str()) {
                    Some("tmux") => BackendType::Tmux,
                    Some("iterm2") => BackendType::ITerm2,
                    Some("in-process") => BackendType::InProcess,
                    _ => BackendType::Tmux,
                },
            })
        })
        .collect();
    Some(TeamFile {
        team_name: json
            .get("teamName")
            .and_then(|v| v.as_str())
            .unwrap_or(team_name)
            .to_string(),
        members: parsed_members,
    })
}

/// Write team file asynchronously.
pub async fn write_team_file_async(team_name: &str, team_file: &TeamFile) -> Result<(), String> {
    let path = get_team_file_path(team_name);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create team dir: {e}"))?;
    }
    let members_json: Vec<serde_json::Value> = team_file
        .members
        .iter()
        .map(|m| {
            serde_json::json!({
                "agentId": m.agent_id,
                "name": m.name,
                "agentType": m.agent_type,
                "model": m.model,
                "prompt": m.prompt,
                "color": m.color,
                "planModeRequired": m.plan_mode_required,
                "joinedAt": m.joined_at,
                "tmuxPaneId": m.tmux_pane_id,
                "cwd": m.cwd,
                "subscriptions": m.subscriptions,
                "backendType": match m.backend_type {
                    BackendType::Tmux => "tmux",
                    BackendType::ITerm2 => "iterm2",
                    BackendType::InProcess => "in-process",
                },
            })
        })
        .collect();
    let json = serde_json::json!({
        "teamName": team_file.team_name,
        "members": members_json,
    });
    let content = serde_json::to_string_pretty(&json)
        .map_err(|e| format!("Failed to serialize team file: {e}"))?;
    tokio::fs::write(&path, content)
        .await
        .map_err(|e| format!("Failed to write team file: {e}"))?;
    Ok(())
}

// ── Mailbox I/O ─────────────────────────────────────────────────────────────

/// Write a message to a teammate's mailbox.
pub async fn write_to_mailbox(
    agent_name: &str,
    from: &str,
    text: &str,
    team_name: &str,
) -> Result<(), String> {
    let base = std::env::var("MOSSEN_TEAMS_DIR").unwrap_or_else(|_| {
        let data_dir = if cfg!(target_os = "macos") {
            std::env::var("HOME")
                .map(|h| format!("{}/Library/Application Support", h))
                .unwrap_or_else(|_| ".".to_string())
        } else if cfg!(target_os = "windows") {
            std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string())
        } else {
            std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
                std::env::var("HOME")
                    .map(|h| format!("{}/.local/share", h))
                    .unwrap_or_else(|_| ".".to_string())
            })
        };
        format!("{}/mossen/teams", data_dir)
    });
    let mailbox_dir = std::path::PathBuf::from(&base)
        .join(sanitize_name(team_name))
        .join("mailbox")
        .join(sanitize_name(agent_name));
    tokio::fs::create_dir_all(&mailbox_dir)
        .await
        .map_err(|e| format!("Failed to create mailbox dir: {e}"))?;

    let timestamp = chrono::Utc::now().to_rfc3339();
    let msg = serde_json::json!({
        "from": from,
        "text": text,
        "timestamp": &timestamp,
    });

    let filename = format!("{}.json", chrono::Utc::now().timestamp_millis());
    let path = mailbox_dir.join(filename);
    let content = serde_json::to_string_pretty(&msg)
        .map_err(|e| format!("Failed to serialize mailbox message: {e}"))?;
    tokio::fs::write(&path, content)
        .await
        .map_err(|e| format!("Failed to write mailbox message: {e}"))?;
    Ok(())
}

// ── Send command to pane ────────────────────────────────────────────────────

/// Send a command string to a tmux pane.
async fn send_command_to_pane(
    pane_id: &str,
    command: &str,
    use_swarm_socket: bool,
) -> Result<(), String> {
    let mut args = vec!["send-keys".to_string()];
    if use_swarm_socket {
        args.push("-L".to_string());
        args.push(SWARM_SESSION_NAME.to_string());
    }
    args.push("-t".to_string());
    args.push(pane_id.to_string());
    args.push(command.to_string());
    args.push("Enter".to_string());

    let output = Command::new(TMUX_COMMAND)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to send keys to tmux pane: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tmux send-keys failed: {stderr}"));
    }
    Ok(())
}

// ── Pane creation ───────────────────────────────────────────────────────────

/// Create a teammate pane in the swarm view. Returns the pane ID and whether
/// this is the first teammate.
async fn create_teammate_pane_in_swarm_view(
    name: &str,
    _color: &str,
) -> Result<(String, bool), String> {
    let inside_tmux = is_inside_tmux().await;

    if inside_tmux {
        // Split current window horizontally for teammate
        let output = Command::new(TMUX_COMMAND)
            .args(["split-window", "-h", "-P", "-F", "#{pane_id}"])
            .output()
            .await
            .map_err(|e| format!("Failed to split tmux pane: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("tmux split-window failed: {stderr}"));
        }

        let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // First teammate if only 2 panes exist now
        let list_output = Command::new(TMUX_COMMAND)
            .args(["list-panes", "-F", "#{pane_id}"])
            .output()
            .await
            .ok();
        let pane_count = list_output
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().count())
            .unwrap_or(2);
        Ok((pane_id, pane_count == 2))
    } else {
        // Outside tmux: create swarm session with the teammate
        ensure_session(SWARM_SESSION_NAME).await?;
        let window_name = format!("teammate-{}", sanitize_name(name));
        let output = Command::new(TMUX_COMMAND)
            .args([
                "new-window",
                "-t",
                SWARM_SESSION_NAME,
                "-n",
                &window_name,
                "-P",
                "-F",
                "#{pane_id}",
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to create tmux window: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("tmux new-window failed: {stderr}"));
        }
        let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok((pane_id, true))
    }
}

/// Enable pane border status display in tmux.
async fn enable_pane_border_status() -> Result<(), String> {
    let output = Command::new(TMUX_COMMAND)
        .args(["set-option", "-w", "pane-border-status", "top"])
        .output()
        .await
        .map_err(|e| format!("Failed to set pane border status: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tmux set-option failed: {stderr}"));
    }
    Ok(())
}

// ── Spawn handlers ──────────────────────────────────────────────────────────

/// Handle spawn operation using split-pane view (default).
async fn handle_spawn_split_pane(
    input: &SpawnTeammateConfig,
    team_name: &str,
    permission_mode: PermissionMode,
    leader_model: Option<&str>,
) -> Result<SpawnOutput, String> {
    let model = resolve_teammate_model(input.model.as_deref(), leader_model);

    if input.name.is_empty() || input.prompt.is_empty() {
        return Err("name and prompt are required for spawn operation".to_string());
    }

    let unique_name = generate_unique_teammate_name(&input.name, Some(team_name)).await;
    let sanitized_name = sanitize_agent_name(&unique_name);
    let teammate_id = format_agent_id(&sanitized_name, team_name);
    let working_dir = input.cwd.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    });

    let inside_tmux = is_inside_tmux().await;
    let teammate_color = assign_teammate_color(&teammate_id);

    let (pane_id, is_first_teammate) =
        create_teammate_pane_in_swarm_view(&sanitized_name, &teammate_color).await?;

    if is_first_teammate && inside_tmux {
        let _ = enable_pane_border_status().await;
    }

    let binary_path = get_teammate_command();
    let plan_mode_required = input.plan_mode_required.unwrap_or(false);
    let session_id = std::env::var("MOSSEN_SESSION_ID").unwrap_or_default();

    let mut teammate_args_parts: Vec<String> = vec![
        format!("--agent-id {}", shell_quote(&teammate_id)),
        format!("--agent-name {}", shell_quote(&sanitized_name)),
        format!("--team-name {}", shell_quote(team_name)),
        format!("--agent-color {}", shell_quote(&teammate_color)),
        format!("--parent-session-id {}", shell_quote(&session_id)),
    ];
    if plan_mode_required {
        teammate_args_parts.push("--plan-mode-required".to_string());
    }
    if let Some(ref at) = input.agent_type {
        teammate_args_parts.push(format!("--agent-type {}", shell_quote(at)));
    }
    let teammate_args = teammate_args_parts.join(" ");

    let mut inherited_flags = build_inherited_cli_flags(plan_mode_required, permission_mode);

    // If teammate has a custom model, replace inherited --model flag
    if !model.is_empty() {
        let parts: Vec<&str> = inherited_flags.split_whitespace().collect();
        let mut filtered: Vec<String> = Vec::new();
        let mut skip_next = false;
        for (i, part) in parts.iter().enumerate() {
            if skip_next {
                skip_next = false;
                continue;
            }
            if *part == "--model" {
                skip_next = true;
                continue;
            }
            if i > 0 && parts[i - 1] == "--model" {
                continue;
            }
            filtered.push(part.to_string());
        }
        inherited_flags = filtered.join(" ");
        if inherited_flags.is_empty() {
            inherited_flags = format!("--model {}", shell_quote(&model));
        } else {
            inherited_flags = format!("{} --model {}", inherited_flags, shell_quote(&model));
        }
    }

    let flags_str = if inherited_flags.is_empty() {
        String::new()
    } else {
        format!(" {inherited_flags}")
    };
    let env_str = build_inherited_env_vars();
    let spawn_command = format!(
        "cd {} && env {} {} {}{}",
        shell_quote(&working_dir),
        env_str,
        shell_quote(&binary_path),
        teammate_args,
        flags_str
    );

    send_command_to_pane(&pane_id, &spawn_command, !inside_tmux).await?;

    let session_name = if inside_tmux {
        "current".to_string()
    } else {
        SWARM_SESSION_NAME.to_string()
    };
    let window_name = if inside_tmux {
        "current".to_string()
    } else {
        "swarm-view".to_string()
    };

    // Register agent in team file
    if let Some(mut team_file) = read_team_file_async(team_name).await {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        team_file.members.push(TeamMember {
            agent_id: teammate_id.clone(),
            name: sanitized_name.clone(),
            agent_type: input.agent_type.clone(),
            model: Some(model.clone()),
            prompt: input.prompt.clone(),
            color: teammate_color.clone(),
            plan_mode_required: input.plan_mode_required,
            joined_at: now,
            tmux_pane_id: pane_id.clone(),
            cwd: working_dir.clone(),
            subscriptions: vec![],
            backend_type: BackendType::Tmux,
        });
        let _ = write_team_file_async(team_name, &team_file).await;
    }

    // Send initial instructions via mailbox
    let _ = write_to_mailbox(&sanitized_name, TEAM_LEAD_NAME, &input.prompt, team_name).await;

    Ok(SpawnOutput {
        teammate_id: teammate_id.clone(),
        agent_id: teammate_id,
        agent_type: input.agent_type.clone(),
        model: Some(model),
        name: sanitized_name,
        color: Some(teammate_color),
        tmux_session_name: session_name,
        tmux_window_name: window_name,
        tmux_pane_id: pane_id,
        team_name: Some(team_name.to_string()),
        is_splitpane: Some(true),
        plan_mode_required: input.plan_mode_required,
    })
}

/// Handle spawn operation using separate windows (legacy behavior).
async fn handle_spawn_separate_window(
    input: &SpawnTeammateConfig,
    team_name: &str,
    permission_mode: PermissionMode,
    leader_model: Option<&str>,
) -> Result<SpawnOutput, String> {
    let model = resolve_teammate_model(input.model.as_deref(), leader_model);

    if input.name.is_empty() || input.prompt.is_empty() {
        return Err("name and prompt are required for spawn operation".to_string());
    }

    let unique_name = generate_unique_teammate_name(&input.name, Some(team_name)).await;
    let sanitized_name = sanitize_agent_name(&unique_name);
    let teammate_id = format_agent_id(&sanitized_name, team_name);
    let window_name = format!("teammate-{}", sanitize_name(&sanitized_name));
    let working_dir = input.cwd.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    });

    ensure_session(SWARM_SESSION_NAME).await?;
    let teammate_color = assign_teammate_color(&teammate_id);

    // Create a new window
    let output = Command::new(TMUX_COMMAND)
        .args([
            "new-window",
            "-t",
            SWARM_SESSION_NAME,
            "-n",
            &window_name,
            "-P",
            "-F",
            "#{pane_id}",
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to create tmux window: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to create tmux window: {stderr}"));
    }
    let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let binary_path = get_teammate_command();
    let plan_mode_required = input.plan_mode_required.unwrap_or(false);
    let session_id = std::env::var("MOSSEN_SESSION_ID").unwrap_or_default();

    let mut teammate_args_parts: Vec<String> = vec![
        format!("--agent-id {}", shell_quote(&teammate_id)),
        format!("--agent-name {}", shell_quote(&sanitized_name)),
        format!("--team-name {}", shell_quote(team_name)),
        format!("--agent-color {}", shell_quote(&teammate_color)),
        format!("--parent-session-id {}", shell_quote(&session_id)),
    ];
    if plan_mode_required {
        teammate_args_parts.push("--plan-mode-required".to_string());
    }
    if let Some(ref at) = input.agent_type {
        teammate_args_parts.push(format!("--agent-type {}", shell_quote(at)));
    }
    let teammate_args = teammate_args_parts.join(" ");

    let mut inherited_flags = build_inherited_cli_flags(plan_mode_required, permission_mode);
    if !model.is_empty() {
        let parts: Vec<&str> = inherited_flags.split_whitespace().collect();
        let mut filtered: Vec<String> = Vec::new();
        let mut skip_next = false;
        for (i, part) in parts.iter().enumerate() {
            if skip_next {
                skip_next = false;
                continue;
            }
            if *part == "--model" {
                skip_next = true;
                continue;
            }
            if i > 0 && parts[i - 1] == "--model" {
                continue;
            }
            filtered.push(part.to_string());
        }
        inherited_flags = filtered.join(" ");
        if inherited_flags.is_empty() {
            inherited_flags = format!("--model {}", shell_quote(&model));
        } else {
            inherited_flags = format!("{} --model {}", inherited_flags, shell_quote(&model));
        }
    }

    let flags_str = if inherited_flags.is_empty() {
        String::new()
    } else {
        format!(" {inherited_flags}")
    };
    let env_str = build_inherited_env_vars();
    let spawn_command = format!(
        "cd {} && env {} {} {}{}",
        shell_quote(&working_dir),
        env_str,
        shell_quote(&binary_path),
        teammate_args,
        flags_str
    );

    // Send keys to the new window
    let send_result = Command::new(TMUX_COMMAND)
        .args([
            "send-keys",
            "-t",
            &format!("{SWARM_SESSION_NAME}:{window_name}"),
            &spawn_command,
            "Enter",
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to send command: {e}"))?;

    if !send_result.status.success() {
        let stderr = String::from_utf8_lossy(&send_result.stderr);
        return Err(format!("Failed to send command to tmux window: {stderr}"));
    }

    // Register in team file
    if let Some(mut team_file) = read_team_file_async(team_name).await {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        team_file.members.push(TeamMember {
            agent_id: teammate_id.clone(),
            name: sanitized_name.clone(),
            agent_type: input.agent_type.clone(),
            model: Some(model.clone()),
            prompt: input.prompt.clone(),
            color: teammate_color.clone(),
            plan_mode_required: input.plan_mode_required,
            joined_at: now,
            tmux_pane_id: pane_id.clone(),
            cwd: working_dir,
            subscriptions: vec![],
            backend_type: BackendType::Tmux,
        });
        let _ = write_team_file_async(team_name, &team_file).await;
    }

    // Send initial instructions via mailbox
    let _ = write_to_mailbox(&sanitized_name, TEAM_LEAD_NAME, &input.prompt, team_name).await;

    Ok(SpawnOutput {
        teammate_id: teammate_id.clone(),
        agent_id: teammate_id,
        agent_type: input.agent_type.clone(),
        model: Some(model),
        name: sanitized_name,
        color: Some(teammate_color),
        tmux_session_name: SWARM_SESSION_NAME.to_string(),
        tmux_window_name: window_name,
        tmux_pane_id: pane_id,
        team_name: Some(team_name.to_string()),
        is_splitpane: Some(false),
        plan_mode_required: input.plan_mode_required,
    })
}

/// Handle spawn for in-process teammates.
async fn handle_spawn_in_process(
    input: &SpawnTeammateConfig,
    team_name: &str,
    leader_model: Option<&str>,
) -> Result<SpawnOutput, String> {
    let model = resolve_teammate_model(input.model.as_deref(), leader_model);

    if input.name.is_empty() || input.prompt.is_empty() {
        return Err("name and prompt are required for spawn operation".to_string());
    }

    let unique_name = generate_unique_teammate_name(&input.name, Some(team_name)).await;
    let sanitized_name = sanitize_agent_name(&unique_name);
    let teammate_id = format_agent_id(&sanitized_name, team_name);
    let teammate_color = assign_teammate_color(&teammate_id);

    // Register in team file
    if let Some(mut team_file) = read_team_file_async(team_name).await {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let cwd = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        team_file.members.push(TeamMember {
            agent_id: teammate_id.clone(),
            name: sanitized_name.clone(),
            agent_type: input.agent_type.clone(),
            model: Some(model.clone()),
            prompt: input.prompt.clone(),
            color: teammate_color.clone(),
            plan_mode_required: input.plan_mode_required,
            joined_at: now,
            tmux_pane_id: "in-process".to_string(),
            cwd,
            subscriptions: vec![],
            backend_type: BackendType::InProcess,
        });
        let _ = write_team_file_async(team_name, &team_file).await;
    }

    // Note: Do NOT send prompt via mailbox for in-process teammates.
    // They receive it directly via the in-process runner.

    Ok(SpawnOutput {
        teammate_id: teammate_id.clone(),
        agent_id: teammate_id,
        agent_type: input.agent_type.clone(),
        model: Some(model),
        name: sanitized_name,
        color: Some(teammate_color),
        tmux_session_name: "in-process".to_string(),
        tmux_window_name: "in-process".to_string(),
        tmux_pane_id: "in-process".to_string(),
        team_name: Some(team_name.to_string()),
        is_splitpane: Some(false),
        plan_mode_required: input.plan_mode_required,
    })
}

// ── Main entry point ────────────────────────────────────────────────────────

/// Check if in-process teammate mode is enabled.
fn is_in_process_enabled() -> bool {
    std::env::var("MOSSEN_TEAMMATE_IN_PROCESS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Spawns a new teammate with the given configuration.
/// This is the main entry point for teammate spawning, used by both TeammateTool and AgentTool.
pub async fn spawn_teammate(
    config: &SpawnTeammateConfig,
    team_name: &str,
    permission_mode: PermissionMode,
    leader_model: Option<&str>,
) -> Result<SpawnOutput, String> {
    // Check if in-process mode is enabled
    if is_in_process_enabled() {
        return handle_spawn_in_process(config, team_name, leader_model).await;
    }

    // Pre-flight: ensure tmux is available
    if !is_tmux_available().await {
        // Fall back to in-process if no pane backend is available
        return handle_spawn_in_process(config, team_name, leader_model).await;
    }

    let use_splitpane = config.use_splitpane.unwrap_or(true);
    if use_splitpane {
        handle_spawn_split_pane(config, team_name, permission_mode, leader_model).await
    } else {
        handle_spawn_separate_window(config, team_name, permission_mode, leader_model).await
    }
}
