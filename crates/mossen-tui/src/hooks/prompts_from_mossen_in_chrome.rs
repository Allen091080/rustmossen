//! Prompts from Mossen in Chrome (usePromptsFromMossenInChrome.tsx).
//! Receives and processes prompts forwarded from the Chrome extension.

use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct ChromePrompt {
    pub id: String,
    pub text: String,
    pub url: Option<String>,
    pub page_title: Option<String>,
    pub selected_text: Option<String>,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct PromptsFromChromeState {
    pub pending_prompts: VecDeque<ChromePrompt>,
    pub processed_ids: Vec<String>,
    pub enabled: bool,
}

impl PromptsFromChromeState {
    pub fn new() -> Self {
        Self { pending_prompts: VecDeque::new(), processed_ids: Vec::new(), enabled: false }
    }
    pub fn receive(&mut self, prompt: ChromePrompt) {
        if !self.enabled { return; }
        if self.processed_ids.contains(&prompt.id) { return; }
        self.pending_prompts.push_back(prompt);
    }
    pub fn take_next(&mut self) -> Option<ChromePrompt> {
        let prompt = self.pending_prompts.pop_front()?;
        self.processed_ids.push(prompt.id.clone());
        Some(prompt)
    }
    pub fn has_pending(&self) -> bool { !self.pending_prompts.is_empty() }
    pub fn set_enabled(&mut self, enabled: bool) { self.enabled = enabled; }
}
impl Default for PromptsFromChromeState { fn default() -> Self { Self::new() } }
