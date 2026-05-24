use std::collections::HashSet;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

/// Exact file name matches (case-insensitive)
static EXCLUDED_FILENAMES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("package-lock.json");
    s.insert("yarn.lock");
    s.insert("pnpm-lock.yaml");
    s.insert("bun.lockb");
    s.insert("bun.lock");
    s.insert("composer.lock");
    s.insert("gemfile.lock");
    s.insert("cargo.lock");
    s.insert("poetry.lock");
    s.insert("pipfile.lock");
    s.insert("shrinkwrap.json");
    s.insert("npm-shrinkwrap.json");
    s
});

/// File extension patterns (case-insensitive)
static EXCLUDED_EXTENSIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert(".lock");
    s.insert(".min.js");
    s.insert(".min.css");
    s.insert(".min.html");
    s.insert(".bundle.js");
    s.insert(".bundle.css");
    s.insert(".generated.ts");
    s.insert(".generated.js");
    s.insert(".d.ts");
    s
});

/// Directory patterns that indicate generated/vendored content
const EXCLUDED_DIRECTORIES: &[&str] = &[
    "/dist/",
    "/build/",
    "/out/",
    "/output/",
    "/node_modules/",
    "/vendor/",
    "/vendored/",
    "/third_party/",
    "/third-party/",
    "/external/",
    "/.next/",
    "/.nuxt/",
    "/.svelte-kit/",
    "/coverage/",
    "/__pycache__/",
    "/.tox/",
    "/venv/",
    "/.venv/",
    "/target/release/",
    "/target/debug/",
];

/// Filename patterns using regex for more complex matching
static EXCLUDED_FILENAME_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"(?i)^.*\.min\.[a-z]+$").unwrap(),
        Regex::new(r"(?i)^.*-min\.[a-z]+$").unwrap(),
        Regex::new(r"(?i)^.*\.bundle\.[a-z]+$").unwrap(),
        Regex::new(r"(?i)^.*\.generated\.[a-z]+$").unwrap(),
        Regex::new(r"(?i)^.*\.gen\.[a-z]+$").unwrap(),
        Regex::new(r"(?i)^.*\.auto\.[a-z]+$").unwrap(),
        Regex::new(r"(?i)^.*_generated\.[a-z]+$").unwrap(),
        Regex::new(r"(?i)^.*_gen\.[a-z]+$").unwrap(),
        Regex::new(r"(?i)^.*\.pb\.(go|js|ts|py|rb)$").unwrap(),
        Regex::new(r"(?i)^.*_pb2?\.py$").unwrap(),
        Regex::new(r"(?i)^.*\.pb\.h$").unwrap(),
        Regex::new(r"(?i)^.*\.grpc\.[a-z]+$").unwrap(),
        Regex::new(r"(?i)^.*\.swagger\.[a-z]+$").unwrap(),
        Regex::new(r"(?i)^.*\.openapi\.[a-z]+$").unwrap(),
    ]
});

/// Check if a file should be excluded from attribution based on Linguist-style rules.
///
/// `file_path` - Relative file path from repository root
/// Returns true if the file should be excluded from attribution
pub fn is_generated_file(file_path: &str) -> bool {
    // Normalize path separators for consistent pattern matching
    let normalized_path = format!("/{}", file_path.replace('\\', "/").trim_start_matches('/'));

    let path = Path::new(file_path);
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
        .unwrap_or_default();

    // Check exact filename matches
    if EXCLUDED_FILENAMES.contains(file_name.as_str()) {
        return true;
    }

    // Check extension matches
    if EXCLUDED_EXTENSIONS.contains(ext.as_str()) {
        return true;
    }

    // Check for compound extensions like .min.js
    let parts: Vec<&str> = file_name.split('.').collect();
    if parts.len() > 2 {
        let compound_ext = format!(".{}", parts[parts.len() - 2..].join("."));
        if EXCLUDED_EXTENSIONS.contains(compound_ext.as_str()) {
            return true;
        }
    }

    // Check directory patterns
    for dir in EXCLUDED_DIRECTORIES {
        if normalized_path.contains(dir) {
            return true;
        }
    }

    // Check filename patterns
    for pattern in EXCLUDED_FILENAME_PATTERNS.iter() {
        if pattern.is_match(&file_name) {
            return true;
        }
    }

    false
}

/// Filter a list of files to exclude generated files.
pub fn filter_generated_files(files: &[String]) -> Vec<String> {
    files
        .iter()
        .filter(|file| !is_generated_file(file))
        .cloned()
        .collect()
}
