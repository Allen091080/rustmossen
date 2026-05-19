//! OpenAI-compatible semantic adapter — converts OpenAI streaming/non-streaming responses to canonical events.

use std::collections::HashMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::services::model_runtime::canonical::{
    AssistantToolRequest, CanonicalStopReason, CanonicalStreamEvent, CanonicalTurnResult,
    CanonicalUsage, OfficialSemanticCapabilities, ToolCallArgsEncoding, ToolResultRoleStyle,
};
use crate::services::model_runtime::provider_policy::ProviderModelPolicy;

/// OpenAI-compatible semantic capabilities.
pub const OPENAI_COMPATIBLE_SEMANTIC_CAPABILITIES: OfficialSemanticCapabilities =
    OfficialSemanticCapabilities {
        mixed_content_tool_use: false,
        native_thinking_blocks: false,
        reasoning_budget: false,
        streaming_tool_arg_deltas: true,
        structured_stop_reasons: false,
        supports_assistant_prelude_before_tool_use: true,
        tool_call_args_encoding: ToolCallArgsEncoding::JsonString,
        tool_result_role_style: ToolResultRoleStyle::OpenaiToolRole,
    };

/// An OpenAI tool call in a streaming chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIToolCall {
    pub function: Option<OpenAIFunction>,
    pub id: Option<String>,
    pub index: Option<usize>,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIFunction {
    pub arguments: Option<String>,
    pub name: Option<String>,
}

/// OpenAI-compatible response chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIResponse {
    pub choices: Option<Vec<OpenAIChoice>>,
    pub id: Option<String>,
    pub model: Option<String>,
    pub usage: Option<OpenAIUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIChoice {
    pub delta: Option<OpenAIDelta>,
    pub finish_reason: Option<String>,
    pub message: Option<OpenAIDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIDelta {
    pub content: Option<Value>,
    pub role: Option<String>,
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIUsage {
    pub completion_tokens: Option<u64>,
    pub prompt_tokens: Option<u64>,
}

// === TS export-name aliases (parity with TS source surface) ===
//
// TS file: services/modelRuntime/semanticAdapters/openaiCompatibleSemanticAdapter.ts
//   export type OpenAICompatibleSemanticToolCall = { ... }
//   export type OpenAICompatibleSemanticChoice   = { ... }
//   export type OpenAICompatibleSemanticResponse = { ... }
pub type OpenAICompatibleSemanticToolCall = OpenAIToolCall;
pub type OpenAICompatibleSemanticChoice = OpenAIChoice;
pub type OpenAICompatibleSemanticResponse = OpenAIResponse;

fn is_complete_json_payload(value: &str) -> bool {
    if value.trim().is_empty() {
        return false;
    }
    serde_json::from_str::<Value>(value).is_ok()
}

fn flatten_text_content(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let mut parts = Vec::new();
            for block in arr {
                if let Value::String(s) = block {
                    parts.push(s.clone());
                } else if let Some(obj) = block.as_object() {
                    if obj.get("type").and_then(|v| v.as_str()) == Some("text") {
                        if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                            parts.push(text.to_string());
                        }
                    }
                }
            }
            parts.join("")
        }
        _ => String::new(),
    }
}

fn escape_regex(value: &str) -> String {
    regex::escape(value)
}

/// Synthetic thinking stream parser for single/two pass strategies.
struct SyntheticThinkingStreamParser {
    buffer: String,
    mode: ParserMode,
    text_open: bool,
    thinking_open: bool,
    thinking_open_tag: String,
    thinking_close_tag: String,
    response_open_tag: String,
    response_close_tag: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ParserMode {
    Searching,
    Thinking,
    AwaitResponse,
    Response,
    FallbackText,
}

#[derive(Debug, Clone)]
enum SyntheticEvent {
    ThinkingStart,
    ThinkingDelta(String),
    ThinkingEnd,
    TextStart,
    TextDelta(String),
    TextEnd,
}

impl SyntheticThinkingStreamParser {
    fn new(policy: &ProviderModelPolicy) -> Self {
        Self {
            buffer: String::new(),
            mode: ParserMode::Searching,
            text_open: false,
            thinking_open: false,
            thinking_open_tag: policy.synthetic_tags.thinking_open.clone(),
            thinking_close_tag: policy.synthetic_tags.thinking_close.clone(),
            response_open_tag: policy.synthetic_tags.response_open.clone(),
            response_close_tag: policy.synthetic_tags.response_close.clone(),
        }
    }

    fn flush_text(&mut self, events: &mut Vec<SyntheticEvent>, text: &str) {
        if text.is_empty() { return; }
        if !self.text_open {
            events.push(SyntheticEvent::TextStart);
            self.text_open = true;
        }
        events.push(SyntheticEvent::TextDelta(text.to_string()));
    }

    fn flush_thinking(&mut self, events: &mut Vec<SyntheticEvent>, text: &str) {
        if text.is_empty() { return; }
        if !self.thinking_open {
            events.push(SyntheticEvent::ThinkingStart);
            self.thinking_open = true;
        }
        events.push(SyntheticEvent::ThinkingDelta(text.to_string()));
    }

    fn consume(&mut self, chunk: &str) -> Vec<SyntheticEvent> {
        let mut events = Vec::new();
        if chunk.is_empty() { return events; }
        self.buffer.push_str(chunk);

        loop {
            if self.buffer.is_empty() { break; }

            match self.mode {
                ParserMode::FallbackText => {
                    let next = self.buffer.clone();
                    self.buffer.clear();
                    self.flush_text(&mut events, &next);
                    break;
                }
                ParserMode::Searching => {
                    if let Some(idx) = self.buffer.find(&self.thinking_open_tag) {
                        if idx == 0 {
                            self.buffer = self.buffer[self.thinking_open_tag.len()..].to_string();
                            self.mode = ParserMode::Thinking;
                        } else {
                            let prefix = self.buffer[..idx].to_string();
                            self.buffer = self.buffer[idx..].to_string();
                            if prefix.trim().is_empty() {
                                continue;
                            }
                            self.mode = ParserMode::FallbackText;
                            self.flush_text(&mut events, &prefix);
                        }
                    } else if self.thinking_open_tag.starts_with(&self.buffer) {
                        break;
                    } else {
                        self.mode = ParserMode::FallbackText;
                    }
                }
                ParserMode::Thinking => {
                    if let Some(idx) = self.buffer.find(&self.thinking_close_tag) {
                        let thinking_text = self.buffer[..idx].to_string();
                        self.buffer = self.buffer[idx + self.thinking_close_tag.len()..].to_string();
                        self.flush_thinking(&mut events, &thinking_text);
                        if self.thinking_open {
                            events.push(SyntheticEvent::ThinkingEnd);
                            self.thinking_open = false;
                        }
                        self.mode = ParserMode::AwaitResponse;
                    } else {
                        let safe_len = self.buffer.len().saturating_sub(self.thinking_close_tag.len() - 1);
                        if safe_len == 0 { break; }
                        let safe = self.buffer[..safe_len].to_string();
                        self.buffer = self.buffer[safe_len..].to_string();
                        self.flush_thinking(&mut events, &safe);
                    }
                }
                ParserMode::AwaitResponse => {
                    if let Some(idx) = self.buffer.find(&self.response_open_tag) {
                        if idx == 0 {
                            self.buffer = self.buffer[self.response_open_tag.len()..].to_string();
                            self.mode = ParserMode::Response;
                        } else {
                            let prefix = self.buffer[..idx].to_string();
                            self.buffer = self.buffer[idx..].to_string();
                            if prefix.trim().is_empty() {
                                continue;
                            }
                            self.mode = ParserMode::FallbackText;
                            self.flush_text(&mut events, &prefix);
                        }
                    } else if self.response_open_tag.starts_with(&self.buffer) {
                        break;
                    } else {
                        self.mode = ParserMode::FallbackText;
                    }
                }
                ParserMode::Response => {
                    if let Some(idx) = self.buffer.find(&self.response_close_tag) {
                        let response_text = self.buffer[..idx].to_string();
                        self.buffer = self.buffer[idx + self.response_close_tag.len()..].to_string();
                        self.flush_text(&mut events, &response_text);
                        if self.text_open {
                            events.push(SyntheticEvent::TextEnd);
                            self.text_open = false;
                        }
                        self.mode = ParserMode::FallbackText;
                    } else {
                        let safe_len = self.buffer.len().saturating_sub(self.response_close_tag.len() - 1);
                        if safe_len == 0 { break; }
                        let safe = self.buffer[..safe_len].to_string();
                        self.buffer = self.buffer[safe_len..].to_string();
                        self.flush_text(&mut events, &safe);
                    }
                }
            }
        }

        events
    }

    fn finish(&mut self) -> Vec<SyntheticEvent> {
        let mut events = Vec::new();
        match self.mode {
            ParserMode::FallbackText | ParserMode::Response => {
                let remaining = self.buffer.clone();
                self.buffer.clear();
                self.flush_text(&mut events, &remaining);
                if self.text_open {
                    events.push(SyntheticEvent::TextEnd);
                    self.text_open = false;
                }
            }
            ParserMode::Thinking => {
                let remaining = self.buffer.clone();
                self.buffer.clear();
                self.flush_thinking(&mut events, &remaining);
                if self.thinking_open {
                    events.push(SyntheticEvent::ThinkingEnd);
                    self.thinking_open = false;
                }
            }
            _ => {}
        }
        events
    }
}

/// Extract thinking/response parts from a complete text using synthetic tags.
pub fn extract_openai_compatible_thinking_parts(
    content: &str,
    policy: &ProviderModelPolicy,
) -> (String, String) {
    use super::super::provider_policy::SyntheticTags;

    if content.is_empty() || !matches!(
        policy.thinking_strategy,
        super::super::canonical::ThinkingParityStrategy::SyntheticSinglePass
            | super::super::canonical::ThinkingParityStrategy::SyntheticTwoPass
    ) {
        return (String::new(), content.to_string());
    }

    let thinking_re = Regex::new(&format!(
        "{}([\\s\\S]*?){}",
        escape_regex(&policy.synthetic_tags.thinking_open),
        escape_regex(&policy.synthetic_tags.thinking_close)
    ))
    .ok();

    let response_re = Regex::new(&format!(
        "{}([\\s\\S]*?){}",
        escape_regex(&policy.synthetic_tags.response_open),
        escape_regex(&policy.synthetic_tags.response_close)
    ))
    .ok();

    let thinking_match = thinking_re.as_ref().and_then(|re| re.captures(content));
    let response_match = response_re.as_ref().and_then(|re| re.captures(content));

    if thinking_match.is_none() && response_match.is_none() {
        return (String::new(), content.to_string());
    }

    let thinking_text = thinking_match
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default();
    let visible_text = response_match
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default();

    (thinking_text, visible_text)
}

fn map_stop_reason(finish_reason: Option<&str>, has_tool_calls: bool) -> CanonicalStopReason {
    match finish_reason {
        Some("length") => CanonicalStopReason::MaxTokens,
        Some("tool_calls") => CanonicalStopReason::ToolUse,
        _ if has_tool_calls => CanonicalStopReason::ToolUse,
        _ => CanonicalStopReason::EndTurn,
    }
}

/// Tool state for streaming.
struct ToolState {
    accumulated_json: String,
    closed: bool,
    emitted_start: bool,
    id: String,
    name: Option<String>,
    pending_json: String,
}

/// OpenAI-compatible stream semantic state machine.
pub struct OpenAICompatibleStreamSemanticState {
    parity_parser: Option<SyntheticThinkingStreamParser>,
    tool_index_by_id: HashMap<String, usize>,
    tool_states: HashMap<usize, ToolState>,
    emitted_any_content: bool,
    emitted_message_start: bool,
    message_id: String,
    message_model: String,
    open_text: bool,
    open_thinking: bool,
    stop_reason: Option<CanonicalStopReason>,
    usage: CanonicalUsage,
}

impl OpenAICompatibleStreamSemanticState {
    pub fn new(fallback_model: &str, policy: &ProviderModelPolicy) -> Self {
        use super::super::canonical::ThinkingParityStrategy;
        let parity_parser = match policy.thinking_strategy {
            ThinkingParityStrategy::SyntheticSinglePass | ThinkingParityStrategy::SyntheticTwoPass => {
                Some(SyntheticThinkingStreamParser::new(policy))
            }
            _ => None,
        };

        Self {
            parity_parser,
            tool_index_by_id: HashMap::new(),
            tool_states: HashMap::new(),
            emitted_any_content: false,
            emitted_message_start: false,
            message_id: format!("msg_{}", Uuid::new_v4()),
            message_model: fallback_model.to_string(),
            open_text: false,
            open_thinking: false,
            stop_reason: None,
            usage: CanonicalUsage::default(),
        }
    }

    fn ensure_message_start(&mut self, events: &mut Vec<CanonicalStreamEvent>) {
        if self.emitted_message_start { return; }
        self.emitted_message_start = true;
        events.push(CanonicalStreamEvent::MessageStart {
            message_id: self.message_id.clone(),
            model: self.message_model.clone(),
        });
    }

    fn start_text(&mut self, events: &mut Vec<CanonicalStreamEvent>) {
        if self.open_text { return; }
        self.stop_thinking(events);
        self.open_text = true;
        self.emitted_any_content = true;
        events.push(CanonicalStreamEvent::TextStart);
    }

    fn stop_text(&mut self, events: &mut Vec<CanonicalStreamEvent>) {
        if !self.open_text { return; }
        self.open_text = false;
        events.push(CanonicalStreamEvent::TextEnd);
    }

    fn start_thinking(&mut self, events: &mut Vec<CanonicalStreamEvent>) {
        if self.open_thinking { return; }
        self.stop_text(events);
        self.open_thinking = true;
        self.emitted_any_content = true;
        events.push(CanonicalStreamEvent::ThinkingStart);
    }

    fn stop_thinking(&mut self, events: &mut Vec<CanonicalStreamEvent>) {
        if !self.open_thinking { return; }
        self.open_thinking = false;
        events.push(CanonicalStreamEvent::ThinkingEnd);
    }

    fn close_open_content(&mut self, events: &mut Vec<CanonicalStreamEvent>) {
        self.stop_text(events);
        self.stop_thinking(events);
    }

    fn apply_parity_event(&mut self, parity_event: SyntheticEvent, events: &mut Vec<CanonicalStreamEvent>) {
        match parity_event {
            SyntheticEvent::ThinkingStart => self.start_thinking(events),
            SyntheticEvent::ThinkingEnd => self.stop_thinking(events),
            SyntheticEvent::TextStart => self.start_text(events),
            SyntheticEvent::TextEnd => self.stop_text(events),
            SyntheticEvent::ThinkingDelta(text) => {
                self.start_thinking(events);
                events.push(CanonicalStreamEvent::ThinkingDelta { text });
            }
            SyntheticEvent::TextDelta(text) => {
                self.start_text(events);
                events.push(CanonicalStreamEvent::TextDelta { text });
            }
        }
    }

    fn emit_text_delta(&mut self, text_delta: &str, events: &mut Vec<CanonicalStreamEvent>) {
        if text_delta.is_empty() { return; }
        if let Some(ref mut parser) = self.parity_parser {
            let parity_events = parser.consume(text_delta);
            for pe in parity_events {
                self.apply_parity_event(pe, events);
            }
        } else {
            self.start_text(events);
            events.push(CanonicalStreamEvent::TextDelta { text: text_delta.to_string() });
        }
    }

    fn resolve_tool_state_index(&mut self, tool_call: &OpenAIToolCall) -> usize {
        if let Some(id) = &tool_call.id {
            if let Some(&idx) = self.tool_index_by_id.get(id) {
                return idx;
            }
        }
        let resolved = tool_call.index.unwrap_or(self.tool_states.len());
        if let Some(id) = &tool_call.id {
            self.tool_index_by_id.insert(id.clone(), resolved);
        }
        resolved
    }

    fn emit_tool_start_if_ready(&mut self, index: usize, events: &mut Vec<CanonicalStreamEvent>) {
        let state = match self.tool_states.get_mut(&index) {
            Some(s) => s,
            None => return,
        };
        if state.emitted_start || state.name.is_none() { return; }
        self.close_open_content(events);
        state.emitted_start = true;
        self.emitted_any_content = true;
        let id = state.id.clone();
        let name = state.name.clone().unwrap_or_else(|| "tool".to_string());
        events.push(CanonicalStreamEvent::ToolUseStart { id: id.clone(), name });
        let pending = std::mem::take(&mut state.pending_json);
        if !pending.is_empty() {
            events.push(CanonicalStreamEvent::ToolUseArgsDelta { id, partial_json: pending });
        }
    }

    fn close_tool_blocks(&mut self, events: &mut Vec<CanonicalStreamEvent>) {
        let mut indices: Vec<usize> = self.tool_states.keys().cloned().collect();
        indices.sort();
        for idx in indices {
            let state = self.tool_states.get_mut(&idx).unwrap();
            if !state.emitted_start {
                if state.name.is_none() {
                    state.name = Some("tool".to_string());
                }
                // emit start
                self.emitted_any_content = true;
                state.emitted_start = true;
                let id = state.id.clone();
                let name = state.name.clone().unwrap_or_default();
                events.push(CanonicalStreamEvent::ToolUseStart { id: id.clone(), name });
                let pending = std::mem::take(&mut state.pending_json);
                if !pending.is_empty() {
                    events.push(CanonicalStreamEvent::ToolUseArgsDelta { id, partial_json: pending });
                }
            }
            let state = self.tool_states.get_mut(&idx).unwrap();
            if state.closed || !state.emitted_start { continue; }
            state.closed = true;
            events.push(CanonicalStreamEvent::ToolUseEnd { id: state.id.clone() });
        }
    }

    /// Consume a streaming chunk and produce canonical events.
    pub fn consume_chunk(&mut self, chunk: &OpenAIResponse) -> Vec<CanonicalStreamEvent> {
        let mut events = Vec::new();

        if let Some(id) = &chunk.id {
            self.message_id = id.clone();
        }
        if let Some(model) = &chunk.model {
            self.message_model = model.clone();
        }
        if let Some(usage) = &chunk.usage {
            self.usage = CanonicalUsage {
                input_tokens: usage.prompt_tokens.unwrap_or(0),
                output_tokens: usage.completion_tokens.unwrap_or(0),
            };
        }

        self.ensure_message_start(&mut events);

        for choice in chunk.choices.as_deref().unwrap_or(&[]) {
            let text_delta = get_choice_content_delta(choice);
            let tool_calls = get_choice_tool_calls(choice);

            self.emit_text_delta(&text_delta, &mut events);

            if !tool_calls.is_empty() {
                self.close_open_content(&mut events);
                for tc in &tool_calls {
                    let oai_index = self.resolve_tool_state_index(tc);
                    if !self.tool_states.contains_key(&oai_index) {
                        let state = ToolState {
                            accumulated_json: String::new(),
                            closed: false,
                            emitted_start: false,
                            id: tc.id.clone().unwrap_or_else(|| format!("toolu_{}", Uuid::new_v4())),
                            name: tc.function.as_ref().and_then(|f| f.name.clone()),
                            pending_json: String::new(),
                        };
                        self.tool_states.insert(oai_index, state);
                    }

                    let state = self.tool_states.get_mut(&oai_index).unwrap();
                    if state.closed { continue; }
                    if !state.emitted_start {
                        if let Some(id) = &tc.id {
                            state.id = id.clone();
                        }
                    }
                    if let Some(f) = &tc.function {
                        if let Some(name) = &f.name {
                            if state.name.is_none() {
                                state.name = Some(name.clone());
                            }
                        }
                        if let Some(args) = &f.arguments {
                            state.accumulated_json.push_str(args);
                            state.pending_json.push_str(args);
                        }
                    }
                    self.emit_tool_start_if_ready(oai_index, &mut events);
                    let state = self.tool_states.get_mut(&oai_index).unwrap();
                    if state.emitted_start && !state.pending_json.is_empty() {
                        let id = state.id.clone();
                        let pj = std::mem::take(&mut state.pending_json);
                        events.push(CanonicalStreamEvent::ToolUseArgsDelta { id, partial_json: pj });
                    }
                }
            }

            if let Some(fr) = &choice.finish_reason {
                self.stop_reason = Some(map_stop_reason(Some(fr.as_str()), !self.tool_states.is_empty()));
            }
        }

        events
    }

    /// Finalize and produce remaining events.
    pub fn finish(&mut self) -> Vec<CanonicalStreamEvent> {
        let mut events = Vec::new();
        if !self.emitted_message_start { return events; }

        if let Some(ref mut parser) = self.parity_parser {
            let parity_events = parser.finish();
            for pe in parity_events {
                self.apply_parity_event(pe, &mut events);
            }
        }

        if !self.emitted_any_content {
            self.start_text(&mut events);
        }
        self.close_open_content(&mut events);
        self.close_tool_blocks(&mut events);
        events.push(CanonicalStreamEvent::MessageStop {
            stop_reason: self.stop_reason.unwrap_or_else(|| map_stop_reason(None, !self.tool_states.is_empty())),
            usage: self.usage,
        });
        events
    }
}

fn get_choice_content_delta(choice: &OpenAIChoice) -> String {
    let content = choice
        .delta
        .as_ref()
        .and_then(|d| d.content.as_ref())
        .or_else(|| choice.message.as_ref().and_then(|m| m.content.as_ref()));
    match content {
        Some(c) => flatten_text_content(c),
        None => String::new(),
    }
}

fn get_choice_tool_calls(choice: &OpenAIChoice) -> Vec<OpenAIToolCall> {
    choice
        .delta
        .as_ref()
        .and_then(|d| d.tool_calls.clone())
        .or_else(|| choice.message.as_ref().and_then(|m| m.tool_calls.clone()))
        .unwrap_or_default()
}

/// Convert a complete OpenAI response to a canonical turn result.
pub fn openai_compatible_completion_to_canonical_turn(
    data: &OpenAIResponse,
    policy: &ProviderModelPolicy,
) -> CanonicalTurnResult {
    let choice = data.choices.as_ref().and_then(|c| c.first());
    let message = choice.and_then(|c| c.message.as_ref());
    let tool_calls = message
        .and_then(|m| m.tool_calls.as_ref())
        .cloned()
        .unwrap_or_default();
    let content = message
        .and_then(|m| m.content.as_ref())
        .cloned()
        .unwrap_or(Value::Null);
    let text_content = flatten_text_content(&content).trim().to_string();
    let (thinking_text, visible_text) = extract_openai_compatible_thinking_parts(&text_content, policy);

    let mut turn = CanonicalTurnResult {
        provider_diagnostics: None,
        stop_reason: map_stop_reason(
            choice.and_then(|c| c.finish_reason.as_deref()),
            !tool_calls.is_empty(),
        ),
        thinking_text,
        tool_requests: Vec::new(),
        usage: CanonicalUsage {
            input_tokens: data.usage.as_ref().and_then(|u| u.prompt_tokens).unwrap_or(0),
            output_tokens: data.usage.as_ref().and_then(|u| u.completion_tokens).unwrap_or(0),
        },
        visible_text,
    };

    for tc in &tool_calls {
        let raw_args = tc.function.as_ref().and_then(|f| f.arguments.as_deref()).unwrap_or("{}");
        let parsed: HashMap<String, Value> = serde_json::from_str(raw_args).unwrap_or_default();
        turn.tool_requests.push(AssistantToolRequest {
            arguments_object: parsed,
            id: tc.id.clone().unwrap_or_else(|| Uuid::new_v4().to_string()),
            name: tc.function.as_ref().and_then(|f| f.name.clone()).unwrap_or_else(|| "tool".to_string()),
        });
    }

    if turn.visible_text.is_empty() && turn.thinking_text.is_empty() && turn.tool_requests.is_empty() {
        turn.visible_text = flatten_text_content(&content);
    }

    turn
}
