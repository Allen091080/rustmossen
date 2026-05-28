//! 命令注册表 — 连接 mossen-commands 的所有 Directive。
//!
//! 对应 TS 的 commands.ts，负责构建完整的斜杠命令注册表。

use mossen_commands::{all_directives, BoxedDirective, CommandContext, Directive};
use tracing::info;

/// 全局命令注册表。
pub struct DirectiveRegistry {
    /// 所有已注册的指令。
    directives: Vec<BoxedDirective>,
}

impl DirectiveRegistry {
    /// 构建完整的命令注册表。
    ///
    /// 加载所有内置指令（80+），对应 TS 的 COMMANDS() + getCommands()。
    pub fn new() -> Self {
        let directives = all_directives();
        info!(count = directives.len(), "directive registry initialized");
        Self { directives }
    }

    /// 按名称或别名查找指令。
    pub fn find(&self, name: &str) -> Option<&dyn Directive> {
        mossen_commands::find_directive(&self.directives, name)
    }

    /// 获取在给定上下文中启用的所有指令。
    pub fn enabled(&self, ctx: &CommandContext) -> Vec<&dyn Directive> {
        mossen_commands::enabled_directives(&self.directives, ctx)
    }

    /// 获取可见（非隐藏且启用的）指令（用于帮助显示）。
    pub fn visible(&self, ctx: &CommandContext) -> Vec<&dyn Directive> {
        mossen_commands::visible_directives(&self.directives, ctx)
    }

    /// 获取所有已注册指令的引用。
    pub fn all(&self) -> &[BoxedDirective] {
        &self.directives
    }

    /// 获取指令数量。
    pub fn len(&self) -> usize {
        self.directives.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.directives.is_empty()
    }
}

impl Default for DirectiveRegistry {
    fn default() -> Self {
        Self::new()
    }
}
