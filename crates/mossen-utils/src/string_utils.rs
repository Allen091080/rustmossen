//! 字符串工具函数
//!
//! 对应 TS `stringUtils.ts`。

/// 转义字符串中的特殊正则表达式字符。
pub fn escape_regexp(s: &str) -> String {
    lazy_static::lazy_static! {
        static ref REGEX: regex::Regex = regex::Regex::new(r"[.*+?^${}()|[\]\\]").unwrap();
    }
    REGEX.replace_all(s, r"\$&").to_string()
}

/// 将字符串的第一个字符大写，其余不变。
///
/// # 示例
/// `capitalize("fooBar")` → "FooBar"
/// `capitalize("hello world")` → "Hello world"
pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

/// 根据计数返回单词的单数或复数形式。
///
/// # 示例
/// `plural(1, "file")` → "file"
/// `plural(3, "file")` → "files"
/// `plural(2, "entry", "entries")` → "entries"
pub fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        word.to_string()
    } else {
        format!("{}s", word)
    }
}

/// 返回字符串的第一行，不分配分割数组。
pub fn first_line_of(s: &str) -> &str {
    match s.find('\n') {
        None => s,
        Some(pos) => &s[..pos],
    }
}

/// 统计字符在字符串中出现的次数。
pub fn count_char_in_string(s: &str, ch: char) -> usize {
    s.matches(ch).count()
}

/// 将全角数字规范化为半角数字。
pub fn normalize_full_width_digits(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for c in input.chars() {
        if ('０'..='９').contains(&c) {
            // 全角数字 (0xFF10-0xFF19) 转换为半角 (0x0030-0x0039)
            let offset = c as u32 - 0xFF10;
            result.push(char::from_u32(0x0030 + offset).unwrap_or(c));
        } else {
            result.push(c);
        }
    }
    result
}

/// 将全角空格规范化为半角空格。
pub fn normalize_full_width_space(input: &str) -> String {
    input.replace('\u{3000}', " ")
}

// 保持内存累积适度以避免 RSS 膨胀
// 超过此限制由 ShellCommand 溢出到磁盘
const MAX_STRING_LENGTH: usize = 2usize.pow(25);

/// 安全地连接字符串数组，如果结果超过 max_size 则截断。
pub fn safe_join_lines(
    lines: &[String],
    delimiter: &str,
    max_size: usize,
) -> String {
    let truncation_marker = "...[truncated]";
    let mut result = String::new();

    for line in lines {
        let delimiter_to_add = if result.is_empty() { "" } else { delimiter };
        let full_addition = format!("{}{}", delimiter_to_add, line);

        if result.len() + full_addition.len() <= max_size {
            result.push_str(&full_addition);
        } else {
            let remaining_space = max_size
                .saturating_sub(result.len())
                .saturating_sub(delimiter_to_add.len())
                .saturating_sub(truncation_marker.len());

            if remaining_space > 0 {
                result.push_str(delimiter_to_add);
                result.push_str(&line[..remaining_space]);
                result.push_str(truncation_marker);
            } else {
                result.push_str(truncation_marker);
            }
            return result;
        }
    }
    result
}

/// 从末尾截断的字符串累加器。
///
/// 当大小超过限制时从末尾截断，防止 RangeError 崩溃。
pub struct EndTruncatingAccumulator {
    content: String,
    is_truncated: bool,
    total_bytes_received: usize,
    max_size: usize,
}

impl EndTruncatingAccumulator {
    /// 创建新的累加器。
    pub fn new(max_size: usize) -> Self {
        EndTruncatingAccumulator {
            content: String::new(),
            is_truncated: false,
            total_bytes_received: 0,
            max_size,
        }
    }

    /// 追加数据到累加器。
    pub fn append(&mut self, data: &str) {
        self.total_bytes_received += data.len();

        if self.is_truncated && self.content.len() >= self.max_size {
            return;
        }

        if self.content.len() + data.len() > self.max_size {
            let remaining_space = self.max_size.saturating_sub(self.content.len());
            if remaining_space > 0 {
                self.content.push_str(&data[..remaining_space]);
            }
            self.is_truncated = true;
        } else {
            self.content.push_str(data);
        }
    }

    /// 返回累积的字符串，截断时带截断标记。
    pub fn to_string(&self) -> String {
        if !self.is_truncated {
            return self.content.clone();
        }

        let truncated_bytes = self.total_bytes_received.saturating_sub(self.max_size);
        let truncated_kb = truncated_bytes / 1024;
        format!(
            "{}\n... [output truncated - {}KB removed]",
            self.content, truncated_kb
        )
    }

    /// 清空所有累积的数据。
    pub fn clear(&mut self) {
        self.content.clear();
        self.is_truncated = false;
        self.total_bytes_received = 0;
    }

    /// 获取当前累积数据的大小。
    pub fn len(&self) -> usize {
        self.content.len()
    }

    /// 检查是否已截断。
    pub fn is_truncated(&self) -> bool {
        self.is_truncated
    }

    /// 获取接收的总字节数（截断前）。
    pub fn total_bytes(&self) -> usize {
        self.total_bytes_received
    }
}

impl Default for EndTruncatingAccumulator {
    fn default() -> Self {
        Self::new(MAX_STRING_LENGTH)
    }
}

/// 将文本截断到最大行数，超出时添加省略号。
pub fn truncate_to_lines(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max_lines {
        return text.to_string();
    }
    format!("{}\n…", lines[..max_lines].join("\n"))
}
