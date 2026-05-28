//! Skill discovery hooks for tools that touch project paths.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use serde_json::Value;
use tracing::info;

const MAX_OBSERVED_TOOL_PATHS: usize = 32;
const CANONICAL_CONFIG_DIR: &str = ".mossen";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolSkillDiscoveryReport {
    pub observed_path_count: usize,
    pub discovered_dir_count: usize,
    pub added_skill_count: usize,
    pub activated_skill_names: Vec<String>,
}

impl ToolSkillDiscoveryReport {
    pub fn to_metadata(&self) -> HashMap<String, Value> {
        let mut metadata = HashMap::new();
        metadata.insert(
            "skill_discovery".to_string(),
            serde_json::json!({
                "observedPathCount": self.observed_path_count,
                "discoveredDirCount": self.discovered_dir_count,
                "addedSkillCount": self.added_skill_count,
                "activatedSkillCount": self.activated_skill_names.len(),
                "activatedSkillNames": self.activated_skill_names,
                "rawPathsIncluded": false,
                "pathsRedacted": true,
            }),
        );
        metadata
    }
}

pub async fn observe_tool_file_paths<I, S>(paths: I, cwd: &str) -> ToolSkillDiscoveryReport
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let cwd_path = normalize_path(PathBuf::from(cwd));
    let mut seen = HashSet::new();
    let mut observed = Vec::new();

    for raw in paths
        .into_iter()
        .map(|path| path.as_ref().trim().to_string())
    {
        if raw.is_empty() {
            continue;
        }
        let expanded = shellexpand::tilde(&raw).to_string();
        let path = PathBuf::from(expanded);
        let absolute = if path.is_absolute() {
            path
        } else {
            cwd_path.join(path)
        };
        let normalized = normalize_path(absolute);
        if !normalized.starts_with(&cwd_path) {
            continue;
        }
        if seen.insert(normalized.clone()) {
            observed.push(normalized);
            if observed.len() >= MAX_OBSERVED_TOOL_PATHS {
                break;
            }
        }
    }

    if observed.is_empty() {
        return ToolSkillDiscoveryReport::default();
    }

    let discovered =
        mossen_skills::discover_skill_dirs_for_paths(&observed, &cwd_path, CANONICAL_CONFIG_DIR)
            .await;
    let added_skill_count = mossen_skills::add_skill_directories(&discovered).await;
    let observed_strings = observed
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let mut activated_skill_names =
        mossen_skills::activate_conditional_skills_for_paths(&observed_strings, &cwd_path);
    activated_skill_names.sort();
    activated_skill_names.dedup();

    let report = ToolSkillDiscoveryReport {
        observed_path_count: observed.len(),
        discovered_dir_count: discovered.len(),
        added_skill_count,
        activated_skill_names,
    };

    if report.discovered_dir_count > 0
        || report.added_skill_count > 0
        || !report.activated_skill_names.is_empty()
    {
        info!(
            target: "mossen_tools::skills",
            observed_paths = report.observed_path_count,
            discovered_dirs = report.discovered_dir_count,
            added_skills = report.added_skill_count,
            activated = ?report.activated_skill_names,
            "tool path triggered skill discovery"
        );
    }

    report
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn observe_tool_file_paths_discovers_project_skill_dir() {
        let _guard = crate::dynamic_skill_test_lock();
        mossen_skills::clear_dynamic_skills();
        let temp = tempfile::tempdir().expect("tempdir");
        let skill_dir = temp.path().join(".mossen").join("skills").join("reviewer");
        tokio::fs::create_dir_all(&skill_dir)
            .await
            .expect("create skill dir");
        tokio::fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: Review changed files\n---\nReview: $ARGUMENTS\n",
        )
        .await
        .expect("write skill");

        let report = observe_tool_file_paths(
            [temp.path().join("src").join("main.rs").to_string_lossy()],
            &temp.path().to_string_lossy(),
        )
        .await;

        assert_eq!(report.observed_path_count, 1);
        assert_eq!(report.discovered_dir_count, 1);
        assert_eq!(report.added_skill_count, 1);
        assert!(mossen_skills::get_dynamic_skills()
            .iter()
            .any(|skill| skill.base.name == "reviewer"));
        let metadata = report.to_metadata();
        assert_eq!(metadata["skill_discovery"]["rawPathsIncluded"], false);
        assert_eq!(metadata["skill_discovery"]["pathsRedacted"], true);
        mossen_skills::clear_dynamic_skills();
    }
}
