//! # worktree_mode_enabled — Worktree 模式启用状态
//!
//! 对应 TypeScript `utils/worktreeModeEnabled.ts`。
//! Worktree 模式现在对所有用户无条件启用。

/// 返回 worktree 模式是否启用（始终为 true）。
pub fn is_worktree_mode_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worktree_mode_always_enabled() {
        assert!(is_worktree_mode_enabled());
    }
}
