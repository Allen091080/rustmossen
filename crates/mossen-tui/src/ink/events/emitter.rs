//! Event emitter with stopImmediatePropagation support (emitter.ts).
use std::collections::HashMap;
use super::event::Event;

pub type EventHandler = Box<dyn Fn(&mut Event) + Send + Sync>;

#[derive(Default)]
pub struct EventEmitter {
    listeners: HashMap<String, Vec<EventHandler>>,
}

impl EventEmitter {
    pub fn new() -> Self { Self { listeners: HashMap::new() } }
    pub fn on(&mut self, event_type: &str, handler: EventHandler) {
        self.listeners.entry(event_type.to_string()).or_default().push(handler);
    }
    pub fn off_all(&mut self, event_type: &str) { self.listeners.remove(event_type); }
    pub fn emit(&self, event_type: &str, event: &mut Event) -> bool {
        if let Some(handlers) = self.listeners.get(event_type) {
            for handler in handlers {
                handler(event);
                if event.did_stop_immediate_propagation() { break; }
            }
            true
        } else { false }
    }
    pub fn listener_count(&self, event_type: &str) -> usize {
        self.listeners.get(event_type).map_or(0, |v| v.len())
    }
}
impl std::fmt::Debug for EventEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventEmitter").field("listener_count", &self.listeners.len()).finish()
    }
}
