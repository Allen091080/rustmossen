//! 版本工具函数

use regex::Regex;

/// 获取显示用的应用版本字符串。
///
/// 如果版本符合 "X.Y.0" 模式（稳定版本），返回 "X.Y"。
/// 否则返回完整版本号。
pub fn get_display_app_version(version: &str) -> String {
    // 稳定版本模式：X.Y.0
    let re = Regex::new(r"^(\d+)\.(\d+)\.0$").unwrap();
    if let Some(caps) = re.captures(version) {
        let major = caps.get(1).map(|m| m.as_str()).unwrap_or("0");
        let minor = caps.get(2).map(|m| m.as_str()).unwrap_or("0");
        format!("{}.{}", major, minor)
    } else {
        version.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stable_version() {
        assert_eq!(get_display_app_version("1.2.0"), "1.2");
        assert_eq!(get_display_app_version("10.25.0"), "10.25");
    }

    #[test]
    fn test_prerelease_version() {
        assert_eq!(get_display_app_version("1.2.3"), "1.2.3");
        assert_eq!(get_display_app_version("1.2.0-beta.1"), "1.2.0-beta.1");
    }
}
