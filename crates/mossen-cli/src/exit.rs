//! 退出流程 — 清理资源、保存状态、打印摘要。
//!
//! 对应 TS 的 cli/exit.ts 和 bootstrap/state.ts 中的退出逻辑。

use anyhow::Result;
use tracing::{error, info};

use crate::bootstrap::SharedBootstrapState;

/// 退出码常量。
pub mod codes {
    /// 正常退出。
    pub const OK: i32 = 0;
    /// 通用错误退出。
    pub const ERROR: i32 = 1;
    /// 信号退出（128 + signal number）。
    pub const SIGNAL_INT: i32 = 130;
}

/// 正常退出 — 对应 TS 的 cliOk()。
///
/// 执行清理流程后以 0 退出。
pub async fn cli_ok(state: &SharedBootstrapState) -> Result<()> {
    info!("cli_ok: initiating clean exit");
    run_cleanup(state, "prompt_input_exit").await?;
    print_session_summary(state);
    Ok(())
}

/// 错误退出 — 对应 TS 的 cliError()。
///
/// 打印错误信息，执行清理流程后以非零码退出。
pub async fn cli_error(state: &SharedBootstrapState, err: &anyhow::Error) -> i32 {
    error!("cli_error: {:#}", err);

    // 记录到内存错误日志
    if let Ok(mut s) = state.write() {
        s.log_error(format!("{:#}", err));
    }

    // 尝试执行清理（但不因清理失败而影响退出）
    if let Err(cleanup_err) = run_cleanup(state, "other").await {
        error!("cleanup failed during error exit: {:#}", cleanup_err);
    }

    print_session_summary(state);
    codes::ERROR
}

/// 执行清理流程。
///
/// 对应 TS 的 gracefulShutdown() 中的清理步骤：
/// 1. 保存会话 transcript
/// 2. 清理临时文件
/// 3. 释放 worktree（如果已创建）
/// 4. 停止后台任务
async fn run_cleanup(state: &SharedBootstrapState, session_end_reason: &str) -> Result<()> {
    let _span = tracing::info_span!("cleanup").entered();
    info!("cleanup: starting");

    // 1. 保存会话 transcript
    save_session_transcript(state).await?;

    // 2. 执行 SessionEnd hooks
    crate::session_hooks::run_session_end_hooks(state, session_end_reason, false).await;

    // 3. 清理临时文件
    cleanup_temp_files().await;

    // 4. 停止后台 MCP 服务器
    cleanup_mcp_servers().await;

    info!("cleanup: completed");
    Ok(())
}

/// 保存会话 transcript。
async fn save_session_transcript(state: &SharedBootstrapState) -> Result<()> {
    use mossen_agent::transcript::{default_transcript_dir, TranscriptManager};

    let (session_id, persistence_disabled, cwd, model_override) = {
        let s = state
            .read()
            .map_err(|e| anyhow::anyhow!("failed to read state: {}", e))?;
        (
            s.session_id.clone(),
            s.session_persistence_disabled,
            s.cwd.to_string_lossy().to_string(),
            s.model_override.clone(),
        )
    };

    if persistence_disabled {
        info!("session persistence disabled, skipping transcript save");
        return Ok(());
    }

    info!(session_id = %session_id, "saving session transcript");

    // 通过 mossen-agent::transcript 落盘空 transcript（消息列表由编排器
    // 在运行期持续写入；退出时只做最终 flush 以确保目录就位）。
    let dir = default_transcript_dir();
    let mut manager = TranscriptManager::new(session_id.clone(), dir);

    // 调用 record（即使为空也会创建目录与 metadata）；若 manager 缓存
    // 中已有更长前缀则会自动跳过。
    if let Err(e) = manager
        .record(&[], model_override.as_deref(), Some(cwd.as_str()))
        .await
    {
        // 退出阶段：transcript 失败不应阻塞退出
        tracing::warn!(error = %e, "failed to flush transcript on exit");
    }

    Ok(())
}

/// 清理临时文件。
///
/// 清空 `.mossensrc/tmp/` 目录中的临时文件（保留目录本身）。
async fn cleanup_temp_files() {
    info!("cleaning up temporary files");

    // 优先使用 cwd 下的 .mossensrc/tmp；这是 TS 端写入临时文件的位置
    let candidates = vec![
        std::path::PathBuf::from(".mossensrc/tmp"),
        mossen_utils::env::get_mossen_config_home_dir().join("tmp"),
    ];

    for tmp_dir in candidates {
        if !tmp_dir.exists() {
            continue;
        }
        let mut entries = match tokio::fs::read_dir(&tmp_dir).await {
            Ok(it) => it,
            Err(e) => {
                tracing::debug!(path = %tmp_dir.display(), error = %e, "skip tmp cleanup");
                continue;
            }
        };
        let mut removed = 0u64;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let metadata = match entry.metadata().await {
                Ok(m) => m,
                Err(_) => continue,
            };
            let res = if metadata.is_dir() {
                tokio::fs::remove_dir_all(&path).await
            } else {
                tokio::fs::remove_file(&path).await
            };
            if res.is_ok() {
                removed += 1;
            }
        }
        info!(path = %tmp_dir.display(), removed, "tmp files removed");
    }
}

/// 清理 MCP 服务器连接。
async fn cleanup_mcp_servers() {
    info!("shutting down MCP server connections");
    if let Some(manager) = crate::repl_mcp::get_manager() {
        manager.disconnect_all().await;
        crate::repl_mcp::clear_manager();
        info!("MCP server connections closed");
    } else {
        tracing::debug!("no active MCP server manager registered");
    }
}

/// 打印会话摘要。
fn print_session_summary(state: &SharedBootstrapState) {
    let s = match state.read() {
        Ok(s) => s,
        Err(_) => return,
    };

    let duration_secs = s.total_duration_ms() / 1000;
    let minutes = duration_secs / 60;
    let seconds = duration_secs % 60;

    // 仅在交互式模式下打印摘要
    if s.is_interactive {
        let cost_str = if s.total_cost_usd > 0.0 {
            format!(" | cost: ${:.4}", s.total_cost_usd)
        } else {
            String::new()
        };

        let lines_str = if s.total_lines_added > 0 || s.total_lines_removed > 0 {
            format!(
                " | +{} -{} lines",
                s.total_lines_added, s.total_lines_removed
            )
        } else {
            String::new()
        };

        info!(
            "session summary: {}m{}s{}{}",
            minutes, seconds, cost_str, lines_str
        );
    }
}
