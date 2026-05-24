//! # queue_processor — 队列处理器
//!
//! 对应 TypeScript `utils/queueProcessor.ts`。

use std::future::Future;
use std::pin::Pin;

/// 队列命令的内容块
#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text { text: String },
    Other { block_type: String },
}

/// 队列命令的值类型
#[derive(Debug, Clone)]
pub enum QueuedCommandValue {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// 队列中的命令
#[derive(Debug, Clone)]
pub struct QueuedCommand {
    pub value: QueuedCommandValue,
    pub mode: String,
    pub agent_id: Option<String>,
}

/// 处理队列的结果
#[derive(Debug, Clone)]
pub struct ProcessQueueResult {
    pub processed: bool,
}

/// 执行输入的回调类型
pub type ExecuteInputFn =
    Box<dyn Fn(Vec<QueuedCommand>) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// 处理队列的参数
pub struct ProcessQueueParams {
    pub execute_input: ExecuteInputFn,
}

/// 检查队列命令是否为斜杠命令（值以 '/' 开头）
fn is_slash_command(cmd: &QueuedCommand) -> bool {
    match &cmd.value {
        QueuedCommandValue::Text(s) => s.trim().starts_with('/'),
        QueuedCommandValue::Blocks(blocks) => {
            for block in blocks {
                if let ContentBlock::Text { text } = block {
                    return text.trim().starts_with('/');
                }
            }
            false
        }
    }
}

/// 从队列中查看下一个满足条件的命令（不移除）
fn peek(
    filter: impl Fn(&QueuedCommand) -> bool,
    queue: &[QueuedCommand],
) -> Option<&QueuedCommand> {
    queue.iter().find(|cmd| filter(cmd))
}

/// 从队列中取出第一个满足条件的命令
fn dequeue(
    filter: impl Fn(&QueuedCommand) -> bool,
    queue: &mut Vec<QueuedCommand>,
) -> Option<QueuedCommand> {
    let pos = queue.iter().position(|cmd| filter(cmd))?;
    Some(queue.remove(pos))
}

/// 从队列中取出所有满足条件的命令
fn dequeue_all_matching(
    filter: impl Fn(&QueuedCommand) -> bool,
    queue: &mut Vec<QueuedCommand>,
) -> Vec<QueuedCommand> {
    let mut matched = Vec::new();
    let mut i = 0;
    while i < queue.len() {
        if filter(&queue[i]) {
            matched.push(queue.remove(i));
        } else {
            i += 1;
        }
    }
    matched
}

/// 检查队列中是否有命令
pub fn has_commands_in_queue(queue: &[QueuedCommand]) -> bool {
    !queue.is_empty()
}

/// 处理队列中的命令。
///
/// 斜杠命令（以 '/' 开头）和 bash 模式命令逐个处理，每个单独通过 execute_input 路径。
/// Bash 命令需要单独处理以保持每个命令的错误隔离、退出码和进度 UI。
/// 其他非斜杠命令批量处理：与最高优先级项具有相同 mode 的所有项一次性取出，
/// 作为单个数组传递给 execute_input。不同 mode 永远不会混合。
///
/// 调用者负责确保当前没有查询正在运行，并在每个命令完成后再次调用此函数，
/// 直到队列为空。
pub fn process_queue_if_ready(
    params: &ProcessQueueParams,
    queue: &mut Vec<QueuedCommand>,
) -> ProcessQueueResult {
    let is_main_thread = |cmd: &QueuedCommand| cmd.agent_id.is_none();

    let next = match peek(&is_main_thread, queue) {
        Some(cmd) => cmd,
        None => return ProcessQueueResult { processed: false },
    };

    // 斜杠命令和 bash 模式命令逐个处理
    if is_slash_command(next) || next.mode == "bash" {
        let cmd = dequeue(&is_main_thread, queue).unwrap();
        let _fut = (params.execute_input)(vec![cmd]);
        return ProcessQueueResult { processed: true };
    }

    // 取出所有与最高优先级项同 mode 的非斜杠命令
    let target_mode = next.mode.clone();
    let commands = dequeue_all_matching(
        |cmd| is_main_thread(cmd) && !is_slash_command(cmd) && cmd.mode == target_mode,
        queue,
    );
    if commands.is_empty() {
        return ProcessQueueResult { processed: false };
    }

    let _fut = (params.execute_input)(commands);
    ProcessQueueResult { processed: true }
}

/// 检查队列中是否有待处理的命令。
/// 用于判断是否应触发队列处理。
pub fn has_queued_commands(queue: &[QueuedCommand]) -> bool {
    has_commands_in_queue(queue)
}
