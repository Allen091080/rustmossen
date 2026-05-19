//! Instances (instances.ts).

#[derive(Debug, Clone, Default)]
pub struct InstancesState {
    pub initialized: bool,
}

impl InstancesState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
