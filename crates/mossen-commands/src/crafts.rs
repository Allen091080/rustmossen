//! `/skills` — Manage and browse skills.
//!
//! Translates `commands/skills/skills.tsx` (18 lines) and related files.
//! Lists installed skills, their sources, and provides management options.

use anyhow::Result;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

const GITHUB_API_ACCEPT: &str = "application/vnd.github+json";
const GITHUB_USER_AGENT: &str = "mossen-skills";

/// `/skills` command.
pub struct CraftsDirective;

#[async_trait]
impl Directive for CraftsDirective {
    fn name(&self) -> &str {
        "skills"
    }

    fn description(&self) -> &str {
        "Manage and browse skills"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[list|install|remove]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let subcommand = args.first().map(|s| s.to_lowercase());

        match subcommand.as_deref() {
            Some("list") | None => {
                // List installed skills
                let cwd = &ctx.cwd;
                let skills_dir = cwd.join(".mossen").join("skills");

                let mut output = String::from("Skills\n\n");

                if skills_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(&skills_dir) {
                        let mut skill_count = 0;
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_dir() {
                                let skill_md = path.join("SKILL.md");
                                if skill_md.exists() {
                                    let name = path
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("unknown");
                                    output.push_str(&format!("  • {} (local)\n", name));
                                    skill_count += 1;
                                }
                            }
                        }
                        if skill_count == 0 {
                            output.push_str("  No skills installed.\n");
                        }
                    } else {
                        output.push_str("  No skills installed.\n");
                    }
                } else {
                    output.push_str("  No skills directory found.\n");
                }

                output.push_str("\nSkills are on-demand capabilities invoked with /skill-name.\n");
                output.push_str("Create skills at .mossen/skills/<name>/SKILL.md\n");
                output.push_str("or install from plugins with /plugin install <name>.");

                Ok(CommandResult::Text(output))
            }

            Some("install") => {
                let target = args.get(1..).unwrap_or(&[]).join(" ");
                if target.trim().is_empty() {
                    Ok(CommandResult::Error(
                        "Usage: /skills install <skill-name-or-url>\n\
                         Install a skill from a GitHub repository or plugin."
                            .to_string(),
                    ))
                } else {
                    let install_root = ctx.cwd.join(".mossen").join("skills");
                    install_github_skill_from_target(
                        &target,
                        &install_root,
                        fetch_github_skill_tree,
                    )
                    .await
                }
            }

            Some("remove") => {
                let name = args.get(1..).unwrap_or(&[]).join(" ");
                if name.trim().is_empty() {
                    Ok(CommandResult::Error(
                        "Usage: /skills remove <skill-name>".to_string(),
                    ))
                } else {
                    remove_project_skill(&name, &ctx.cwd).await
                }
            }

            Some("help" | "-h" | "--help") => Ok(CommandResult::Text(
                "Usage: /skills [list|install|remove]\n\n\
                 Manage skills — on-demand capabilities for the assistant.\n\n\
                 Subcommands:\n\
                   list              List installed skills (default)\n\
                   install <name>    Install a skill from a plugin or URL\n\
                   remove <name>     Remove an installed skill"
                    .to_string(),
            )),

            Some(unknown) => Ok(CommandResult::Error(format!(
                "Unknown subcommand: \"{}\". Use /skills help.",
                unknown
            ))),
        }
    }
}

async fn install_github_skill_from_target<F, Fut>(
    target: &str,
    install_root: &Path,
    fetch_tree: F,
) -> Result<CommandResult>
where
    F: FnOnce(mossen_utils::skills_utils::GitHubSkillInstallTarget) -> Fut,
    Fut: std::future::Future<
        Output = Result<(
            Vec<mossen_utils::skills_utils::GitHubSkillInstallFile>,
            HashMap<String, Value>,
        )>,
    >,
{
    let install_root = install_root.to_string_lossy().to_string();
    let Some(plan) = mossen_utils::skills_utils::get_github_skill_install_plan(
        target,
        &install_root,
        fetch_tree,
    )
    .await?
    else {
        return Ok(CommandResult::Error(format!(
            "No installable skill found at {target}. Point to a GitHub repository or folder containing SKILL.md."
        )));
    };

    let result = mossen_utils::skills_utils::execute_github_skill_install_plan(&plan).await?;
    match result {
        mossen_utils::skills_utils::GitHubSkillInstallResult::Installed {
            skill_name,
            install_dir,
            files_written,
            total_bytes,
            warnings,
        } => {
            if let Some(root) = Path::new(&install_dir).parent() {
                let _ = mossen_skills::add_skill_directories(&[root.to_path_buf()]).await;
            }
            let mut output = format!(
                "Installed skill /{skill_name}\npath: {install_dir}\nfiles: {files_written}\nbytes: {total_bytes}"
            );
            if !warnings.is_empty() {
                output.push_str("\nwarnings:");
                for warning in warnings {
                    output.push_str(&format!("\n- {warning}"));
                }
            }
            Ok(CommandResult::System(output))
        }
        mossen_utils::skills_utils::GitHubSkillInstallResult::AlreadyExists { install_dir } => {
            Ok(CommandResult::Error(format!(
                "Skill already exists at {install_dir}. Remove it first or choose another skill name."
            )))
        }
        mossen_utils::skills_utils::GitHubSkillInstallResult::UnknownToken => {
            Ok(CommandResult::Error(
                "GitHub skill install plan expired or was already used.".to_string(),
            ))
        }
        mossen_utils::skills_utils::GitHubSkillInstallResult::ExpiredToken => {
            Ok(CommandResult::Error(
                "GitHub skill install plan expired. Run /skills install again.".to_string(),
            ))
        }
        mossen_utils::skills_utils::GitHubSkillInstallResult::InvalidTarget { reason } => {
            Ok(CommandResult::Error(format!(
                "GitHub skill install target changed during install: {reason}"
            )))
        }
    }
}

async fn remove_project_skill(name: &str, cwd: &Path) -> Result<CommandResult> {
    let skill_name = mossen_utils::skills_utils::to_skill_slug(name);
    if skill_name.is_empty() {
        return Ok(CommandResult::Error(
            "Usage: /skills remove <skill-name>".to_string(),
        ));
    }

    let root = cwd.join(".mossen").join("skills");
    let skill_dir = root.join(&skill_name);
    if !skill_dir.starts_with(&root) {
        return Ok(CommandResult::Error(
            "Skill name resolves outside the project skills directory.".to_string(),
        ));
    }
    if tokio::fs::metadata(skill_dir.join("SKILL.md"))
        .await
        .is_err()
    {
        return Ok(CommandResult::Error(format!(
            "Project skill /{skill_name} is not installed at {}.",
            skill_dir.display()
        )));
    }

    tokio::fs::remove_dir_all(&skill_dir).await?;
    mossen_utils::skills_utils::SKILL_CHANGE_DETECTOR.notify_change(&skill_dir.to_string_lossy());
    mossen_skills::clear_dynamic_skills();
    let _ = mossen_skills::load_startup_skill_directories(cwd, ".mossen").await;

    Ok(CommandResult::System(format!(
        "Removed skill /{skill_name}\npath: {}",
        skill_dir.display()
    )))
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum GitHubContentsResponse {
    File(GitHubContentItem),
    Directory(Vec<GitHubContentItem>),
}

#[derive(Debug, Deserialize)]
struct GitHubContentItem {
    #[serde(rename = "type")]
    kind: String,
    path: String,
    size: Option<usize>,
    download_url: Option<String>,
}

async fn fetch_github_skill_tree(
    target: mossen_utils::skills_utils::GitHubSkillInstallTarget,
) -> Result<(
    Vec<mossen_utils::skills_utils::GitHubSkillInstallFile>,
    HashMap<String, Value>,
)> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let headers = github_headers();
    let root_path = normalize_github_skill_path(&target.path);
    let mut files = Vec::new();
    fetch_github_contents_recursive(
        &client, &headers, &target, &root_path, &root_path, &mut files,
    )
    .await?;

    let frontmatter = files
        .iter()
        .find(|file| file.path == "SKILL.md" || file.path.ends_with("/SKILL.md"))
        .and_then(|file| file.content.as_deref())
        .map(parse_skill_frontmatter_json)
        .unwrap_or_default();
    Ok((files, frontmatter))
}

async fn fetch_github_contents_recursive(
    client: &reqwest::Client,
    headers: &HeaderMap,
    target: &mossen_utils::skills_utils::GitHubSkillInstallTarget,
    api_path: &str,
    root_path: &str,
    files: &mut Vec<mossen_utils::skills_utils::GitHubSkillInstallFile>,
) -> Result<()> {
    let response = fetch_github_contents(client, headers, target, api_path).await?;
    match response {
        GitHubContentsResponse::File(item) => {
            push_github_skill_file(client, headers, item, root_path, files).await?;
        }
        GitHubContentsResponse::Directory(items) => {
            for item in items {
                match item.kind.as_str() {
                    "file" => {
                        push_github_skill_file(client, headers, item, root_path, files).await?
                    }
                    "dir" => {
                        let path = item.path.clone();
                        Box::pin(fetch_github_contents_recursive(
                            client, headers, target, &path, root_path, files,
                        ))
                        .await?;
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

async fn fetch_github_contents(
    client: &reqwest::Client,
    headers: &HeaderMap,
    target: &mossen_utils::skills_utils::GitHubSkillInstallTarget,
    api_path: &str,
) -> Result<GitHubContentsResponse> {
    let path_suffix = if api_path.is_empty() {
        String::new()
    } else {
        format!("/{}", encode_github_path(api_path))
    };
    let url = format!(
        "https://api.github.com/repos/{}/{}/contents{}",
        target.owner, target.repo, path_suffix
    );
    let mut request = client.get(url).headers(headers.clone());
    if let Some(ref_name) = target.ref_name.as_deref() {
        request = request.query(&[("ref", ref_name)]);
    }
    Ok(request
        .send()
        .await?
        .error_for_status()?
        .json::<GitHubContentsResponse>()
        .await?)
}

async fn push_github_skill_file(
    client: &reqwest::Client,
    headers: &HeaderMap,
    item: GitHubContentItem,
    root_path: &str,
    files: &mut Vec<mossen_utils::skills_utils::GitHubSkillInstallFile>,
) -> Result<()> {
    let Some(download_url) = item.download_url.clone() else {
        return Ok(());
    };
    let Some(relative_path) = relative_skill_file_path(root_path, &item.path) else {
        return Ok(());
    };
    let bytes = client
        .get(&download_url)
        .headers(headers.clone())
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec();
    files.push(mossen_utils::skills_utils::GitHubSkillInstallFile {
        path: relative_path,
        size_bytes: item.size.unwrap_or(bytes.len()),
        download_url,
        content: Some(bytes),
    });
    Ok(())
}

fn github_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(GITHUB_USER_AGENT));
    headers.insert(ACCEPT, HeaderValue::from_static(GITHUB_API_ACCEPT));
    let token = std::env::var("GITHUB_TOKEN")
        .ok()
        .or_else(|| std::env::var("GH_TOKEN").ok());
    if let Some(token) = token.filter(|value| !value.trim().is_empty()) {
        if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", token.trim())) {
            headers.insert(AUTHORIZATION, value);
        }
    }
    headers
}

fn normalize_github_skill_path(path: &str) -> String {
    path.trim().trim_matches('/').replace('\\', "/")
}

fn encode_github_path(path: &str) -> String {
    path.split('/')
        .map(|part| url::form_urlencoded::byte_serialize(part.as_bytes()).collect::<String>())
        .collect::<Vec<_>>()
        .join("/")
}

fn relative_skill_file_path(root_path: &str, file_path: &str) -> Option<String> {
    let root = normalize_github_skill_path(root_path);
    let file = normalize_github_skill_path(file_path);
    if file.is_empty() {
        return None;
    }
    if root.is_empty() {
        return Some(file);
    }
    if file == root {
        return Path::new(&file)
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string());
    }
    file.strip_prefix(&format!("{root}/"))
        .map(|relative| relative.to_string())
}

fn parse_skill_frontmatter_json(bytes: &[u8]) -> HashMap<String, Value> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return HashMap::new();
    };
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return HashMap::new();
    }
    let mut yaml = String::new();
    for line in lines {
        if line.trim() == "---" {
            let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&yaml) else {
                return HashMap::new();
            };
            let Ok(Value::Object(map)) = serde_json::to_value(value) else {
                return HashMap::new();
            };
            return map.into_iter().collect();
        }
        yaml.push_str(line);
        yaml.push('\n');
    }
    HashMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context(cwd: PathBuf) -> CommandContext {
        CommandContext {
            cwd,
            is_non_interactive: false,
            is_remote_mode: false,
            is_custom_backend: false,
            user_type: None,
            env_vars: HashMap::new(),
            product_name: "Mossen".to_string(),
            cli_name: "mossen".to_string(),
            version: "test".to_string(),
            build_time: None,
            cost_snapshot: Default::default(),
        }
    }

    fn skill_file(path: &str, body: &str) -> mossen_utils::skills_utils::GitHubSkillInstallFile {
        mossen_utils::skills_utils::GitHubSkillInstallFile {
            path: path.to_string(),
            size_bytes: body.len(),
            download_url: format!("https://raw.invalid/{path}"),
            content: Some(body.as_bytes().to_vec()),
        }
    }

    #[tokio::test]
    async fn skills_install_executes_github_plan_and_loads_skill() {
        let _guard = crate::test_support::skill_state_lock();
        mossen_utils::skills_utils::reset_github_skill_install_plan_store_for_testing();
        mossen_skills::clear_dynamic_skills();
        let temp = tempfile::tempdir().expect("tempdir");
        let install_root = temp.path().join(".mossen").join("skills");

        let result = install_github_skill_from_target(
            "https://github.com/example/skills/tree/main/review-skill",
            &install_root,
            |_target| async move {
                let mut frontmatter = HashMap::new();
                frontmatter.insert("name".to_string(), Value::String("Review Skill".to_string()));
                frontmatter.insert(
                    "description".to_string(),
                    Value::String("Review changes".to_string()),
                );
                Ok((
                    vec![skill_file(
                        "SKILL.md",
                        "---\nname: Review Skill\ndescription: Review changes\n---\nReview $ARGUMENTS\n",
                    )],
                    frontmatter,
                ))
            },
        )
        .await
        .expect("install command");

        let CommandResult::System(text) = result else {
            panic!("expected install system result");
        };
        assert!(text.contains("Installed skill /review-skill"), "{text}");
        assert!(install_root.join("review-skill").join("SKILL.md").exists());
        assert!(mossen_skills::get_dynamic_skills()
            .iter()
            .any(|skill| skill.name() == "review-skill"));

        mossen_skills::clear_dynamic_skills();
        mossen_utils::skills_utils::reset_github_skill_install_plan_store_for_testing();
    }

    #[tokio::test]
    async fn skills_install_rejects_target_without_skill_md() {
        let _guard = crate::test_support::skill_state_lock();
        mossen_utils::skills_utils::reset_github_skill_install_plan_store_for_testing();
        let temp = tempfile::tempdir().expect("tempdir");
        let install_root = temp.path().join(".mossen").join("skills");

        let result = install_github_skill_from_target(
            "example/skills",
            &install_root,
            |_target| async move {
                Ok((
                    vec![skill_file("README.md", "# no skill\n")],
                    HashMap::new(),
                ))
            },
        )
        .await
        .expect("install command");

        let CommandResult::Error(text) = result else {
            panic!("expected install error");
        };
        assert!(text.contains("No installable skill found"), "{text}");
        mossen_utils::skills_utils::reset_github_skill_install_plan_store_for_testing();
    }

    #[tokio::test]
    async fn skills_remove_deletes_project_skill_and_refreshes_inventory() {
        let _guard = crate::test_support::skill_state_lock();
        mossen_skills::clear_dynamic_skills();
        let temp = tempfile::tempdir().expect("tempdir");
        let skill_dir = temp.path().join(".mossen").join("skills").join("obsolete");
        tokio::fs::create_dir_all(&skill_dir)
            .await
            .expect("create skill dir");
        tokio::fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: Old skill\n---\nOld body\n",
        )
        .await
        .expect("write skill");
        let _ = mossen_skills::load_startup_skill_directories(temp.path(), ".mossen").await;
        assert!(mossen_skills::get_dynamic_skills()
            .iter()
            .any(|skill| skill.name() == "obsolete"));

        let ctx = test_context(temp.path().to_path_buf());
        let result = CraftsDirective
            .execute(&["remove", "obsolete"], &ctx)
            .await
            .expect("remove command");

        let CommandResult::System(text) = result else {
            panic!("expected remove system result");
        };
        assert!(text.contains("Removed skill /obsolete"), "{text}");
        assert!(!skill_dir.exists());
        assert!(!mossen_skills::get_dynamic_skills()
            .iter()
            .any(|skill| skill.name() == "obsolete"));
        mossen_skills::clear_dynamic_skills();
    }

    #[test]
    fn relative_skill_file_path_strips_selected_dir() {
        assert_eq!(
            relative_skill_file_path("skills/review-skill", "skills/review-skill/SKILL.md"),
            Some("SKILL.md".to_string())
        );
        assert_eq!(
            relative_skill_file_path("skills/review-skill", "skills/review-skill/assets/a.txt"),
            Some("assets/a.txt".to_string())
        );
        assert_eq!(
            relative_skill_file_path(
                "skills/review-skill/SKILL.md",
                "skills/review-skill/SKILL.md"
            ),
            Some("SKILL.md".to_string())
        );
    }

    #[test]
    fn parse_skill_frontmatter_extracts_scalar_fields() {
        let parsed = parse_skill_frontmatter_json(
            b"---\nname: Review Skill\ndescription: Review changes\nallowed-tools:\n  - Read\n---\nBody\n",
        );
        assert_eq!(
            parsed.get("name").and_then(Value::as_str),
            Some("Review Skill")
        );
        assert_eq!(
            parsed.get("description").and_then(Value::as_str),
            Some("Review changes")
        );
        assert!(parsed
            .get("allowed-tools")
            .and_then(Value::as_array)
            .is_some());
    }
}
