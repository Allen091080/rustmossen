use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::string_utils::prefix_chars;

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PromptInputMode {
    Prompt,
    Bash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentBlockParam {
    Text { text: String },
    Image { source: ImageSource },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PastedContent {
    pub id: usize,
    pub content: String,
    pub media_type: Option<String>,
    pub dimensions: Option<ImageDimensions>,
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IDESelection {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    User(UserMessage),
    Assistant(AssistantMessage),
    Attachment(AttachmentMessage),
    System(SystemMessage),
    Progress(ProgressMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub content: UserContent,
    pub uuid: Option<String>,
    pub image_paste_ids: Option<Vec<usize>>,
    pub permission_mode: Option<String>,
    pub is_meta: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserContent {
    Text(String),
    Blocks(Vec<ContentBlockParam>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMessage {
    pub attachment: Attachment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub attachment_type: String,
    pub content: Option<String>,
    pub hook_name: Option<String>,
    pub tool_use_id: Option<String>,
    pub hook_event: Option<String>,
    pub agent_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMessage {
    pub content: String,
    pub level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressMessage {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessUserInputBaseResult {
    pub messages: Vec<Message>,
    pub should_query: bool,
    pub allowed_tools: Option<Vec<String>>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub result_text: Option<String>,
    pub next_input: Option<String>,
    pub submit_next_input: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct ProcessUserInputParams {
    pub input: UserContent,
    pub pre_expansion_input: Option<String>,
    pub mode: PromptInputMode,
    pub pasted_contents: Option<HashMap<usize, PastedContent>>,
    pub ide_selection: Option<IDESelection>,
    pub messages: Option<Vec<Message>>,
    pub uuid: Option<String>,
    pub is_already_processing: Option<bool>,
    pub query_source: Option<String>,
    pub skip_slash_commands: Option<bool>,
    pub bridge_origin: Option<bool>,
    pub is_meta: Option<bool>,
    pub skip_attachments: Option<bool>,
}

pub struct HookResult {
    pub message: Option<AttachmentMessage>,
    pub blocking_error: Option<String>,
    pub prevent_continuation: bool,
    pub stop_reason: Option<String>,
    pub additional_contexts: Option<Vec<String>>,
}

// ─── Constants ───────────────────────────────────────────────────────────────

const MAX_HOOK_OUTPUT_LENGTH: usize = 10000;

// ─── Helper Functions ────────────────────────────────────────────────────────

fn apply_truncation(content: &str) -> String {
    if content.chars().count() > MAX_HOOK_OUTPUT_LENGTH {
        format!(
            "{}… [output truncated - exceeded {} characters]",
            prefix_chars(content, MAX_HOOK_OUTPUT_LENGTH),
            MAX_HOOK_OUTPUT_LENGTH
        )
    } else {
        content.to_string()
    }
}

fn get_content_text(input: &UserContent) -> Option<String> {
    match input {
        UserContent::Text(s) => {
            if s.is_empty() {
                None
            } else {
                Some(s.clone())
            }
        }
        UserContent::Blocks(blocks) => {
            for block in blocks {
                if let ContentBlockParam::Text { text } = block {
                    if !text.is_empty() {
                        return Some(text.clone());
                    }
                }
            }
            None
        }
    }
}

fn create_user_message(
    content: UserContent,
    uuid: Option<String>,
    image_paste_ids: Option<Vec<usize>>,
    permission_mode: Option<String>,
    is_meta: Option<bool>,
) -> UserMessage {
    UserMessage {
        content,
        uuid,
        image_paste_ids,
        permission_mode,
        is_meta,
    }
}

fn create_system_message(content: &str, level: &str) -> SystemMessage {
    SystemMessage {
        content: content.to_string(),
        level: level.to_string(),
    }
}

fn create_command_input_message(content: &str) -> UserMessage {
    UserMessage {
        content: UserContent::Text(content.to_string()),
        uuid: None,
        image_paste_ids: None,
        permission_mode: None,
        is_meta: Some(true),
    }
}

fn is_valid_image_paste(pasted: &PastedContent) -> bool {
    let media = pasted.media_type.as_deref().unwrap_or("");
    media.starts_with("image/")
}

fn parse_slash_command(input: &str) -> Option<(String, String)> {
    if !input.starts_with('/') {
        return None;
    }
    let trimmed = &input[1..];
    let (cmd, rest) = match trimmed.find(|c: char| c.is_whitespace()) {
        Some(idx) => (
            trimmed[..idx].to_string(),
            trimmed[idx..].trim().to_string(),
        ),
        None => (trimmed.to_string(), String::new()),
    };
    Some((cmd, rest))
}

fn matches_negative_keyword(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    matches!(
        lower.as_str(),
        "no" | "nope" | "don't" | "dont" | "stop" | "cancel" | "nevermind" | "never mind" | "abort"
    )
}

fn matches_keep_going_keyword(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    matches!(
        lower.as_str(),
        "continue"
            | "keep going"
            | "go on"
            | "proceed"
            | "yes"
            | "yep"
            | "yeah"
            | "sure"
            | "ok"
            | "okay"
            | "go ahead"
            | "do it"
    )
}

fn is_bridge_safe_command(cmd_name: &str) -> bool {
    matches!(
        cmd_name,
        "help" | "clear" | "compact" | "cost" | "status" | "model" | "think" | "plan" | "ultraplan"
    )
}

// ─── Image Processing ────────────────────────────────────────────────────────

pub fn create_image_metadata_text(
    dimensions: &ImageDimensions,
    source_path: Option<&str>,
) -> Option<String> {
    let dim_str = format!("{}x{}", dimensions.width, dimensions.height);
    match source_path {
        Some(path) => Some(format!("[Image: {} from {}]", dim_str, path)),
        None => Some(format!("[Image: {}]", dim_str)),
    }
}

pub async fn maybe_resize_and_downsample_image_block(
    block: &ContentBlockParam,
) -> (ContentBlockParam, Option<ImageDimensions>) {
    // In Rust, actual image resizing would use the `image` crate.
    // Return as-is with no dimensions change for now - the actual resize
    // logic depends on API limits (max 1568px on longest side).
    match block {
        ContentBlockParam::Image { source } => {
            let dimensions = None; // Would decode and measure in production
            (
                ContentBlockParam::Image {
                    source: source.clone(),
                },
                dimensions,
            )
        }
        other => (other.clone(), None),
    }
}

// ─── Main Entry Point ────────────────────────────────────────────────────────

pub async fn process_user_input(params: ProcessUserInputParams) -> ProcessUserInputBaseResult {
    let _input_string = match &params.input {
        UserContent::Text(s) => Some(s.clone()),
        UserContent::Blocks(_) => None,
    };

    if matches!(params.mode, PromptInputMode::Prompt) && !params.is_meta.unwrap_or(false) {
        if let Some(text) = get_content_text(&params.input) {
            observe_interactive_language(&text);
        }
    }

    let result = process_user_input_base(&params).await;

    if !result.should_query {
        return result;
    }

    // Execute UserPromptSubmit hooks
    let input_message = get_content_text(&params.input).unwrap_or_default();
    let hook_results = execute_user_prompt_submit_hooks(&input_message).await;

    let mut final_result = result;

    for hook_result in hook_results {
        // Skip progress messages
        if hook_result
            .message
            .as_ref()
            .map_or(false, |m| m.attachment.attachment_type == "progress")
        {
            continue;
        }

        // Handle blocking error
        if let Some(ref blocking_error) = hook_result.blocking_error {
            let blocking_message = get_user_prompt_submit_hook_blocking_message(blocking_error);
            let content_str = match &params.input {
                UserContent::Text(s) => s.clone(),
                UserContent::Blocks(_) => String::from("[complex content]"),
            };
            return ProcessUserInputBaseResult {
                messages: vec![Message::System(create_system_message(
                    &format!("{}\n\nOriginal prompt: {}", blocking_message, content_str),
                    "warning",
                ))],
                should_query: false,
                allowed_tools: final_result.allowed_tools,
                model: None,
                effort: None,
                result_text: None,
                next_input: None,
                submit_next_input: None,
            };
        }

        // Handle prevent continuation
        if hook_result.prevent_continuation {
            let message = match &hook_result.stop_reason {
                Some(reason) => format!("Operation stopped by hook: {}", reason),
                None => "Operation stopped by hook".to_string(),
            };
            final_result.messages.push(Message::User(UserMessage {
                content: UserContent::Text(message),
                uuid: None,
                image_paste_ids: None,
                permission_mode: None,
                is_meta: None,
            }));
            final_result.should_query = false;
            return final_result;
        }

        // Collect additional contexts
        if let Some(ref contexts) = hook_result.additional_contexts {
            if !contexts.is_empty() {
                final_result
                    .messages
                    .push(Message::Attachment(AttachmentMessage {
                        attachment: Attachment {
                            attachment_type: "hook_additional_context".to_string(),
                            content: Some(
                                contexts
                                    .iter()
                                    .map(|c| apply_truncation(c))
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                            ),
                            hook_name: Some("UserPromptSubmit".to_string()),
                            tool_use_id: Some(format!("hook-{}", Uuid::new_v4())),
                            hook_event: Some("UserPromptSubmit".to_string()),
                            agent_type: None,
                        },
                    }));
            }
        }

        // Handle hook message
        if let Some(msg) = hook_result.message {
            if msg.attachment.attachment_type == "hook_success" {
                if let Some(ref content) = msg.attachment.content {
                    if !content.is_empty() {
                        final_result
                            .messages
                            .push(Message::Attachment(AttachmentMessage {
                                attachment: Attachment {
                                    attachment_type: msg.attachment.attachment_type,
                                    content: Some(apply_truncation(content)),
                                    hook_name: msg.attachment.hook_name,
                                    tool_use_id: msg.attachment.tool_use_id,
                                    hook_event: msg.attachment.hook_event,
                                    agent_type: msg.attachment.agent_type,
                                },
                            }));
                    }
                }
            } else {
                final_result.messages.push(Message::Attachment(msg));
            }
        }
    }

    final_result
}

// ─── processUserInputBase ────────────────────────────────────────────────────

async fn process_user_input_base(params: &ProcessUserInputParams) -> ProcessUserInputBaseResult {
    let mut input_string: Option<String> = None;
    let mut preceding_input_blocks: Vec<ContentBlockParam> = Vec::new();
    let mut image_metadata_texts: Vec<String> = Vec::new();
    let mut normalized_input = params.input.clone();

    match &params.input {
        UserContent::Text(s) => {
            input_string = Some(s.clone());
        }
        UserContent::Blocks(blocks) if !blocks.is_empty() => {
            let mut processed_blocks: Vec<ContentBlockParam> = Vec::new();
            for block in blocks {
                if let ContentBlockParam::Image { .. } = block {
                    let (resized, dims) = maybe_resize_and_downsample_image_block(block).await;
                    if let Some(ref d) = dims {
                        if let Some(text) = create_image_metadata_text(d, None) {
                            image_metadata_texts.push(text);
                        }
                    }
                    processed_blocks.push(resized);
                } else {
                    processed_blocks.push(block.clone());
                }
            }
            normalized_input = UserContent::Blocks(processed_blocks.clone());

            // Extract text from last block
            if let Some(last) = processed_blocks.last() {
                if let ContentBlockParam::Text { text } = last {
                    input_string = Some(text.clone());
                    preceding_input_blocks =
                        processed_blocks[..processed_blocks.len() - 1].to_vec();
                } else {
                    preceding_input_blocks = processed_blocks;
                }
            }
        }
        _ => {}
    }

    if input_string.is_none() && !matches!(params.mode, PromptInputMode::Prompt) {
        return ProcessUserInputBaseResult {
            messages: vec![Message::System(create_system_message(
                "Mode requires a string input",
                "error",
            ))],
            should_query: false,
            allowed_tools: None,
            model: None,
            effort: None,
            result_text: None,
            next_input: None,
            submit_next_input: None,
        };
    }

    // Process pasted images
    let image_contents: Vec<&PastedContent> = params
        .pasted_contents
        .as_ref()
        .map(|pc| pc.values().filter(|p| is_valid_image_paste(p)).collect())
        .unwrap_or_default();

    let image_paste_ids: Vec<usize> = image_contents.iter().map(|img| img.id).collect();

    let mut image_content_blocks: Vec<ContentBlockParam> = Vec::new();
    for pasted_image in &image_contents {
        let image_block = ContentBlockParam::Image {
            source: ImageSource {
                source_type: "base64".to_string(),
                media_type: pasted_image
                    .media_type
                    .clone()
                    .unwrap_or_else(|| "image/png".to_string()),
                data: pasted_image.content.clone(),
            },
        };
        let (resized, dims) = maybe_resize_and_downsample_image_block(&image_block).await;

        if let Some(ref d) = dims {
            if let Some(text) = create_image_metadata_text(d, pasted_image.source_path.as_deref()) {
                image_metadata_texts.push(text);
            }
        } else if let Some(ref orig_dims) = pasted_image.dimensions {
            if let Some(text) =
                create_image_metadata_text(orig_dims, pasted_image.source_path.as_deref())
            {
                image_metadata_texts.push(text);
            }
        } else if let Some(ref path) = pasted_image.source_path {
            image_metadata_texts.push(format!("[Image source: {}]", path));
        }
        image_content_blocks.push(resized);
    }

    // Bridge-safe slash command handling
    let skip_slash = params.skip_slash_commands.unwrap_or(false);
    let bridge_origin = params.bridge_origin.unwrap_or(false);
    let mut effective_skip_slash = skip_slash;

    if bridge_origin {
        if let Some(ref istr) = input_string {
            if istr.starts_with('/') {
                if let Some((cmd_name, _)) = parse_slash_command(istr) {
                    if is_bridge_safe_command(&cmd_name) {
                        effective_skip_slash = false;
                    } else {
                        let msg = format!("/{} isn't available over Remote Control.", cmd_name);
                        return ProcessUserInputBaseResult {
                            messages: vec![
                                Message::User(create_user_message(
                                    UserContent::Text(istr.clone()),
                                    params.uuid.clone(),
                                    None,
                                    None,
                                    None,
                                )),
                                Message::User(create_command_input_message(&format!(
                                    "<local-command-stdout>{}</local-command-stdout>",
                                    msg
                                ))),
                            ],
                            should_query: false,
                            allowed_tools: None,
                            model: None,
                            effort: None,
                            result_text: Some(msg),
                            next_input: None,
                            submit_next_input: None,
                        };
                    }
                }
            }
        }
    }

    // Determine if we should extract attachments
    let skip_attachments = params.skip_attachments.unwrap_or(false);
    let should_extract_attachments = !skip_attachments
        && input_string.is_some()
        && (!matches!(params.mode, PromptInputMode::Prompt)
            || effective_skip_slash
            || !input_string.as_ref().unwrap().starts_with('/'));

    let attachment_messages: Vec<AttachmentMessage> = if should_extract_attachments {
        get_attachment_messages(input_string.as_deref().unwrap_or(""), &params.ide_selection).await
    } else {
        Vec::new()
    };

    // Bash mode
    if input_string.is_some() && matches!(params.mode, PromptInputMode::Bash) {
        let bash_result = process_bash_command(
            input_string.as_ref().unwrap(),
            &preceding_input_blocks,
            &attachment_messages,
        )
        .await;
        return add_image_metadata_message(bash_result, &image_metadata_texts);
    }

    // Slash commands
    if let Some(ref istr) = input_string {
        if !effective_skip_slash && istr.starts_with('/') {
            let slash_result = process_slash_command(
                istr,
                &preceding_input_blocks,
                &image_content_blocks,
                &attachment_messages,
                params.uuid.as_deref(),
            )
            .await;
            return add_image_metadata_message(slash_result, &image_metadata_texts);
        }
    }

    // Log agent mention
    if let Some(ref istr) = input_string {
        if matches!(params.mode, PromptInputMode::Prompt) {
            let _trimmed = istr.trim();
            // Agent mention logging - analytics only
        }
    }

    // Regular text prompt
    let text_result = process_text_prompt(
        &normalized_input,
        &image_content_blocks,
        &image_paste_ids,
        &attachment_messages,
        params.uuid.as_deref(),
        None, // permission_mode
        params.is_meta,
    );
    add_image_metadata_message(text_result, &image_metadata_texts)
}

// ─── processTextPrompt ───────────────────────────────────────────────────────

pub fn process_text_prompt(
    input: &UserContent,
    image_content_blocks: &[ContentBlockParam],
    image_paste_ids: &[usize],
    attachment_messages: &[AttachmentMessage],
    uuid: Option<&str>,
    permission_mode: Option<&str>,
    is_meta: Option<bool>,
) -> ProcessUserInputBaseResult {
    let _prompt_id = Uuid::new_v4().to_string();

    let user_prompt_text = match input {
        UserContent::Text(s) => s.clone(),
        UserContent::Blocks(blocks) => blocks
            .iter()
            .find_map(|b| {
                if let ContentBlockParam::Text { text } = b {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default(),
    };

    let _is_negative = matches_negative_keyword(&user_prompt_text);
    let _is_keep_going = matches_keep_going_keyword(&user_prompt_text);

    // If we have pasted images, create a message with image content
    if !image_content_blocks.is_empty() {
        let text_content: Vec<ContentBlockParam> = match input {
            UserContent::Text(s) => {
                if s.trim().is_empty() {
                    Vec::new()
                } else {
                    vec![ContentBlockParam::Text { text: s.clone() }]
                }
            }
            UserContent::Blocks(blocks) => blocks.clone(),
        };

        let mut combined = text_content;
        combined.extend(image_content_blocks.iter().cloned());

        let user_message = create_user_message(
            UserContent::Blocks(combined),
            uuid.map(|s| s.to_string()),
            if image_paste_ids.is_empty() {
                None
            } else {
                Some(image_paste_ids.to_vec())
            },
            permission_mode.map(|s| s.to_string()),
            if is_meta.unwrap_or(false) {
                Some(true)
            } else {
                None
            },
        );

        let mut messages: Vec<Message> = vec![Message::User(user_message)];
        messages.extend(attachment_messages.iter().cloned().map(Message::Attachment));

        return ProcessUserInputBaseResult {
            messages,
            should_query: true,
            allowed_tools: None,
            model: None,
            effort: None,
            result_text: None,
            next_input: None,
            submit_next_input: None,
        };
    }

    let user_message = create_user_message(
        input.clone(),
        uuid.map(|s| s.to_string()),
        None,
        permission_mode.map(|s| s.to_string()),
        if is_meta.unwrap_or(false) {
            Some(true)
        } else {
            None
        },
    );

    let mut messages: Vec<Message> = vec![Message::User(user_message)];
    messages.extend(attachment_messages.iter().cloned().map(Message::Attachment));

    ProcessUserInputBaseResult {
        messages,
        should_query: true,
        allowed_tools: None,
        model: None,
        effort: None,
        result_text: None,
        next_input: None,
        submit_next_input: None,
    }
}

// ─── addImageMetadataMessage ─────────────────────────────────────────────────

fn add_image_metadata_message(
    mut result: ProcessUserInputBaseResult,
    image_metadata_texts: &[String],
) -> ProcessUserInputBaseResult {
    if !image_metadata_texts.is_empty() {
        let blocks: Vec<ContentBlockParam> = image_metadata_texts
            .iter()
            .map(|text| ContentBlockParam::Text { text: text.clone() })
            .collect();
        result.messages.push(Message::User(UserMessage {
            content: UserContent::Blocks(blocks),
            uuid: None,
            image_paste_ids: None,
            permission_mode: None,
            is_meta: Some(true),
        }));
    }
    result
}

// ─── Stub async helpers (depend on external modules) ─────────────────────────

fn observe_interactive_language(_text: &str) {
    // Language detection for analytics
}

async fn execute_user_prompt_submit_hooks(_input: &str) -> Vec<HookResult> {
    // Hook execution - returns results from registered hooks
    Vec::new()
}

fn get_user_prompt_submit_hook_blocking_message(error: &str) -> String {
    format!("Blocked by hook: {}", error)
}

async fn get_attachment_messages(
    _input: &str,
    _ide_selection: &Option<IDESelection>,
) -> Vec<AttachmentMessage> {
    Vec::new()
}

async fn process_bash_command(
    input: &str,
    _preceding_blocks: &[ContentBlockParam],
    _attachment_messages: &[AttachmentMessage],
) -> ProcessUserInputBaseResult {
    ProcessUserInputBaseResult {
        messages: vec![Message::User(UserMessage {
            content: UserContent::Text(format!("$ {}", input)),
            uuid: None,
            image_paste_ids: None,
            permission_mode: None,
            is_meta: None,
        })],
        should_query: true,
        allowed_tools: None,
        model: None,
        effort: None,
        result_text: None,
        next_input: None,
        submit_next_input: None,
    }
}

async fn process_slash_command(
    input: &str,
    _preceding_blocks: &[ContentBlockParam],
    _image_blocks: &[ContentBlockParam],
    _attachment_messages: &[AttachmentMessage],
    uuid: Option<&str>,
) -> ProcessUserInputBaseResult {
    if let Some((cmd, args)) = parse_slash_command(input) {
        ProcessUserInputBaseResult {
            messages: vec![Message::User(create_user_message(
                UserContent::Text(format!("/{} {}", cmd, args)),
                uuid.map(|s| s.to_string()),
                None,
                None,
                None,
            ))],
            should_query: true,
            allowed_tools: None,
            model: None,
            effort: None,
            result_text: None,
            next_input: None,
            submit_next_input: None,
        }
    } else {
        ProcessUserInputBaseResult {
            messages: vec![Message::System(create_system_message(
                "Unknown command",
                "error",
            ))],
            should_query: false,
            allowed_tools: None,
            model: None,
            effort: None,
            result_text: None,
            next_input: None,
            submit_next_input: None,
        }
    }
}

/// 对应 TS `ProcessUserInputContext`：处理用户输入时的上下文。
#[derive(Debug, Clone, Default)]
pub struct ProcessUserInputContext {
    pub session_id: String,
    pub cwd: String,
    pub mode: String,
}

/// 对应 TS `looksLikeCommand`：判断字符串是否像 slash 命令。
pub fn looks_like_command(input: &str) -> bool {
    input.starts_with('/')
}

/// 对应 TS `formatSkillLoadingMetadata`：把 skill 加载元数据格式化为字符串。
pub fn format_skill_loading_metadata(skill_name: &str, count: usize) -> String {
    format!("loaded skill {} ({} files)", skill_name, count)
}

/// 对应 TS `processPromptSlashCommand`：处理 `/command` 风格的 prompt。
pub async fn process_prompt_slash_command(input: &str) -> serde_json::Value {
    let trimmed = input.trim();
    let body = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let (cmd, args) = body
        .split_once(' ')
        .map(|(c, a)| (c.to_string(), a.to_string()))
        .unwrap_or((body.to_string(), String::new()));
    serde_json::json!({
        "command": cmd,
        "args": args,
    })
}
