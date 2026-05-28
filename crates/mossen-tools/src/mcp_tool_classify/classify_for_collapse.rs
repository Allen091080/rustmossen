//! MCP tool classification for UI collapsing.
//!
//! Translated from tools/MCPTool/classifyForCollapse.ts
//!
//! Classifies an MCP tool as a search/read operation for UI collapsing.
//! Returns { is_search: false, is_read: false } for tools that should not
//! collapse (e.g., send_message, create_*, update_*).
//!
//! Uses explicit per-tool allowlists for the most common MCP servers.

use std::collections::HashSet;
use std::sync::LazyLock;

static SEARCH_TOOLS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let tools: &[&str] = &[
        // Slack
        "slack_search_public",
        "slack_search_public_and_private",
        "slack_search_channels",
        "slack_search_users",
        // GitHub
        "search_code",
        "search_repositories",
        "search_issues",
        "search_pull_requests",
        "search_orgs",
        "search_users",
        // Linear
        "search_documentation",
        // Datadog
        "search_logs",
        "search_spans",
        "search_rum_events",
        "search_audit_logs",
        "search_monitors",
        "search_monitor_groups",
        "find_slow_spans",
        "find_monitors_matching_pattern",
        // Sentry
        "search_docs",
        "search_events",
        "search_issue_events",
        "find_organizations",
        "find_teams",
        "find_projects",
        "find_releases",
        "find_dsns",
        // Notion
        "search",
        // Gmail/GDrive/GCal
        "gmail_search_messages",
        "google_drive_search",
        "gcal_find_my_free_time",
        "gcal_find_meeting_times",
        "gcal_find_user_emails",
        // Atlassian/Jira
        "search_jira_issues_using_jql",
        "search_confluence_using_cql",
        "lookup_jira_account_id",
        "confluence_search",
        "jira_search",
        "jira_search_fields",
        // Asana
        "asana_search_tasks",
        "asana_typeahead_search",
        // Filesystem
        "search_files",
        // Memory
        "search_nodes",
        // Brave Search
        "brave_web_search",
        "brave_local_search",
        // Grafana
        "search_dashboards",
        "search_folders",
        // Supabase
        "search_docs",
        // Stripe
        "search_stripe_resources",
        "search_stripe_documentation",
        // PubMed
        "search_articles",
        "find_related_articles",
        "lookup_article_by_citation",
        "search_papers",
        "search_pubmed",
        "search_pubmed_key_words",
        "search_pubmed_advanced",
        "pubmed_search",
        "pubmed_mesh_lookup",
        // Firecrawl
        "firecrawl_search",
        // Exa
        "web_search_exa",
        "web_search_advanced_exa",
        "people_search_exa",
        "linkedin_search_exa",
        "deep_search_exa",
        // Perplexity
        "perplexity_search",
        "perplexity_search_web",
        // Tavily
        "tavily_search",
        // Obsidian
        "obsidian_simple_search",
        "obsidian_complex_search",
        // MongoDB
        "find",
        "search_knowledge",
        // Neo4j
        "search_memories",
        "find_memories_by_name",
        // Airtable
        "search_records",
        // Todoist
        "find_tasks",
        "find_tasks_by_date",
        "find_completed_tasks",
        "find_projects",
        "find_sections",
        "find_comments",
        "find_project_collaborators",
        "find_activity",
        "find_labels",
        "find_filters",
        // AWS
        "search_catalog",
        // Terraform
        "search_modules",
        "search_providers",
        "search_policies",
    ];
    tools.iter().copied().collect()
});

static READ_TOOLS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let tools: &[&str] = &[
        // Slack
        "slack_read_channel",
        "slack_read_thread",
        "slack_read_canvas",
        "slack_read_user_profile",
        "slack_list_channels",
        "slack_get_channel_history",
        "slack_get_thread_replies",
        "slack_get_users",
        "slack_get_user_profile",
        // GitHub
        "get_me",
        "get_team_members",
        "get_teams",
        "get_commit",
        "get_file_contents",
        "get_repository_tree",
        "list_branches",
        "list_commits",
        "list_releases",
        "list_tags",
        "get_latest_release",
        "get_release_by_tag",
        "get_tag",
        "list_issues",
        "issue_read",
        "list_issue_types",
        "get_label",
        "list_label",
        "pull_request_read",
        "get_gist",
        "list_gists",
        "list_notifications",
        "get_notification_details",
        "projects_list",
        "projects_get",
        "actions_get",
        "actions_list",
        "get_job_logs",
        "get_code_scanning_alert",
        "list_code_scanning_alerts",
        "get_dependabot_alert",
        "list_dependabot_alerts",
        "get_secret_scanning_alert",
        "list_secret_scanning_alerts",
        "get_global_security_advisory",
        "list_global_security_advisories",
        "list_org_repository_security_advisories",
        "list_repository_security_advisories",
        "get_discussion",
        "get_discussion_comments",
        "list_discussion_categories",
        "list_discussions",
        "list_starred_repositories",
        "get_issue",
        "get_pull_request",
        "list_pull_requests",
        "get_pull_request_files",
        "get_pull_request_status",
        "get_pull_request_comments",
        "get_pull_request_reviews",
        // Linear
        "list_comments",
        "list_cycles",
        "get_document",
        "list_documents",
        "list_issue_statuses",
        "get_issue_status",
        "list_my_issues",
        "list_issue_labels",
        "list_projects",
        "get_project",
        "list_project_labels",
        "list_teams",
        "get_team",
        "list_users",
        "get_user",
        // Datadog
        "aggregate_logs",
        "list_spans",
        "aggregate_spans",
        "analyze_trace",
        "trace_critical_path",
        "query_metrics",
        "aggregate_rum_events",
        "list_rum_metrics",
        "get_rum_metric",
        "list_monitors",
        "get_monitor",
        "check_can_delete_monitor",
        "validate_monitor",
        "validate_existing_monitor",
        "list_dashboards",
        "get_dashboard",
        "query_dashboard_widget",
        "list_notebooks",
        "get_notebook",
        "query_notebook_cell",
        "get_profiling_metrics",
        "compare_profiling_metrics",
        // Sentry
        "whoami",
        "get_issue_details",
        "get_issue_tag_values",
        "get_trace_details",
        "get_event_attachment",
        "get_doc",
        "get_sentry_resource",
        "list_events",
        "list_issue_events",
        "get_sentry_issue",
        // Notion
        "fetch",
        "get_comments",
        "get_users",
        "get_self",
        // Gmail
        "gmail_get_profile",
        "gmail_read_message",
        "gmail_read_thread",
        "gmail_list_drafts",
        "gmail_list_labels",
        // Google Drive / Calendar
        "google_drive_fetch",
        "google_drive_export",
        "gcal_list_calendars",
        "gcal_list_events",
        "gcal_get_event",
        // Atlassian
        "atlassian_user_info",
        "get_accessible_atlassian_resources",
        "get_visible_jira_projects",
        "get_jira_project_issue_types_metadata",
        "get_jira_issue",
        "get_transitions_for_jira_issue",
        "get_jira_issue_remote_issue_links",
        "get_confluence_spaces",
        "get_confluence_page",
        "get_pages_in_confluence_space",
        "get_confluence_page_ancestors",
        "get_confluence_page_descendants",
        "get_confluence_page_footer_comments",
        "get_confluence_page_inline_comments",
        "confluence_get_page",
        "confluence_get_page_children",
        "confluence_get_comments",
        "confluence_get_labels",
        "jira_get_issue",
        "jira_get_transitions",
        "jira_get_worklog",
        "jira_get_agile_boards",
        "jira_get_board_issues",
        "jira_get_sprints_from_board",
        "jira_get_sprint_issues",
        "jira_get_link_types",
        "jira_download_attachments",
        "jira_batch_get_changelogs",
        "jira_get_user_profile",
        "jira_get_project_issues",
        "jira_get_project_versions",
        // Filesystem
        "read_file",
        "read_text_file",
        "read_media_file",
        "read_multiple_files",
        "list_directory",
        "list_directory_with_sizes",
        "directory_tree",
        "get_file_info",
        "list_allowed_directories",
        // Memory
        "read_graph",
        "open_nodes",
        // Postgres / SQLite
        "query",
        "read_query",
        "list_tables",
        "describe_table",
        // Git
        "git_status",
        "git_diff",
        "git_diff_unstaged",
        "git_diff_staged",
        "git_log",
        "git_show",
        "git_branch",
        // Grafana (partial list)
        "list_users_by_org",
        "get_dashboard_by_uid",
        "get_dashboard_summary",
        "query_prometheus",
        "query_loki_logs",
        "list_incidents",
        "get_incident",
        // Stripe
        "get_stripe_account_info",
        "retrieve_balance",
        "list_customers",
        "list_products",
        "list_prices",
        "list_invoices",
        "list_payment_intents",
        "list_subscriptions",
        // MongoDB
        "list_databases",
        "list_collections",
        "collection_indexes",
        "collection_schema",
        "db_stats",
        "explain",
        "aggregate",
        "count",
        "export",
        // Playwright
        "browser_console_messages",
        "browser_network_requests",
        "browser_take_screenshot",
        "browser_snapshot",
    ];
    tools.iter().copied().collect()
});

/// Normalize a tool name from camelCase/kebab-case to snake_case.
fn normalize(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 4);
    let chars: Vec<char> = name.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 && chars[i - 1].is_ascii_lowercase() {
            result.push('_');
            result.push(ch.to_ascii_lowercase());
        } else if ch == '-' {
            result.push('_');
        } else {
            result.push(ch.to_ascii_lowercase());
        }
    }
    result
}

/// Classification result for MCP tool UI collapsing.
pub struct McpToolClassification {
    pub is_search: bool,
    pub is_read: bool,
}

/// Classify an MCP tool as search/read for UI collapsing.
pub fn classify_mcp_tool_for_collapse(
    _server_name: &str,
    tool_name: &str,
) -> McpToolClassification {
    let normalized = normalize(tool_name);
    McpToolClassification {
        is_search: SEARCH_TOOLS.contains(normalized.as_str()),
        is_read: READ_TOOLS.contains(normalized.as_str()),
    }
}
