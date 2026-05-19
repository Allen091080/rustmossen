//! # cli_highlight_utils — CLI 语法高亮
//!
//! 对应 TypeScript `utils/cliHighlight.ts`。

use std::path::Path;
use std::sync::Mutex;
use std::collections::HashMap;

static LANGUAGE_CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// 根据文件路径获取编程语言名称。
///
/// 基于文件扩展名推断语言。
pub async fn get_language_name(file_path: &str) -> String {
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    if ext.is_empty() {
        return "unknown".to_string();
    }

    extension_to_language_name(ext).unwrap_or_else(|| "unknown".to_string())
}

/// 将文件扩展名映射为语言名。
fn extension_to_language_name(ext: &str) -> Option<String> {
    let name = match ext.to_lowercase().as_str() {
        "rs" => "Rust",
        "ts" | "tsx" => "TypeScript",
        "js" | "jsx" | "mjs" | "cjs" => "JavaScript",
        "py" => "Python",
        "go" => "Go",
        "java" => "Java",
        "c" | "h" => "C",
        "cpp" | "cc" | "cxx" | "hpp" => "C++",
        "cs" => "C#",
        "rb" => "Ruby",
        "php" => "PHP",
        "swift" => "Swift",
        "kt" | "kts" => "Kotlin",
        "scala" => "Scala",
        "html" | "htm" => "HTML",
        "css" | "scss" | "sass" | "less" => "CSS",
        "json" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "xml" => "XML",
        "md" | "markdown" => "Markdown",
        "sh" | "bash" | "zsh" => "Shell",
        "sql" => "SQL",
        "r" => "R",
        "dart" => "Dart",
        "lua" => "Lua",
        "zig" => "Zig",
        "ex" | "exs" => "Elixir",
        "erl" | "hrl" => "Erlang",
        "hs" => "Haskell",
        "ml" | "mli" => "OCaml",
        "vim" => "Vim Script",
        "el" => "Emacs Lisp",
        _ => return None,
    };
    Some(name.to_string())
}
