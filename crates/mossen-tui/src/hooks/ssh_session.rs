//! Ssh Session hook (useSshSession.ts).
//! Manages SSH session connection and tunneling.

#[derive(Debug, Clone)]
pub struct SshSessionState {
    pub active: bool,
    pub initialized: bool,
}

impl SshSessionState {
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
impl Default for SshSessionState {
    fn default() -> Self {
        Self::new()
    }
}
