//! # object_group_by — 对象分组工具
//!
//! 对应 TypeScript `utils/objectGroupBy.ts`。
//! TC39 proposal Object.groupBy 的 polyfill。

use std::collections::HashMap;

/// 按键对项目进行分组。
///
/// 类似于 Array.prototype.groupBy，但适用于任何可迭代对象。
pub fn object_group_by<T, K: std::hash::Hash + Eq>(
    items: Vec<T>,
    key_selector: impl Fn(&T, usize) -> K,
) -> HashMap<K, Vec<T>> {
    let mut result: HashMap<K, Vec<T>> = HashMap::new();
    for (index, item) in items.into_iter().enumerate() {
        let key = key_selector(&item, index);
        result.entry(key).or_default().push(item);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_group_by() {
        let items = vec![1, 2, 3, 4, 5, 6];
        let result = object_group_by(items, |&x, _| x % 2);

        assert_eq!(result.get(&0).unwrap(), &vec![2, 4, 6]);
        assert_eq!(result.get(&1).unwrap(), &vec![1, 3, 5]);
    }

    #[test]
    fn test_object_group_by_empty() {
        let items: Vec<i32> = vec![];
        let result = object_group_by(items, |&x, _| x);
        assert!(result.is_empty());
    }
}
