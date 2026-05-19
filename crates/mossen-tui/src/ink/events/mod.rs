//! Event system — DOM-style event propagation for terminal UI.

mod click_event;
mod dispatcher;
mod emitter;
mod event;
mod event_handlers;
mod focus_event;
mod input_event;
mod keyboard_event;
mod terminal_event;
mod terminal_focus_event;

pub use click_event::*;
pub use dispatcher::*;
pub use emitter::*;
pub use event::*;
pub use event_handlers::*;
pub use focus_event::*;
pub use input_event::*;
pub use keyboard_event::*;
pub use terminal_event::*;
pub use terminal_focus_event::*;
