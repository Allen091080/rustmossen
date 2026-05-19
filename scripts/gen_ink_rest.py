#!/usr/bin/env python3
"""Generate ink/events, ink/layout, ink/hooks, ink/components files."""
import os

BASE = "/Users/allen/Documents/rustmossen/crates/mossen-tui/src/ink"

def write_file(relpath, content):
    path = os.path.join(BASE, relpath)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w') as f:
        f.write(content)

# ===== EVENTS =====
write_file("events/event.rs", '''//! Base event class (event.ts).
#[derive(Debug, Clone)]
pub struct Event {
    stop_immediate_propagation: bool,
}
impl Event {
    pub fn new() -> Self { Self { stop_immediate_propagation: false } }
    pub fn stop_immediate_propagation(&mut self) { self.stop_immediate_propagation = true; }
    pub fn did_stop_immediate_propagation(&self) -> bool { self.stop_immediate_propagation }
}
impl Default for Event { fn default() -> Self { Self::new() } }
''')

write_file("events/emitter.rs", '''//! Event emitter with stopImmediatePropagation support (emitter.ts).
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
''')

write_file("events/click_event.rs", '''//! Click event (click-event.ts).
#[derive(Debug, Clone)]
pub struct ClickEvent {
    pub col: u16, pub row: u16,
    pub local_col: u16, pub local_row: u16,
    pub cell_is_blank: bool,
    stopped: bool,
}
impl ClickEvent {
    pub fn new(col: u16, row: u16, cell_is_blank: bool) -> Self {
        Self { col, row, local_col: 0, local_row: 0, cell_is_blank, stopped: false }
    }
    pub fn stop_immediate_propagation(&mut self) { self.stopped = true; }
    pub fn did_stop(&self) -> bool { self.stopped }
    pub fn set_local(&mut self, col: u16, row: u16) { self.local_col = col; self.local_row = row; }
}
''')

write_file("events/focus_event.rs", '''//! Focus event (focus-event.ts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusEventType { Focus, Blur }

#[derive(Debug, Clone)]
pub struct FocusEvent {
    pub event_type: FocusEventType,
    pub related_target: Option<usize>,
    stopped: bool,
}
impl FocusEvent {
    pub fn new(event_type: FocusEventType) -> Self { Self { event_type, related_target: None, stopped: false } }
    pub fn stop_propagation(&mut self) { self.stopped = true; }
    pub fn is_stopped(&self) -> bool { self.stopped }
}
''')

write_file("events/input_event.rs", '''//! Input event (input-event.ts).
#[derive(Debug, Clone)]
pub struct InputEvent {
    pub data: String,
    pub input_type: InputType,
    stopped: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType { InsertText, DeleteContent, InsertFromPaste }
impl InputEvent {
    pub fn new(data: String, input_type: InputType) -> Self { Self { data, input_type, stopped: false } }
    pub fn stop_propagation(&mut self) { self.stopped = true; }
}
''')

write_file("events/keyboard_event.rs", '''//! Keyboard event (keyboard-event.ts).
#[derive(Debug, Clone)]
pub struct KeyboardEvent {
    pub key: String,
    pub code: String,
    pub ctrl: bool, pub meta: bool, pub shift: bool, pub alt: bool,
    pub repeat: bool,
    stopped: bool,
    default_prevented: bool,
}
impl KeyboardEvent {
    pub fn new(key: &str) -> Self {
        Self { key: key.to_string(), code: key.to_string(), ctrl: false, meta: false, shift: false, alt: false, repeat: false, stopped: false, default_prevented: false }
    }
    pub fn with_modifiers(mut self, ctrl: bool, meta: bool, shift: bool, alt: bool) -> Self {
        self.ctrl = ctrl; self.meta = meta; self.shift = shift; self.alt = alt; self
    }
    pub fn stop_propagation(&mut self) { self.stopped = true; }
    pub fn stop_immediate_propagation(&mut self) { self.stopped = true; }
    pub fn prevent_default(&mut self) { self.default_prevented = true; }
    pub fn is_stopped(&self) -> bool { self.stopped }
    pub fn is_default_prevented(&self) -> bool { self.default_prevented }
}
''')

write_file("events/terminal_event.rs", '''//! Terminal event base (terminal-event.ts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventPhase { None, Capturing, AtTarget, Bubbling }

#[derive(Debug, Clone)]
pub struct TerminalEvent {
    pub event_type: String,
    pub timestamp: f64,
    pub bubbles: bool,
    pub cancelable: bool,
    pub target: Option<usize>,
    pub current_target: Option<usize>,
    pub phase: EventPhase,
    propagation_stopped: bool,
    immediate_propagation_stopped: bool,
    default_prevented: bool,
}
impl TerminalEvent {
    pub fn new(event_type: &str, bubbles: bool, cancelable: bool) -> Self {
        Self { event_type: event_type.to_string(), timestamp: 0.0, bubbles, cancelable, target: None, current_target: None, phase: EventPhase::None, propagation_stopped: false, immediate_propagation_stopped: false, default_prevented: false }
    }
    pub fn stop_propagation(&mut self) { self.propagation_stopped = true; }
    pub fn stop_immediate_propagation(&mut self) { self.propagation_stopped = true; self.immediate_propagation_stopped = true; }
    pub fn prevent_default(&mut self) { if self.cancelable { self.default_prevented = true; } }
    pub fn is_propagation_stopped(&self) -> bool { self.propagation_stopped }
    pub fn is_immediate_propagation_stopped(&self) -> bool { self.immediate_propagation_stopped }
    pub fn set_target(&mut self, target: usize) { self.target = Some(target); }
    pub fn set_current_target(&mut self, target: Option<usize>) { self.current_target = target; }
    pub fn set_phase(&mut self, phase: EventPhase) { self.phase = phase; }
}
''')

write_file("events/terminal_focus_event.rs", '''//! Terminal focus/blur event (terminal-focus-event.ts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalFocusEventType { Focus, Blur }

#[derive(Debug, Clone)]
pub struct TerminalFocusEvent {
    pub event_type: TerminalFocusEventType,
}
impl TerminalFocusEvent {
    pub fn new(event_type: TerminalFocusEventType) -> Self { Self { event_type } }
    pub fn is_focus(&self) -> bool { self.event_type == TerminalFocusEventType::Focus }
    pub fn is_blur(&self) -> bool { self.event_type == TerminalFocusEventType::Blur }
}
''')

write_file("events/dispatcher.rs", '''//! Event dispatcher with capture/bubble phases (dispatcher.ts).
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
''')

write_file("events/event_handlers.rs", '''//! Event handler prop definitions (event-handlers.ts).
use std::collections::HashSet;

/// Map from event type to handler prop names.
pub struct HandlerMapping {
    pub bubble: Option<&\'static str>,
    pub capture: Option<&\'static str>,
}

pub fn handler_for_event(event_type: &str) -> Option<HandlerMapping> {
    match event_type {
        "keydown" => Some(HandlerMapping { bubble: Some("onKeyDown"), capture: Some("onKeyDownCapture") }),
        "focus" => Some(HandlerMapping { bubble: Some("onFocus"), capture: Some("onFocusCapture") }),
        "blur" => Some(HandlerMapping { bubble: Some("onBlur"), capture: Some("onBlurCapture") }),
        "paste" => Some(HandlerMapping { bubble: Some("onPaste"), capture: Some("onPasteCapture") }),
        "resize" => Some(HandlerMapping { bubble: Some("onResize"), capture: None }),
        "click" => Some(HandlerMapping { bubble: Some("onClick"), capture: None }),
        _ => None,
    }
}

/// Set of all event handler prop names.
pub fn event_handler_props() -> HashSet<&\'static str> {
    let mut set = HashSet::new();
    for prop in &["onKeyDown", "onKeyDownCapture", "onFocus", "onFocusCapture", "onBlur", "onBlurCapture", "onPaste", "onPasteCapture", "onResize", "onClick", "onMouseEnter", "onMouseLeave"] {
        set.insert(*prop);
    }
    set
}
''')

# ===== LAYOUT =====
write_file("layout/geometry.rs", '''//! Geometry types (geometry.ts).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }
impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self { Self { x, y, width: w, height: h } }
    pub fn contains(&self, px: f32, py: f32) -> bool { px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height }
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.width && self.x + self.width > other.x && self.y < other.y + other.height && self.y + self.height > other.y
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Edges { pub top: f32, pub right: f32, pub bottom: f32, pub left: f32 }
impl Edges {
    pub fn uniform(v: f32) -> Self { Self { top: v, right: v, bottom: v, left: v } }
    pub fn horizontal(&self) -> f32 { self.left + self.right }
    pub fn vertical(&self) -> f32 { self.top + self.bottom }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Size { pub width: f32, pub height: f32 }
''')

write_file("layout/node.rs", '''//! Layout node (node.ts).
use super::geometry::{Edges, Rect, Size};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection { Row, Column, RowReverse, ColumnReverse }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexWrap { NoWrap, Wrap, WrapReverse }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems { Stretch, FlexStart, FlexEnd, Center, Baseline }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent { FlexStart, FlexEnd, Center, SpaceBetween, SpaceAround, SpaceEvenly }

#[derive(Debug, Clone)]
pub struct LayoutNode {
    pub id: usize,
    pub rect: Rect,
    pub content_size: Size,
    pub padding: Edges,
    pub margin: Edges,
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub align_items: AlignItems,
    pub justify_content: JustifyContent,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
}

impl LayoutNode {
    pub fn new(id: usize) -> Self {
        Self { id, rect: Rect::default(), content_size: Size::default(), padding: Edges::default(), margin: Edges::default(), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::NoWrap, flex_grow: 0.0, flex_shrink: 1.0, align_items: AlignItems::Stretch, justify_content: JustifyContent::FlexStart, children: Vec::new(), parent: None }
    }
    pub fn inner_rect(&self) -> Rect {
        Rect { x: self.rect.x + self.padding.left, y: self.rect.y + self.padding.top, width: self.rect.width - self.padding.horizontal(), height: self.rect.height - self.padding.vertical() }
    }
}
''')

write_file("layout/engine.rs", '''//! Layout engine (engine.ts) — computes flexbox layout.
use super::node::LayoutNode;
use super::geometry::Size;

/// Compute layout for a tree of nodes.
pub fn compute_layout(nodes: &mut [LayoutNode], root_id: usize, available: Size) {
    if let Some(root) = nodes.iter_mut().find(|n| n.id == root_id) {
        root.rect.width = available.width;
        root.rect.height = available.height;
    }
    // Simplified flex layout - distribute space among children
    layout_children(nodes, root_id);
}

fn layout_children(nodes: &mut [LayoutNode], parent_id: usize) {
    let (direction, inner_width, inner_height, children) = {
        let parent = nodes.iter().find(|n| n.id == parent_id).unwrap();
        let inner = parent.inner_rect();
        (parent.flex_direction, inner.width, inner.height, parent.children.clone())
    };
    if children.is_empty() { return; }
    let count = children.len() as f32;
    let is_row = matches!(direction, super::node::FlexDirection::Row | super::node::FlexDirection::RowReverse);
    let item_w = if is_row { inner_width / count } else { inner_width };
    let item_h = if is_row { inner_height } else { inner_height / count };
    for (i, &child_id) in children.iter().enumerate() {
        if let Some(child) = nodes.iter_mut().find(|n| n.id == child_id) {
            if is_row { child.rect.x = i as f32 * item_w; child.rect.y = 0.0; }
            else { child.rect.x = 0.0; child.rect.y = i as f32 * item_h; }
            child.rect.width = item_w; child.rect.height = item_h;
        }
        layout_children(nodes, child_id);
    }
}
''')

write_file("layout/yoga.rs", '''//! Yoga-like layout helpers (yoga.ts).
use super::geometry::Edges;
use super::node::{AlignItems, FlexDirection, FlexWrap, JustifyContent, LayoutNode};

/// Parse flex shorthand into grow/shrink/basis.
pub fn parse_flex(flex: f32) -> (f32, f32, f32) {
    if flex <= 0.0 { (0.0, 1.0, 0.0) } else { (flex, 1.0, 0.0) }
}

/// Apply gap between children.
pub fn apply_gap(nodes: &mut [LayoutNode], parent_id: usize, gap: f32) {
    let children: Vec<usize> = nodes.iter().find(|n| n.id == parent_id).map(|n| n.children.clone()).unwrap_or_default();
    if children.len() < 2 { return; }
    let direction = nodes.iter().find(|n| n.id == parent_id).map(|n| n.flex_direction).unwrap_or(FlexDirection::Row);
    let is_row = matches!(direction, FlexDirection::Row | FlexDirection::RowReverse);
    for (i, &child_id) in children.iter().enumerate().skip(1) {
        if let Some(child) = nodes.iter_mut().find(|n| n.id == child_id) {
            if is_row { child.rect.x += gap * i as f32; }
            else { child.rect.y += gap * i as f32; }
        }
    }
}

/// Set padding from style values.
pub fn edges_from_style(top: Option<f32>, right: Option<f32>, bottom: Option<f32>, left: Option<f32>, x: Option<f32>, y: Option<f32>, all: Option<f32>) -> Edges {
    let base = all.unwrap_or(0.0);
    Edges { top: top.or(y).unwrap_or(base), right: right.or(x).unwrap_or(base), bottom: bottom.or(y).unwrap_or(base), left: left.or(x).unwrap_or(base) }
}
''')

# ===== INK HOOKS =====
ink_hooks = [
    ("use_animation_frame", "AnimationFrame", "Calls a callback on each animation frame tick."),
    ("use_app", "App", "Provides access to the ink app instance."),
    ("use_declared_cursor", "DeclaredCursor", "Manages declared cursor position for rendering."),
    ("use_input", "Input", "Subscribes to raw terminal input events."),
    ("use_interval", "Interval", "Runs a callback at regular intervals."),
    ("use_search_highlight", "SearchHighlight", "Manages search term highlighting in output."),
    ("use_selection", "Selection", "Manages text selection state in the terminal."),
    ("use_stdin", "Stdin", "Provides access to stdin stream."),
    ("use_tab_status", "TabStatus", "Manages per-tab status chrome metadata."),
    ("use_terminal_focus", "TerminalFocus", "Tracks terminal window focus state."),
    ("use_terminal_title", "TerminalTitle", "Sets the terminal window title."),
    ("use_terminal_viewport", "TerminalViewport", "Manages the terminal viewport/scrollback."),
]

for fname, prefix, doc in ink_hooks:
    struct_name = f"{prefix}HookState"
    content = f'''//! {prefix} hook ({fname.replace("_", "-")}.ts).
//! {doc}

#[derive(Debug, Clone)]
pub struct {struct_name} {{
    pub active: bool,
'''
    if "animation" in fname:
        content += '''    pub interval_ms: Option<u64>,
    pub time: u64,
    pub frame_count: u64,
}
impl AnimationFrameHookState {
    pub fn new(interval_ms: Option<u64>) -> Self { Self { active: interval_ms.is_some(), interval_ms, time: 0, frame_count: 0 } }
    pub fn tick(&mut self, delta_ms: u64) { if self.active { self.time += delta_ms; self.frame_count += 1; } }
    pub fn set_interval(&mut self, ms: Option<u64>) { self.interval_ms = ms; self.active = ms.is_some(); }
}
impl Default for AnimationFrameHookState { fn default() -> Self { Self::new(None) } }
'''
    elif "interval" in fname:
        content += '''    pub interval_ms: u64,
    pub tick_count: u64,
}
impl IntervalHookState {
    pub fn new(interval_ms: u64) -> Self { Self { active: true, interval_ms, tick_count: 0 } }
    pub fn tick(&mut self) { self.tick_count += 1; }
    pub fn set_active(&mut self, active: bool) { self.active = active; }
}
impl Default for IntervalHookState { fn default() -> Self { Self::new(1000) } }
'''
    elif "selection" in fname:
        content += '''    pub start: Option<(u16, u16)>,
    pub end: Option<(u16, u16)>,
    pub text: String,
}
impl SelectionHookState {
    pub fn new() -> Self { Self { active: false, start: None, end: None, text: String::new() } }
    pub fn start_selection(&mut self, col: u16, row: u16) { self.active = true; self.start = Some((col, row)); self.end = Some((col, row)); }
    pub fn update_selection(&mut self, col: u16, row: u16) { if self.active { self.end = Some((col, row)); } }
    pub fn end_selection(&mut self, text: String) { self.text = text; self.active = false; }
    pub fn clear(&mut self) { self.active = false; self.start = None; self.end = None; self.text.clear(); }
    pub fn has_selection(&self) -> bool { !self.text.is_empty() }
}
impl Default for SelectionHookState { fn default() -> Self { Self::new() } }
'''
    elif "focus" in fname:
        content += '''    pub focused: bool,
}
impl TerminalFocusHookState {
    pub fn new() -> Self { Self { active: true, focused: true } }
    pub fn set_focused(&mut self, focused: bool) { self.focused = focused; }
    pub fn is_focused(&self) -> bool { self.focused }
}
impl Default for TerminalFocusHookState { fn default() -> Self { Self::new() } }
'''
    elif "title" in fname:
        content += '''    pub title: String,
}
impl TerminalTitleHookState {
    pub fn new() -> Self { Self { active: true, title: String::new() } }
    pub fn set_title(&mut self, title: &str) { self.title = title.to_string(); }
    pub fn get_title(&self) -> &str { &self.title }
    pub fn to_escape_sequence(&self) -> String { format!("\\x1b]2;{}\\x07", self.title) }
}
impl Default for TerminalTitleHookState { fn default() -> Self { Self::new() } }
'''
    elif "tab_status" in fname:
        content += '''    pub indicator: Option<String>,
    pub status: Option<String>,
}
impl TabStatusHookState {
    pub fn new() -> Self { Self { active: true, indicator: None, status: None } }
    pub fn set_indicator(&mut self, color: Option<String>) { self.indicator = color; }
    pub fn set_status(&mut self, text: Option<String>) { self.status = text; }
    pub fn clear(&mut self) { self.indicator = None; self.status = None; }
}
impl Default for TabStatusHookState { fn default() -> Self { Self::new() } }
'''
    else:
        content += f'''}}
impl {struct_name} {{
    pub fn new() -> Self {{ Self {{ active: true }} }}
    pub fn set_active(&mut self, active: bool) {{ self.active = active; }}
}}
impl Default for {struct_name} {{ fn default() -> Self {{ Self::new() }} }}
'''
    write_file(f"hooks/{fname}.rs", content)

print("Events, Layout, and Ink Hooks created")
