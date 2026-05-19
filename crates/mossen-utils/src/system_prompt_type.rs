//! # system_prompt_type — System Prompt 类型标记
//!
//! 对应 TypeScript `utils/systemPromptType.ts`。
//! System Prompt 数组的品牌类型标记。

use std::marker::PhantomData;

/// System Prompt 品牌类型标记。
#[derive(Debug, Clone)]
pub struct SystemPrompt {
    inner: Vec<String>,
    _brand: PhantomData<fn() -> &'static str>,
}

impl SystemPrompt {
    /// 从 Vec<String> 创建 SystemPrompt。
    pub fn new(value: Vec<String>) -> Self {
        SystemPrompt {
            inner: value,
            _brand: PhantomData,
        }
    }

    /// 获取内部的字符串数组引用。
    pub fn as_ref(&self) -> &[String] {
        &self.inner
    }
}

impl AsRef<[String]> for SystemPrompt {
    fn as_ref(&self) -> &[String] {
        &self.inner
    }
}

/// 将值转换为 SystemPrompt 类型标记。
pub fn as_system_prompt(value: Vec<String>) -> SystemPrompt {
    SystemPrompt::new(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_new() {
        let prompts = vec!["You are helpful.".to_string(), "Be concise.".to_string()];
        let sp = SystemPrompt::new(prompts);
        assert_eq!(sp.as_ref().len(), 2);
    }

    #[test]
    fn test_as_system_prompt() {
        let prompts = vec!["You are helpful.".to_string()];
        let sp = as_system_prompt(prompts);
        assert_eq!(sp.as_ref().len(), 1);
    }
}
