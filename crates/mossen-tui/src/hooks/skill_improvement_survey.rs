//! Skill Improvement Survey hook (useSkillImprovementSurvey.ts).
//! Shows skill improvement survey after usage milestones.

#[derive(Debug, Clone)]
pub struct SkillImprovementSurveyState {
    pub active: bool,
    pub initialized: bool,
}

impl SkillImprovementSurveyState {
    pub fn new() -> Self { Self { active: false, initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
    pub fn activate(&mut self) { self.active = true; }
    pub fn deactivate(&mut self) { self.active = false; }
    pub fn is_active(&self) -> bool { self.active }
}
impl Default for SkillImprovementSurveyState { fn default() -> Self { Self::new() } }
