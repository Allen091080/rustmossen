//! # array — 数组工具函数
//!
//! 对应 TypeScript `utils/array.ts`。
//! 数组操作辅助函数。

/// 在数组元素之间插入分隔符。
pub fn intersperse<T>(items: Vec<T>, separator: impl Fn(usize) -> T) -> Vec<T> {
    let mut result = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            result.push(separator(i));
        }
        result.push(item);
    }
    result
}

/// 计算满足条件的元素数量。
pub fn count<T>(arr: &[T], pred: impl Fn(&T) -> bool) -> usize {
    arr.iter().filter(|x| pred(x)).count()
}

/// 返回数组的唯一元素。
pub fn uniq<T: Eq + std::hash::Hash>(items: impl IntoIterator<Item = T>) -> Vec<T> {
    let set: std::collections::HashSet<T> = items.into_iter().collect();
    set.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intersperse() {
        let result = intersperse(vec![1, 2, 3], |i| i * 10);
        assert_eq!(result, vec![1, 10, 2, 20, 3]);
    }

    #[test]
    fn test_count() {
        let arr = vec![1, 2, 3, 4, 5];
        assert_eq!(count(&arr, |x| x % 2 == 0), 2);
    }

    #[test]
    fn test_uniq() {
        let result = uniq(vec![1, 2, 1, 3, 2]);
        assert_eq!(result, vec![1, 2, 3]);
    }
}
