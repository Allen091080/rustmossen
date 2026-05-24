//! Swarm Initialization hook (useSwarmInitialization.ts).
//! Initializes swarm (multi-agent) mode for the session.

#[derive(Debug, Clone)]
pub struct SwarmInitializationState {
    pub active: bool,
    pub initialized: bool,
}

impl SwarmInitializationState {
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
impl Default for SwarmInitializationState {
    fn default() -> Self {
        Self::new()
    }
}
