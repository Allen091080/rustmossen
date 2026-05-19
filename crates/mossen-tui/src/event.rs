//! Event handling system for the TUI layer.
//!
//! Translates the Ink EventEmitter pattern into a channel-based event bus
//! using tokio mpsc channels.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use tokio::sync::mpsc;

/// Top-level application events consumed by the main event loop.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Keyboard input event
    Key(KeyEvent),
    /// Mouse input event
    Mouse(MouseEvent),
    /// Terminal resize
    Resize { width: u16, height: u16 },
    /// Terminal focus gained/lost
    FocusChange(bool),
    /// Tick event for animations (emitted at ~30fps)
    Tick,
    /// Request to quit the application
    Quit,
}

/// Channel-based event bus replacing Ink's EventEmitter.
pub struct EventBus {
    tx: mpsc::UnboundedSender<AppEvent>,
    rx: mpsc::UnboundedReceiver<AppEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self { tx, rx }
    }

    /// Get a clone of the sender for dispatching events from background tasks.
    pub fn sender(&self) -> mpsc::UnboundedSender<AppEvent> {
        self.tx.clone()
    }

    /// Receive the next event (blocks until available).
    pub async fn recv(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }

    /// Try to receive without blocking.
    pub fn try_recv(&mut self) -> Option<AppEvent> {
        self.rx.try_recv().ok()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawn a background task that reads crossterm events and forwards them.
pub fn spawn_crossterm_reader(tx: mpsc::UnboundedSender<AppEvent>) {
    tokio::spawn(async move {
        loop {
            if crossterm::event::poll(std::time::Duration::from_millis(16)).unwrap_or(false) {
                match crossterm::event::read() {
                    Ok(crossterm::event::Event::Key(key)) => {
                        if tx.send(AppEvent::Key(key)).is_err() {
                            break;
                        }
                    }
                    Ok(crossterm::event::Event::Mouse(mouse)) => {
                        if tx.send(AppEvent::Mouse(mouse)).is_err() {
                            break;
                        }
                    }
                    Ok(crossterm::event::Event::Resize(w, h)) => {
                        if tx
                            .send(AppEvent::Resize {
                                width: w,
                                height: h,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(crossterm::event::Event::FocusGained) => {
                        let _ = tx.send(AppEvent::FocusChange(true));
                    }
                    Ok(crossterm::event::Event::FocusLost) => {
                        let _ = tx.send(AppEvent::FocusChange(false));
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }
    });
}

/// Spawn a tick timer for animations.
pub fn spawn_tick_timer(tx: mpsc::UnboundedSender<AppEvent>, interval_ms: u64) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(interval_ms));
        loop {
            interval.tick().await;
            if tx.send(AppEvent::Tick).is_err() {
                break;
            }
        }
    });
}

/// Input action — high-level interpretation of key events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    /// Character input
    Char(char),
    /// Backspace / delete backward
    Backspace,
    /// Delete forward
    Delete,
    /// Move cursor left
    Left,
    /// Move cursor right
    Right,
    /// Move to start of line
    Home,
    /// Move to end of line
    End,
    /// History navigation up
    Up,
    /// History navigation down
    Down,
    /// Submit current input
    Submit,
    /// Tab completion
    Tab,
    /// Escape / cancel
    Escape,
    /// Ctrl+C interrupt
    Interrupt,
    /// Ctrl+D EOF
    Eof,
    /// Page up
    PageUp,
    /// Page down
    PageDown,
    /// Paste content
    Paste(String),
}

impl InputAction {
    /// Convert a crossterm KeyEvent into a high-level InputAction.
    pub fn from_key_event(key: &KeyEvent) -> Option<Self> {
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => Some(Self::Interrupt),
            (KeyCode::Char('d'), m) if m.contains(KeyModifiers::CONTROL) => Some(Self::Eof),
            (KeyCode::Char(c), _) => Some(Self::Char(c)),
            (KeyCode::Backspace, _) => Some(Self::Backspace),
            (KeyCode::Delete, _) => Some(Self::Delete),
            (KeyCode::Left, _) => Some(Self::Left),
            (KeyCode::Right, _) => Some(Self::Right),
            (KeyCode::Up, _) => Some(Self::Up),
            (KeyCode::Down, _) => Some(Self::Down),
            (KeyCode::Home, _) => Some(Self::Home),
            (KeyCode::End, _) => Some(Self::End),
            (KeyCode::Enter, _) => Some(Self::Submit),
            (KeyCode::Tab, _) => Some(Self::Tab),
            (KeyCode::Esc, _) => Some(Self::Escape),
            (KeyCode::PageUp, _) => Some(Self::PageUp),
            (KeyCode::PageDown, _) => Some(Self::PageDown),
            _ => None,
        }
    }
}
