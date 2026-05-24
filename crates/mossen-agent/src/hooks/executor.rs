//! # executor — Hook 执行引擎
//!
//! 对应 TS `utils/hooks.ts` 中的 Hook 执行逻辑。
//! 负责并发执行 Hook、超时控制、错误隔离。
//!
//! 按文档 12 命名：
//! - `executeStopHooks()` → `fire_halt_watchers()`
//! - `executeStopFailureHooks()` → `fire_fault_watchers()`

use std::time::Duration;

use mossen_types::hooks::{HookEvent, HookOutcome};
use tokio::time::timeout;
use tracing::{debug, warn};

use super::settings::{HookCommand, IndividualHookConfig};

/// Hook 执行结果 — 单个 Hook 执行的结果。
#[derive(Debug, Clone)]
pub struct HookExecResult {
    /// Hook 命令配置。
    pub hook: HookCommand,
    /// 执行结果状态。
    pub outcome: HookOutcome,
    /// 标准输出。
    pub stdout: String,
    /// 标准错误。
    pub stderr: String,
    /// 退出码（仅 Command 类型）。
    pub exit_code: Option<i32>,
    /// 阻塞错误消息。
    pub blocking_error: Option<String>,
    /// 是否阻止继续。
    pub prevent_continuation: bool,
    /// 停止原因。
    pub stop_reason: Option<String>,
    /// JSON 输出（解析后的 Hook 响应）。
    pub json_output: Option<serde_json::Value>,
}

/// Hook 执行器配置。
#[derive(Debug, Clone)]
pub struct HookExecutorConfig {
    /// 默认超时（秒）。
    pub default_timeout_secs: f64,
    /// 最大并发 Hook 数。
    pub max_concurrent: usize,
    /// 是否启用错误隔离。
    pub error_isolation: bool,
}

impl Default for HookExecutorConfig {
    fn default() -> Self {
        Self {
            default_timeout_secs: 600.0, // 10 分钟，与 TS TOOL_HOOK_EXECUTION_TIMEOUT_MS 一致
            max_concurrent: 10,
            error_isolation: true,
        }
    }
}

/// Hook 执行器 — 负责执行 Hook 并收集结果。
///
/// 对应 TS 中的 Hook 执行逻辑（分散在多个文件中）。
pub struct HookExecutor {
    config: HookExecutorConfig,
}

impl HookExecutor {
    /// 创建新的 Hook 执行器。
    pub fn new(config: HookExecutorConfig) -> Self {
        Self { config }
    }

    /// 触发停止观察者 — 执行 Stop 事件的所有 Hook。
    ///
    /// 对应 TS `executeStopHooks()` → Rust `fire_halt_watchers()`。
    pub async fn fire_halt_watchers(
        &self,
        hooks: &[IndividualHookConfig],
        json_input: &str,
        abort_signal: tokio_util::sync::CancellationToken,
    ) -> Vec<HookExecResult> {
        self.execute_hooks(HookEvent::Stop, hooks, json_input, abort_signal)
            .await
    }

    /// 触发故障观察者 — 执行 StopFailure 事件的所有 Hook。
    ///
    /// 对应 TS `executeStopFailureHooks()` → Rust `fire_fault_watchers()`。
    /// Fire-and-forget — Hook 输出和退出码被忽略。
    pub async fn fire_fault_watchers(
        &self,
        hooks: &[IndividualHookConfig],
        json_input: &str,
        abort_signal: tokio_util::sync::CancellationToken,
    ) {
        let _results = self
            .execute_hooks(HookEvent::StopFailure, hooks, json_input, abort_signal)
            .await;
        // StopFailure 是 fire-and-forget，忽略结果
    }

    /// 通用 Hook 执行 — 并发执行一组 Hook。
    ///
    /// Guard: 执行前检查条件（if 字段）
    /// Reset: 执行后清理资源
    /// Exception: 错误隔离，单个 Hook 失败不影响其他
    pub async fn execute_hooks(
        &self,
        event: HookEvent,
        hooks: &[IndividualHookConfig],
        json_input: &str,
        abort_signal: tokio_util::sync::CancellationToken,
    ) -> Vec<HookExecResult> {
        if hooks.is_empty() {
            return vec![];
        }

        debug!(
            event = %event,
            hook_count = hooks.len(),
            "Executing hooks for event"
        );

        let mut results = Vec::with_capacity(hooks.len());

        for hook_config in hooks {
            if abort_signal.is_cancelled() {
                debug!("Hook execution aborted by cancellation token");
                break;
            }

            // Guard: 检查条件
            if let Some(condition) = hook_config.config.condition() {
                if !self.evaluate_condition(condition, json_input) {
                    debug!(
                        hook = hook_config.config.display_text(),
                        condition = condition,
                        "Hook condition not met, skipping"
                    );
                    continue;
                }
            }

            let timeout_secs = hook_config
                .config
                .timeout_secs()
                .unwrap_or(self.config.default_timeout_secs);
            let timeout_duration = Duration::from_secs_f64(timeout_secs);

            // 执行 Hook（带超时和错误隔离）
            let result = if self.config.error_isolation {
                match timeout(
                    timeout_duration,
                    self.execute_single_hook(hook_config, json_input),
                )
                .await
                {
                    Ok(r) => r,
                    Err(_) => {
                        warn!(
                            hook = hook_config.config.display_text(),
                            timeout_secs = timeout_secs,
                            "Hook execution timed out"
                        );
                        HookExecResult {
                            hook: hook_config.config.clone(),
                            outcome: HookOutcome::Cancelled,
                            stdout: String::new(),
                            stderr: format!("Hook timed out after {timeout_secs}s"),
                            exit_code: None,
                            blocking_error: None,
                            prevent_continuation: false,
                            stop_reason: None,
                            json_output: None,
                        }
                    }
                }
            } else {
                self.execute_single_hook(hook_config, json_input).await
            };

            results.push(result);
        }

        results
    }

    /// 执行单个 Hook。
    async fn execute_single_hook(
        &self,
        hook_config: &IndividualHookConfig,
        json_input: &str,
    ) -> HookExecResult {
        debug!(
            hook_type = hook_config.config.type_name(),
            hook = hook_config.config.display_text(),
            "Executing hook"
        );

        match &hook_config.config {
            HookCommand::Command { command, shell, .. } => {
                self.execute_command_hook(command, shell.as_deref(), json_input)
                    .await
            }
            HookCommand::Http {
                url,
                headers,
                allowed_env_vars,
                ..
            } => {
                self.execute_http_hook(
                    url,
                    headers.as_ref(),
                    allowed_env_vars.as_deref(),
                    json_input,
                )
                .await
            }
            HookCommand::Prompt {
                prompt,
                model,
                timeout,
                ..
            } => {
                let cfg = super::exec_prompt::PromptHookConfig {
                    prompt: prompt.clone(),
                    model: model.clone(),
                    timeout_secs: *timeout,
                };
                let res =
                    super::exec_prompt::exec_prompt_hook(&cfg, json_input, "prompt_hook").await;
                HookExecResult {
                    hook: hook_config.config.clone(),
                    outcome: res.outcome,
                    stdout: res.response_text.clone().unwrap_or_default(),
                    stderr: res.blocking_error.clone().unwrap_or_default(),
                    exit_code: None,
                    blocking_error: res.blocking_error,
                    prevent_continuation: res.prevent_continuation,
                    stop_reason: res.stop_reason,
                    json_output: res
                        .response_text
                        .as_deref()
                        .and_then(|s| serde_json::from_str(s).ok()),
                }
            }
            HookCommand::Agent {
                prompt,
                model,
                timeout,
                ..
            } => {
                let cfg = super::exec_agent::AgentHookConfig {
                    prompt: prompt.clone(),
                    model: model.clone(),
                    timeout_secs: *timeout,
                };
                let res = super::exec_agent::exec_agent_hook(&cfg, json_input, "agent_hook").await;
                let stop_reason = res
                    .structured_output
                    .as_ref()
                    .and_then(|s| s.reason.clone());
                HookExecResult {
                    hook: hook_config.config.clone(),
                    outcome: res.outcome,
                    stdout: res
                        .structured_output
                        .as_ref()
                        .map(|s| {
                            serde_json::to_string(&serde_json::json!({
                                "ok": s.ok,
                                "reason": s.reason,
                            }))
                            .unwrap_or_default()
                        })
                        .unwrap_or_default(),
                    stderr: res.blocking_error.clone().unwrap_or_default(),
                    exit_code: None,
                    blocking_error: res.blocking_error,
                    prevent_continuation: matches!(res.outcome, HookOutcome::Blocking),
                    stop_reason,
                    json_output: None,
                }
            }
        }
    }

    /// 执行 Command 类型 Hook。
    async fn execute_command_hook(
        &self,
        command: &str,
        shell: Option<&str>,
        json_input: &str,
    ) -> HookExecResult {
        let shell_cmd = shell.unwrap_or("bash");

        let output = tokio::process::Command::new(shell_cmd)
            .arg("-c")
            .arg(command)
            .env("HOOK_INPUT", json_input)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        let child = match output {
            Ok(c) => c,
            Err(e) => {
                return HookExecResult {
                    hook: HookCommand::Command {
                        command: command.to_string(),
                        shell: shell.map(String::from),
                        timeout: None,
                        condition: None,
                        once: None,
                    },
                    outcome: HookOutcome::NonBlockingError,
                    stdout: String::new(),
                    stderr: format!("Failed to spawn hook command: {e}"),
                    exit_code: None,
                    blocking_error: None,
                    prevent_continuation: false,
                    stop_reason: None,
                    json_output: None,
                };
            }
        };

        match child.wait_with_output().await {
            Ok(output) => {
                let exit_code = output.status.code().unwrap_or(-1);
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                // 解析 JSON 输出
                let json_output = stdout
                    .lines()
                    .find(|line| line.trim().starts_with('{'))
                    .and_then(|line| serde_json::from_str(line.trim()).ok());

                let (outcome, blocking_error) = match exit_code {
                    0 => (HookOutcome::Success, None),
                    2 => (HookOutcome::Blocking, Some(stderr.clone())),
                    _ => (HookOutcome::NonBlockingError, None),
                };

                HookExecResult {
                    hook: HookCommand::Command {
                        command: command.to_string(),
                        shell: shell.map(String::from),
                        timeout: None,
                        condition: None,
                        once: None,
                    },
                    outcome,
                    stdout,
                    stderr,
                    exit_code: Some(exit_code),
                    blocking_error,
                    prevent_continuation: exit_code == 2,
                    stop_reason: None,
                    json_output,
                }
            }
            Err(e) => HookExecResult {
                hook: HookCommand::Command {
                    command: command.to_string(),
                    shell: shell.map(String::from),
                    timeout: None,
                    condition: None,
                    once: None,
                },
                outcome: HookOutcome::NonBlockingError,
                stdout: String::new(),
                stderr: format!("Failed to wait for hook command: {e}"),
                exit_code: None,
                blocking_error: None,
                prevent_continuation: false,
                stop_reason: None,
                json_output: None,
            },
        }
    }

    /// 执行 HTTP 类型 Hook（委托到 exec_http 模块）。
    async fn execute_http_hook(
        &self,
        url: &str,
        headers: Option<&std::collections::HashMap<String, String>>,
        _allowed_env_vars: Option<&[String]>,
        json_input: &str,
    ) -> HookExecResult {
        let client = reqwest::Client::new();
        let mut request = client
            .post(url)
            .header("Content-Type", "application/json")
            .body(json_input.to_string());

        if let Some(hdrs) = headers {
            for (name, value) in hdrs {
                request = request.header(name.as_str(), value.as_str());
            }
        }

        match request.send().await {
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                let ok = status.is_success();

                let json_output: Option<serde_json::Value> = serde_json::from_str(&body).ok();

                HookExecResult {
                    hook: HookCommand::Http {
                        url: url.to_string(),
                        headers: headers.cloned(),
                        timeout: None,
                        condition: None,
                        allowed_env_vars: None,
                        once: None,
                    },
                    outcome: if ok {
                        HookOutcome::Success
                    } else {
                        HookOutcome::NonBlockingError
                    },
                    stdout: body,
                    stderr: String::new(),
                    exit_code: Some(status.as_u16() as i32),
                    blocking_error: None,
                    prevent_continuation: false,
                    stop_reason: None,
                    json_output,
                }
            }
            Err(e) => HookExecResult {
                hook: HookCommand::Http {
                    url: url.to_string(),
                    headers: headers.cloned(),
                    timeout: None,
                    condition: None,
                    allowed_env_vars: None,
                    once: None,
                },
                outcome: HookOutcome::NonBlockingError,
                stdout: String::new(),
                stderr: format!("HTTP hook error: {e}"),
                exit_code: None,
                blocking_error: None,
                prevent_continuation: false,
                stop_reason: None,
                json_output: None,
            },
        }
    }

    /// 评估 Hook 条件（`if` 字段）。
    ///
    /// 对应 TS `prepareIfConditionMatcher()`（`utils/hooks.ts:1385`）。支持：
    ///
    /// - `ToolName(glob)` —— 仅在 `tool_name` 匹配且 `tool_input.command`（或
    ///   `tool_input` 文本表示）匹配 glob 时通过。Bash 是最常见用法
    ///   (`Bash(git *)`)，其他工具按相同规则解析。
    /// - `ToolName` —— 不带括号时仅按工具名匹配。
    /// - `env(VAR=value)` —— 比较环境变量等于指定字面值；
    ///   `env(VAR)` 则仅要求变量存在且非空。
    ///
    /// 任何无法解析的表达式视为不通过（与 TS 中
    /// `if (!ifMatcher) return false` 一致）。
    fn evaluate_condition(&self, condition: &str, json_input: &str) -> bool {
        let cond = condition.trim();
        if cond.is_empty() {
            return true;
        }

        // env(VAR=value) / env(VAR) 形式。
        if let Some(rest) = cond.strip_prefix("env(").and_then(|s| s.strip_suffix(')')) {
            return match rest.split_once('=') {
                Some((var, expected)) => std::env::var(var.trim())
                    .map(|v| v == expected.trim())
                    .unwrap_or(false),
                None => std::env::var(rest.trim())
                    .map(|v| !v.is_empty())
                    .unwrap_or(false),
            };
        }

        // ToolName(glob) / ToolName 形式。
        let (cond_tool, cond_pattern) = match cond.find('(') {
            Some(open) if cond.ends_with(')') => {
                let tool = cond[..open].trim();
                let pat = &cond[open + 1..cond.len() - 1];
                (tool, Some(pat))
            }
            _ => (cond, None),
        };

        let parsed: serde_json::Value = match serde_json::from_str(json_input) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let tool_name = parsed.get("tool_name").and_then(|v| v.as_str());

        if tool_name != Some(cond_tool) {
            return false;
        }

        let Some(pattern) = cond_pattern else {
            return true;
        };
        if pattern.is_empty() {
            return true;
        }

        // 提取 tool_input 的可比较字符串：优先 `.command`（Bash 习惯），
        // 否则将整个 tool_input 序列化为 JSON 字符串。
        let tool_input = parsed.get("tool_input");
        let target = tool_input
            .and_then(|v| v.get("command"))
            .and_then(|c| c.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| tool_input.map(|v| v.to_string()).unwrap_or_default());

        glob_match(pattern, &target)
    }
}

/// 简易 glob 匹配：`*` 匹配任意字符序列，`?` 匹配单字符。
///
/// 对应 TS 中 `preparePermissionMatcher` 的 minimatch 行为简化版——足以覆盖
/// 实际使用中的 `Bash(git *)` / `Bash(npm test)` 这类工具命令前缀匹配。
fn glob_match(pattern: &str, text: &str) -> bool {
    fn matches(p: &[u8], t: &[u8]) -> bool {
        match (p.first(), t.first()) {
            (None, None) => true,
            (None, _) => false,
            (Some(b'*'), _) => {
                // 贪婪：尝试 0 或多次。
                if matches(&p[1..], t) {
                    return true;
                }
                if t.is_empty() {
                    return false;
                }
                matches(p, &t[1..])
            }
            (Some(b'?'), Some(_)) => matches(&p[1..], &t[1..]),
            (Some(a), Some(b)) if a == b => matches(&p[1..], &t[1..]),
            _ => false,
        }
    }
    matches(pattern.as_bytes(), text.as_bytes())
}

impl Default for HookExecutor {
    fn default() -> Self {
        Self::new(HookExecutorConfig::default())
    }
}
