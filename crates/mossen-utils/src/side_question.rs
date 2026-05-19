//! Side Question ("/btw") feature — allows asking quick questions without
//! interrupting the main agent context.
//!
//! Uses a forked agent to leverage prompt caching from the parent context
//! while keeping the side question response separate from main conversation.

use regex::Regex;
use once_cell::sync::Lazy;

/// Pattern to detect "/btw" at start of input (case-insensitive).
static BTW_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^/btw\b").expect("BTW_PATTERN regex should compile")
});

/// Legacy product name components, obfuscated via char codes.
fn external_text(codes: &[u32]) -> String {
    codes
        .iter()
        .filter_map(|&c| char::from_u32(c))
        .collect()
}

fn legacy_product_root() -> String {
    external_text(&[67, 108, 97, 117, 100, 101])
}

fn legacy_product_terms() -> Vec<String> {
    let root = legacy_product_root();
    let code = format!("{root} Code");
    let cli = format!("{root} CLI");
    let code_cli = format!("{code} CLI");
    vec![code_cli, code, cli, root]
}

/// Position of a "/btw" trigger in text.
#[derive(Debug, Clone)]
pub struct BtwTriggerPosition {
    pub word: String,
    pub start: usize,
    pub end: usize,
}

/// Find positions of "/btw" keyword at the start of text for highlighting.
pub fn find_btw_trigger_positions(text: &str) -> Vec<BtwTriggerPosition> {
    let mut positions = Vec::new();
    for m in BTW_PATTERN.find_iter(text) {
        positions.push(BtwTriggerPosition {
            word: m.as_str().to_string(),
            start: m.start(),
            end: m.end(),
        });
    }
    positions
}

/// Usage info from a side question API call.
#[derive(Debug, Clone, Default)]
pub struct NonNullableUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

/// Result of a side question.
#[derive(Debug, Clone)]
pub struct SideQuestionResult {
    pub response: Option<String>,
    pub usage: NonNullableUsage,
}

/// Build the wrapped side question prompt with instructions.
pub fn build_side_question_prompt(
    question: &str,
    is_chinese: bool,
    runtime_name: &str,
    assistant_name: &str,
) -> String {
    let instructions = if is_chinese {
        format!(
            r#"这是用户发来的旁路问题。你必须直接给出一次性答案。

重要上下文：
- 你是一个轻量的独立实例，只负责回答这个问题
- 主实例没有被打断，它会继续在后台工作
- 你共享上下文，但你是一个单独的实例
- 不要说自己被打断了，也不要说你"刚才正在做什么"
- 当你提到产品或运行环境时，使用"{assistant_name}"或"{runtime_name}"，不要使用旧产品名

关键约束：
- 你没有任何工具，不能读文件、执行命令、搜索代码，也不能采取动作
- 这是一次性回答，不会有后续轮次
- 你只能基于现有上下文直接回答
- 不要说"我来试试""我现在去检查""让我看看"这类会暗示采取动作的话
- 如果你不知道答案，就直接说明，不要承诺稍后去查

请直接用中文回答这个问题。"#
        )
    } else {
        format!(
            r#"This is a side question from the user. You must answer this question directly in a single response.

IMPORTANT CONTEXT:
- You are a separate, lightweight agent spawned to answer this one question
- The main agent is NOT interrupted - it continues working independently in the background
- You share the conversation context but are a completely separate instance
- Do NOT reference being interrupted or what you were "previously doing" - that framing is incorrect
- When referring to the product or runtime, use "{assistant_name}" or "{runtime_name}", never legacy product names

CRITICAL CONSTRAINTS:
- You have NO tools available - you cannot read files, run commands, search, or take any actions
- This is a one-off response - there will be no follow-up turns
- You can ONLY provide information based on what you already know from the conversation context
- NEVER say things like "Let me try...", "I'll now...", "Let me check...", or promise to take any action
- If you don't know the answer, say so - do not offer to look it up or investigate

Simply answer the question in English with the information you have."#
        )
    };

    format!("<system-reminder>{instructions}</system-reminder>\n\n{question}")
}

/// Normalize branding in a side question response, replacing legacy product names.
pub fn normalize_side_question_branding(
    response: Option<&str>,
    runtime_name: &str,
) -> Option<String> {
    let response = response?;

    let chinese_runtime = format!("本地{runtime_name}环境");
    let english_runtime = format!("{runtime_name} environment");
    let chinese_assistant = "专注于软件工程任务的编码助手";

    let mut normalized = response.to_string();
    for term in legacy_product_terms() {
        let escaped = regex::escape(&term);

        // Replace quoted variants
        let quoted_re = Regex::new(&format!(r#"[`"""]{}[`"""]"#, escaped))
            .unwrap_or_else(|_| Regex::new(r"(?!x)x").unwrap());
        normalized = quoted_re
            .replace_all(&normalized, runtime_name)
            .to_string();

        // Replace environment references (Chinese)
        let env_zh_re = Regex::new(&format!(r"{}\s*环境", escaped))
            .unwrap_or_else(|_| Regex::new(r"(?!x)x").unwrap());
        normalized = env_zh_re
            .replace_all(&normalized, chinese_runtime.as_str())
            .to_string();

        // Replace environment references (English)
        let env_en_re = Regex::new(&format!(r"{}\s*environment", escaped))
            .unwrap_or_else(|_| Regex::new(r"(?!x)x").unwrap());
        normalized = env_en_re
            .replace_all(&normalized, english_runtime.as_str())
            .to_string();

        // Exact string replacements
        normalized = normalized.replace(
            &format!("软件工程助手，运行在本地 {term} 环境中"),
            &format!("软件工程助手，运行在{chinese_runtime}中"),
        );
        normalized = normalized.replace(
            &format!("我运行在 {term} 环境中"),
            &format!("我运行在{chinese_runtime}中"),
        );
        normalized = normalized.replace(
            &format!("我运行在本地 {term} 环境中"),
            &format!("我运行在{chinese_runtime}中"),
        );
        normalized = normalized.replace(
            &format!("I run in the {term} environment"),
            &format!("I run in the {runtime_name} environment"),
        );
        normalized = normalized.replace(
            &format!("我是 {term}"),
            &format!("我是 {runtime_name}"),
        );
        normalized = normalized.replace(&term, runtime_name);
    }

    normalized = normalized.replace(
        "我是一个编码助手",
        &format!("我是 {runtime_name}，一个{chinese_assistant}"),
    );
    normalized = normalized.replace(
        "我是一个软件工程助手",
        &format!("我是 {runtime_name}，一个{chinese_assistant}"),
    );

    Some(normalized)
}

/// Content block type enum for message parsing.
#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text { text: String },
    ToolUse { name: String },
    Thinking { thinking: String },
}

/// Message type for side question response extraction.
#[derive(Debug, Clone)]
pub struct SideMessage {
    pub msg_type: String,
    pub content: Vec<ContentBlock>,
}

/// Extract a display string from forked agent messages.
///
/// With adaptive thinking enabled, a thinking response arrives as:
///   messages[0] = assistant { content: [thinking_block] }
///   messages[1] = assistant { content: [text_block] }
///
/// Must flatten all assistant content blocks across the per-block messages.
/// 对应 TS `runSideQuestion`：组装提示并通过 forked agent 调用。
///
/// 在 Rust 端我们尚未集成 `runForkedAgent` / `cacheSafeParams`，因此该函数把
/// 已有的纯函数组合起来，返回构建好的 prompt 与处理回调，由调用方驱动实际的
/// fork 调用。这保留了与 TS 同名同义入口，便于后续直接接线。
pub struct SideQuestionRunInputs {
    pub wrapped_prompt: String,
    pub is_chinese: bool,
    pub runtime_name: String,
    pub fork_label: &'static str,
    pub query_source: &'static str,
    pub max_turns: usize,
    pub skip_cache_write: bool,
}

/// 构造可直接传给 fork agent runner 的输入。
pub fn run_side_question(
    question: &str,
    runtime_name: &str,
    assistant_name: &str,
    interactive_language_tag: &str,
) -> SideQuestionRunInputs {
    let is_chinese = interactive_language_tag == "zh";
    let wrapped_prompt = build_side_question_prompt(
        question,
        is_chinese,
        runtime_name,
        assistant_name,
    );
    SideQuestionRunInputs {
        wrapped_prompt,
        is_chinese,
        runtime_name: runtime_name.to_string(),
        fork_label: "side_question",
        query_source: "side_question",
        max_turns: 1,
        skip_cache_write: true,
    }
}

pub fn extract_side_question_response(
    messages: &[SideMessage],
    is_chinese: bool,
) -> Option<String> {
    let assistant_blocks: Vec<&ContentBlock> = messages
        .iter()
        .filter(|m| m.msg_type == "assistant")
        .flat_map(|m| m.content.iter())
        .collect();

    if !assistant_blocks.is_empty() {
        // Concatenate all text blocks
        let text: String = assistant_blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n")
            .trim()
            .to_string();

        if !text.is_empty() {
            return Some(text);
        }

        // Check if the model tried to call a tool
        let tool_use = assistant_blocks.iter().find(|b| matches!(b, ContentBlock::ToolUse { .. }));
        if let Some(ContentBlock::ToolUse { name }) = tool_use {
            return Some(if is_chinese {
                format!("（模型尝试调用 {name}，而不是直接回答。请换一种问法，或回到主对话里提问。）")
            } else {
                format!("(The model tried to call {name} instead of answering directly. Try rephrasing or ask in the main conversation.)")
            });
        }
    }

    None
}
