//! # set_ops — 集合操作
//!
//! 对应 TypeScript `utils/set.ts`。
//! 热路径代码，已优化速度。

use std::collections::HashSet;
use std::hash::Hash;

/// 计算集合差集 (a - b)。
pub fn difference<A: Eq + Hash + Clone>(a: &HashSet<A>, b: &HashSet<A>) -> HashSet<A> {
    let mut result = HashSet::new();
    for item in a {
        if !b.contains(item) {
            result.insert(item.clone());
        }
    }
    result
}

/// 检查两个集合是否有交集。
pub fn intersects<A: Eq + Hash>(a: &HashSet<A>, b: &HashSet<A>) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    for item in a {
        if b.contains(item) {
            return true;
        }
    }
    false
}

/// 检查 a 的所有元素是否都在 b 中（a ⊆ b）。
pub fn every<A: Eq + Hash>(a: &HashSet<A>, b: &HashSet<A>) -> bool {
    for item in a {
        if !b.contains(item) {
            return false;
        }
    }
    true
}

/// 计算集合并集。
pub fn union<A: Eq + Hash + Clone>(a: &HashSet<A>, b: &HashSet<A>) -> HashSet<A> {
    let mut result = HashSet::new();
    for item in a {
        result.insert(item.clone());
    }
    for item in b {
        result.insert(item.clone());
    }
    result
}
