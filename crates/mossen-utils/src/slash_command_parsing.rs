//! # slash_command_parsing — 斜杠命令解析工具
//!
//! 对应 TypeScript `utils/slashCommandParsing.ts`。
//! 集中式斜杠命令解析工具。

/// 解析后的斜杠命令。
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSlashCommand {
    pub command_name: String,
    pub args: String,
    pub is_mcp: bool,
}

/// 将斜杠命令输入字符串解析为组成部分。
///
/// # 参数
/// - `input`: 原始输入字符串（应以 '/' 开头）
///
/// # 返回
/// 解析的命令名、参数和 MCP 标志，无效时返回 None
///
/// # 示例
/// ```
/// use mossen_utils::slash_command_parsing::parse_slash_command;
///
/// let result = parse_slash_command("/search foo bar");
/// assert_eq!(result.unwrap().command_name, "search");
/// ```
pub fn parse_slash_command(input: &str) -> Option<ParsedSlashCommand> {
    let trimmed_input = input.trim();

    // 检查输入是否以 '/' 开头
    if !trimmed_input.starts_with('/') {
        return None;
    }

    // 移除前导 '/' 并按空格分割
    let without_slash = &trimmed_input[1..];
    let words: Vec<&str> = without_slash.split(' ').collect();

    if words.is_empty() || words[0].is_empty() {
        return None;
    }

    let mut command_name = words[0].to_string();
    let mut is_mcp = false;
    let mut args_start_index = 1;

    // 检查 MCP 命令（第二个词是 '(MCP)'）
    if words.len() > 1 && words[1] == "(MCP)" {
        command_name = format!("{} (MCP)", command_name);
        is_mcp = true;
        args_start_index = 2;
    }

    // 提取参数（命令名后的所有内容）
    let args = words[args_start_index..].join(" ");

    Some(ParsedSlashCommand {
        command_name,
        args,
        is_mcp,
    })
}
