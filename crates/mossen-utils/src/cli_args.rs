//! # cli_args — CLI 参数解析工具
//!
//! 对应 TypeScript `utils/cliArgs.ts`。
//! 在 Commander.js 处理参数之前早期解析 CLI 标志。

/// 早期解析 CLI 标志值。
/// 支持空格分隔 (--flag value) 和等号分隔 (--flag=value) 语法。
pub fn eager_parse_cli_flag(flag_name: &str, argv: Option<Vec<String>>) -> Option<String> {
    let args = argv.unwrap_or_else(|| std::env::args().collect());
    for i in 0..args.len() {
        let arg = args.get(i)?;
        // 处理 --flag=value 语法
        if arg.starts_with(&format!("{}=", flag_name)) {
            return Some(arg[flag_name.len() + 1..].to_string());
        }
        // 处理 --flag value 语法
        if arg == flag_name && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
    }
    None
}

/// Commander-style `--` 分隔符处理。
///
/// 当解析得到的 positional 为 `--` 时，将其后的参数提升为命令；否则透传。
/// 对应 TS `extractArgsAfterDoubleDash`。
pub fn extract_args_after_double_dash(
    command_or_value: &str,
    args: &[String],
) -> (String, Vec<String>) {
    if command_or_value == "--" && !args.is_empty() {
        return (args[0].clone(), args[1..].to_vec());
    }
    (command_or_value.to_string(), args.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eager_parse_flag_with_equals() {
        let argv = vec![
            "prog".to_string(),
            "--settings=/path/to/settings.json".to_string(),
        ];
        let result = eager_parse_cli_flag("--settings", Some(argv));
        assert_eq!(result, Some("/path/to/settings.json".to_string()));
    }

    #[test]
    fn test_eager_parse_flag_with_space() {
        let argv = vec![
            "prog".to_string(),
            "--settings".to_string(),
            "/path/to/settings.json".to_string(),
        ];
        let result = eager_parse_cli_flag("--settings", Some(argv));
        assert_eq!(result, Some("/path/to/settings.json".to_string()));
    }
}
