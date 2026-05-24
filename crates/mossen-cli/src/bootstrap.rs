//! 启动状态管理 — 对应 TS 的 bootstrap/state.ts。
//!
//! 管理会话 ID、项目根目录、成本追踪等全局启动状态。
//! 使用 Arc<RwLock<...>> 实现线程安全的全局状态。

use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// 会话 ID 类型别名。
pub type SessionId = String;

/// 全局启动状态。
#[derive(Debug)]
pub struct BootstrapState {
    /// 原始工作目录（启动时确定）。
    pub original_cwd: PathBuf,
    /// 稳定项目根目录（启动时设定，中途 worktree 切换不影响）。
    pub project_root: PathBuf,
    /// 当前工作目录。
    pub cwd: PathBuf,
    /// 当前会话 ID。
    pub session_id: SessionId,
    /// 父会话 ID（用于追踪会话谱系）。
    pub parent_session_id: Option<SessionId>,
    /// 启动时间戳。
    pub start_time: i64,
    /// 最后交互时间。
    pub last_interaction_time: i64,
    /// 总成本（美元）。
    pub total_cost_usd: f64,
    /// 总 API 持续时间（毫秒）。
    pub total_api_duration_ms: u64,
    /// 总工具持续时间（毫秒）。
    pub total_tool_duration_ms: u64,
    /// 总新增行数。
    pub total_lines_added: u64,
    /// 总删除行数。
    pub total_lines_removed: u64,
    /// 是否为交互式会话。
    pub is_interactive: bool,
    /// 精简模式。
    pub bare_mode: bool,
    /// 远程模式。
    pub remote_mode: bool,
    /// 客户端类型。
    pub client_type: String,
    /// 主 Agent 类型。
    pub main_agent_type: Option<String>,
    /// 模型覆盖（来自 --model CLI 参数）。
    pub model_override: Option<String>,
    /// 额外目录（来自 --include-dir）。
    pub additional_dirs: Vec<PathBuf>,
    /// 会话中创建的团队（用于退出时清理）。
    pub session_created_teams: HashSet<String>,
    /// 内存中的错误日志。
    pub in_memory_error_log: Vec<ErrorLogEntry>,
    /// 已调用的技能缓存。
    pub invoked_skills: HashMap<String, InvokedSkillInfo>,
    /// 会话持久化是否禁用。
    pub session_persistence_disabled: bool,
    /// 定时任务是否启用。
    pub scheduled_tasks_enabled: bool,
}

/// 错误日志条目。
#[derive(Debug, Clone)]
pub struct ErrorLogEntry {
    pub error: String,
    pub timestamp: String,
}

/// 已调用技能信息。
#[derive(Debug, Clone)]
pub struct InvokedSkillInfo {
    pub skill_name: String,
    pub skill_path: String,
    pub content: String,
    pub invoked_at: i64,
    pub agent_id: Option<String>,
}

impl BootstrapState {
    /// 使用给定的工作目录创建初始状态。
    pub fn new(cwd: PathBuf) -> Self {
        let now = Utc::now().timestamp_millis();
        let session_id = Uuid::new_v4().to_string();
        Self {
            original_cwd: cwd.clone(),
            project_root: cwd.clone(),
            cwd,
            session_id,
            parent_session_id: None,
            start_time: now,
            last_interaction_time: now,
            total_cost_usd: 0.0,
            total_api_duration_ms: 0,
            total_tool_duration_ms: 0,
            total_lines_added: 0,
            total_lines_removed: 0,
            is_interactive: false,
            bare_mode: false,
            remote_mode: false,
            client_type: "cli".to_string(),
            main_agent_type: None,
            model_override: None,
            additional_dirs: Vec::new(),
            session_created_teams: HashSet::new(),
            in_memory_error_log: Vec::new(),
            invoked_skills: HashMap::new(),
            session_persistence_disabled: false,
            scheduled_tasks_enabled: false,
        }
    }

    /// 重新生成会话 ID，可选设置当前 ID 为父 ID。
    pub fn regenerate_session_id(&mut self, set_parent: bool) -> &str {
        if set_parent {
            self.parent_session_id = Some(self.session_id.clone());
        }
        self.session_id = Uuid::new_v4().to_string();
        &self.session_id
    }

    /// 切换到指定会话。
    pub fn switch_session(&mut self, session_id: SessionId) {
        self.session_id = session_id;
    }

    /// 更新最后交互时间。
    pub fn touch_interaction(&mut self) {
        self.last_interaction_time = Utc::now().timestamp_millis();
    }

    /// 累加成本。
    pub fn add_cost(&mut self, cost: f64) {
        self.total_cost_usd += cost;
    }

    /// 累加 API 持续时间。
    pub fn add_api_duration(&mut self, duration_ms: u64) {
        self.total_api_duration_ms += duration_ms;
    }

    /// 累加代码行变更。
    pub fn add_lines_changed(&mut self, added: u64, removed: u64) {
        self.total_lines_added += added;
        self.total_lines_removed += removed;
    }

    /// 获取总会话持续时间（毫秒）。
    pub fn total_duration_ms(&self) -> u64 {
        let now = Utc::now().timestamp_millis();
        (now - self.start_time).max(0) as u64
    }

    /// 添加错误到内存日志（保持最近 100 条）。
    pub fn log_error(&mut self, error: String) {
        const MAX_ERRORS: usize = 100;
        if self.in_memory_error_log.len() >= MAX_ERRORS {
            self.in_memory_error_log.remove(0);
        }
        self.in_memory_error_log.push(ErrorLogEntry {
            error,
            timestamp: Utc::now().to_rfc3339(),
        });
    }

    /// 重置成本状态（用于新会话或恢复）。
    pub fn reset_cost_state(&mut self) {
        self.total_cost_usd = 0.0;
        self.total_api_duration_ms = 0;
        self.total_tool_duration_ms = 0;
        self.total_lines_added = 0;
        self.total_lines_removed = 0;
        self.start_time = Utc::now().timestamp_millis();
    }
}

/// 频道许可列表条目。
#[derive(Debug, Clone)]
pub enum ChannelEntry {
    Plugin {
        name: String,
        marketplace: String,
        dev: bool,
    },
    Server {
        name: String,
        dev: bool,
    },
}

/// 会话 Cron 任务（非持久化）。
#[derive(Debug, Clone)]
pub struct SessionCronTask {
    pub id: String,
    pub cron: String,
    pub prompt: String,
    pub created_at: u64,
    pub recurring: bool,
    pub agent_id: Option<String>,
}

/// 传送会话信息。
#[derive(Debug, Clone)]
pub struct TeleportedSessionInfo {
    pub is_teleported: bool,
    pub has_logged_first_message: bool,
    pub session_id: Option<String>,
}

/// 系统 prompt 装配层信息。
#[derive(Debug, Clone)]
pub struct SystemPromptAssemblyLayer {
    pub layer: String,
    pub label: String,
    pub section_names: Vec<String>,
    pub item_count: usize,
}

/// 生效的系统 prompt 装配信息。
#[derive(Debug, Clone)]
pub struct EffectiveSystemPromptAssembly {
    pub base_source: String, // "default"|"custom"|"agent"|"coordinator"|"override"|"unknown"
    pub overlay_sources: Vec<String>,
    pub item_count: usize,
}

/// 慢操作记录。
#[derive(Debug, Clone)]
pub struct SlowOperation {
    pub operation: String,
    pub duration_ms: u64,
    pub timestamp: i64,
}

/// 模型使用统计。
#[derive(Debug, Clone, Default)]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub web_search_requests: u64,
    pub cost_usd: f64,
    pub context_window: u64,
    pub max_output_tokens: u64,
}

/// 扩展启动状态 — 对应 TS state.ts 的完整字段集。
#[derive(Debug)]
pub struct BootstrapStateExtended {
    // ---- 成本与持续时间 ----
    pub total_api_duration_without_retries: u64,
    pub turn_hook_duration_ms: u64,
    pub turn_tool_duration_ms: u64,
    pub turn_classifier_duration_ms: u64,
    pub turn_tool_count: u64,
    pub turn_hook_count: u64,
    pub turn_classifier_count: u64,
    pub has_unknown_model_cost: bool,
    pub model_usage: HashMap<String, ModelUsage>,
    // ---- 模型 ----
    pub main_loop_model_override: Option<String>,
    pub initial_main_loop_model: Option<String>,
    pub model_strings: Option<serde_json::Value>,
    // ---- 会话 ----
    pub kairos_active: bool,
    pub strict_tool_result_pairing: bool,
    pub sdk_agent_progress_summaries_enabled: bool,
    pub user_msg_opt_in: bool,
    pub session_source: Option<String>,
    pub question_preview_format: Option<String>,
    pub flag_settings_path: Option<String>,
    pub flag_settings_inline: Option<serde_json::Value>,
    pub allowed_setting_sources: Vec<String>,
    pub session_ingress_token: Option<String>,
    pub oauth_token_from_fd: Option<String>,
    pub api_key_from_fd: Option<String>,
    pub stats_store: Option<Box<dyn std::any::Any + Send + Sync>>,
    // ---- Agent ----
    pub agent_color_map: HashMap<String, String>,
    pub agent_color_index: usize,
    pub main_thread_agent_type: Option<String>,
    // ---- API ----
    pub last_api_request: Option<serde_json::Value>,
    pub last_classifier_requests: Option<Vec<serde_json::Value>>,
    pub cached_mossen_md_content: Option<String>,
    // ---- Plugins ----
    pub inline_plugins: Vec<String>,
    pub chrome_flag_override: Option<bool>,
    pub use_cowork_plugins: bool,
    // ---- 会话权限 ----
    pub session_bypass_permissions_mode: bool,
    pub session_trust_accepted: bool,
    // ---- Cron 任务 ----
    pub session_cron_tasks: Vec<SessionCronTask>,
    // ---- Plan mode ----
    pub has_exited_plan_mode: bool,
    pub needs_plan_mode_exit_attachment: bool,
    pub needs_auto_mode_exit_attachment: bool,
    pub lsp_recommendation_shown_this_session: bool,
    // ---- SDK ----
    pub init_json_schema: Option<serde_json::Value>,
    pub registered_hooks: Option<HashMap<String, Vec<serde_json::Value>>>,
    pub registered_plugin_hooks: Option<HashMap<String, Vec<serde_json::Value>>>,
    pub plan_slug_cache: HashMap<String, String>,
    // ---- Teleport ----
    pub teleported_session_info: Option<TeleportedSessionInfo>,
    // ---- Slow ops ----
    pub slow_operations: Vec<SlowOperation>,
    // ---- SDK betas ----
    pub sdk_betas: Option<Vec<String>>,
    // ---- Remote mode ----
    pub is_remote_mode: bool,
    pub direct_connect_server_url: Option<String>,
    // ---- System prompt ----
    pub system_prompt_section_cache: HashMap<String, Option<String>>,
    pub last_system_prompt_assembly: Vec<SystemPromptAssemblyLayer>,
    pub last_effective_system_prompt_assembly: Option<EffectiveSystemPromptAssembly>,
    // ---- 其他 ----
    pub last_emitted_date: Option<String>,
    pub additional_directories_for_mossen_md: Vec<String>,
    pub allowed_channels: Vec<ChannelEntry>,
    pub has_dev_channels: bool,
    pub session_project_dir: Option<String>,
    pub prompt_cache_1h_allowlist: Option<Vec<String>>,
    pub prompt_cache_1h_eligible: Option<bool>,
    pub afk_mode_header_latched: Option<bool>,
    pub fast_mode_header_latched: Option<bool>,
    pub cache_editing_header_latched: Option<bool>,
    pub thinking_clear_latched: Option<bool>,
    pub prompt_id: Option<String>,
    pub last_main_request_id: Option<String>,
    pub last_api_completion_timestamp: Option<i64>,
    pub pending_post_compaction: bool,
    // ---- 输出 token 追踪 ----
    pub output_tokens_at_turn_start: u64,
    pub current_turn_token_budget: Option<u64>,
    pub budget_continuation_count: u64,
    // ---- 滚动排空 ----
    pub scroll_draining: bool,
}

impl Default for BootstrapStateExtended {
    fn default() -> Self {
        Self {
            total_api_duration_without_retries: 0,
            turn_hook_duration_ms: 0,
            turn_tool_duration_ms: 0,
            turn_classifier_duration_ms: 0,
            turn_tool_count: 0,
            turn_hook_count: 0,
            turn_classifier_count: 0,
            has_unknown_model_cost: false,
            model_usage: HashMap::new(),
            main_loop_model_override: None,
            initial_main_loop_model: None,
            model_strings: None,
            kairos_active: false,
            strict_tool_result_pairing: false,
            sdk_agent_progress_summaries_enabled: false,
            user_msg_opt_in: false,
            session_source: None,
            question_preview_format: None,
            flag_settings_path: None,
            flag_settings_inline: None,
            allowed_setting_sources: vec![
                "userSettings".into(),
                "projectSettings".into(),
                "localSettings".into(),
                "flagSettings".into(),
                "policySettings".into(),
            ],
            session_ingress_token: None,
            oauth_token_from_fd: None,
            api_key_from_fd: None,
            stats_store: None,
            agent_color_map: HashMap::new(),
            agent_color_index: 0,
            main_thread_agent_type: None,
            last_api_request: None,
            last_classifier_requests: None,
            cached_mossen_md_content: None,
            inline_plugins: Vec::new(),
            chrome_flag_override: None,
            use_cowork_plugins: false,
            session_bypass_permissions_mode: false,
            session_trust_accepted: false,
            session_cron_tasks: Vec::new(),
            has_exited_plan_mode: false,
            needs_plan_mode_exit_attachment: false,
            needs_auto_mode_exit_attachment: false,
            lsp_recommendation_shown_this_session: false,
            init_json_schema: None,
            registered_hooks: None,
            registered_plugin_hooks: None,
            plan_slug_cache: HashMap::new(),
            teleported_session_info: None,
            slow_operations: Vec::new(),
            sdk_betas: None,
            is_remote_mode: false,
            direct_connect_server_url: None,
            system_prompt_section_cache: HashMap::new(),
            last_system_prompt_assembly: Vec::new(),
            last_effective_system_prompt_assembly: None,
            last_emitted_date: None,
            additional_directories_for_mossen_md: Vec::new(),
            allowed_channels: Vec::new(),
            has_dev_channels: false,
            session_project_dir: None,
            prompt_cache_1h_allowlist: None,
            prompt_cache_1h_eligible: None,
            afk_mode_header_latched: None,
            fast_mode_header_latched: None,
            cache_editing_header_latched: None,
            thinking_clear_latched: None,
            prompt_id: None,
            last_main_request_id: None,
            last_api_completion_timestamp: None,
            pending_post_compaction: false,
            output_tokens_at_turn_start: 0,
            current_turn_token_budget: None,
            budget_continuation_count: 0,
            scroll_draining: false,
        }
    }
}

impl BootstrapStateExtended {
    // ---- Duration helpers ----

    pub fn add_to_total_duration(&mut self, duration: u64, duration_without_retries: u64) {
        // total_api_duration_ms lives in BootstrapState; this adds the without-retries part
        self.total_api_duration_without_retries += duration_without_retries;
        let _ = duration; // caller adds to BootstrapState.total_api_duration_ms
    }

    pub fn add_to_tool_duration(&mut self, duration: u64) {
        self.turn_tool_duration_ms += duration;
        self.turn_tool_count += 1;
    }

    pub fn reset_turn_tool_duration(&mut self) {
        self.turn_tool_duration_ms = 0;
        self.turn_tool_count = 0;
    }

    pub fn add_to_turn_hook_duration(&mut self, duration: u64) {
        self.turn_hook_duration_ms += duration;
        self.turn_hook_count += 1;
    }

    pub fn reset_turn_hook_duration(&mut self) {
        self.turn_hook_duration_ms = 0;
        self.turn_hook_count = 0;
    }

    pub fn add_to_turn_classifier_duration(&mut self, duration: u64) {
        self.turn_classifier_duration_ms += duration;
        self.turn_classifier_count += 1;
    }

    pub fn reset_turn_classifier_duration(&mut self) {
        self.turn_classifier_duration_ms = 0;
        self.turn_classifier_count = 0;
    }

    // ---- Token tracking ----

    pub fn get_total_input_tokens(&self) -> u64 {
        self.model_usage.values().map(|u| u.input_tokens).sum()
    }

    pub fn get_total_output_tokens(&self) -> u64 {
        self.model_usage.values().map(|u| u.output_tokens).sum()
    }

    pub fn get_total_cache_read_input_tokens(&self) -> u64 {
        self.model_usage
            .values()
            .map(|u| u.cache_read_input_tokens)
            .sum()
    }

    pub fn get_total_cache_creation_input_tokens(&self) -> u64 {
        self.model_usage
            .values()
            .map(|u| u.cache_creation_input_tokens)
            .sum()
    }

    pub fn get_total_web_search_requests(&self) -> u64 {
        self.model_usage
            .values()
            .map(|u| u.web_search_requests)
            .sum()
    }

    pub fn get_turn_output_tokens(&self) -> u64 {
        self.get_total_output_tokens()
            .saturating_sub(self.output_tokens_at_turn_start)
    }

    pub fn snapshot_output_tokens_for_turn(&mut self, budget: Option<u64>) {
        self.output_tokens_at_turn_start = self.get_total_output_tokens();
        self.current_turn_token_budget = budget;
        self.budget_continuation_count = 0;
    }

    pub fn increment_budget_continuation_count(&mut self) {
        self.budget_continuation_count += 1;
    }

    // ---- Cost state ----

    pub fn add_to_total_cost_state(&mut self, model: &str, usage: ModelUsage) {
        self.model_usage.insert(model.to_string(), usage);
    }

    pub fn set_has_unknown_model_cost(&mut self) {
        self.has_unknown_model_cost = true;
    }

    // ---- Post-compaction ----

    pub fn mark_post_compaction(&mut self) {
        self.pending_post_compaction = true;
    }

    pub fn consume_post_compaction(&mut self) -> bool {
        let was = self.pending_post_compaction;
        self.pending_post_compaction = false;
        was
    }

    // ---- Scroll draining ----

    pub fn mark_scroll_activity(&mut self) {
        self.scroll_draining = true;
    }

    pub fn clear_scroll_draining(&mut self) {
        self.scroll_draining = false;
    }

    pub fn is_scroll_draining(&self) -> bool {
        self.scroll_draining
    }

    // ---- Plan mode transitions ----

    pub fn handle_plan_mode_transition(&mut self, from_mode: &str, to_mode: &str) {
        if to_mode == "plan" && from_mode != "plan" {
            self.needs_plan_mode_exit_attachment = false;
        }
        if from_mode == "plan" && to_mode != "plan" {
            self.needs_plan_mode_exit_attachment = true;
        }
    }

    pub fn handle_auto_mode_transition(&mut self, from_mode: &str, to_mode: &str) {
        if (from_mode == "auto" && to_mode == "plan") || (from_mode == "plan" && to_mode == "auto")
        {
            return;
        }
        let from_is_auto = from_mode == "auto";
        let to_is_auto = to_mode == "auto";
        if to_is_auto && !from_is_auto {
            self.needs_auto_mode_exit_attachment = false;
        }
        if from_is_auto && !to_is_auto {
            self.needs_auto_mode_exit_attachment = true;
        }
    }

    // ---- Hook registration ----

    pub fn register_hook_callbacks(&mut self, hooks: HashMap<String, Vec<serde_json::Value>>) {
        let reg = self.registered_hooks.get_or_insert_with(HashMap::new);
        for (event, matchers) in hooks {
            reg.entry(event).or_insert_with(Vec::new).extend(matchers);
        }
    }

    pub fn register_plugin_hook_callbacks(
        &mut self,
        hooks: HashMap<String, Vec<serde_json::Value>>,
    ) {
        let reg = self
            .registered_plugin_hooks
            .get_or_insert_with(HashMap::new);
        for (event, matchers) in hooks {
            reg.entry(event).or_insert_with(Vec::new).extend(matchers);
        }
    }

    pub fn clear_registered_hooks(&mut self) {
        self.registered_hooks = None;
        self.registered_plugin_hooks = None;
    }

    pub fn clear_registered_plugin_hooks(&mut self) {
        self.registered_plugin_hooks = None;
    }

    pub fn reset_sdk_init_state(&mut self) {
        self.init_json_schema = None;
        self.registered_hooks = None;
        self.registered_plugin_hooks = None;
    }

    // ---- Invoked skills (via InvokedSkillInfo) ----

    pub fn get_invoked_skills_for_agent(
        &self,
        agent_id: Option<&str>,
        invoked_skills: &HashMap<String, InvokedSkillInfo>,
    ) -> HashMap<String, InvokedSkillInfo> {
        invoked_skills
            .iter()
            .filter(|(_, skill)| skill.agent_id.as_deref() == agent_id)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    // ---- Session cron tasks ----

    pub fn add_session_cron_task(&mut self, task: SessionCronTask) {
        self.session_cron_tasks.push(task);
    }

    pub fn remove_session_cron_tasks(&mut self, ids: &[String]) -> usize {
        if ids.is_empty() {
            return 0;
        }
        let id_set: HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
        let before = self.session_cron_tasks.len();
        self.session_cron_tasks
            .retain(|t| !id_set.contains(t.id.as_str()));
        before - self.session_cron_tasks.len()
    }

    // ---- Slow operations ----

    const MAX_SLOW_OPERATIONS: usize = 10;
    const SLOW_OPERATION_TTL_MS: i64 = 10000;

    pub fn add_slow_operation(&mut self, operation: String, duration_ms: u64) {
        let now = Utc::now().timestamp_millis();
        // 移除过期操作
        self.slow_operations
            .retain(|op| now - op.timestamp < Self::SLOW_OPERATION_TTL_MS);
        // 跳过编辑器会话
        if operation.contains("exec") && operation.contains("mossen-prompt-") {
            return;
        }
        self.slow_operations.push(SlowOperation {
            operation,
            duration_ms,
            timestamp: now,
        });
        if self.slow_operations.len() > Self::MAX_SLOW_OPERATIONS {
            let start = self.slow_operations.len() - Self::MAX_SLOW_OPERATIONS;
            self.slow_operations = self.slow_operations[start..].to_vec();
        }
    }

    pub fn get_slow_operations(&mut self) -> &[SlowOperation] {
        let now = Utc::now().timestamp_millis();
        self.slow_operations
            .retain(|op| now - op.timestamp < Self::SLOW_OPERATION_TTL_MS);
        &self.slow_operations
    }

    // ---- Teleport ----

    pub fn set_teleported_session_info(&mut self, session_id: Option<String>) {
        self.teleported_session_info = Some(TeleportedSessionInfo {
            is_teleported: true,
            has_logged_first_message: false,
            session_id,
        });
    }

    pub fn mark_first_teleport_message_logged(&mut self) {
        if let Some(ref mut info) = self.teleported_session_info {
            info.has_logged_first_message = true;
        }
    }

    // ---- System prompt section ----

    pub fn set_system_prompt_section_cache_entry(&mut self, name: String, value: Option<String>) {
        self.system_prompt_section_cache.insert(name, value);
    }

    pub fn clear_system_prompt_section_state(&mut self) {
        self.system_prompt_section_cache.clear();
        self.last_system_prompt_assembly.clear();
        self.last_effective_system_prompt_assembly = None;
    }

    // ---- Beta header latches ----

    pub fn clear_beta_header_latches(&mut self) {
        self.afk_mode_header_latched = None;
        self.fast_mode_header_latched = None;
        self.cache_editing_header_latched = None;
        self.thinking_clear_latched = None;
    }

    // ---- 3P auth preference ----

    pub fn prefer_third_party_authentication(
        &self,
        is_non_interactive: bool,
        client_type: &str,
    ) -> bool {
        is_non_interactive && client_type != "mossen-vscode"
    }

    // ---- Reset for restore ----

    pub fn set_cost_state_for_restore(
        &mut self,
        total_cost_usd: f64,
        total_api_duration: u64,
        total_api_duration_without_retries: u64,
        total_tool_duration: u64,
        total_lines_added: u64,
        total_lines_removed: u64,
        last_duration: Option<u64>,
        model_usage: Option<HashMap<String, ModelUsage>>,
        state: &mut BootstrapState,
    ) {
        state.total_cost_usd = total_cost_usd;
        state.total_api_duration_ms = total_api_duration;
        self.total_api_duration_without_retries = total_api_duration_without_retries;
        state.total_tool_duration_ms = total_tool_duration;
        state.total_lines_added = total_lines_added;
        state.total_lines_removed = total_lines_removed;
        if let Some(usage) = model_usage {
            self.model_usage = usage;
        }
        if let Some(dur) = last_duration {
            state.start_time = Utc::now().timestamp_millis() - dur as i64;
        }
    }

    // ---- Full reset (test) ----

    pub fn reset_cost_state(&mut self) {
        self.has_unknown_model_cost = false;
        self.model_usage.clear();
        self.prompt_id = None;
        self.total_api_duration_without_retries = 0;
    }
}

/// 线程安全的全局启动状态句柄。
pub type SharedBootstrapState = Arc<RwLock<BootstrapState>>;

/// 创建新的共享启动状态。
pub fn new_shared_state(cwd: PathBuf) -> SharedBootstrapState {
    Arc::new(RwLock::new(BootstrapState::new(cwd)))
}

// =============================================================================
// 全局状态单例 — 对应 TS bootstrap/state.ts 的 STATE 顶层变量
// =============================================================================

use once_cell::sync::Lazy;

/// 内部聚合容器：BootstrapState + BootstrapStateExtended + InvokedSkillInfo 映射。
#[derive(Debug)]
struct GlobalBootstrap {
    base: BootstrapState,
    ext: BootstrapStateExtended,
    invoked_skills: HashMap<String, InvokedSkillInfo>,
    plan_slug_cache: HashMap<String, String>,
}

impl GlobalBootstrap {
    fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            base: BootstrapState::new(cwd),
            ext: BootstrapStateExtended::default(),
            invoked_skills: HashMap::new(),
            plan_slug_cache: HashMap::new(),
        }
    }
}

static GLOBAL_STATE: Lazy<RwLock<GlobalBootstrap>> =
    Lazy::new(|| RwLock::new(GlobalBootstrap::new()));

fn with_state<R>(f: impl FnOnce(&GlobalBootstrap) -> R) -> R {
    f(&*GLOBAL_STATE.read().expect("bootstrap state poisoned"))
}

fn with_state_mut<R>(f: impl FnOnce(&mut GlobalBootstrap) -> R) -> R {
    f(&mut *GLOBAL_STATE.write().expect("bootstrap state poisoned"))
}

// ---- Session ID / project / cwd ----

pub fn get_session_id() -> SessionId {
    with_state(|s| s.base.session_id.clone())
}

pub fn regenerate_session_id(set_parent: bool) -> SessionId {
    with_state_mut(|s| {
        s.base.regenerate_session_id(set_parent);
        s.base.session_id.clone()
    })
}

pub fn get_parent_session_id() -> Option<SessionId> {
    with_state(|s| s.base.parent_session_id.clone())
}

pub fn switch_session(session_id: SessionId) {
    with_state_mut(|s| s.base.switch_session(session_id));
}

pub fn get_session_project_dir() -> Option<String> {
    with_state(|s| s.ext.session_project_dir.clone())
}

pub fn get_original_cwd() -> String {
    with_state(|s| s.base.original_cwd.display().to_string())
}

pub fn get_project_root() -> String {
    with_state(|s| s.base.project_root.display().to_string())
}

pub fn set_original_cwd(cwd: String) {
    with_state_mut(|s| s.base.original_cwd = PathBuf::from(cwd));
}

pub fn set_project_root(cwd: String) {
    with_state_mut(|s| s.base.project_root = PathBuf::from(cwd));
}

pub fn get_cwd_state() -> String {
    with_state(|s| s.base.cwd.display().to_string())
}

pub fn set_cwd_state(cwd: String) {
    with_state_mut(|s| s.base.cwd = PathBuf::from(cwd));
}

pub fn get_direct_connect_server_url() -> Option<String> {
    with_state(|s| s.ext.direct_connect_server_url.clone())
}

pub fn set_direct_connect_server_url(url: String) {
    with_state_mut(|s| s.ext.direct_connect_server_url = Some(url));
}

// ---- Cost / duration ----

pub fn add_to_total_duration_state(duration: u64, duration_without_retries: u64) {
    with_state_mut(|s| {
        s.base.total_api_duration_ms += duration;
        s.ext.total_api_duration_without_retries += duration_without_retries;
    });
}

pub fn reset_total_duration_state_and_cost_for_tests_only() {
    with_state_mut(|s| {
        s.base.reset_cost_state();
        s.ext.reset_cost_state();
    });
}

pub fn add_to_total_cost_state(model: String, usage: ModelUsage, cost: f64) {
    with_state_mut(|s| {
        s.base.total_cost_usd += cost;
        s.ext.add_to_total_cost_state(&model, usage);
    });
}

pub fn get_total_cost_usd() -> f64 {
    with_state(|s| s.base.total_cost_usd)
}

pub fn get_total_api_duration() -> u64 {
    with_state(|s| s.base.total_api_duration_ms)
}

pub fn get_total_duration() -> u64 {
    with_state(|s| s.base.total_duration_ms())
}

pub fn get_total_api_duration_without_retries() -> u64 {
    with_state(|s| s.ext.total_api_duration_without_retries)
}

pub fn get_total_tool_duration() -> u64 {
    with_state(|s| s.base.total_tool_duration_ms)
}

pub fn add_to_tool_duration(duration: u64) {
    with_state_mut(|s| {
        s.base.total_tool_duration_ms += duration;
        s.ext.add_to_tool_duration(duration);
    });
}

pub fn get_turn_hook_duration_ms() -> u64 {
    with_state(|s| s.ext.turn_hook_duration_ms)
}

pub fn add_to_turn_hook_duration(duration: u64) {
    with_state_mut(|s| s.ext.add_to_turn_hook_duration(duration));
}

pub fn reset_turn_hook_duration() {
    with_state_mut(|s| s.ext.reset_turn_hook_duration());
}

pub fn get_turn_hook_count() -> u64 {
    with_state(|s| s.ext.turn_hook_count)
}

pub fn get_turn_tool_duration_ms() -> u64 {
    with_state(|s| s.ext.turn_tool_duration_ms)
}

pub fn reset_turn_tool_duration() {
    with_state_mut(|s| s.ext.reset_turn_tool_duration());
}

pub fn get_turn_tool_count() -> u64 {
    with_state(|s| s.ext.turn_tool_count)
}

pub fn get_turn_classifier_duration_ms() -> u64 {
    with_state(|s| s.ext.turn_classifier_duration_ms)
}

pub fn add_to_turn_classifier_duration(duration: u64) {
    with_state_mut(|s| s.ext.add_to_turn_classifier_duration(duration));
}

pub fn reset_turn_classifier_duration() {
    with_state_mut(|s| s.ext.reset_turn_classifier_duration());
}

pub fn get_turn_classifier_count() -> u64 {
    with_state(|s| s.ext.turn_classifier_count)
}

// ---- Stats store (opaque) ----

pub fn get_stats_store() -> Option<()> {
    // 不透明类型；这里返回是否存在。
    with_state(|s| s.ext.stats_store.as_ref().map(|_| ()))
}

/// 设置任意类型的 stats store。
///
/// 真实存储槽位是 `Option<Box<dyn Any + Send + Sync>>`；
/// 调用方传入实现 `Any + Send + Sync` 的具体类型，
/// 后续通过 `with_state` 读取并 downcast。
pub fn set_stats_store<T: std::any::Any + Send + Sync>(store: T) {
    with_state_mut(|s| s.ext.stats_store = Some(Box::new(store)));
}

// ---- Interaction time ----

pub fn update_last_interaction_time(_immediate: Option<bool>) {
    with_state_mut(|s| s.base.touch_interaction());
}

pub fn flush_interaction_time() {
    with_state_mut(|s| s.base.touch_interaction());
}

pub fn get_last_interaction_time() -> i64 {
    with_state(|s| s.base.last_interaction_time)
}

// ---- Lines changed ----

pub fn add_to_total_lines_changed(added: u64, removed: u64) {
    with_state_mut(|s| s.base.add_lines_changed(added, removed));
}

pub fn get_total_lines_added() -> u64 {
    with_state(|s| s.base.total_lines_added)
}

pub fn get_total_lines_removed() -> u64 {
    with_state(|s| s.base.total_lines_removed)
}

// ---- Tokens ----

pub fn get_total_input_tokens() -> u64 {
    with_state(|s| s.ext.get_total_input_tokens())
}

pub fn get_total_output_tokens() -> u64 {
    with_state(|s| s.ext.get_total_output_tokens())
}

pub fn get_total_cache_read_input_tokens() -> u64 {
    with_state(|s| s.ext.get_total_cache_read_input_tokens())
}

pub fn get_total_cache_creation_input_tokens() -> u64 {
    with_state(|s| s.ext.get_total_cache_creation_input_tokens())
}

pub fn get_total_web_search_requests() -> u64 {
    with_state(|s| s.ext.get_total_web_search_requests())
}

pub fn get_turn_output_tokens() -> u64 {
    with_state(|s| s.ext.get_turn_output_tokens())
}

pub fn get_current_turn_token_budget() -> Option<u64> {
    with_state(|s| s.ext.current_turn_token_budget)
}

pub fn snapshot_output_tokens_for_turn(budget: Option<u64>) {
    with_state_mut(|s| s.ext.snapshot_output_tokens_for_turn(budget));
}

pub fn get_budget_continuation_count() -> u64 {
    with_state(|s| s.ext.budget_continuation_count)
}

pub fn increment_budget_continuation_count() {
    with_state_mut(|s| s.ext.increment_budget_continuation_count());
}

pub fn set_has_unknown_model_cost() {
    with_state_mut(|s| s.ext.set_has_unknown_model_cost());
}

pub fn has_unknown_model_cost() -> bool {
    with_state(|s| s.ext.has_unknown_model_cost)
}

// ---- Last request ----

pub fn get_last_main_request_id() -> Option<String> {
    with_state(|s| s.ext.last_main_request_id.clone())
}

pub fn set_last_main_request_id(request_id: String) {
    with_state_mut(|s| s.ext.last_main_request_id = Some(request_id));
}

pub fn get_last_api_completion_timestamp() -> Option<i64> {
    with_state(|s| s.ext.last_api_completion_timestamp)
}

pub fn set_last_api_completion_timestamp(ts: i64) {
    with_state_mut(|s| s.ext.last_api_completion_timestamp = Some(ts));
}

// ---- Post-compaction ----

pub fn mark_post_compaction() {
    with_state_mut(|s| s.ext.mark_post_compaction());
}

pub fn consume_post_compaction() -> bool {
    with_state_mut(|s| s.ext.consume_post_compaction())
}

// ---- Scroll draining ----

pub fn mark_scroll_activity() {
    with_state_mut(|s| s.ext.mark_scroll_activity());
}

pub fn get_is_scroll_draining() -> bool {
    with_state(|s| s.ext.is_scroll_draining())
}

pub async fn wait_for_scroll_idle() {
    // 简化等待循环：每 50ms 检查一次，最多等 5 秒。
    use std::time::Duration;
    for _ in 0..100 {
        if !get_is_scroll_draining() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ---- Model usage ----

pub fn get_model_usage() -> HashMap<String, ModelUsage> {
    with_state(|s| s.ext.model_usage.clone())
}

pub fn get_usage_for_model(model: &str) -> Option<ModelUsage> {
    with_state(|s| s.ext.model_usage.get(model).cloned())
}

pub fn get_main_loop_model_override() -> Option<String> {
    with_state(|s| s.ext.main_loop_model_override.clone())
}

pub fn get_initial_main_loop_model() -> Option<String> {
    with_state(|s| s.ext.initial_main_loop_model.clone())
}

pub fn set_main_loop_model_override(model: Option<String>) {
    with_state_mut(|s| s.ext.main_loop_model_override = model);
}

pub fn set_initial_main_loop_model(model: String) {
    with_state_mut(|s| s.ext.initial_main_loop_model = Some(model));
}

pub fn get_sdk_betas() -> Option<Vec<String>> {
    with_state(|s| s.ext.sdk_betas.clone())
}

pub fn set_sdk_betas(betas: Option<Vec<String>>) {
    with_state_mut(|s| s.ext.sdk_betas = betas);
}

pub fn reset_cost_state() {
    with_state_mut(|s| {
        s.base.reset_cost_state();
        s.ext.reset_cost_state();
    });
}

#[allow(clippy::too_many_arguments)]
pub fn set_cost_state_for_restore(
    total_cost_usd: f64,
    total_api_duration: u64,
    total_api_duration_without_retries: u64,
    total_tool_duration: u64,
    total_lines_added: u64,
    total_lines_removed: u64,
    last_duration: Option<u64>,
    model_usage: Option<HashMap<String, ModelUsage>>,
) {
    with_state_mut(|s| {
        let GlobalBootstrap { base, ext, .. } = s;
        ext.set_cost_state_for_restore(
            total_cost_usd,
            total_api_duration,
            total_api_duration_without_retries,
            total_tool_duration,
            total_lines_added,
            total_lines_removed,
            last_duration,
            model_usage,
            base,
        );
    });
}

pub fn reset_state_for_tests() {
    with_state_mut(|s| {
        *s = GlobalBootstrap::new();
    });
}

// ---- Model strings ----

pub fn get_model_strings() -> Option<serde_json::Value> {
    with_state(|s| s.ext.model_strings.clone())
}

pub fn set_model_strings(model_strings: serde_json::Value) {
    with_state_mut(|s| s.ext.model_strings = Some(model_strings));
}

pub fn reset_model_strings_for_testing_only() {
    with_state_mut(|s| s.ext.model_strings = None);
}

// ---- Session flags ----

pub fn get_is_non_interactive_session() -> bool {
    with_state(|s| !s.base.is_interactive)
}

pub fn get_is_interactive() -> bool {
    with_state(|s| s.base.is_interactive)
}

pub fn set_is_interactive(value: bool) {
    with_state_mut(|s| s.base.is_interactive = value);
}

pub fn get_client_type() -> String {
    with_state(|s| s.base.client_type.clone())
}

pub fn set_client_type(t: String) {
    with_state_mut(|s| s.base.client_type = t);
}

pub fn get_sdk_agent_progress_summaries_enabled() -> bool {
    with_state(|s| s.ext.sdk_agent_progress_summaries_enabled)
}

pub fn set_sdk_agent_progress_summaries_enabled(value: bool) {
    with_state_mut(|s| s.ext.sdk_agent_progress_summaries_enabled = value);
}

pub fn get_kairos_active() -> bool {
    with_state(|s| s.ext.kairos_active)
}

pub fn set_kairos_active(value: bool) {
    with_state_mut(|s| s.ext.kairos_active = value);
}

pub fn get_strict_tool_result_pairing() -> bool {
    with_state(|s| s.ext.strict_tool_result_pairing)
}

pub fn set_strict_tool_result_pairing(value: bool) {
    with_state_mut(|s| s.ext.strict_tool_result_pairing = value);
}

pub fn get_user_msg_opt_in() -> bool {
    with_state(|s| s.ext.user_msg_opt_in)
}

pub fn set_user_msg_opt_in(value: bool) {
    with_state_mut(|s| s.ext.user_msg_opt_in = value);
}

pub fn get_session_source() -> Option<String> {
    with_state(|s| s.ext.session_source.clone())
}

pub fn set_session_source(source: String) {
    with_state_mut(|s| s.ext.session_source = Some(source));
}

pub fn get_question_preview_format() -> Option<String> {
    with_state(|s| s.ext.question_preview_format.clone())
}

pub fn set_question_preview_format(format: String) {
    with_state_mut(|s| s.ext.question_preview_format = Some(format));
}

pub fn get_agent_color_map() -> HashMap<String, String> {
    with_state(|s| s.ext.agent_color_map.clone())
}

pub fn get_flag_settings_path() -> Option<String> {
    with_state(|s| s.ext.flag_settings_path.clone())
}

pub fn set_flag_settings_path(path: Option<String>) {
    with_state_mut(|s| s.ext.flag_settings_path = path);
}

pub fn get_flag_settings_inline() -> Option<serde_json::Value> {
    with_state(|s| s.ext.flag_settings_inline.clone())
}

pub fn set_flag_settings_inline(v: Option<serde_json::Value>) {
    with_state_mut(|s| s.ext.flag_settings_inline = v);
}

pub fn get_session_ingress_token() -> Option<String> {
    with_state(|s| s.ext.session_ingress_token.clone())
}

pub fn set_session_ingress_token(token: Option<String>) {
    with_state_mut(|s| s.ext.session_ingress_token = token);
}

pub fn get_oauth_token_from_fd() -> Option<String> {
    with_state(|s| s.ext.oauth_token_from_fd.clone())
}

pub fn set_oauth_token_from_fd(token: Option<String>) {
    with_state_mut(|s| s.ext.oauth_token_from_fd = token);
}

pub fn get_api_key_from_fd() -> Option<String> {
    with_state(|s| s.ext.api_key_from_fd.clone())
}

pub fn set_api_key_from_fd(key: Option<String>) {
    with_state_mut(|s| s.ext.api_key_from_fd = key);
}

pub fn set_last_api_request(req: serde_json::Value) {
    with_state_mut(|s| s.ext.last_api_request = Some(req));
}

pub fn get_last_api_request() -> Option<serde_json::Value> {
    with_state(|s| s.ext.last_api_request.clone())
}

pub fn set_last_classifier_requests(requests: Option<Vec<serde_json::Value>>) {
    with_state_mut(|s| s.ext.last_classifier_requests = requests);
}

pub fn get_last_classifier_requests() -> Option<Vec<serde_json::Value>> {
    with_state(|s| s.ext.last_classifier_requests.clone())
}

pub fn set_cached_mossen_md_content(content: Option<String>) {
    with_state_mut(|s| s.ext.cached_mossen_md_content = content);
}

pub fn get_cached_mossen_md_content() -> Option<String> {
    with_state(|s| s.ext.cached_mossen_md_content.clone())
}

pub fn add_to_in_memory_error_log(error: String) {
    with_state_mut(|s| s.base.log_error(error));
}

pub fn get_allowed_setting_sources() -> Vec<String> {
    with_state(|s| s.ext.allowed_setting_sources.clone())
}

pub fn set_allowed_setting_sources(sources: Vec<String>) {
    with_state_mut(|s| s.ext.allowed_setting_sources = sources);
}

pub fn prefer_third_party_authentication() -> bool {
    with_state(|s| {
        s.ext
            .prefer_third_party_authentication(!s.base.is_interactive, &s.base.client_type)
    })
}

pub fn set_inline_plugins(plugins: Vec<String>) {
    with_state_mut(|s| s.ext.inline_plugins = plugins);
}

pub fn get_inline_plugins() -> Vec<String> {
    with_state(|s| s.ext.inline_plugins.clone())
}

pub fn set_chrome_flag_override(value: Option<bool>) {
    with_state_mut(|s| s.ext.chrome_flag_override = value);
}

pub fn get_chrome_flag_override() -> Option<bool> {
    with_state(|s| s.ext.chrome_flag_override)
}

pub fn set_use_cowork_plugins(value: bool) {
    with_state_mut(|s| s.ext.use_cowork_plugins = value);
}

pub fn get_use_cowork_plugins() -> bool {
    with_state(|s| s.ext.use_cowork_plugins)
}

pub fn set_session_bypass_permissions_mode(enabled: bool) {
    with_state_mut(|s| s.ext.session_bypass_permissions_mode = enabled);
}

pub fn get_session_bypass_permissions_mode() -> bool {
    with_state(|s| s.ext.session_bypass_permissions_mode)
}

pub fn set_scheduled_tasks_enabled(enabled: bool) {
    with_state_mut(|s| s.base.scheduled_tasks_enabled = enabled);
}

pub fn get_scheduled_tasks_enabled() -> bool {
    with_state(|s| s.base.scheduled_tasks_enabled)
}

// ---- Session cron tasks ----

pub fn get_session_cron_tasks() -> Vec<SessionCronTask> {
    with_state(|s| s.ext.session_cron_tasks.clone())
}

pub fn add_session_cron_task(task: SessionCronTask) {
    with_state_mut(|s| s.ext.add_session_cron_task(task));
}

pub fn remove_session_cron_tasks(ids: &[String]) -> usize {
    with_state_mut(|s| s.ext.remove_session_cron_tasks(ids))
}

pub fn set_session_trust_accepted(accepted: bool) {
    with_state_mut(|s| s.ext.session_trust_accepted = accepted);
}

pub fn get_session_trust_accepted() -> bool {
    with_state(|s| s.ext.session_trust_accepted)
}

pub fn set_session_persistence_disabled(disabled: bool) {
    with_state_mut(|s| s.base.session_persistence_disabled = disabled);
}

pub fn is_session_persistence_disabled() -> bool {
    with_state(|s| s.base.session_persistence_disabled)
}

pub fn has_exited_plan_mode_in_session() -> bool {
    with_state(|s| s.ext.has_exited_plan_mode)
}

pub fn set_has_exited_plan_mode(value: bool) {
    with_state_mut(|s| s.ext.has_exited_plan_mode = value);
}

pub fn needs_plan_mode_exit_attachment() -> bool {
    with_state(|s| s.ext.needs_plan_mode_exit_attachment)
}

pub fn set_needs_plan_mode_exit_attachment(value: bool) {
    with_state_mut(|s| s.ext.needs_plan_mode_exit_attachment = value);
}

pub fn handle_plan_mode_transition(from_mode: &str, to_mode: &str) {
    with_state_mut(|s| s.ext.handle_plan_mode_transition(from_mode, to_mode));
}

pub fn needs_auto_mode_exit_attachment() -> bool {
    with_state(|s| s.ext.needs_auto_mode_exit_attachment)
}

pub fn set_needs_auto_mode_exit_attachment(value: bool) {
    with_state_mut(|s| s.ext.needs_auto_mode_exit_attachment = value);
}

pub fn handle_auto_mode_transition(from_mode: &str, to_mode: &str) {
    with_state_mut(|s| s.ext.handle_auto_mode_transition(from_mode, to_mode));
}

pub fn has_shown_lsp_recommendation_this_session() -> bool {
    with_state(|s| s.ext.lsp_recommendation_shown_this_session)
}

pub fn set_lsp_recommendation_shown_this_session(value: bool) {
    with_state_mut(|s| s.ext.lsp_recommendation_shown_this_session = value);
}

pub fn set_init_json_schema(schema: serde_json::Value) {
    with_state_mut(|s| s.ext.init_json_schema = Some(schema));
}

pub fn get_init_json_schema() -> Option<serde_json::Value> {
    with_state(|s| s.ext.init_json_schema.clone())
}

pub fn register_hook_callbacks(hooks: HashMap<String, Vec<serde_json::Value>>) {
    with_state_mut(|s| s.ext.register_hook_callbacks(hooks));
}

pub fn register_plugin_hook_callbacks(hooks: HashMap<String, Vec<serde_json::Value>>) {
    with_state_mut(|s| s.ext.register_plugin_hook_callbacks(hooks));
}

pub fn get_registered_hooks() -> Option<HashMap<String, Vec<serde_json::Value>>> {
    with_state(|s| {
        let mut merged = s.ext.registered_hooks.clone().unwrap_or_default();
        if let Some(plugin_hooks) = &s.ext.registered_plugin_hooks {
            for (event, matchers) in plugin_hooks {
                merged
                    .entry(event.clone())
                    .or_insert_with(Vec::new)
                    .extend(matchers.clone());
            }
        }
        if merged.is_empty() {
            None
        } else {
            Some(merged)
        }
    })
}

pub fn clear_registered_hooks() {
    with_state_mut(|s| s.ext.clear_registered_hooks());
}

pub fn clear_registered_plugin_hooks() {
    with_state_mut(|s| s.ext.clear_registered_plugin_hooks());
}

pub fn reset_sdk_init_state() {
    with_state_mut(|s| s.ext.reset_sdk_init_state());
}

pub fn get_plan_slug_cache() -> HashMap<String, String> {
    with_state(|s| s.plan_slug_cache.clone())
}

pub fn get_session_created_teams() -> HashSet<String> {
    with_state(|s| s.base.session_created_teams.clone())
}

pub fn set_teleported_session_info(session_id: Option<String>) {
    with_state_mut(|s| s.ext.set_teleported_session_info(session_id));
}

pub fn get_teleported_session_info() -> Option<TeleportedSessionInfo> {
    with_state(|s| s.ext.teleported_session_info.clone())
}

pub fn mark_first_teleport_message_logged() {
    with_state_mut(|s| s.ext.mark_first_teleport_message_logged());
}

pub fn add_invoked_skill(
    skill_name: String,
    skill_path: String,
    content: String,
    agent_id: Option<String>,
) {
    let info = InvokedSkillInfo {
        skill_name: skill_name.clone(),
        skill_path,
        content,
        invoked_at: Utc::now().timestamp_millis(),
        agent_id,
    };
    with_state_mut(|s| {
        s.invoked_skills.insert(skill_name, info);
    });
}

pub fn get_invoked_skills() -> HashMap<String, InvokedSkillInfo> {
    with_state(|s| s.invoked_skills.clone())
}

pub fn get_invoked_skills_for_agent(agent_id: Option<&str>) -> HashMap<String, InvokedSkillInfo> {
    with_state(|s| {
        s.invoked_skills
            .iter()
            .filter(|(_, sk)| sk.agent_id.as_deref() == agent_id)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    })
}

pub fn clear_invoked_skills() {
    with_state_mut(|s| s.invoked_skills.clear());
}

pub fn clear_invoked_skills_for_agent(agent_id: &str) {
    with_state_mut(|s| {
        s.invoked_skills
            .retain(|_, sk| sk.agent_id.as_deref() != Some(agent_id));
    });
}

pub fn add_slow_operation(operation: String, duration_ms: u64) {
    with_state_mut(|s| s.ext.add_slow_operation(operation, duration_ms));
}

pub fn get_slow_operations() -> Vec<SlowOperation> {
    with_state_mut(|s| s.ext.get_slow_operations().to_vec())
}

pub fn get_main_thread_agent_type() -> Option<String> {
    with_state(|s| s.ext.main_thread_agent_type.clone())
}

pub fn set_main_thread_agent_type(agent_type: Option<String>) {
    with_state_mut(|s| s.ext.main_thread_agent_type = agent_type);
}

pub fn get_is_remote_mode() -> bool {
    with_state(|s| s.ext.is_remote_mode)
}

pub fn set_is_remote_mode(value: bool) {
    with_state_mut(|s| s.ext.is_remote_mode = value);
}

pub fn get_system_prompt_section_cache() -> HashMap<String, Option<String>> {
    with_state(|s| s.ext.system_prompt_section_cache.clone())
}

pub fn set_system_prompt_section_cache_entry(name: String, value: Option<String>) {
    with_state_mut(|s| s.ext.set_system_prompt_section_cache_entry(name, value));
}

pub fn clear_system_prompt_section_state() {
    with_state_mut(|s| s.ext.clear_system_prompt_section_state());
}

pub fn set_last_system_prompt_assembly(layers: Vec<SystemPromptAssemblyLayer>) {
    with_state_mut(|s| s.ext.last_system_prompt_assembly = layers);
}

pub fn get_last_system_prompt_assembly() -> Vec<SystemPromptAssemblyLayer> {
    with_state(|s| s.ext.last_system_prompt_assembly.clone())
}

pub fn set_last_effective_system_prompt_assembly(assembly: EffectiveSystemPromptAssembly) {
    with_state_mut(|s| s.ext.last_effective_system_prompt_assembly = Some(assembly));
}

pub fn get_last_effective_system_prompt_assembly() -> Option<EffectiveSystemPromptAssembly> {
    with_state(|s| s.ext.last_effective_system_prompt_assembly.clone())
}

pub fn get_last_emitted_date() -> Option<String> {
    with_state(|s| s.ext.last_emitted_date.clone())
}

pub fn set_last_emitted_date(date: Option<String>) {
    with_state_mut(|s| s.ext.last_emitted_date = date);
}

pub fn get_additional_directories_for_mossen_md() -> Vec<String> {
    with_state(|s| s.ext.additional_directories_for_mossen_md.clone())
}

pub fn set_additional_directories_for_mossen_md(dirs: Vec<String>) {
    with_state_mut(|s| s.ext.additional_directories_for_mossen_md = dirs);
}

pub fn get_allowed_channels() -> Vec<ChannelEntry> {
    with_state(|s| s.ext.allowed_channels.clone())
}

pub fn set_allowed_channels(entries: Vec<ChannelEntry>) {
    with_state_mut(|s| s.ext.allowed_channels = entries);
}

pub fn get_has_dev_channels() -> bool {
    with_state(|s| s.ext.has_dev_channels)
}

pub fn set_has_dev_channels(value: bool) {
    with_state_mut(|s| s.ext.has_dev_channels = value);
}

pub fn get_prompt_cache_1h_allowlist() -> Option<Vec<String>> {
    with_state(|s| s.ext.prompt_cache_1h_allowlist.clone())
}

pub fn set_prompt_cache_1h_allowlist(allowlist: Option<Vec<String>>) {
    with_state_mut(|s| s.ext.prompt_cache_1h_allowlist = allowlist);
}

pub fn get_prompt_cache_1h_eligible() -> Option<bool> {
    with_state(|s| s.ext.prompt_cache_1h_eligible)
}

pub fn set_prompt_cache_1h_eligible(eligible: Option<bool>) {
    with_state_mut(|s| s.ext.prompt_cache_1h_eligible = eligible);
}

pub fn get_afk_mode_header_latched() -> Option<bool> {
    with_state(|s| s.ext.afk_mode_header_latched)
}

pub fn set_afk_mode_header_latched(v: bool) {
    with_state_mut(|s| s.ext.afk_mode_header_latched = Some(v));
}

pub fn get_fast_mode_header_latched() -> Option<bool> {
    with_state(|s| s.ext.fast_mode_header_latched)
}

pub fn set_fast_mode_header_latched(v: bool) {
    with_state_mut(|s| s.ext.fast_mode_header_latched = Some(v));
}

pub fn get_cache_editing_header_latched() -> Option<bool> {
    with_state(|s| s.ext.cache_editing_header_latched)
}

pub fn set_cache_editing_header_latched(v: bool) {
    with_state_mut(|s| s.ext.cache_editing_header_latched = Some(v));
}

pub fn get_thinking_clear_latched() -> Option<bool> {
    with_state(|s| s.ext.thinking_clear_latched)
}

pub fn set_thinking_clear_latched(v: bool) {
    with_state_mut(|s| s.ext.thinking_clear_latched = Some(v));
}

pub fn clear_beta_header_latches() {
    with_state_mut(|s| s.ext.clear_beta_header_latches());
}

pub fn get_prompt_id() -> Option<String> {
    with_state(|s| s.ext.prompt_id.clone())
}

pub fn set_prompt_id(id: Option<String>) {
    with_state_mut(|s| s.ext.prompt_id = id);
}

// ---- 会话切换订阅 ----
//
// 真实实现：维护一个全局监听器列表；当上层调用 `notify_session_switch`
// 时，逐一回调所有已注册的监听器。`on_session_switch` 返回一个
// `FnOnce` unsubscribe 闭包，调用即可移除监听器。

pub type SessionSwitchListener = Box<dyn Fn(&str) + Send + Sync>;

static SESSION_SWITCH_LISTENERS: Lazy<RwLock<Vec<SessionSwitchListener>>> =
    Lazy::new(|| RwLock::new(Vec::new()));

/// 订阅 session 切换事件。返回一个 unsubscribe 闭包。
pub fn on_session_switch<F: Fn(&str) + Send + Sync + 'static>(listener: F) -> impl FnOnce() {
    let boxed: SessionSwitchListener = Box::new(listener);
    let mut list = SESSION_SWITCH_LISTENERS.write().expect("listener poisoned");
    let idx = list.len();
    list.push(boxed);
    drop(list);
    move || {
        let mut list = SESSION_SWITCH_LISTENERS.write().expect("listener poisoned");
        if idx < list.len() {
            let _ = list.remove(idx);
        }
    }
}

/// 广播 session 切换事件给所有已订阅的监听器。
///
/// 对应 TS bootstrap/state.ts 中的 `notifySessionSwitch(newId)`。
pub fn notify_session_switch(new_session_id: &str) {
    let list = match SESSION_SWITCH_LISTENERS.read() {
        Ok(l) => l,
        Err(_) => return,
    };
    for listener in list.iter() {
        listener(new_session_id);
    }
}
