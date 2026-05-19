#!/usr/bin/env python3
"""Generate ink/ module structure with all subdirectories."""
import os

BASE = "/Users/allen/Documents/rustmossen/crates/mossen-tui/src/ink"
files = []

# Main ink/mod.rs
files.append(("mod.rs", '''//! Ink terminal UI framework — Rust translation of the Ink rendering engine.
//!
//! Translates the custom Ink framework (React-for-terminals) into native
//! Rust using ratatui/crossterm primitives.

#![allow(dead_code, unused_variables)]

pub mod components;
pub mod events;
pub mod hooks;
pub mod layout;
pub mod termio;

// Root-level modules corresponding to ink/*.ts files
mod ansi_render;
mod bidi;
mod clear_terminal;
mod colorize;
mod constants;
mod dom;
mod focus;
mod frame;
mod get_max_width;
mod hit_test;
mod ink_app;
mod instances;
mod line_width_cache;
mod log_update;
mod measure_element;
mod measure_text;
mod node_cache;
mod optimizer;
mod output;
mod parse_keypress;
mod reconciler;
mod render_border;
mod render_node_to_output;
mod render_to_screen;
mod renderer;
mod root;
mod screen;
mod search_highlight;
mod selection;
mod squash_text_nodes;
mod string_width;
mod styles;
mod supports_hyperlinks;
mod tabstops;
mod terminal_focus_state;
mod terminal_querier;
mod terminal;
mod terminal_io;
mod terminal_notification;
mod warn;
mod widest_line;
mod wrap_text;
mod wrap_ansi;

pub use ansi_render::*;
pub use bidi::*;
pub use clear_terminal::*;
pub use colorize::*;
pub use constants::*;
pub use dom::*;
pub use focus::*;
pub use frame::*;
pub use get_max_width::*;
pub use hit_test::*;
pub use ink_app::*;
pub use instances::*;
pub use line_width_cache::*;
pub use log_update::*;
pub use measure_element::*;
pub use measure_text::*;
pub use node_cache::*;
pub use optimizer::*;
pub use output::*;
pub use parse_keypress::*;
pub use reconciler::*;
pub use render_border::*;
pub use render_node_to_output::*;
pub use render_to_screen::*;
pub use renderer::*;
pub use root::*;
pub use screen::*;
pub use search_highlight::*;
pub use selection::*;
pub use squash_text_nodes::*;
pub use string_width::*;
pub use styles::*;
pub use supports_hyperlinks::*;
pub use tabstops::*;
pub use terminal_focus_state::*;
pub use terminal_querier::*;
pub use terminal::*;
pub use terminal_io::*;
pub use terminal_notification::*;
pub use warn::*;
pub use widest_line::*;
pub use wrap_text::*;
pub use wrap_ansi::*;
'''))

# ink/termio/mod.rs
files.append(("termio/mod.rs", '''//! Terminal I/O — ANSI parser, tokenizer, and escape sequence handling.

mod ansi;
mod csi;
mod dec;
mod esc;
mod osc;
mod parser;
mod sgr;
mod tokenize;
mod types;

pub use ansi::*;
pub use csi::*;
pub use dec::*;
pub use esc::*;
pub use osc::*;
pub use parser::*;
pub use sgr::*;
pub use tokenize::*;
pub use types::*;
'''))

# ink/events/mod.rs
files.append(("events/mod.rs", '''//! Event system — DOM-style event propagation for terminal UI.

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
'''))

# ink/layout/mod.rs
files.append(("layout/mod.rs", '''//! Layout engine — flexbox-like layout for terminal UI elements.

mod engine;
mod geometry;
mod node;
mod yoga;

pub use engine::*;
pub use geometry::*;
pub use node::*;
pub use yoga::*;
'''))

# ink/hooks/mod.rs
files.append(("hooks/mod.rs", '''//! Ink hooks — low-level UI hooks for the ink rendering framework.

mod use_animation_frame;
mod use_app;
mod use_declared_cursor;
mod use_input;
mod use_interval;
mod use_search_highlight;
mod use_selection;
mod use_stdin;
mod use_tab_status;
mod use_terminal_focus;
mod use_terminal_title;
mod use_terminal_viewport;

pub use use_animation_frame::*;
pub use use_app::*;
pub use use_declared_cursor::*;
pub use use_input::*;
pub use use_interval::*;
pub use use_search_highlight::*;
pub use use_selection::*;
pub use use_stdin::*;
pub use use_tab_status::*;
pub use use_terminal_focus::*;
pub use use_terminal_title::*;
pub use use_terminal_viewport::*;
'''))

# ink/components/mod.rs
files.append(("components/mod.rs", '''//! Ink UI components — translated to Rust widget/state equivalents.

mod alternate_screen;
mod app;
mod app_context;
mod box_component;
mod button;
mod clock_context;
mod cursor_declaration_context;
mod error_overview;
mod link;
mod newline;
mod no_select;
mod raw_ansi;
mod scroll_box;
mod spacer;
mod stdin_context;
mod terminal_focus_context;
mod terminal_size_context;
mod text;

pub use alternate_screen::*;
pub use app::*;
pub use app_context::*;
pub use box_component::*;
pub use button::*;
pub use clock_context::*;
pub use cursor_declaration_context::*;
pub use error_overview::*;
pub use link::*;
pub use newline::*;
pub use no_select::*;
pub use raw_ansi::*;
pub use scroll_box::*;
pub use spacer::*;
pub use stdin_context::*;
pub use terminal_focus_context::*;
pub use terminal_size_context::*;
pub use text::*;
'''))

for fname, content in files:
    path = os.path.join(BASE, fname)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w') as f:
        f.write(content)
    print(f"Created {fname}")

print(f"\nTotal: {len(files)} files created")
