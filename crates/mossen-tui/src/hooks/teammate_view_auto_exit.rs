//! Teammate View Auto Exit hook (useTeammateViewAutoExit.ts).
//! Auto-exits teammate view when the teammate disconnects.

#[derive(Debug, Clone)]
pub struct TeammateViewAutoExitState {
    pub active: bool,
    pub initialized: bool,
}

impl TeammateViewAutoExitState {
    pub fn new() -> Self { Self { active: false, initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
    pub fn activate(&mut self) { self.active = true; }
    pub fn deactivate(&mut self) { self.active = false; }
    pub fn is_active(&self) -> bool { self.active }
}
impl Default for TeammateViewAutoExitState { fn default() -> Self { Self::new() } }
