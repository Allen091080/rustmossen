//! `/reload-plugins` — Reload local plugin caches and dynamic skills.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive};

/// Reload plugins directive metadata.
pub struct ReloadPluginsDirective;

/// Result of inspecting installed plugin manifests.
struct RefreshResult {
    enabled_count: usize,
    command_count: usize,
    skill_count: usize,
    agent_count: usize,
    hook_count: usize,
    mcp_count: usize,
    lsp_count: usize,
    error_count: usize,
}

/// Result of refreshing local runtime state owned by the personal CLI process.
struct RuntimeReloadResult {
    user_skill_dir_present: bool,
    project_skill_dir_count: usize,
    added_skill_count: usize,
    activated_skill_count: usize,
    active_dynamic_skill_count: usize,
}

/// Pluralize a noun based on count.
fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("{} {}", count, noun)
    } else {
        format!("{} {}s", count, noun)
    }
}

fn component_count(value: &serde_json::Value) -> usize {
    value
        .as_array()
        .map(|items| items.len())
        .or_else(|| value.as_object().map(|items| items.len()))
        .unwrap_or(0)
}

/// Inspect installed plugin manifests and return counts.
async fn inspect_plugin_manifests(ctx: &CommandContext) -> RefreshResult {
    let plugin_dir = ctx
        .env_vars
        .get("MOSSEN_PLUGIN_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            ctx.env_vars
                .get("HOME")
                .map(|h| std::path::PathBuf::from(h).join(".mossen").join("plugins"))
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp/.mossen/plugins"))
        });

    let mut enabled_count = 0;
    let mut command_count = 0;
    let mut skill_count = 0;
    let mut agent_count = 0;
    let mut hook_count = 0;
    let mut mcp_count = 0;
    let mut lsp_count = 0;
    let mut error_count = 0;

    if let Ok(entries) = std::fs::read_dir(&plugin_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let manifest_path = entry.path().join("manifest.json");
                if manifest_path.exists() {
                    match std::fs::read_to_string(&manifest_path) {
                        Ok(content) => {
                            if let Ok(manifest) =
                                serde_json::from_str::<serde_json::Value>(&content)
                            {
                                enabled_count += 1;
                                if let Some(cmds) = manifest.get("commands") {
                                    command_count += component_count(cmds);
                                }
                                if let Some(skills) = manifest.get("skills") {
                                    skill_count += component_count(skills);
                                }
                                if let Some(agents) = manifest.get("agents") {
                                    agent_count += component_count(agents);
                                }
                                if let Some(hooks) = manifest.get("hooks") {
                                    hook_count += component_count(hooks);
                                }
                                if let Some(mcp) = manifest.get("mcpServers") {
                                    mcp_count += component_count(mcp);
                                }
                                if let Some(lsp) = manifest.get("lspServers") {
                                    lsp_count += component_count(lsp);
                                }
                            } else {
                                error_count += 1;
                            }
                        }
                        Err(_) => {
                            error_count += 1;
                        }
                    }
                }
            }
        }
    }

    RefreshResult {
        enabled_count,
        command_count,
        skill_count,
        agent_count,
        hook_count,
        mcp_count,
        lsp_count,
        error_count,
    }
}

fn clear_runtime_caches() {
    mossen_utils::plugins::installed_plugins_manager::clear_installed_plugins_cache();
    mossen_utils::plugins::load_plugin_commands::clear_plugin_command_cache();
    mossen_utils::plugins::load_plugin_commands::clear_plugin_skills_cache();
    mossen_utils::plugins::load_plugin_agents::clear_plugin_agent_cache();
    mossen_utils::plugins::load_plugin_output_styles::clear_plugin_output_style_cache();
    mossen_utils::plugins::orphaned_plugin_filter::clear_plugin_cache_exclusions();
    mossen_utils::plugins::plugin_options_storage::clear_plugin_options_cache();
    mossen_agent::commands::clear_commands_cache();
    mossen_tools::agent_tool::load_agents_dir::clear_agent_definitions_cache();
    mossen_tools::skill_tool::prompt::clear_prompt_cache();
}

async fn reload_runtime(ctx: &CommandContext) -> RuntimeReloadResult {
    clear_runtime_caches();
    mossen_skills::clear_dynamic_skills();

    let report = mossen_skills::load_startup_skill_directories(&ctx.cwd, ".mossen").await;
    let cwd_path = ctx.cwd.to_string_lossy().to_string();
    let activated = mossen_skills::activate_conditional_skills_for_paths(&[cwd_path], &ctx.cwd);
    let active_dynamic_skill_count = mossen_skills::get_dynamic_skills().len();

    RuntimeReloadResult {
        user_skill_dir_present: report.user_dir_present,
        project_skill_dir_count: report.project_dir_count,
        added_skill_count: report.added_skill_count,
        activated_skill_count: activated.len(),
        active_dynamic_skill_count,
    }
}

#[async_trait]
impl Directive for ReloadPluginsDirective {
    fn name(&self) -> &str {
        "reload-plugins"
    }

    fn description(&self) -> &str {
        "Reload local plugin caches and skills"
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let r = inspect_plugin_manifests(ctx).await;
        let runtime = reload_runtime(ctx).await;

        let parts = [
            plural(r.enabled_count, "plugin"),
            plural(r.command_count, "plugin command"),
            plural(r.skill_count, "plugin skill"),
            plural(r.agent_count, "agent"),
            plural(r.hook_count, "hook"),
            plural(r.mcp_count, "plugin MCP server"),
            plural(r.lsp_count, "plugin LSP server"),
        ];

        let mut msg = format!(
            "Plugin/skill runtime reloaded: {}\nLocal skills: {} project dir(s), user dir {}, {} loaded, {} activated, {} active.",
            parts.join(" · "),
            runtime.project_skill_dir_count,
            if runtime.user_skill_dir_present { "present" } else { "absent" },
            plural(runtime.added_skill_count, "skill"),
            runtime.activated_skill_count,
            runtime.active_dynamic_skill_count
        );

        if r.error_count > 0 {
            msg.push_str(&format!(
                "\n{} during load. Run /doctor for details.",
                plural(r.error_count, "error")
            ));
        }

        Ok(CommandResult::Text(msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CommandContext;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_context(cwd: PathBuf) -> CommandContext {
        CommandContext {
            cwd,
            is_non_interactive: true,
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

    #[test]
    fn reload_plugins_refreshes_project_skill_inventory() {
        let _lock = crate::test_support::skill_state_lock();
        mossen_skills::clear_dynamic_skills();
        let temp = tempfile::tempdir().expect("tempdir");
        let skill_dir = temp
            .path()
            .join(".mossen")
            .join("skills")
            .join("live-reload-skill");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: Live reload skill\n---\nUse this skill after reload.\n",
        )
        .expect("write skill");

        assert!(mossen_skills::get_dynamic_skills().is_empty());
        let output = tokio_test::block_on(
            ReloadPluginsDirective.execute(&[], &test_context(temp.path().to_path_buf())),
        )
        .expect("reload-plugins command");

        let CommandResult::Text(text) = output else {
            panic!("reload-plugins should return text");
        };
        assert!(text.contains("runtime reloaded"), "{text}");
        assert!(text.contains("1 project dir"), "{text}");
        assert!(text.contains("1 skill loaded"), "{text}");
        assert!(text.contains("1 active"), "{text}");
        assert!(!text.contains("not reloaded"), "{text}");

        mossen_skills::clear_dynamic_skills();
    }
}
