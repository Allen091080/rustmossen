//! # mossen-utils
//!
//! Mossen 通用工具库 — 提供文件操作、Git 集成、进程管理、加密、
//! 网络请求、文本处理等基础功能函数。

#![allow(
    dead_code,
    deprecated,
    non_upper_case_globals,
    private_interfaces,
    unexpected_cfgs,
    unused_assignments,
    unused_doc_comments,
    unused_imports,
    unused_must_use,
    unused_mut,
    unused_unsafe,
    unused_variables,
    clippy::await_holding_lock,
    clippy::collapsible_match,
    clippy::doc_lazy_continuation,
    clippy::field_reassign_with_default,
    clippy::if_same_then_else,
    clippy::inherent_to_string,
    clippy::large_enum_variant,
    clippy::len_without_is_empty,
    clippy::let_underscore_future,
    clippy::manual_clamp,
    clippy::manual_strip,
    clippy::match_like_matches_macro,
    clippy::module_inception,
    clippy::needless_range_loop,
    clippy::only_used_in_recursion,
    clippy::redundant_guards,
    clippy::redundant_pattern_matching,
    clippy::regex_creation_in_loops,
    clippy::same_item_push,
    clippy::should_implement_trait,
    clippy::single_match,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::unnecessary_get_then_check,
    clippy::unnecessary_sort_by,
    clippy::unnecessary_unwrap,
    clippy::vec_init_then_push,
    clippy::while_let_loop
)]

pub mod abort_controller;
pub mod activity_manager;
pub mod advisor;
pub mod agent_context;
pub mod agent_id;
pub mod agent_swarms_enabled;
pub mod agentic_session_search;
pub mod analyze_context;
pub mod ansi_to_png;
pub mod ansi_to_svg;
pub mod api;
pub mod api_preconnect;
pub mod apple_terminal_backup;
pub mod argument_substitution;
pub mod array;
pub mod asciicast;
pub mod async_utils;
pub mod attachments;
pub mod attribution;
pub mod auth;
pub mod auth_file_descriptor;
pub mod auth_portable;
pub mod auto_mode_denials;
pub mod auto_run_issue;
pub mod auto_updater;
pub mod aws;
pub mod aws_auth_status_manager;
pub mod background;
pub mod background_housekeeping;
pub mod bash;
pub mod betas;
pub mod billing;
pub mod binary_check;
pub mod binary_check_utils;
pub mod browser;
pub mod buffered_writer;
pub mod bundled_mode;
pub mod ca_certs;
pub mod ca_certs_config;
pub mod cache_paths;
pub mod change_detector;
pub mod circular_buffer;
pub mod classifier_approvals;
pub mod classifier_approvals_hook;
pub mod cleanup;
pub mod cleanup_registry;
pub mod cli_args;
pub mod cli_highlight;
pub mod cli_highlight_utils;
pub mod code_indexing;
pub mod collapse_background_bash_notifications;
pub mod collapse_hook_summaries;
pub mod collapse_read_search;
pub mod collapse_teammate_shutdowns;
pub mod combined_abort_signal;
pub mod command_description;
pub mod command_lifecycle;
pub mod commit_attribution;
pub mod completion_cache;
pub mod computer_use;
pub mod concurrent_sessions;
pub mod config;
pub mod config_constants;
pub mod content_array;
pub mod context;
pub mod context_analysis;
pub mod context_suggestions;
pub mod control_message_compat;
pub mod conversation_recovery;
pub mod cron;
pub mod cron_jitter_config;
pub mod cron_scheduler;
pub mod cron_tasks;
pub mod cron_tasks_lock;
pub mod cross_project_resume;
pub mod crypto;
pub mod cursor;
pub mod custom_backend;
pub mod cwd;
pub mod debug;
pub mod debug_filter;
pub mod debug_utils;
pub mod deep_link;
pub mod deferred_slash_commands;
pub mod desktop_deep_link;
pub mod detect_repository;
pub mod diag_logs;
pub mod diff;
pub mod direct_member_message;
pub mod display_tags;
pub mod display_tags_utils;
pub mod doctor_context_warnings;
pub mod doctor_diagnostic;
pub mod dxt;
pub mod early_input;
pub mod editor;
pub mod effort;
pub mod embedded_tools;
pub mod env;
pub mod env_detection;
pub mod env_dynamic;
pub mod env_utils;
pub mod env_validation;
pub mod error;
pub mod error_log_sink;
pub mod errors;
pub mod errors_utils;
pub mod example_commands;
pub mod exec_file_no_throw;
pub mod exec_file_no_throw_portable;
pub mod exec_sync_wrapper;
pub mod extra_usage;
pub mod fast_mode;
pub mod file;
pub mod file_history;
pub mod file_operation_analytics;
pub mod file_persistence;
pub mod file_read;
pub mod file_read_cache;
pub mod file_state_cache;
pub mod file_utils;
pub mod find_executable;
pub mod fingerprint;
pub mod forked_agent;
pub mod format;
pub mod format_brief_timestamp;
pub mod fps_tracker;
pub mod frontmatter_parser;
pub mod fs;
pub mod fs_operations;
pub mod fullscreen;
pub mod generated_files;
pub mod generators;
pub mod generic_process_utils;
pub mod get_worktree_paths;
pub mod get_worktree_paths_portable;
pub mod gh_pr_status;
pub mod git;
pub mod git_diff;
pub mod git_settings;
pub mod git_utils;
pub mod github;
pub mod github_repo_path_mapping;
pub mod glob;
pub mod glob_match;
pub mod graceful_shutdown;
pub mod group_tool_uses;
pub mod handle_prompt_submit;
pub mod hash;
pub mod headless_profiler;
pub mod heap_dump_service;
pub mod heatmap;
pub mod highlight_match;
pub mod hooks;
pub mod hooks_dir;
pub mod hooks_utils;
pub mod horizontal_scroll;
pub mod hosted_feature_gates;
pub mod http;
pub mod hyperlink;
pub mod i18n;
pub mod i_term_backup;
pub mod ide;
pub mod ide_path_conversion;
pub mod idle_timeout;
pub mod idle_timeout_utils;
pub mod image_paste;
pub mod image_resizer;
pub mod image_store;
pub mod image_validation;
pub mod immediate_command;
pub mod in_process_teammate_helpers;
pub mod internal_logging;
pub mod intl;
pub mod iterm_backup;
pub mod jetbrains;
pub mod json;
pub mod json_read;
pub mod json_utils;
pub mod keyboard_shortcuts;
pub mod lazy_schema;
pub mod list_sessions_impl;
pub mod local_installer;
pub mod lockfile;
pub mod log;
pub mod log_service;
pub mod logging;
pub mod logo_v2_utils;
pub mod mailbox;
pub mod managed_env;
pub mod managed_env_constants;
pub mod markdown;
pub mod markdown_config_loader;
pub mod markdown_render;
pub mod mcp_instructions_delta;
pub mod mcp_output_storage;
pub mod mcp_utils;
pub mod mcp_validation;
pub mod mcp_web_socket_transport;
pub mod mcp_websocket_transport;
pub mod memoize;
pub mod memory_file_detection;
pub mod memory_types;
pub mod message_predicates;
pub mod message_queue_manager;
pub mod messages;
pub mod messages_utils;
pub mod model_cost;
pub mod model_utils;
pub mod modifiers;
pub mod mossen_desktop;
pub mod mossen_hints;
pub mod mossen_in_chrome;
pub mod mossenmd;
pub mod mtls;
pub mod naming;
pub mod native_installer;
pub mod notebook;
pub mod notifier;
pub mod object_group_by;
pub mod paste_store;
pub mod path;
pub mod pdf;
pub mod pdf_utils;
pub mod peer_address;
pub mod permissions;
pub mod plan_mode_v2;
pub mod plans;
pub mod platform;
pub mod plugins;
pub mod powershell;
pub mod preflight_checks;
pub mod prevent_sleep;
pub mod privacy_level;
pub mod process;
pub mod process_io;
pub mod process_user_input;
pub mod process_utils;
pub mod profile;
pub mod profiler_base;
pub mod project_inventory;
pub mod project_purge;
pub mod prompt_category;
pub mod prompt_editor;
pub mod prompt_shell_execution;
pub mod proxy;
pub mod query_context;
pub mod query_guard;
pub mod query_helpers;
pub mod query_profiler;
pub mod queue_processor;
pub mod read_edit_context;
pub mod read_file_in_range;
pub mod release_notes;
pub mod ripgrep;
pub mod sandbox_utils;
pub mod sanitization;
pub mod screenshot_clipboard;
pub mod sdk_event_queue;
pub mod secure_storage;
pub mod semantic_boolean;
pub mod semantic_number;
pub mod semver;
pub mod semver_utils;
pub mod sequential;
pub mod session_activity;
pub mod session_env_vars;
pub mod session_environment;
pub mod session_file_access_hooks;
pub mod session_ingress_auth;
pub mod session_restore;
pub mod session_start;
pub mod session_state;
pub mod session_storage;
pub mod session_storage_portable;
pub mod session_title;
pub mod session_transcript;
pub mod session_url;
pub mod set_ops;
pub mod set_utils;
pub mod settings;
pub mod settings_config;
pub mod settings_constants;
pub mod shell;
pub mod shell_command;
pub mod shell_config;
pub mod shell_utils;
pub mod side_query;
pub mod side_question;
pub mod signal;
pub mod sinks;
pub mod skills_utils;
pub mod slash_command_parsing;
pub mod sleep;
pub mod slice_ansi;
pub mod slow_operations;
pub mod stale_session;
pub mod standalone_agent;
pub mod startup_profiler;
pub mod stats;
pub mod stats_cache;
pub mod status;
pub mod status_line_observability;
pub mod status_notice_definitions;
pub mod status_notice_helpers;
pub mod stream;
pub mod stream_json_stdout_guard;
pub mod streamlined_transform;
pub mod string;
pub mod string_utils;
pub mod subprocess_env;
pub mod suggestions;
pub mod swarm;
pub mod system_directories;
pub mod system_prompt;
pub mod system_prompt_type;
pub mod system_theme;
pub mod tagged_id;
pub mod task_utils;
pub mod tasks;
pub mod team_discovery;
pub mod team_helpers;
pub mod team_memory_ops;
pub mod teammate;
pub mod teammate_context;
pub mod teammate_mailbox;
pub mod teleport;
pub mod tempfile;
pub mod tempfile_utils;
pub mod terminal;
pub mod terminal_panel;
pub mod terminal_render;
pub mod text_highlighting;
pub mod theme;
pub mod thinking;
pub mod time;
pub mod timeouts;
pub mod tmux_socket;
pub mod todo;
pub mod token_budget;
pub mod tokens;
pub mod tool_errors;
pub mod tool_pool;
pub mod tool_result_storage;
pub mod tool_schema_cache;
pub mod tool_search;
pub mod transcript_search;
pub mod treeify;
pub mod truncate;
pub mod ui_language;
pub mod unary_logging;
pub mod undercover;
pub mod user;
pub mod user_agent;
pub mod user_prompt_keywords;
pub mod user_type;
pub mod user_type_runtime_lock;
pub mod uuid_utils;
pub mod version;
pub mod warning_handler;
pub mod which;
pub mod windows_paths;
pub mod with_resolvers;
pub mod words;
pub mod workload_context;
pub mod worktree;
pub mod worktree_mode_enabled;
pub mod worktree_resume;
pub mod xdg;
pub mod xml;
pub mod yaml;
pub mod zod_to_json_schema;

// ---------------------------------------------------------------------------
// 解除 stub 后纳入 lib 的模块（原本是孤儿源文件，未编译）。
// ---------------------------------------------------------------------------
pub mod all_errors;
pub mod background_preconditions;
pub mod background_remote_session;
pub mod in_process_runner;
pub mod reconnection;
pub mod spawn_in_process;
pub mod spawn_utils;
pub mod teammate_init;
pub mod teammate_layout_manager;
pub mod teammate_model;
pub mod validation;
