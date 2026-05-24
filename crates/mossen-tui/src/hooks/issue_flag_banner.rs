//! Issue flag banner hook (useIssueFlagBanner.ts).
//!
//! Displays a banner when a known issue affects the current session.

/// State for issue flag banner display.
#[derive(Debug, Clone)]
pub struct IssueFlagBannerState {
    pub active_issues: Vec<IssueFlagEntry>,
    pub dismissed: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct IssueFlagEntry {
    pub id: String,
    pub message: String,
    pub severity: IssueSeverity,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueSeverity {
    Info,
    Warning,
    Critical,
}

impl IssueFlagBannerState {
    pub fn new() -> Self {
        Self {
            active_issues: Vec::new(),
            dismissed: Vec::new(),
        }
    }

    /// Add an issue flag.
    pub fn add_issue(&mut self, issue: IssueFlagEntry) {
        if !self.dismissed.contains(&issue.id) {
            self.active_issues.push(issue);
        }
    }

    /// Dismiss an issue.
    pub fn dismiss(&mut self, id: &str) {
        self.active_issues.retain(|i| i.id != id);
        self.dismissed.push(id.to_string());
    }

    /// Get visible (non-dismissed) issues.
    pub fn visible_issues(&self) -> &[IssueFlagEntry] {
        &self.active_issues
    }

    /// Check if there are any visible issues.
    pub fn has_issues(&self) -> bool {
        !self.active_issues.is_empty()
    }
}

impl Default for IssueFlagBannerState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Heuristics translated from useIssueFlagBanner.ts
// ============================================================================

/// A simplified assistant tool-use record for compatibility checks.
#[derive(Debug, Clone)]
pub struct AssistantToolUse {
    pub tool_name: String,
    pub command: Option<String>,
}

/// Summary of one chat message — only the fields the heuristics use.
#[derive(Debug, Clone)]
pub struct IssueFlagMessage {
    pub is_assistant: bool,
    pub is_user: bool,
    pub user_text: Option<String>,
    pub tool_uses: Vec<AssistantToolUse>,
}

/// External-command patterns (as plain substrings — Rust regex isn't
/// imported here). Each entry is matched as a word boundary against the
/// command lowercased.
const EXTERNAL_COMMANDS: &[&str] = &[
    "curl", "wget", "ssh", "kubectl", "srun", "docker", "bq", "gsutil", "gcloud", "aws", "nc",
    "ncat", "telnet", "ftp",
];

/// More elaborate patterns translated from `EXTERNAL_COMMAND_PATTERNS` —
/// these are multi-token sequences (git push/pull/fetch, gh pr/issue).
const EXTERNAL_COMMAND_PHRASES: &[&str] =
    &["git push", "git pull", "git fetch", "gh pr", "gh issue"];

fn is_word_boundary(prev: Option<char>, next: Option<char>) -> bool {
    !prev
        .map(|c| c.is_alphanumeric() || c == '_')
        .unwrap_or(false)
        && !next
            .map(|c| c.is_alphanumeric() || c == '_')
            .unwrap_or(false)
}

fn contains_word(haystack: &str, needle: &str) -> bool {
    let mut idx = 0;
    let bytes = haystack.as_bytes();
    let nb = needle.as_bytes();
    while idx + nb.len() <= bytes.len() {
        if &bytes[idx..idx + nb.len()] == nb {
            let prev = if idx > 0 {
                haystack[..idx].chars().last()
            } else {
                None
            };
            let next = haystack[idx + nb.len()..].chars().next();
            if is_word_boundary(prev, next) {
                return true;
            }
        }
        idx += 1;
    }
    false
}

fn contains_command_phrase(haystack: &str, phrase: &str) -> bool {
    // Matches `<phrase>` separated by 1+ whitespace at word boundaries.
    let parts: Vec<&str> = phrase.split_whitespace().collect();
    if parts.is_empty() {
        return false;
    }
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(parts[0]) {
        let absolute = start + pos;
        let prev = if absolute > 0 {
            haystack[..absolute].chars().last()
        } else {
            None
        };
        let next = haystack[absolute + parts[0].len()..].chars().next();
        let leading_ok = !prev
            .map(|c| c.is_alphanumeric() || c == '_')
            .unwrap_or(false);
        let trailing_ok = next.map(|c| c.is_whitespace()).unwrap_or(false);
        if leading_ok && trailing_ok {
            // Scan remaining parts after whitespace.
            let mut cursor = absolute + parts[0].len();
            let mut ok = true;
            for part in &parts[1..] {
                let rest = &haystack[cursor..];
                let mut iter = rest.char_indices();
                let mut nonspace = None;
                for (i, c) in iter.by_ref() {
                    if !c.is_whitespace() {
                        nonspace = Some(i);
                        break;
                    }
                }
                let Some(start_part) = nonspace else {
                    ok = false;
                    break;
                };
                let abs_start = cursor + start_part;
                if haystack[abs_start..].starts_with(part) {
                    let end = abs_start + part.len();
                    let after = haystack[end..].chars().next();
                    let after_ok = after
                        .map(|c| c.is_whitespace() || !(c.is_alphanumeric() || c == '_'))
                        .unwrap_or(true);
                    if !after_ok {
                        ok = false;
                        break;
                    }
                    cursor = end;
                } else {
                    ok = false;
                    break;
                }
            }
            if ok {
                return true;
            }
        }
        start = absolute + parts[0].len();
    }
    false
}

/// True if the session contains no calls to MCP tools or external-network
/// shell commands — i.e. it's safe to package up as a portable container.
///
/// TS source: `isSessionContainerCompatible(messages)`.
pub fn is_session_container_compatible(messages: &[IssueFlagMessage]) -> bool {
    for msg in messages {
        if !msg.is_assistant {
            continue;
        }
        for use_block in &msg.tool_uses {
            if use_block.tool_name.starts_with("mcp__") {
                return false;
            }
            if use_block.tool_name == "Bash" {
                if let Some(cmd) = &use_block.command {
                    let lower = cmd.to_lowercase();
                    for c in EXTERNAL_COMMANDS {
                        if contains_word(&lower, c) {
                            return false;
                        }
                    }
                    for c in EXTERNAL_COMMAND_PHRASES {
                        if contains_command_phrase(&lower, c) {
                            return false;
                        }
                    }
                }
            }
        }
    }
    true
}

const FRICTION_NEEDLES: &[&str] = &[
    "no, ",
    "no! ",
    "that's wrong",
    "that's incorrect",
    "thats wrong",
    "thats incorrect",
    "not what i asked",
    "not what i wanted",
    "not what i meant",
    "not what i said",
    "i said",
    "i asked",
    "i wanted",
    "i told you",
    "i already said",
    "why did you",
    "you should have",
    "you shouldn't have",
    "you shouldnt have",
    "you should not have",
    "you were supposed to",
    "try again",
    "undo that",
    "undo this",
    "undo it",
    "undo what you",
    "revert that",
    "revert this",
    "revert it",
    "revert what you",
];

/// True if the most recent user message looks like a friction signal —
/// implicit pushback that suggests the assistant misunderstood.
///
/// TS source: `hasFrictionSignal(messages)`.
pub fn has_friction_signal(messages: &[IssueFlagMessage]) -> bool {
    for msg in messages.iter().rev() {
        if !msg.is_user {
            continue;
        }
        let Some(text) = msg.user_text.as_deref() else {
            continue;
        };
        let lower = text.to_lowercase();
        for needle in FRICTION_NEEDLES {
            if lower.contains(needle) {
                return true;
            }
        }
        return false;
    }
    false
}

#[cfg(test)]
mod issue_flag_tests {
    use super::*;

    #[test]
    fn container_compatible_when_no_mcp_or_curl() {
        let msgs = vec![IssueFlagMessage {
            is_assistant: true,
            is_user: false,
            user_text: None,
            tool_uses: vec![AssistantToolUse {
                tool_name: "Bash".into(),
                command: Some("ls -al".into()),
            }],
        }];
        assert!(is_session_container_compatible(&msgs));
    }

    #[test]
    fn incompatible_with_curl() {
        let msgs = vec![IssueFlagMessage {
            is_assistant: true,
            is_user: false,
            user_text: None,
            tool_uses: vec![AssistantToolUse {
                tool_name: "Bash".into(),
                command: Some("curl https://x.com".into()),
            }],
        }];
        assert!(!is_session_container_compatible(&msgs));
    }

    #[test]
    fn incompatible_with_git_push() {
        let msgs = vec![IssueFlagMessage {
            is_assistant: true,
            is_user: false,
            user_text: None,
            tool_uses: vec![AssistantToolUse {
                tool_name: "Bash".into(),
                command: Some("git push origin main".into()),
            }],
        }];
        assert!(!is_session_container_compatible(&msgs));
    }

    #[test]
    fn friction_signal_detected() {
        let msgs = vec![IssueFlagMessage {
            is_assistant: false,
            is_user: true,
            user_text: Some("No, that's wrong".into()),
            tool_uses: Vec::new(),
        }];
        assert!(has_friction_signal(&msgs));
    }

    #[test]
    fn friction_signal_not_detected_for_neutral_text() {
        let msgs = vec![IssueFlagMessage {
            is_assistant: false,
            is_user: true,
            user_text: Some("looks good".into()),
            tool_uses: Vec::new(),
        }];
        assert!(!has_friction_signal(&msgs));
    }
}
