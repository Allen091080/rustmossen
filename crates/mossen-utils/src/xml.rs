//! XML 转义工具
//!
//! 对应 TS `xml.ts`。

/// 对元素文本内容进行 XML/HTML 转义。
///
/// 用于将不可信字符串（进程 stdout、用户输入、外部数据）
/// 安全地插入到 `<tag>${here}</tag>` 中。
pub fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// 对属性值进行 XML 转义。
///
/// 用于将不可信字符串插入到 `<tag attr="${here}">` 中。
/// 除了 `& < >` 还会转义引号。
pub fn escape_xml_attr(s: &str) -> String {
    escape_xml(s)
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("<div>"), "&lt;div&gt;");
        assert_eq!(escape_xml("a & b"), "a &amp; b");
    }

    #[test]
    fn test_escape_xml_attr() {
        assert_eq!(escape_xml_attr("it's \"cool\""), "it&apos;s &quot;cool&quot;");
    }
}
