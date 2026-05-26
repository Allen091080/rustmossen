//! Command spec registry for bash completions.
//!
//! Translated from `registry.ts` (54 lines).

use crate::bash::specs;

/// A command specification (fig-spec compatible).
#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub name: String,
    pub description: Option<String>,
    pub subcommands: Vec<CommandSpec>,
    pub args: Vec<Argument>,
    pub options: Vec<SpecOption>,
}

/// An argument in a command spec.
#[derive(Debug, Clone, Default)]
pub struct Argument {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_dangerous: bool,
    pub is_variadic: bool,
    pub is_optional: bool,
    pub is_command: bool,
    pub is_module: Option<String>,
    pub is_script: bool,
}

/// 类型别名 — 对应 TS `Option`。Rust 标准库已经占用 `Option<T>`，因此本仓库
/// 内部使用 [`SpecOption`]；下面的两个别名分别提供方便检索的名字以及与 TS
/// 一致的外部名。
pub type BashCommandSpecOption = SpecOption;
/// 与 TS 同名的别名（避免阴影 `core::option::Option`，调用方需通过完整路径
/// `bash::registry::Option` 引用）。
#[allow(non_camel_case_types)]
pub type Option_ = SpecOption;

/// An option in a command spec.
#[derive(Debug, Clone, Default)]
pub struct SpecOption {
    pub names: Vec<String>,
    pub description: Option<String>,
    pub args: Vec<Argument>,
    pub is_required: bool,
}

/// Load a fig spec for a given command. Returns None if not found.
pub fn load_fig_spec(command: &str) -> Option<CommandSpec> {
    if command.is_empty() || command.contains('/') || command.contains('\\') {
        return None;
    }
    if command.contains("..") {
        return None;
    }
    if command.starts_with('-') && command != "-" {
        return None;
    }
    // We only have built-in specs; no dynamic loading in Rust
    None
}

/// Get a command spec, checking built-in specs first then fig.
pub fn get_command_spec(command: &str) -> Option<CommandSpec> {
    // Check built-in specs
    let builtin_specs = specs::get_all_specs();
    for spec in &builtin_specs {
        if spec.name == command {
            return Some(spec.clone());
        }
    }
    load_fig_spec(command)
}
