//! Plugin management service — install, uninstall, enable, disable, update operations.

pub mod cli_commands;
pub mod installation_manager;
pub mod operations;

pub use cli_commands::*;
pub use installation_manager::*;
pub use operations::*;
