//! English strings — translated from utils/i18n/strings.en.ts

use std::collections::HashMap;
use once_cell::sync::Lazy;

/// English source-of-truth dictionary for Mossen UI
pub static STRINGS_EN: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    // --- cmd.* — command registry metadata ---
    m.insert("cmd.help.description", "Show help and available commands");
    m.insert("cmd.exit.description", "Exit the REPL");
    m.insert("cmd.files.description", "List all files currently in context");
    m.insert("cmd.memory.description", "Edit {product} memory files");
    m.insert("cmd.mcp.description", "Manage MCP servers");
    m.insert("cmd.skills.description", "List available skills");
    m.insert("cmd.hooks.description", "View hook configurations for tool events");
    m.insert("cmd.resume.description", "Resume a previous conversation");
    m.insert("cmd.lang.description", "Quickly switch interface language");
    m.insert("cmd.clear.description", "Clear conversation history and free up context");
    m.insert("cmd.compact.description", "Clear conversation history but keep a summary in context. Optional: /compact [instructions for summarization]");
    m.insert("cmd.diff.description", "View uncommitted changes and per-turn diffs");
    m.insert("cmd.copy.description", "Copy {product}'s last response to clipboard (or /copy N for the Nth-latest)");
    m.insert("cmd.export.description", "Export the current conversation to a file or clipboard");
    m.insert("cmd.branch.description", "Create a branch of the current conversation at this point");
    m.insert("cmd.rename.description", "Rename the current conversation");
    m.insert("cmd.tasks.description", "List and manage background tasks");
    m.insert("cmd.usage.description", "Show plan usage limits");
    m.insert("cmd.rewind.description", "Restore the code and/or conversation to a previous point");
    m.insert("cmd.config.description", "Open config panel");
    m.insert("cmd.theme.description", "Change the theme");
    m.insert("cmd.color.description", "Set the prompt bar color for this session");
    m.insert("cmd.keybindings.description", "Open or create your keybindings configuration file");
    m.insert("cmd.vim.description", "Toggle between Vim and Normal editing modes");
    m.insert("cmd.effort.description", "Set effort level for model usage");
    m.insert("cmd.profile.description", "Set execution and reasoning profiles for personal workflows");
    m.insert("cmd.plan.description", "Enable plan mode or view the current session plan");
    m.insert("cmd.advisor.description", "Configure the advisor model");
    m.insert("cmd.security-review.description", "Complete a security review of the pending changes on the current branch");
    m.insert("cmd.permissions.description", "Manage allow & deny tool permission rules");
    m.insert("cmd.login.description", "Show {product} backend credential setup guidance");
    m.insert("cmd.reload-plugins.description", "Activate pending plugin changes in the current session");
    m.insert("cmd.agents.description", "Manage agent configurations");
    m.insert("cmd.ide.description", "Manage IDE integrations and show status");
    m.insert("cmd.init-verifiers.description", "Create verifier skill(s) for automated verification of code changes");
    m.insert("cmd.add-dir.description", "Add a new working directory");
    m.insert("cmd.btw.description", "Ask a quick side question without interrupting the main conversation");
    // --- ui.* ---
    m.insert("ui.welcome.title", "Welcome to {product}");
    m.insert("ui.taskSummary.tasks", "tasks");
    m.insert("ui.taskSummary.done", "done");
    m.insert("ui.taskSummary.inProgress", "in progress");
    m.insert("ui.taskSummary.open", "open");
    m.insert("ui.taskSummary.pending", "pending");
    m.insert("ui.taskSummary.completed", "completed");
    m.insert("ui.task.blockedByLabel", "blocked by");
    m.insert("ui.taskActivity.stopping", "stopping");
    m.insert("ui.taskActivity.awaitingApproval", "awaiting approval");
    m.insert("ui.taskActivity.idle", "idle");
    m.insert("ui.taskActivity.working", "working");
    // --- lang.* ---
    m.insert("lang.cleared.message", "Interface language preference cleared. Runtime UI now follows your recent conversation or system language.");
    m.insert("lang.current.label", "Current interface language: {language}");
    m.insert("lang.preference.label", "Preference: {preference}");
    m.insert("lang.preference.auto", "Auto");
    m.insert("lang.usage.line", "Usage: /lang [zh|\u{4e2d}\u{6587}|en|english|auto]");
    m.insert("lang.usage.shortcut", "Shortcut: /lang toggle switches between Chinese and English UI.");
    m.insert("lang.usage.note", "Note: /lang changes UI text only. Assistant replies follow the conversation unless you set a response language in /config.");
    m.insert("lang.switched.message", "Interface language switched to English. Assistant replies still follow the conversation language.");
    // --- ui.exit.* / ui.interrupted.* ---
    m.insert("ui.exit.goodbye1", "Goodbye!");
    m.insert("ui.exit.goodbye2", "See ya!");
    m.insert("ui.exit.goodbye3", "Bye!");
    m.insert("ui.exit.goodbye4", "Catch you later!");
    m.insert("ui.interrupted.label", "Interrupted ");
    m.insert("ui.interrupted.hint", "What should {product} do instead?");
    // --- ui.compact.* ---
    m.insert("ui.compact.summarizedTitle", "Summarized conversation");
    m.insert("ui.compact.summarizedDetailUpTo", "Summarized {count} messages up to this point");
    m.insert("ui.compact.summarizedDetailFrom", "Summarized {count} messages from this point");
    m.insert("ui.compact.contextLabel", "Context: ");
    m.insert("ui.compact.summaryTitle", "Compact summary");
    m.insert("ui.compact.expandHistoryHint", "expand history");
    m.insert("ui.compact.expandHint", "expand");
    m
});
