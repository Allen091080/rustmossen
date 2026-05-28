//! MCP 名称规范化
//!
//! 提供 MCP 服务器名称和工具名称的规范化工具，
//! 确保名称符合 API 格式要求 `^[a-zA-Z0-9_-]{1,64}$`。

/// 托管服务器名称前缀
const HOSTED_SERVER_PREFIX: &str = "hosted ";

/// 将名称规范化为 MCP 兼容格式
///
/// 将非法字符替换为下划线，对于 hosted 前缀的名称额外进行清理。
///
/// # 示例
/// ```
/// use mossen_mcp::normalization::normalize_name_for_mcp;
/// assert_eq!(normalize_name_for_mcp("my-server"), "my-server");
/// assert_eq!(normalize_name_for_mcp("my.server name"), "my_server_name");
/// ```
pub fn normalize_name_for_mcp(name: &str) -> String {
    let mut normalized: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();

    // 对 hosted 前缀的名称，折叠连续下划线并去除首尾下划线
    if name.starts_with(HOSTED_SERVER_PREFIX) {
        // 折叠连续下划线
        let mut prev_underscore = false;
        let collapsed: String = normalized
            .chars()
            .filter(|&c| {
                if c == '_' {
                    if prev_underscore {
                        return false;
                    }
                    prev_underscore = true;
                } else {
                    prev_underscore = false;
                }
                true
            })
            .collect();
        // 去除首尾下划线
        normalized = collapsed.trim_matches('_').to_string();
    }

    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_normalization() {
        assert_eq!(normalize_name_for_mcp("my-server"), "my-server");
        assert_eq!(normalize_name_for_mcp("my_server"), "my_server");
        assert_eq!(normalize_name_for_mcp("MyServer123"), "MyServer123");
    }

    #[test]
    fn test_replace_invalid_chars() {
        assert_eq!(normalize_name_for_mcp("my.server"), "my_server");
        assert_eq!(normalize_name_for_mcp("my server"), "my_server");
        assert_eq!(normalize_name_for_mcp("server@v2!"), "server_v2_");
    }

    #[test]
    fn test_hosted_prefix_normalization() {
        assert_eq!(
            normalize_name_for_mcp("hosted My Server"),
            "hosted_My_Server"
        );
        assert_eq!(
            normalize_name_for_mcp("hosted  Extra  Spaces"),
            "hosted_Extra_Spaces"
        );
    }
}
