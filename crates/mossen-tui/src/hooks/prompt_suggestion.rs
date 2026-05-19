//! Prompt suggestion hook (usePromptSuggestion.ts).
//! Provides autocomplete suggestions for the prompt input.

#[derive(Debug, Clone)]
pub struct PromptSuggestion {
    pub text: String,
    pub description: Option<String>,
    pub source: SuggestionSource,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionSource { History, Command, File, Context }

#[derive(Debug, Clone)]
pub struct PromptSuggestionState {
    pub suggestions: Vec<PromptSuggestion>,
    pub selected_index: Option<usize>,
    pub query: String,
    pub is_active: bool,
}

impl PromptSuggestionState {
    pub fn new() -> Self {
        Self { suggestions: Vec::new(), selected_index: None, query: String::new(), is_active: false }
    }
    pub fn update(&mut self, query: &str, suggestions: Vec<PromptSuggestion>) {
        self.query = query.to_string();
        self.suggestions = suggestions;
        self.selected_index = if self.suggestions.is_empty() { None } else { Some(0) };
        self.is_active = !self.suggestions.is_empty();
    }
    pub fn next(&mut self) {
        if let Some(idx) = &mut self.selected_index {
            *idx = (*idx + 1) % self.suggestions.len().max(1);
        }
    }
    pub fn prev(&mut self) {
        if let Some(idx) = &mut self.selected_index {
            *idx = if *idx == 0 { self.suggestions.len().saturating_sub(1) } else { *idx - 1 };
        }
    }
    pub fn accept(&mut self) -> Option<String> {
        let text = self.selected_index.and_then(|i| self.suggestions.get(i)).map(|s| s.text.clone());
        self.clear();
        text
    }
    pub fn clear(&mut self) { self.suggestions.clear(); self.selected_index = None; self.is_active = false; }
    pub fn selected(&self) -> Option<&PromptSuggestion> {
        self.selected_index.and_then(|i| self.suggestions.get(i))
    }
}
impl Default for PromptSuggestionState { fn default() -> Self { Self::new() } }
