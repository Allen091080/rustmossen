//! Mossen hint recommendation (useMossenHintRecommendation.tsx).
//! Shows contextual hints to help users discover features.

#[derive(Debug, Clone)]
pub struct MossenHint {
    pub id: String,
    pub text: String,
    pub action: Option<String>,
    pub priority: HintPriority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HintPriority { Low, Medium, High }

#[derive(Debug, Clone)]
pub struct MossenHintRecommendationState {
    pub active_hint: Option<MossenHint>,
    pub dismissed_hints: Vec<String>,
    pub shown_count: u32,
    pub max_shown: u32,
}

impl MossenHintRecommendationState {
    pub fn new() -> Self {
        Self { active_hint: None, dismissed_hints: Vec::new(), shown_count: 0, max_shown: 3 }
    }
    pub fn suggest(&mut self, hint: MossenHint) {
        if self.shown_count >= self.max_shown { return; }
        if self.dismissed_hints.contains(&hint.id) { return; }
        self.active_hint = Some(hint);
        self.shown_count += 1;
    }
    pub fn dismiss(&mut self) {
        if let Some(hint) = self.active_hint.take() { self.dismissed_hints.push(hint.id); }
    }
    pub fn clear(&mut self) { self.active_hint = None; }
}
impl Default for MossenHintRecommendationState { fn default() -> Self { Self::new() } }
