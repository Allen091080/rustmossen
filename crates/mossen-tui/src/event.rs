//! Event handling system for the TUI layer.
//!
//! Channel-based event bus using tokio mpsc channels.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use tokio::sync::mpsc;

const TUI_EVENT_LOG_PATH_ENV: &str = "MOSSEN_TUI_EVENT_LOG_PATH";

/// Maximum pending terminal events the App should coalesce after one wakeup.
///
/// Ticks are intentionally lossy: a long render should not leave hundreds of
/// stale animation ticks ahead of real keyboard, mouse, resize, or engine work.
pub const DEFAULT_EVENT_BATCH_LIMIT: usize = 256;

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

/// Channel-based event bus for input, resize, focus, and tick events.
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

    /// Receive one event, then drain currently queued events up to `max_events`.
    ///
    /// Redundant ticks are collapsed so the render loop can catch up after a
    /// slow frame without delaying input behind stale animation wakeups.
    pub async fn recv_coalesced(&mut self, max_events: usize) -> Option<Vec<AppEvent>> {
        let first = self.rx.recv().await?;
        let max_events = max_events.max(1);
        let mut events = Vec::with_capacity(max_events.min(16));
        let mut non_lossy_events = usize::from(!app_event_is_lossy(&first));
        events.push(first);
        while non_lossy_events < max_events {
            match self.rx.try_recv() {
                Ok(event) => {
                    if !app_event_is_lossy(&event) {
                        non_lossy_events = non_lossy_events.saturating_add(1);
                    }
                    events.push(event);
                }
                Err(_) => break,
            }
        }
        let raw_count = events.len();
        let coalesced = coalesce_event_batch(events);
        append_tui_event_log_line(format!(
            "recv_coalesced raw_count={raw_count} coalesced_count={} {}",
            coalesced.len(),
            summarize_app_events(&coalesced)
        ));
        Some(coalesced)
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

/// Spawn a background reader that forwards terminal events.
///
/// Crossterm event polling is blocking. Keep it off the async runtime so slow
/// renders or engine-message bursts cannot starve keyboard input while a
/// background tool is still running.
pub fn spawn_crossterm_reader(tx: mpsc::UnboundedSender<AppEvent>) {
    let _ = std::thread::Builder::new()
        .name("mossen-tui-event-reader".to_string())
        .spawn(move || loop {
            let has_event =
                crossterm::event::poll(std::time::Duration::from_millis(16)).unwrap_or(false);
            if !has_event {
                continue;
            }

            if !forward_next_crossterm_event(&tx) {
                break;
            }

            while crossterm::event::poll(std::time::Duration::from_millis(0)).unwrap_or(false) {
                if !forward_next_crossterm_event(&tx) {
                    return;
                }
            }
        });
}

fn forward_next_crossterm_event(tx: &mpsc::UnboundedSender<AppEvent>) -> bool {
    match crossterm::event::read() {
        Ok(crossterm::event::Event::Key(key)) => tx.send(AppEvent::Key(key)).is_ok(),
        Ok(crossterm::event::Event::Mouse(mouse)) => tx.send(AppEvent::Mouse(mouse)).is_ok(),
        Ok(crossterm::event::Event::Resize(width, height)) => {
            tx.send(AppEvent::Resize { width, height }).is_ok()
        }
        Ok(crossterm::event::Event::FocusGained) => tx.send(AppEvent::FocusChange(true)).is_ok(),
        Ok(crossterm::event::Event::FocusLost) => tx.send(AppEvent::FocusChange(false)).is_ok(),
        Ok(_) => true,
        Err(_) => false,
    }
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

fn coalesce_event_batch(events: Vec<AppEvent>) -> Vec<AppEvent> {
    let mut tick_seen = false;
    let mut pending_lossy: Option<AppEvent> = None;
    let mut coalesced = Vec::with_capacity(events.len());
    for event in events {
        match event {
            AppEvent::Tick => {
                flush_pending_lossy(&mut coalesced, &mut pending_lossy);
                if tick_seen {
                    continue;
                }
                tick_seen = true;
                coalesced.push(AppEvent::Tick);
            }
            AppEvent::Resize { .. } => {
                if matches!(&pending_lossy, Some(AppEvent::Resize { .. })) {
                    pending_lossy = Some(event);
                } else {
                    flush_pending_lossy(&mut coalesced, &mut pending_lossy);
                    pending_lossy = Some(event);
                }
            }
            AppEvent::FocusChange(_) => {
                if matches!(&pending_lossy, Some(AppEvent::FocusChange(_))) {
                    pending_lossy = Some(event);
                } else {
                    flush_pending_lossy(&mut coalesced, &mut pending_lossy);
                    pending_lossy = Some(event);
                }
            }
            event => {
                flush_pending_lossy(&mut coalesced, &mut pending_lossy);
                coalesced.push(event);
            }
        }
    }
    flush_pending_lossy(&mut coalesced, &mut pending_lossy);
    coalesced
}

fn app_event_is_lossy(event: &AppEvent) -> bool {
    matches!(
        event,
        AppEvent::Tick | AppEvent::Resize { .. } | AppEvent::FocusChange(_)
    )
}

fn append_tui_event_log_line(line: String) {
    let Some(path) = std::env::var_os(TUI_EVENT_LOG_PATH_ENV) else {
        return;
    };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write as _;
        let _ = writeln!(file, "{line}");
    }
}

fn summarize_app_events(events: &[AppEvent]) -> String {
    let mut keys = 0usize;
    let mut mouse_up = 0usize;
    let mut mouse_down = 0usize;
    let mut mouse_other = 0usize;
    let mut resizes = 0usize;
    let mut focus = 0usize;
    let mut ticks = 0usize;
    let mut quits = 0usize;
    for event in events {
        match event {
            AppEvent::Key(_) => keys = keys.saturating_add(1),
            AppEvent::Mouse(mouse) => match mouse.kind {
                crossterm::event::MouseEventKind::ScrollUp => {
                    mouse_up = mouse_up.saturating_add(1);
                }
                crossterm::event::MouseEventKind::ScrollDown => {
                    mouse_down = mouse_down.saturating_add(1);
                }
                _ => mouse_other = mouse_other.saturating_add(1),
            },
            AppEvent::Resize { .. } => resizes = resizes.saturating_add(1),
            AppEvent::FocusChange(_) => focus = focus.saturating_add(1),
            AppEvent::Tick => ticks = ticks.saturating_add(1),
            AppEvent::Quit => quits = quits.saturating_add(1),
        }
    }
    format!(
        "keys={keys} mouse_up={mouse_up} mouse_down={mouse_down} mouse_other={mouse_other} resizes={resizes} focus={focus} ticks={ticks} quits={quits}"
    )
}

fn flush_pending_lossy(coalesced: &mut Vec<AppEvent>, pending_lossy: &mut Option<AppEvent>) {
    if let Some(event) = pending_lossy.take() {
        coalesced.push(event);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn recv_coalesced_drops_stale_ticks_without_dropping_input() {
        let mut bus = EventBus::new();
        let tx = bus.sender();
        tx.send(AppEvent::Tick).expect("send first tick");
        tx.send(AppEvent::Tick).expect("send stale tick");
        tx.send(AppEvent::Resize {
            width: 100,
            height: 30,
        })
        .expect("send resize");
        tx.send(AppEvent::Tick).expect("send later stale tick");
        tx.send(AppEvent::FocusChange(false))
            .expect("send focus change");
        tx.send(AppEvent::Quit).expect("send quit");

        let batch = bus
            .recv_coalesced(DEFAULT_EVENT_BATCH_LIMIT)
            .await
            .expect("coalesced batch");

        assert_eq!(batch.len(), 4, "{batch:?}");
        assert!(matches!(batch[0], AppEvent::Tick));
        assert!(matches!(
            batch[1],
            AppEvent::Resize {
                width: 100,
                height: 30
            }
        ));
        assert!(matches!(batch[2], AppEvent::FocusChange(false)));
        assert!(matches!(batch[3], AppEvent::Quit));
    }

    #[tokio::test]
    async fn recv_coalesced_drains_stale_tick_backlog_to_reach_input() {
        let mut bus = EventBus::new();
        let tx = bus.sender();
        for _ in 0..2048 {
            tx.send(AppEvent::Tick).expect("send stale tick");
        }
        tx.send(AppEvent::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::ScrollDown,
            column: 7,
            row: 3,
            modifiers: KeyModifiers::NONE,
        }))
        .expect("send mouse");

        let batch = bus.recv_coalesced(8).await.expect("coalesced batch");

        assert_eq!(batch.len(), 2, "{batch:?}");
        assert!(matches!(batch[0], AppEvent::Tick));
        assert!(matches!(batch[1], AppEvent::Mouse(_)));
        assert!(
            bus.try_recv().is_none(),
            "stale ticks should not remain queued"
        );
    }

    #[tokio::test]
    async fn recv_coalesced_collapses_resize_and_focus_storms_at_input_boundaries() {
        let mut bus = EventBus::new();
        let tx = bus.sender();
        tx.send(AppEvent::Resize {
            width: 80,
            height: 20,
        })
        .expect("send first resize");
        tx.send(AppEvent::Resize {
            width: 100,
            height: 30,
        })
        .expect("send latest resize");
        tx.send(AppEvent::FocusChange(false))
            .expect("send first focus");
        tx.send(AppEvent::FocusChange(true))
            .expect("send latest focus");
        tx.send(AppEvent::Key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::NONE,
        )))
        .expect("send key");
        tx.send(AppEvent::Resize {
            width: 120,
            height: 40,
        })
        .expect("send post-key resize");
        tx.send(AppEvent::Resize {
            width: 132,
            height: 48,
        })
        .expect("send latest post-key resize");
        tx.send(AppEvent::Quit).expect("send quit");

        let batch = bus
            .recv_coalesced(DEFAULT_EVENT_BATCH_LIMIT)
            .await
            .expect("coalesced batch");

        assert_eq!(batch.len(), 5, "{batch:?}");
        assert!(matches!(
            batch[0],
            AppEvent::Resize {
                width: 100,
                height: 30
            }
        ));
        assert!(matches!(batch[1], AppEvent::FocusChange(true)));
        assert!(matches!(
            batch[2],
            AppEvent::Key(KeyEvent {
                code: KeyCode::Char('x'),
                ..
            })
        ));
        assert!(matches!(
            batch[3],
            AppEvent::Resize {
                width: 132,
                height: 48
            }
        ));
        assert!(matches!(batch[4], AppEvent::Quit));
    }
}
