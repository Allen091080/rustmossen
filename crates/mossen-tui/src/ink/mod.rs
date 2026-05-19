//! Ink terminal UI framework — Rust translation of the Ink rendering engine.
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
pub mod terminal_notification;
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
