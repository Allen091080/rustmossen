// Translated from utils/memory/types.ts and utils/memory/versions.ts

use std::path::Path;

// ============================================================================
// types.ts
// ============================================================================

/// Memory type values.
pub const MEMORY_TYPE_VALUES: &[&str] = &[
    "User",
    "Project",
    "Local",
    "Managed",
    "AutoMem",
    "TeamMem",
];

/// Represents the type of memory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryType {
    User,
    Project,
    Local,
    Managed,
    AutoMem,
    TeamMem,
}

impl MemoryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "User",
            Self::Project => "Project",
            Self::Local => "Local",
            Self::Managed => "Managed",
            Self::AutoMem => "AutoMem",
            Self::TeamMem => "TeamMem",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "User" => Some(Self::User),
            "Project" => Some(Self::Project),
            "Local" => Some(Self::Local),
            "Managed" => Some(Self::Managed),
            "AutoMem" => Some(Self::AutoMem),
            "TeamMem" => Some(Self::TeamMem),
            _ => None,
        }
    }
}

// ============================================================================
// versions.ts
// ============================================================================

/// Check if a project directory is in a git repo.
/// Uses find_git_root which walks the filesystem (no subprocess).
/// Prefer `dir_is_in_git_repo()` for async checks.
pub fn project_is_in_git_repo(cwd: &str) -> bool {
    find_git_root(cwd).is_some()
}

/// Walk up the filesystem to find the git root.
fn find_git_root(start: &str) -> Option<String> {
    let mut current = Path::new(start).to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current.to_string_lossy().to_string());
        }
        if !current.pop() {
            return None;
        }
    }
}
