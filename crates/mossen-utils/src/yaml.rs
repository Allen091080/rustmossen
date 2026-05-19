//! YAML 解析包装
//!
//! 对应 TS `yaml.ts`。

/// 解析 YAML 字符串。
///
/// 在 Rust 中使用 serde_yaml 进行解析。
pub fn parse_yaml(input: &str) -> Result<serde_yaml::Value, serde_yaml::Error> {
    serde_yaml::from_str(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_yaml() {
        let yaml = "key: value\nlist:\n  - item1\n  - item2";
        let result = parse_yaml(yaml);
        assert!(result.is_ok());
    }
}
