//! YOLO (auto-mode) classifier for permission decisions.
//!
//! Translates `utils/permissions/yoloClassifier.ts` — transcript building,
//! system prompt assembly, 2-stage XML classifier, and the main
//! `classify_yolo_action` entry point.

use once_cell::sync::Lazy;
use regex::Regex;

use super::permission_result::ToolPermissionContext;

// ─── Types ───────────────────────────────────────────────────────────────────

/// A block within a transcript entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranscriptBlock {
    Text {
        text: String,
    },
    ToolUse {
        name: String,
        input: serde_json::Value,
    },
}

/// A single entry in the classifier transcript.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TranscriptEntry {
    pub role: String, // "user" or "assistant"
    pub content: Vec<TranscriptBlock>,
}

/// Auto mode rules from user settings.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AutoModeRules {
    pub allow: Vec<String>,
    pub soft_deny: Vec<String>,
    pub environment: Vec<String>,
}

/// Result of YOLO classifier invocation.
#[derive(Debug, Clone)]
pub struct YoloClassifierResult {
    pub should_block: bool,
    pub reason: String,
    pub model: Option<String>,
    pub unavailable: bool,
    pub transcript_too_long: bool,
    pub thinking: Option<String>,
    pub usage: Option<ClassifierUsage>,
    pub duration_ms: Option<u64>,
    pub error_dump_path: Option<String>,
    pub prompt_lengths: Option<PromptLengths>,
    pub stage: Option<String>,
    pub stage1_usage: Option<ClassifierUsage>,
    pub stage1_duration_ms: Option<u64>,
    pub stage1_request_id: Option<String>,
    pub stage1_msg_id: Option<String>,
    pub stage2_usage: Option<ClassifierUsage>,
    pub stage2_duration_ms: Option<u64>,
    pub stage2_request_id: Option<String>,
    pub stage2_msg_id: Option<String>,
}

/// Usage stats from classifier API call.
#[derive(Debug, Clone)]
pub struct ClassifierUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
}

/// Lengths of prompt components for telemetry.
#[derive(Debug, Clone)]
pub struct PromptLengths {
    pub system_prompt: usize,
    pub tool_calls: usize,
    pub user_prompts: usize,
}

/// Two-stage mode configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TwoStageMode {
    Both,
    Fast,
    Thinking,
}

/// Auto mode configuration from feature flags.
#[derive(Debug, Clone, Default)]
pub struct AutoModeConfig {
    pub model: Option<String>,
    pub two_stage_classifier: Option<TwoStageConfigValue>,
    pub force_external_permissions: bool,
    pub jsonl_transcript: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TwoStageConfigValue {
    Bool(bool),
    Fast,
    Thinking,
}

// ─── Constants ───────────────────────────────────────────────────────────────

pub const YOLO_CLASSIFIER_TOOL_NAME: &str = "classify_result";

/// Stage 1 suffix for immediate decision.
const XML_S1_SUFFIX: &str = "\nErr on the side of blocking. <block> immediately.";

/// Stage 2 suffix for chain-of-thought.
const XML_S2_SUFFIX: &str = "\nReview the classification process and follow it carefully, making sure you deny actions that should be blocked. As a reminder, explicit (not suggestive or implicit) user confirmation is required to override blocks. Use <thinking> before responding with <block>.";

/// PowerShell-specific deny guidance for the classifier.
pub const POWERSHELL_DENY_GUIDANCE: &[&str] = &[
    "PowerShell Download-and-Execute: `iex (iwr ...)`, `Invoke-Expression (Invoke-WebRequest ...)`, `Invoke-Expression (New-Object Net.WebClient).DownloadString(...)`, and any pipeline feeding remote content into `Invoke-Expression`/`iex` fall under \"Code from External\" — same as `curl | bash`.",
    "PowerShell Irreversible Destruction: `Remove-Item -Recurse -Force`, `rm -r -fo`, `Clear-Content`, and `Set-Content` truncation of pre-existing files fall under \"Irreversible Local Destruction\" — same as `rm -rf` and `> file`.",
    "PowerShell Persistence: modifying `$PROFILE` (any of the four profile paths), `Register-ScheduledTask`, `New-Service`, writing to registry Run keys, and WMI event subscriptions fall under \"Unauthorized Persistence\" — same as `.bashrc` edits and cron jobs.",
    "PowerShell Elevation: `Start-Process -Verb RunAs`, `-ExecutionPolicy Bypass`, and disabling AMSI/Defender fall under \"Security Weaken\".",
];

// ─── Transcript Building ─────────────────────────────────────────────────────

/// Message type for building transcripts.
#[derive(Debug, Clone)]
pub enum Message {
    User {
        content: MessageContent,
    },
    Assistant {
        content: Vec<AssistantBlock>,
    },
    Attachment {
        attachment_type: String,
        prompt: MessageContent,
    },
}

#[derive(Debug, Clone)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone)]
pub struct ContentBlock {
    pub block_type: String,
    pub text: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AssistantBlock {
    Text {
        text: String,
    },
    ToolUse {
        name: String,
        input: serde_json::Value,
    },
}

/// Build transcript entries from messages for the classifier.
pub fn build_transcript_entries(messages: &[Message]) -> Vec<TranscriptEntry> {
    let mut transcript = Vec::new();

    for msg in messages {
        match msg {
            Message::Attachment {
                attachment_type,
                prompt,
            } if attachment_type == "queued_command" => {
                let text = match prompt {
                    MessageContent::Text(s) => Some(s.clone()),
                    MessageContent::Blocks(blocks) => {
                        let texts: Vec<&str> = blocks
                            .iter()
                            .filter(|b| b.block_type == "text")
                            .filter_map(|b| b.text.as_deref())
                            .collect();
                        if texts.is_empty() {
                            None
                        } else {
                            Some(texts.join("\n"))
                        }
                    }
                };
                if let Some(t) = text {
                    transcript.push(TranscriptEntry {
                        role: "user".to_string(),
                        content: vec![TranscriptBlock::Text { text: t }],
                    });
                }
            }
            Message::User { content } => {
                let text_blocks: Vec<TranscriptBlock> = match content {
                    MessageContent::Text(s) => {
                        vec![TranscriptBlock::Text { text: s.clone() }]
                    }
                    MessageContent::Blocks(blocks) => blocks
                        .iter()
                        .filter(|b| b.block_type == "text")
                        .filter_map(|b| b.text.as_ref())
                        .map(|t| TranscriptBlock::Text { text: t.clone() })
                        .collect(),
                };
                if !text_blocks.is_empty() {
                    transcript.push(TranscriptEntry {
                        role: "user".to_string(),
                        content: text_blocks,
                    });
                }
            }
            Message::Assistant { content } => {
                let blocks: Vec<TranscriptBlock> = content
                    .iter()
                    .filter_map(|b| match b {
                        AssistantBlock::ToolUse { name, input } => Some(TranscriptBlock::ToolUse {
                            name: name.clone(),
                            input: input.clone(),
                        }),
                        _ => None,
                    })
                    .collect();
                if !blocks.is_empty() {
                    transcript.push(TranscriptEntry {
                        role: "assistant".to_string(),
                        content: blocks,
                    });
                }
            }
            _ => {}
        }
    }
    transcript
}

/// Serialize a transcript block to compact format.
pub fn to_compact_block(
    block: &TranscriptBlock,
    role: &str,
    tool_encoder: &dyn Fn(&str, &serde_json::Value) -> Option<String>,
    jsonl: bool,
) -> String {
    match block {
        TranscriptBlock::ToolUse { name, input } => {
            let encoded = tool_encoder(name, input);
            match encoded {
                Some(s) if s.is_empty() => String::new(),
                Some(s) => {
                    if jsonl {
                        format!(
                            "{{{:?}:{}}}\n",
                            name,
                            serde_json::to_string(&s).unwrap_or_default()
                        )
                    } else {
                        format!("{} {}\n", name, s)
                    }
                }
                None => {
                    let s = serde_json::to_string(input).unwrap_or_default();
                    if jsonl {
                        format!("{{{:?}:{}}}\n", name, s)
                    } else {
                        format!("{} {}\n", name, s)
                    }
                }
            }
        }
        TranscriptBlock::Text { text } if role == "user" => {
            if jsonl {
                format!(
                    "{{\"user\":{}}}\n",
                    serde_json::to_string(text).unwrap_or_default()
                )
            } else {
                format!("User: {}\n", text)
            }
        }
        _ => String::new(),
    }
}

/// Serialize a full transcript entry to compact format.
pub fn to_compact(
    entry: &TranscriptEntry,
    tool_encoder: &dyn Fn(&str, &serde_json::Value) -> Option<String>,
    jsonl: bool,
) -> String {
    entry
        .content
        .iter()
        .map(|b| to_compact_block(b, &entry.role, tool_encoder, jsonl))
        .collect()
}

/// Build a compact transcript string for the classifier.
pub fn build_transcript_for_classifier(
    messages: &[Message],
    tool_encoder: &dyn Fn(&str, &serde_json::Value) -> Option<String>,
    jsonl: bool,
) -> String {
    build_transcript_entries(messages)
        .iter()
        .map(|e| to_compact(e, tool_encoder, jsonl))
        .collect()
}

// ─── XML Response Parsing ────────────────────────────────────────────────────

/// Strip thinking content to avoid matching tags inside reasoning.
fn strip_thinking(text: &str) -> String {
    static THINKING_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?s)<thinking>.*?</thinking>").unwrap());
    static THINKING_OPEN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)<thinking>.*$").unwrap());
    let s = THINKING_RE.replace_all(text, "");
    THINKING_OPEN.replace_all(&s, "").to_string()
}

/// Parse XML block response: <block>yes/no</block>.
pub fn parse_xml_block(text: &str) -> Option<bool> {
    static BLOCK_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)<block>(yes|no)\b(</block>)?").unwrap());
    let cleaned = strip_thinking(text);
    BLOCK_RE
        .captures(&cleaned)
        .map(|cap| cap.get(1).unwrap().as_str().to_lowercase() == "yes")
}

/// Parse XML reason: <reason>...</reason>.
pub fn parse_xml_reason(text: &str) -> Option<String> {
    static REASON_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?s)<reason>(.*?)</reason>").unwrap());
    let cleaned = strip_thinking(text);
    REASON_RE
        .captures(&cleaned)
        .map(|cap| cap.get(1).unwrap().as_str().trim().to_string())
}

/// Parse XML thinking content.
pub fn parse_xml_thinking(text: &str) -> Option<String> {
    static THINK_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?s)<thinking>(.*?)</thinking>").unwrap());
    THINK_RE
        .captures(text)
        .map(|cap| cap.get(1).unwrap().as_str().trim().to_string())
}

/// Combine usage from two classifier stages.
pub fn combine_usage(a: &ClassifierUsage, b: &ClassifierUsage) -> ClassifierUsage {
    ClassifierUsage {
        input_tokens: a.input_tokens + b.input_tokens,
        output_tokens: a.output_tokens + b.output_tokens,
        cache_read_input_tokens: a.cache_read_input_tokens + b.cache_read_input_tokens,
        cache_creation_input_tokens: a.cache_creation_input_tokens + b.cache_creation_input_tokens,
    }
}

// ─── System Prompt Assembly ──────────────────────────────────────────────────

/// Replace the tool_use format instruction with XML format.
pub fn replace_output_format_with_xml(system_prompt: &str) -> String {
    let tool_use_line = "Use the classify_result tool to report your classification.";
    let xml_format = [
        "## Output Format",
        "",
        "If the action should be blocked:",
        "<block>yes</block><reason>one short sentence</reason>",
        "",
        "If the action should be allowed:",
        "<block>no</block>",
        "",
        "Do NOT include a <reason> tag when the action is allowed.",
        "Your ENTIRE response MUST begin with <block>. Do NOT output any analysis, reasoning, or commentary before <block>. No \"Looking at...\" or similar preamble.",
    ]
    .join("\n");
    system_prompt.replace(tool_use_line, &xml_format)
}

/// Configuration for building the system prompt.
pub struct SystemPromptConfig {
    pub base_prompt: String,
    pub external_permissions_template: String,
    pub mossen_permissions_template: String,
    pub user_type: String,
    pub force_external_permissions: bool,
    pub bash_classifier_enabled: bool,
    pub powershell_auto_mode_enabled: bool,
    pub auto_mode_rules: Option<AutoModeRules>,
    pub bash_allow_descriptions: Vec<String>,
    pub bash_deny_descriptions: Vec<String>,
}

/// Build the YOLO classifier system prompt.
pub fn build_yolo_system_prompt(
    _context: &ToolPermissionContext,
    config: &SystemPromptConfig,
) -> String {
    let using_external = config.user_type != "internal" || config.force_external_permissions;

    let permissions_template = if using_external {
        &config.external_permissions_template
    } else {
        &config.mossen_permissions_template
    };

    let system_prompt = config
        .base_prompt
        .replace("<permissions_template>", permissions_template);

    let auto_mode = config.auto_mode_rules.as_ref();
    let include_bash_rules = config.bash_classifier_enabled && !using_external;
    let include_ps_guidance = config.powershell_auto_mode_enabled && !using_external;

    let mut allow_descriptions: Vec<String> = Vec::new();
    if include_bash_rules {
        allow_descriptions.extend(config.bash_allow_descriptions.iter().cloned());
    }
    if let Some(rules) = auto_mode {
        allow_descriptions.extend(rules.allow.iter().cloned());
    }

    let mut deny_descriptions: Vec<String> = Vec::new();
    if include_bash_rules {
        deny_descriptions.extend(config.bash_deny_descriptions.iter().cloned());
    }
    if include_ps_guidance {
        deny_descriptions.extend(POWERSHELL_DENY_GUIDANCE.iter().map(|s| s.to_string()));
    }
    if let Some(rules) = auto_mode {
        deny_descriptions.extend(rules.soft_deny.iter().cloned());
    }

    let user_allow = if !allow_descriptions.is_empty() {
        Some(
            allow_descriptions
                .iter()
                .map(|d| format!("- {}", d))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    } else {
        None
    };

    let user_deny = if !deny_descriptions.is_empty() {
        Some(
            deny_descriptions
                .iter()
                .map(|d| format!("- {}", d))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    } else {
        None
    };

    let user_environment = auto_mode.and_then(|rules| {
        if rules.environment.is_empty() {
            None
        } else {
            Some(
                rules
                    .environment
                    .iter()
                    .map(|e| format!("- {}", e))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        }
    });

    // Replace tagged sections
    static ALLOW_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?s)<user_allow_rules_to_replace>(.*?)</user_allow_rules_to_replace>").unwrap()
    });
    static DENY_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?s)<user_deny_rules_to_replace>(.*?)</user_deny_rules_to_replace>").unwrap()
    });
    static ENV_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?s)<user_environment_to_replace>(.*?)</user_environment_to_replace>").unwrap()
    });

    let result = ALLOW_RE.replace(&system_prompt, |caps: &regex::Captures| {
        user_allow.as_deref().unwrap_or(&caps[1]).to_string()
    });
    let result = DENY_RE.replace(&result, |caps: &regex::Captures| {
        user_deny.as_deref().unwrap_or(&caps[1]).to_string()
    });
    let result = ENV_RE.replace(&result, |caps: &regex::Captures| {
        user_environment.as_deref().unwrap_or(&caps[1]).to_string()
    });

    result.to_string()
}

/// Get default external auto mode rules by parsing the template.
pub fn get_default_external_auto_mode_rules(template: &str) -> AutoModeRules {
    AutoModeRules {
        allow: extract_tagged_bullets(template, "user_allow_rules_to_replace"),
        soft_deny: extract_tagged_bullets(template, "user_deny_rules_to_replace"),
        environment: extract_tagged_bullets(template, "user_environment_to_replace"),
    }
}

fn extract_tagged_bullets(template: &str, tag_name: &str) -> Vec<String> {
    let pattern = format!(r"(?s)<{}>(.*?)</{}>", tag_name, tag_name);
    let re = Regex::new(&pattern).unwrap();
    match re.captures(template) {
        Some(caps) => caps
            .get(1)
            .unwrap()
            .as_str()
            .lines()
            .map(|l| l.trim())
            .filter(|l| l.starts_with("- "))
            .map(|l| l[2..].to_string())
            .collect(),
        None => Vec::new(),
    }
}

/// Build the default external system prompt (no user overrides).
pub fn build_default_external_system_prompt(base_prompt: &str, external_template: &str) -> String {
    static ALLOW_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?s)<user_allow_rules_to_replace>(.*?)</user_allow_rules_to_replace>").unwrap()
    });
    static DENY_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?s)<user_deny_rules_to_replace>(.*?)</user_deny_rules_to_replace>").unwrap()
    });
    static ENV_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?s)<user_environment_to_replace>(.*?)</user_environment_to_replace>").unwrap()
    });

    let prompt = base_prompt.replace("<permissions_template>", external_template);
    let prompt = ALLOW_RE.replace(&prompt, |caps: &regex::Captures| caps[1].to_string());
    let prompt = DENY_RE.replace(&prompt, |caps: &regex::Captures| caps[1].to_string());
    let prompt = ENV_RE.replace(&prompt, |caps: &regex::Captures| caps[1].to_string());
    prompt.to_string()
}

// ─── Classifier Invocation ───────────────────────────────────────────────────

/// Configuration for the classifier invocation.
pub struct ClassifierConfig {
    pub model: String,
    pub two_stage_mode: Option<TwoStageMode>,
    pub jsonl_transcript: bool,
    pub user_type: String,
    pub system_prompt_config: SystemPromptConfig,
}

/// Side query function type for making API calls.
pub type SideQueryFn =
    Box<dyn Fn(SideQueryRequest) -> Result<SideQueryResponse, ClassifierError> + Send + Sync>;

pub struct SideQueryRequest {
    pub model: String,
    pub max_tokens: u32,
    pub system_prompt: String,
    pub messages: Vec<serde_json::Value>,
    pub temperature: f32,
    pub tools: Option<Vec<serde_json::Value>>,
    pub tool_choice: Option<serde_json::Value>,
    pub stop_sequences: Option<Vec<String>>,
}

pub struct SideQueryResponse {
    pub content: String,
    pub usage: ClassifierUsage,
    pub id: String,
    pub request_id: Option<String>,
}

#[derive(Debug)]
pub enum ClassifierError {
    Aborted,
    PromptTooLong {
        actual_tokens: u64,
        limit_tokens: u64,
    },
    ApiError(String),
}

/// Format an action for the classifier from tool name and input.
pub fn format_action_for_classifier(
    tool_name: &str,
    tool_input: &serde_json::Value,
) -> TranscriptEntry {
    TranscriptEntry {
        role: "assistant".to_string(),
        content: vec![TranscriptBlock::ToolUse {
            name: tool_name.to_string(),
            input: tool_input.clone(),
        }],
    }
}

/// Main classifier entry point.
pub fn classify_yolo_action(
    messages: &[Message],
    action: &TranscriptEntry,
    tool_encoder: &dyn Fn(&str, &serde_json::Value) -> Option<String>,
    context: &ToolPermissionContext,
    config: &ClassifierConfig,
    side_query: &SideQueryFn,
) -> YoloClassifierResult {
    let jsonl = config.jsonl_transcript;
    let action_compact = to_compact(action, tool_encoder, jsonl);

    // Empty action = no security relevance
    if action_compact.is_empty() {
        return YoloClassifierResult {
            should_block: false,
            reason: "Tool declares no classifier-relevant input".to_string(),
            model: Some(config.model.clone()),
            unavailable: false,
            transcript_too_long: false,
            thinking: None,
            usage: None,
            duration_ms: None,
            error_dump_path: None,
            prompt_lengths: None,
            stage: None,
            stage1_usage: None,
            stage1_duration_ms: None,
            stage1_request_id: None,
            stage1_msg_id: None,
            stage2_usage: None,
            stage2_duration_ms: None,
            stage2_request_id: None,
            stage2_msg_id: None,
        };
    }

    let system_prompt = build_yolo_system_prompt(context, &config.system_prompt_config);
    let transcript_entries = build_transcript_entries(messages);

    let mut tool_calls_length = action_compact.len();
    let mut user_prompts_length = 0;
    let mut user_content_parts = Vec::new();

    for entry in &transcript_entries {
        for block in &entry.content {
            let serialized = to_compact_block(block, &entry.role, tool_encoder, jsonl);
            if serialized.is_empty() {
                continue;
            }
            match entry.role.as_str() {
                "user" => user_prompts_length += serialized.len(),
                "assistant" => tool_calls_length += serialized.len(),
                _ => {}
            }
            user_content_parts.push(serialized);
        }
    }

    let user_prompt: String = user_content_parts.join("") + &action_compact;
    let prompt_lengths = PromptLengths {
        system_prompt: system_prompt.len(),
        tool_calls: tool_calls_length,
        user_prompts: user_prompts_length,
    };

    // Dispatch based on two-stage mode
    let use_xml = config.two_stage_mode.is_some();
    let mode = config.two_stage_mode.unwrap_or(TwoStageMode::Both);

    if use_xml {
        classify_yolo_action_xml(
            &system_prompt,
            &user_prompt,
            &action_compact,
            &config.model,
            &prompt_lengths,
            mode,
            side_query,
        )
    } else {
        classify_yolo_action_tool_use(
            &system_prompt,
            &user_prompt,
            &action_compact,
            &config.model,
            &prompt_lengths,
            side_query,
        )
    }
}

/// XML-based 2-stage classifier.
fn classify_yolo_action_xml(
    system_prompt: &str,
    user_prompt: &str,
    _action_compact: &str,
    model: &str,
    prompt_lengths: &PromptLengths,
    mode: TwoStageMode,
    side_query: &SideQueryFn,
) -> YoloClassifierResult {
    let xml_system_prompt = replace_output_format_with_xml(system_prompt);
    let wrapped_content = format!("<transcript>\n{}</transcript>\n", user_prompt);

    // Stage 1 (fast)
    let mut stage1_usage: Option<ClassifierUsage> = None;
    let mut stage1_duration_ms: Option<u64> = None;
    let mut stage1_request_id: Option<String> = None;
    let mut stage1_msg_id: Option<String> = None;

    if mode != TwoStageMode::Thinking {
        let stage1_content = format!("{}{}", wrapped_content, XML_S1_SUFFIX);
        let max_tokens = if mode == TwoStageMode::Fast { 256 } else { 64 };
        let stop_sequences = if mode != TwoStageMode::Fast {
            Some(vec!["</block>".to_string()])
        } else {
            None
        };

        let start = std::time::Instant::now();
        let result = side_query(SideQueryRequest {
            model: model.to_string(),
            max_tokens,
            system_prompt: xml_system_prompt.clone(),
            messages: vec![serde_json::json!({"role": "user", "content": stage1_content})],
            temperature: 0.0,
            tools: None,
            tool_choice: None,
            stop_sequences,
        });
        let elapsed = start.elapsed().as_millis() as u64;

        match result {
            Ok(response) => {
                stage1_usage = Some(response.usage.clone());
                stage1_duration_ms = Some(elapsed);
                stage1_request_id = response.request_id.clone();
                stage1_msg_id = Some(response.id.clone());

                let block = parse_xml_block(&response.content);
                if block == Some(false) {
                    return YoloClassifierResult {
                        should_block: false,
                        reason: "Allowed by fast classifier".to_string(),
                        model: Some(model.to_string()),
                        unavailable: false,
                        transcript_too_long: false,
                        thinking: None,
                        usage: stage1_usage,
                        duration_ms: stage1_duration_ms,
                        error_dump_path: None,
                        prompt_lengths: Some(prompt_lengths.clone()),
                        stage: Some("fast".to_string()),
                        stage1_usage: None,
                        stage1_duration_ms: None,
                        stage1_request_id,
                        stage1_msg_id,
                        stage2_usage: None,
                        stage2_duration_ms: None,
                        stage2_request_id: None,
                        stage2_msg_id: None,
                    };
                }

                if mode == TwoStageMode::Fast {
                    let reason = if block.is_none() {
                        "Classifier stage 1 unparseable - blocking for safety".to_string()
                    } else {
                        parse_xml_reason(&response.content)
                            .unwrap_or_else(|| "Blocked by fast classifier".to_string())
                    };
                    return YoloClassifierResult {
                        should_block: true,
                        reason,
                        model: Some(model.to_string()),
                        unavailable: false,
                        transcript_too_long: false,
                        thinking: None,
                        usage: stage1_usage,
                        duration_ms: stage1_duration_ms,
                        error_dump_path: None,
                        prompt_lengths: Some(prompt_lengths.clone()),
                        stage: Some("fast".to_string()),
                        stage1_usage: None,
                        stage1_duration_ms: None,
                        stage1_request_id,
                        stage1_msg_id,
                        stage2_usage: None,
                        stage2_duration_ms: None,
                        stage2_request_id: None,
                        stage2_msg_id: None,
                    };
                }
            }
            Err(e) => {
                return handle_classifier_error(e, model, prompt_lengths, stage1_usage.as_ref());
            }
        }
    }

    // Stage 2 (thinking)
    let stage2_content = format!("{}{}", wrapped_content, XML_S2_SUFFIX);
    let start = std::time::Instant::now();
    let result = side_query(SideQueryRequest {
        model: model.to_string(),
        max_tokens: 4096,
        system_prompt: replace_output_format_with_xml(system_prompt),
        messages: vec![serde_json::json!({"role": "user", "content": stage2_content})],
        temperature: 0.0,
        tools: None,
        tool_choice: None,
        stop_sequences: None,
    });
    let stage2_elapsed = start.elapsed().as_millis() as u64;

    match result {
        Ok(response) => {
            let stage2_usage = response.usage.clone();
            let total_usage = match &stage1_usage {
                Some(s1) => combine_usage(s1, &stage2_usage),
                None => stage2_usage.clone(),
            };
            let total_duration = stage1_duration_ms.unwrap_or(0) + stage2_elapsed;
            let block = parse_xml_block(&response.content);
            let thinking = parse_xml_thinking(&response.content);

            if block.is_none() {
                return YoloClassifierResult {
                    should_block: true,
                    reason: "Classifier stage 2 unparseable - blocking for safety".to_string(),
                    model: Some(model.to_string()),
                    unavailable: false,
                    transcript_too_long: false,
                    thinking,
                    usage: Some(total_usage),
                    duration_ms: Some(total_duration),
                    error_dump_path: None,
                    prompt_lengths: Some(prompt_lengths.clone()),
                    stage: Some("thinking".to_string()),
                    stage1_usage,
                    stage1_duration_ms,
                    stage1_request_id,
                    stage1_msg_id,
                    stage2_usage: Some(stage2_usage),
                    stage2_duration_ms: Some(stage2_elapsed),
                    stage2_request_id: response.request_id.clone(),
                    stage2_msg_id: Some(response.id.clone()),
                };
            }

            YoloClassifierResult {
                should_block: block.unwrap(),
                reason: parse_xml_reason(&response.content)
                    .unwrap_or_else(|| "No reason provided".to_string()),
                model: Some(model.to_string()),
                unavailable: false,
                transcript_too_long: false,
                thinking,
                usage: Some(total_usage),
                duration_ms: Some(total_duration),
                error_dump_path: None,
                prompt_lengths: Some(prompt_lengths.clone()),
                stage: Some("thinking".to_string()),
                stage1_usage,
                stage1_duration_ms,
                stage1_request_id,
                stage1_msg_id,
                stage2_usage: Some(stage2_usage),
                stage2_duration_ms: Some(stage2_elapsed),
                stage2_request_id: response.request_id,
                stage2_msg_id: Some(response.id),
            }
        }
        Err(e) => handle_classifier_error(e, model, prompt_lengths, stage1_usage.as_ref()),
    }
}

/// Tool-use based classifier (legacy path).
fn classify_yolo_action_tool_use(
    system_prompt: &str,
    user_prompt: &str,
    _action_compact: &str,
    model: &str,
    prompt_lengths: &PromptLengths,
    side_query: &SideQueryFn,
) -> YoloClassifierResult {
    let tool_schema = serde_json::json!({
        "type": "custom",
        "name": YOLO_CLASSIFIER_TOOL_NAME,
        "description": "Report the security classification result for the agent action",
        "input_schema": {
            "type": "object",
            "properties": {
                "thinking": { "type": "string", "description": "Brief step-by-step reasoning." },
                "shouldBlock": { "type": "boolean", "description": "Whether the action should be blocked" },
                "reason": { "type": "string", "description": "Brief explanation" }
            },
            "required": ["thinking", "shouldBlock", "reason"]
        }
    });

    let start = std::time::Instant::now();
    let result = side_query(SideQueryRequest {
        model: model.to_string(),
        max_tokens: 4096,
        system_prompt: system_prompt.to_string(),
        messages: vec![serde_json::json!({"role": "user", "content": user_prompt})],
        temperature: 0.0,
        tools: Some(vec![tool_schema]),
        tool_choice: Some(serde_json::json!({"type": "tool", "name": YOLO_CLASSIFIER_TOOL_NAME})),
        stop_sequences: None,
    });
    let elapsed = start.elapsed().as_millis() as u64;

    match result {
        Ok(response) => {
            let usage = response.usage.clone();
            // Try to parse tool_use response as JSON
            let parsed: Option<serde_json::Value> = serde_json::from_str(&response.content).ok();
            match parsed {
                Some(value) => {
                    let should_block = value
                        .get("shouldBlock")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    let reason = value
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("No reason provided")
                        .to_string();
                    let thinking = value
                        .get("thinking")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    YoloClassifierResult {
                        should_block,
                        reason,
                        model: Some(model.to_string()),
                        unavailable: false,
                        transcript_too_long: false,
                        thinking,
                        usage: Some(usage),
                        duration_ms: Some(elapsed),
                        error_dump_path: None,
                        prompt_lengths: Some(prompt_lengths.clone()),
                        stage: None,
                        stage1_usage: None,
                        stage1_duration_ms: None,
                        stage1_request_id: response.request_id,
                        stage1_msg_id: Some(response.id),
                        stage2_usage: None,
                        stage2_duration_ms: None,
                        stage2_request_id: None,
                        stage2_msg_id: None,
                    }
                }
                None => YoloClassifierResult {
                    should_block: true,
                    reason: "Invalid classifier response - blocking for safety".to_string(),
                    model: Some(model.to_string()),
                    unavailable: false,
                    transcript_too_long: false,
                    thinking: None,
                    usage: Some(usage),
                    duration_ms: Some(elapsed),
                    error_dump_path: None,
                    prompt_lengths: Some(prompt_lengths.clone()),
                    stage: None,
                    stage1_usage: None,
                    stage1_duration_ms: None,
                    stage1_request_id: None,
                    stage1_msg_id: None,
                    stage2_usage: None,
                    stage2_duration_ms: None,
                    stage2_request_id: None,
                    stage2_msg_id: None,
                },
            }
        }
        Err(e) => handle_classifier_error(e, model, prompt_lengths, None),
    }
}

/// Handle classifier errors (aborted, prompt too long, API error).
fn handle_classifier_error(
    error: ClassifierError,
    model: &str,
    prompt_lengths: &PromptLengths,
    stage1_usage: Option<&ClassifierUsage>,
) -> YoloClassifierResult {
    match error {
        ClassifierError::Aborted => YoloClassifierResult {
            should_block: true,
            reason: "Classifier request aborted".to_string(),
            model: Some(model.to_string()),
            unavailable: true,
            transcript_too_long: false,
            thinking: None,
            usage: stage1_usage.cloned(),
            duration_ms: None,
            error_dump_path: None,
            prompt_lengths: Some(prompt_lengths.clone()),
            stage: None,
            stage1_usage: None,
            stage1_duration_ms: None,
            stage1_request_id: None,
            stage1_msg_id: None,
            stage2_usage: None,
            stage2_duration_ms: None,
            stage2_request_id: None,
            stage2_msg_id: None,
        },
        ClassifierError::PromptTooLong { .. } => YoloClassifierResult {
            should_block: true,
            reason: "Classifier transcript exceeded context window".to_string(),
            model: Some(model.to_string()),
            unavailable: stage1_usage.is_none(),
            transcript_too_long: true,
            thinking: None,
            usage: stage1_usage.cloned(),
            duration_ms: None,
            error_dump_path: None,
            prompt_lengths: Some(prompt_lengths.clone()),
            stage: if stage1_usage.is_some() {
                Some("thinking".to_string())
            } else {
                None
            },
            stage1_usage: None,
            stage1_duration_ms: None,
            stage1_request_id: None,
            stage1_msg_id: None,
            stage2_usage: None,
            stage2_duration_ms: None,
            stage2_request_id: None,
            stage2_msg_id: None,
        },
        ClassifierError::ApiError(_) => YoloClassifierResult {
            should_block: true,
            reason: if stage1_usage.is_some() {
                "Stage 2 classifier error - blocking based on stage 1 assessment".to_string()
            } else {
                "Classifier unavailable - blocking for safety".to_string()
            },
            model: Some(model.to_string()),
            unavailable: stage1_usage.is_none(),
            transcript_too_long: false,
            thinking: None,
            usage: stage1_usage.cloned(),
            duration_ms: None,
            error_dump_path: None,
            prompt_lengths: Some(prompt_lengths.clone()),
            stage: if stage1_usage.is_some() {
                Some("thinking".to_string())
            } else {
                None
            },
            stage1_usage: None,
            stage1_duration_ms: None,
            stage1_request_id: None,
            stage1_msg_id: None,
            stage2_usage: None,
            stage2_duration_ms: None,
            stage2_request_id: None,
            stage2_msg_id: None,
        },
    }
}

/// 对应 TS `getAutoModeClassifierErrorDumpPath`：classifier 出错时的 dump 路径。
pub fn get_auto_mode_classifier_error_dump_path() -> std::path::PathBuf {
    dirs::home_dir()
        .map(|h| {
            h.join(".mossen")
                .join("logs")
                .join("auto-mode-classifier-error.json")
        })
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/auto-mode-classifier-error.json"))
}

/// 对应 TS `getAutoModeClassifierTranscript`：返回最近一次 classifier transcript。
pub async fn get_auto_mode_classifier_transcript() -> Option<String> {
    let path = get_auto_mode_classifier_error_dump_path();
    tokio::fs::read_to_string(path).await.ok()
}
