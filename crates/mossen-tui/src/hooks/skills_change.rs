//! Skills Change hook (useSkillsChange.ts).
//! Detects changes to skill configuration and reloads.

#[derive(Debug, Clone)]
pub struct SkillsChangeState {
    pub active: bool,
    pub initialized: bool,
}

impl SkillsChangeState {
    pub fn new() -> Self {
        Self {
            active: false,
            initialized: false,
        }
    }
    pub fn initialize(&mut self) {
        self.initialized = true;
    }
    pub fn activate(&mut self) {
        self.active = true;
    }
    pub fn deactivate(&mut self) {
        self.active = false;
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
}
impl Default for SkillsChangeState {
    fn default() -> Self {
        Self::new()
    }
}
