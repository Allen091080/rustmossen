//! External editor detection and launching utilities.

use std::path::Path;
use std::process::{Command, Stdio};

use once_cell::sync::Lazy;
use regex::Regex;

/// GUI editors that open in a separate window.
const GUI_EDITORS: &[&str] = &[
    "code",
    "cursor",
    "windsurf",
    "codium",
    "subl",
    "atom",
    "gedit",
    "notepad++",
    "notepad",
];

/// Editors that accept +N as a goto-line argument.
static PLUS_N_EDITORS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(vi|vim|nvim|nano|emacs|pico|micro|helix|hx)\b").unwrap());

/// VS Code and forks use -g file:line.
const VSCODE_FAMILY: &[&str] = &["code", "cursor", "windsurf", "codium"];

/// Classify the editor as GUI or not. Returns the matched GUI family name.
pub fn classify_gui_editor(editor: &str) -> Option<&'static str> {
    let base = Path::new(editor.split_whitespace().next().unwrap_or(""))
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    GUI_EDITORS.iter().find(|&&g| base.contains(g)).copied()
}

/// Build goto-line argv for a GUI editor.
fn gui_goto_argv(gui_family: &str, file_path: &str, line: Option<u32>) -> Vec<String> {
    match line {
        None => vec![file_path.to_string()],
        Some(l) => {
            if VSCODE_FAMILY.contains(&gui_family) {
                vec!["-g".to_string(), format!("{}:{}", file_path, l)]
            } else if gui_family == "subl" {
                vec![format!("{}:{}", file_path, l)]
            } else {
                vec![file_path.to_string()]
            }
        }
    }
}

/// Launch a file in the user's external editor.
///
/// For GUI editors: spawns detached.
/// For terminal editors: blocks until the editor exits.
/// Returns true if the editor was launched.
pub fn open_file_in_external_editor(file_path: &str, line: Option<u32>) -> bool {
    let editor = match get_external_editor() {
        Some(e) => e,
        None => return false,
    };

    let parts: Vec<&str> = editor.split_whitespace().collect();
    let base = parts.first().copied().unwrap_or(&editor);
    let editor_args: Vec<&str> = parts.iter().skip(1).copied().collect();
    let gui_family = classify_gui_editor(&editor);

    if let Some(family) = gui_family {
        let goto_argv = gui_goto_argv(family, file_path, line);
        let mut cmd = Command::new(base);
        cmd.args(&editor_args);
        cmd.args(&goto_argv);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        match cmd.spawn() {
            Ok(child) => {
                // Detach — don't wait for it
                let _ = child.id();
                return true;
            }
            Err(e) => {
                tracing::debug!("editor spawn failed: {}", e);
                return false;
            }
        }
    }

    // Terminal editor — blocks until editor exits
    let base_path = Path::new(base)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(base);
    let use_goto_line = line.is_some() && PLUS_N_EDITORS.is_match(base_path);

    let mut args: Vec<String> = editor_args.iter().map(|s| s.to_string()).collect();
    if let (true, Some(l)) = (use_goto_line, line) {
        args.push(format!("+{}", l));
    }
    args.push(file_path.to_string());

    let result = Command::new(base)
        .args(&args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    match result {
        Ok(status) => status.success(),
        Err(e) => {
            tracing::debug!("editor spawn failed: {}", e);
            false
        }
    }
}

/// Get the user's preferred external editor.
/// Checks VISUAL, EDITOR env vars, then common editors in PATH.
pub fn get_external_editor() -> Option<String> {
    if let Ok(visual) = std::env::var("VISUAL") {
        let trimmed = visual.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    if let Ok(editor) = std::env::var("EDITOR") {
        let trimmed = editor.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    if cfg!(target_os = "windows") {
        return Some("start /wait notepad".to_string());
    }

    // Search for available editors in order of preference
    let editors = ["code", "vi", "nano"];
    for ed in &editors {
        if which::which(ed).is_ok() {
            return Some(ed.to_string());
        }
    }

    None
}
