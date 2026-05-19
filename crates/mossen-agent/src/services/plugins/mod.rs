//! Plugin management service — install, uninstall, enable, disable, update operations.

pub mod operations;
pub mod cli_commands;
pub mod installation_manager;

pub use operations::*;
pub use cli_commands::*;
pub use installation_manager::*;
