//! # lazy_schema — 延迟 Schema 工厂
//!
//! 对应 TypeScript `utils/lazySchema.ts`。
//! 返回一个记忆化工厂函数，在首次调用时构造值。

use std::cell::RefCell;
use std::rc::Rc;

/// 返回一个记忆化工厂函数，在首次调用时构造值。
pub fn lazy_schema<T: Clone + 'static, F>(factory: F) -> Rc<dyn Fn() -> T>
where
    F: Fn() -> T + 'static,
{
    let cached = Rc::new(RefCell::new(None::<T>));
    Rc::new(move || {
        if cached.borrow().is_none() {
            *cached.borrow_mut() = Some(factory());
        }
        // Safety: we've just ensured Some value exists
        cached.borrow().as_ref().unwrap().clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lazy_schema_calls_factory_once() {
        let factory = || 42;
        let lazy_fn = lazy_schema(factory);

        assert_eq!(lazy_fn(), 42);
        assert_eq!(lazy_fn(), 42);
    }

    #[test]
    fn test_lazy_schema_with_string() {
        let factory = || "hello".to_string();
        let lazy_fn = lazy_schema(factory);

        assert_eq!(lazy_fn(), "hello");
        assert_eq!(lazy_fn(), "hello");
    }
}
