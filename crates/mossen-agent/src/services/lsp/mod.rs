//! LSP (Language Server Protocol) service — manages LSP server instances,
//! routes requests by file extension, and provides passive diagnostic feedback.

pub mod client;
pub mod config;
pub mod diagnostic_registry;
pub mod manager;
pub mod passive_feedback;
pub mod server_instance;
pub mod server_manager;
