use std::path::Path;
use std::sync::OnceLock;

use tokio::sync::OnceCell;

/// Represents the CLI highlight functionality.
pub struct CliHighlight {
    languages: Vec<LanguageEntry>,
}

#[derive(Clone)]
struct LanguageEntry {
    name: String,
    extensions: Vec<String>,
}

impl CliHighlight {
    /// Highlight source code text for the given language.
    pub fn highlight(&self, code: &str, language: &str) -> String {
        // In Rust we use syntect or tree-sitter for highlighting.
        // This is a placeholder that returns the code as-is since
        // cli-highlight is a Node.js specific package.
        let _ = language;
        code.to_string()
    }

    /// Check if a language is supported by extension name.
    pub fn supports_language(&self, lang: &str) -> bool {
        self.languages
            .iter()
            .any(|entry| entry.name.eq_ignore_ascii_case(lang))
    }

    /// Get language name by extension.
    pub fn get_language_name(&self, ext: &str) -> Option<&str> {
        self.languages
            .iter()
            .find(|entry| entry.extensions.iter().any(|e| e == ext))
            .map(|entry| entry.name.as_str())
    }
}

/// Default known language extensions mapping.
fn build_default_languages() -> Vec<LanguageEntry> {
    let entries: &[(&str, &[&str])] = &[
        ("TypeScript", &["ts", "tsx"]),
        ("JavaScript", &["js", "jsx", "mjs", "cjs"]),
        ("Python", &["py", "pyw"]),
        ("Rust", &["rs"]),
        ("Go", &["go"]),
        ("Java", &["java"]),
        ("C", &["c", "h"]),
        ("C++", &["cpp", "cxx", "cc", "hpp", "hxx"]),
        ("C#", &["cs"]),
        ("Ruby", &["rb"]),
        ("PHP", &["php"]),
        ("Swift", &["swift"]),
        ("Kotlin", &["kt", "kts"]),
        ("Scala", &["scala"]),
        ("HTML", &["html", "htm"]),
        ("CSS", &["css"]),
        ("SCSS", &["scss"]),
        ("JSON", &["json"]),
        ("YAML", &["yaml", "yml"]),
        ("TOML", &["toml"]),
        ("Markdown", &["md", "markdown"]),
        ("Shell", &["sh", "bash", "zsh"]),
        ("SQL", &["sql"]),
        ("XML", &["xml"]),
        ("Dart", &["dart"]),
        ("Lua", &["lua"]),
        ("R", &["r", "R"]),
        ("Perl", &["pl", "pm"]),
        ("Haskell", &["hs"]),
        ("Elixir", &["ex", "exs"]),
        ("Clojure", &["clj", "cljs"]),
        ("Zig", &["zig"]),
        ("Dockerfile", &["Dockerfile"]),
    ];
    entries
        .iter()
        .map(|(name, exts)| LanguageEntry {
            name: name.to_string(),
            extensions: exts.iter().map(|e| e.to_string()).collect(),
        })
        .collect()
}

static CLI_HIGHLIGHT: OnceLock<CliHighlight> = OnceLock::new();

/// Get the shared CLI highlight instance (lazy-initialized).
pub fn get_cli_highlight() -> &'static CliHighlight {
    CLI_HIGHLIGHT.get_or_init(|| CliHighlight {
        languages: build_default_languages(),
    })
}

/// One shared future for async initialization (mirrors the TS Promise pattern).
static CLI_HIGHLIGHT_CELL: OnceCell<Option<CliHighlight>> = OnceCell::const_new();

/// Get the CLI highlight promise equivalent - async lazy load.
pub async fn get_cli_highlight_promise() -> Option<&'static CliHighlight> {
    let result = CLI_HIGHLIGHT_CELL
        .get_or_init(|| async {
            Some(CliHighlight {
                languages: build_default_languages(),
            })
        })
        .await;
    result.as_ref()
}

/// Get the language name for a file path based on its extension.
/// e.g. "foo/bar.ts" → "TypeScript"
pub async fn get_language_name(file_path: &str) -> String {
    let highlight = get_cli_highlight_promise().await;
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    if ext.is_empty() {
        return "unknown".to_string();
    }
    match highlight {
        Some(h) => h
            .get_language_name(ext)
            .unwrap_or("unknown")
            .to_string(),
        None => "unknown".to_string(),
    }
}
