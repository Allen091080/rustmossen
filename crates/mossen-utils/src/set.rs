//! # set — 集合操作工具
//!
//! 对应 TypeScript `utils/set.ts`。
//! 优化的集合操作函数。

/// 计算两个集合的差集。
/// 注意：此代码是热路径，已针对速度进行优化。
pub fn difference<T: Eq + std::hash::Hash + Clone>(
    a: &std::collections::HashSet<T>,
    b: &std::collections::HashSet<T>,
) -> std::collections::HashSet<T> {
    a.iter().filter(|item| !b.contains(item)).cloned().collect()
}

/// 检查两个集合是否有交集。
/// 注意：此代码是热路径，已针对速度进行优化。
pub fn intersects<T: Eq + std::hash::Hash>(
    a: &std::collections::HashSet<T>,
    b: &std::collections::HashSet<T>,
) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    a.iter().any(|item| b.contains(item))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_difference() {
        let mut a = std::collections::HashSet::new();
        a.insert(1);
        a.insert(2);
        a.insert(3);
        let mut b = std::collections::HashSet::new();
        b.insert(2);
        b.insert(3);
        let result = difference(&a, &b);
        assert!(result.contains(&1));
        assert!(!result.contains(&2));
    }

    #[test]
    fn test_intersects() {
        let mut a = std::collections::HashSet::new();
        a.insert(1);
        a.insert(2);
        let mut b = std::collections::HashSet::new();
        b.insert(3);
        b.insert(4);
        assert!(!intersects(&a, &b));

        b.insert(2);
        assert!(intersects(&a, &b));
    }
}
