//! Event dispatcher with capture/bubble phases (dispatcher.ts).
use super::terminal_event::{EventPhase, TerminalEvent};

/// Dispatch priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DispatchPriority { Discrete, Default, Continuous }

/// Event dispatcher managing capture/bubble propagation.
#[derive(Debug, Clone)]
pub struct Dispatcher {
    pub current_priority: DispatchPriority,
}
impl Dispatcher {
    pub fn new() -> Self { Self { current_priority: DispatchPriority::Default } }
    pub fn dispatch(&self, target_id: usize, event: &mut TerminalEvent, ancestors: &[usize]) -> bool {
        event.set_target(target_id);
        // Capture phase: root -> target
        event.set_phase(EventPhase::Capturing);
        for &ancestor in ancestors.iter().rev() {
            if event.is_propagation_stopped() { break; }
            event.set_current_target(Some(ancestor));
        }
        // At target
        if !event.is_propagation_stopped() {
            event.set_phase(EventPhase::AtTarget);
            event.set_current_target(Some(target_id));
        }
        // Bubble phase: target -> root
        if event.bubbles && !event.is_propagation_stopped() {
            event.set_phase(EventPhase::Bubbling);
            for &ancestor in ancestors.iter() {
                if event.is_propagation_stopped() { break; }
                event.set_current_target(Some(ancestor));
            }
        }
        event.set_phase(EventPhase::None);
        event.set_current_target(None);
        !event.is_immediate_propagation_stopped()
    }
    pub fn dispatch_discrete(&mut self, target_id: usize, event: &mut TerminalEvent, ancestors: &[usize]) -> bool {
        let prev = self.current_priority;
        self.current_priority = DispatchPriority::Discrete;
        let result = self.dispatch(target_id, event, ancestors);
        self.current_priority = prev;
        result
    }
}
impl Default for Dispatcher { fn default() -> Self { Self::new() } }
