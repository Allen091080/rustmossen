//! # classifier_approvals_hook — 分类器审批 React hook 等价
//!
//! 对应 TypeScript `utils/classifierApprovalsHook.ts`。
//! 纯状态查询模块，从 classifierApprovals.ts 分离以避免 UI 依赖。

use crate::classifier_approvals::ClassifierApprovals;

/// 查询指定 tool_use_id 是否正在进行分类器检查。
///
/// 在 TypeScript 中这是一个 React hook (useSyncExternalStore)，
/// 在 Rust 中简化为直接查询函数。
pub fn use_is_classifier_checking(approvals: &ClassifierApprovals, tool_use_id: &str) -> bool {
    approvals.is_classifier_checking(tool_use_id)
}
