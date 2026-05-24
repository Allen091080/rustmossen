//! Shell-agnostic git operation tracking for usage metrics.
//!
//! Detects `git commit`, `git push`, `gh pr create`, `glab mr create`, and
//! curl-based PR creation in command strings, then fires analytics events.
//! The regexes operate on raw command text so they work identically for Bash
//! and PowerShell (both invoke git/gh/glab/curl as external binaries with the
//! same argv syntax).

use regex::Regex;
use std::sync::LazyLock;

// ── Regex patterns ──────────────────────────────────────────────────────────

/// Build a regex that matches `git <subcmd>` while tolerating git's global
/// options between `git` and the subcommand (e.g. `-c key=val`, `-C path`,
/// `--git-dir=path`).
fn git_cmd_re(subcmd: &str, suffix: &str) -> Regex {
    let pattern = format!(r"\bgit(?:\s+-[cC]\s+\S+|\s+--\S+=\S+)*\s+{subcmd}\b{suffix}");
    Regex::new(&pattern).expect("invalid git regex pattern")
}

static GIT_COMMIT_RE: LazyLock<Regex> = LazyLock::new(|| git_cmd_re("commit", ""));
static GIT_PUSH_RE: LazyLock<Regex> = LazyLock::new(|| git_cmd_re("push", ""));
static GIT_CHERRY_PICK_RE: LazyLock<Regex> = LazyLock::new(|| git_cmd_re("cherry-pick", ""));
static GIT_MERGE_RE: LazyLock<Regex> = LazyLock::new(|| git_cmd_re("merge", "(?!-)"));
static GIT_REBASE_RE: LazyLock<Regex> = LazyLock::new(|| git_cmd_re("rebase", ""));

static GIT_AMEND_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"--amend\b").unwrap());
static FAST_FORWARD_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(Fast-forward|Merge made by)").unwrap());
static REBASE_SUCCESS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Successfully rebased").unwrap());
static GLAB_MR_CREATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bglab\s+mr\s+create\b").unwrap());

// ── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitKind {
    Committed,
    Amended,
    CherryPicked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchAction {
    Merged,
    Rebased,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrAction {
    Created,
    Edited,
    Merged,
    Commented,
    Closed,
    Ready,
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub sha: String,
    pub kind: CommitKind,
}

#[derive(Debug, Clone)]
pub struct PushInfo {
    pub branch: String,
}

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub ref_name: String,
    pub action: BranchAction,
}

#[derive(Debug, Clone)]
pub struct PrInfo {
    pub number: u64,
    pub url: Option<String>,
    pub action: PrAction,
}

/// Result of detecting git operations in a command + output pair.
#[derive(Debug, Clone, Default)]
pub struct GitOperationResult {
    pub commit: Option<CommitInfo>,
    pub push: Option<PushInfo>,
    pub branch: Option<BranchInfo>,
    pub pr: Option<PrInfo>,
}

// ── GH PR Actions ───────────────────────────────────────────────────────────

struct GhPrActionDef {
    pattern: &'static str,
    action: PrAction,
    op: &'static str,
}

static GH_PR_ACTION_DEFS: &[GhPrActionDef] = &[
    GhPrActionDef {
        pattern: r"\bgh\s+pr\s+create\b",
        action: PrAction::Created,
        op: "pr_create",
    },
    GhPrActionDef {
        pattern: r"\bgh\s+pr\s+edit\b",
        action: PrAction::Edited,
        op: "pr_edit",
    },
    GhPrActionDef {
        pattern: r"\bgh\s+pr\s+merge\b",
        action: PrAction::Merged,
        op: "pr_merge",
    },
    GhPrActionDef {
        pattern: r"\bgh\s+pr\s+comment\b",
        action: PrAction::Commented,
        op: "pr_comment",
    },
    GhPrActionDef {
        pattern: r"\bgh\s+pr\s+close\b",
        action: PrAction::Closed,
        op: "pr_close",
    },
    GhPrActionDef {
        pattern: r"\bgh\s+pr\s+ready\b",
        action: PrAction::Ready,
        op: "pr_ready",
    },
];

/// Find the first matching GH PR action in a command string.
fn find_pr_action(command: &str) -> Option<(PrAction, &'static str)> {
    for def in GH_PR_ACTION_DEFS {
        if let Ok(re) = Regex::new(def.pattern) {
            if re.is_match(command) {
                return Some((def.action, def.op));
            }
        }
    }
    None
}

// ── Helper parsers ──────────────────────────────────────────────────────────

/// Parse PR info from a GitHub PR URL.
fn parse_pr_url(url: &str) -> Option<(u64, String, String)> {
    static PR_URL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"https://github\.com/([^/]+/[^/]+)/pull/(\d+)").unwrap());
    let caps = PR_URL_RE.captures(url)?;
    let repo = caps.get(1)?.as_str().to_string();
    let num: u64 = caps.get(2)?.as_str().parse().ok()?;
    Some((num, url.to_string(), repo))
}

/// Find a GitHub PR URL embedded anywhere in stdout and parse it.
fn find_pr_in_stdout(stdout: &str) -> Option<(u64, String, String)> {
    static PR_FIND_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"https://github\.com/[^/\s]+/[^/\s]+/pull/\d+").unwrap());
    let m = PR_FIND_RE.find(stdout)?;
    parse_pr_url(m.as_str())
}

/// Parse a git commit SHA from output. Matches `[branch abc1234] message` or
/// `[branch (root-commit) abc1234] message`.
pub fn parse_git_commit_id(stdout: &str) -> Option<&str> {
    static COMMIT_ID_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\[[\w./-]+(?: \(root-commit\))? ([0-9a-f]+)\]").unwrap());
    COMMIT_ID_RE
        .captures(stdout)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
}

/// Parse branch name from git push output.
fn parse_git_push_branch(output: &str) -> Option<&str> {
    static PUSH_BRANCH_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?m)^\s*[+\-*!= ]?\s*(?:\[new branch\]|\S+\.\.+\S+)\s+\S+\s*->\s*(\S+)")
            .unwrap()
    });
    PUSH_BRANCH_RE
        .captures(output)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
}

/// Parse PR number from text like "Pull request owner/repo#1234".
fn parse_pr_number_from_text(stdout: &str) -> Option<u64> {
    static PR_NUM_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[Pp]ull request (?:\S+#)?#?(\d+)").unwrap());
    PR_NUM_RE
        .captures(stdout)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

/// Extract target ref from `git merge <ref>` / `git rebase <ref>` command.
/// Skips flags and keywords — first non-flag argument is the ref.
fn parse_ref_from_command(command: &str, verb: &str) -> Option<String> {
    let re = git_cmd_re(verb, "");
    let parts: Vec<&str> = re.split(command).collect();
    let after = parts.get(1)?;
    for token in after.trim().split_whitespace() {
        if token.starts_with('&')
            || token.starts_with('|')
            || token.starts_with(';')
            || token.starts_with('>')
            || token.starts_with('<')
        {
            break;
        }
        if token.starts_with('-') {
            continue;
        }
        return Some(token.to_string());
    }
    None
}

// ── Main Detection ──────────────────────────────────────────────────────────

/// Scan bash command + output for git operations worth surfacing in the
/// collapsed tool-use summary ("committed a1b2c3, created PR #42, ran 3 bash
/// commands"). Checks the command to avoid matching SHAs/URLs that merely
/// appear in unrelated output (e.g. `git log`).
///
/// Pass stdout+stderr concatenated — git push writes the ref update to stderr.
pub fn detect_git_operation(command: &str, output: &str) -> GitOperationResult {
    let mut result = GitOperationResult::default();

    // commit and cherry-pick both produce "[branch sha] msg" output
    let is_cherry_pick = GIT_CHERRY_PICK_RE.is_match(command);
    if GIT_COMMIT_RE.is_match(command) || is_cherry_pick {
        if let Some(sha) = parse_git_commit_id(output) {
            let short_sha = if sha.len() > 6 { &sha[..6] } else { sha };
            let kind = if is_cherry_pick {
                CommitKind::CherryPicked
            } else if GIT_AMEND_RE.is_match(command) {
                CommitKind::Amended
            } else {
                CommitKind::Committed
            };
            result.commit = Some(CommitInfo {
                sha: short_sha.to_string(),
                kind,
            });
        }
    }

    if GIT_PUSH_RE.is_match(command) {
        if let Some(branch) = parse_git_push_branch(output) {
            result.push = Some(PushInfo {
                branch: branch.to_string(),
            });
        }
    }

    if GIT_MERGE_RE.is_match(command) && FAST_FORWARD_RE.is_match(output) {
        if let Some(ref_name) = parse_ref_from_command(command, "merge") {
            result.branch = Some(BranchInfo {
                ref_name,
                action: BranchAction::Merged,
            });
        }
    }

    if GIT_REBASE_RE.is_match(command) && REBASE_SUCCESS_RE.is_match(output) {
        if let Some(ref_name) = parse_ref_from_command(command, "rebase") {
            result.branch = Some(BranchInfo {
                ref_name,
                action: BranchAction::Rebased,
            });
        }
    }

    // Check gh pr actions
    if let Some((action, _op)) = find_pr_action(command) {
        if let Some((pr_number, pr_url, _repo)) = find_pr_in_stdout(output) {
            result.pr = Some(PrInfo {
                number: pr_number,
                url: Some(pr_url),
                action,
            });
        } else if let Some(num) = parse_pr_number_from_text(output) {
            result.pr = Some(PrInfo {
                number: num,
                url: None,
                action,
            });
        }
    }

    result
}

// ── Analytics Tracking ──────────────────────────────────────────────────────

/// Placeholder analytics event logger. In production this would send to OTLP/analytics.
fn log_event(_event_name: &str, _metadata: &[(&str, &str)]) {
    // In the full implementation, this forwards to the analytics service.
    // For now, the function signature exists so all call sites compile.
}

/// Track git operations for analytics. Called after shell command execution.
pub fn track_git_operations(command: &str, exit_code: i32, stdout: Option<&str>) {
    if exit_code != 0 {
        return;
    }

    if GIT_COMMIT_RE.is_match(command) {
        log_event("mossen_git_operation", &[("operation", "commit")]);
        if GIT_AMEND_RE.is_match(command) {
            log_event("mossen_git_operation", &[("operation", "commit_amend")]);
        }
    }

    if GIT_PUSH_RE.is_match(command) {
        log_event("mossen_git_operation", &[("operation", "push")]);
    }

    let pr_hit = find_pr_action(command);
    if let Some((_action, op)) = pr_hit {
        log_event("mossen_git_operation", &[("operation", op)]);

        if matches!(_action, PrAction::Created) {
            // Auto-link session to PR if we can extract PR URL from stdout
            if let Some(output) = stdout {
                if let Some((_pr_number, _pr_url, _pr_repository)) = find_pr_in_stdout(output) {
                    // In the full implementation, this would call linkSessionToPR
                    // via the session storage module.
                }
            }
        }
    }

    if GLAB_MR_CREATE_RE.is_match(command) {
        log_event("mossen_git_operation", &[("operation", "pr_create")]);
    }

    // Detect PR creation via curl to REST APIs
    let is_curl_post = command.contains("curl")
        && (Regex::new(r"-X\s*POST\b").unwrap().is_match(command)
            || Regex::new(r"--request\s*=?\s*POST\b")
                .unwrap()
                .is_match(command)
            || command.contains(" -d "));

    static PR_ENDPOINT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"https?://[^\s']*/(pulls|pull-requests|merge[-_]requests)(?!/\d)").unwrap()
    });
    let is_pr_endpoint = PR_ENDPOINT_RE.is_match(command);

    if is_curl_post && is_pr_endpoint {
        log_event("mossen_git_operation", &[("operation", "pr_create")]);
    }
}
