//! Parser for sed edit commands (-i flag substitutions).
//! Extracts file paths and substitution patterns to enable file-edit-style rendering.

/// Information about a sed in-place edit command.
#[derive(Debug, Clone, PartialEq)]
pub struct SedEditInfo {
    /// The file path being edited.
    pub file_path: String,
    /// The search pattern (regex).
    pub pattern: String,
    /// The replacement string.
    pub replacement: String,
    /// Substitution flags (g, i, etc.).
    pub flags: String,
    /// Whether to use extended regex (-E or -r flag).
    pub extended_regex: bool,
}

/// Check if a command is a sed in-place edit command.
/// Returns true only for simple `sed -i 's/pattern/replacement/flags' file` commands.
pub fn is_sed_in_place_edit(command: &str) -> bool {
    parse_sed_edit_command(command).is_some()
}

/// Parse a sed edit command and extract the edit information.
/// Returns None if the command is not a valid sed in-place edit.
pub fn parse_sed_edit_command(command: &str) -> Option<SedEditInfo> {
    let trimmed = command.trim();

    // Must start with sed
    if !trimmed.starts_with("sed") {
        return None;
    }
    let after_sed = trimmed.strip_prefix("sed")?;
    if !after_sed.starts_with(char::is_whitespace) {
        return None;
    }
    let without_sed = after_sed.trim_start();

    // Tokenize arguments (simplified shell parsing)
    let tokens = tokenize_shell_args(without_sed)?;

    // Parse flags and arguments
    let mut has_in_place_flag = false;
    let mut extended_regex = false;
    let mut expression: Option<String> = None;
    let mut file_path: Option<String> = None;

    let mut i = 0;
    while i < tokens.len() {
        let arg = &tokens[i];

        // Handle -i flag (with or without backup suffix)
        if arg == "-i" || arg == "--in-place" {
            has_in_place_flag = true;
            i += 1;
            // On macOS, -i requires a suffix argument (even if empty string)
            if i < tokens.len() {
                let next_arg = &tokens[i];
                if !next_arg.starts_with('-')
                    && (next_arg.is_empty() || next_arg.starts_with('.'))
                {
                    i += 1; // Skip the backup suffix
                }
            }
            continue;
        }
        if arg.starts_with("-i") {
            // -i.bak or similar (inline suffix)
            has_in_place_flag = true;
            i += 1;
            continue;
        }

        // Handle extended regex flags
        if arg == "-E" || arg == "-r" || arg == "--regexp-extended" {
            extended_regex = true;
            i += 1;
            continue;
        }

        // Handle -e flag with expression
        if arg == "-e" || arg == "--expression" {
            if i + 1 < tokens.len() {
                if expression.is_some() {
                    return None; // Only support single expression
                }
                expression = Some(tokens[i + 1].clone());
                i += 2;
                continue;
            }
            return None;
        }
        if let Some(expr_val) = arg.strip_prefix("--expression=") {
            if expression.is_some() {
                return None;
            }
            expression = Some(expr_val.to_string());
            i += 1;
            continue;
        }

        // Skip other flags we don't understand
        if arg.starts_with('-') {
            return None;
        }

        // Non-flag argument
        if expression.is_none() {
            expression = Some(arg.clone());
        } else if file_path.is_none() {
            file_path = Some(arg.clone());
        } else {
            // More than one file — not supported for simple rendering
            return None;
        }

        i += 1;
    }

    // Must have -i flag, expression, and file path
    let expression = expression?;
    let file_path = file_path?;
    if !has_in_place_flag {
        return None;
    }

    // Parse the substitution expression: s/pattern/replacement/flags
    if !expression.starts_with("s/") {
        return None;
    }
    let rest = &expression[2..]; // Skip 's/'

    // Find pattern and replacement by tracking escaped characters
    let mut pattern = String::new();
    let mut replacement = String::new();
    let mut flags = String::new();
    let mut state = 0u8; // 0=pattern, 1=replacement, 2=flags
    let chars: Vec<char> = rest.chars().collect();
    let mut j = 0;

    while j < chars.len() {
        let ch = chars[j];

        if ch == '\\' && j + 1 < chars.len() {
            // Escaped character
            let escaped_pair: String = [ch, chars[j + 1]].iter().collect();
            match state {
                0 => pattern.push_str(&escaped_pair),
                1 => replacement.push_str(&escaped_pair),
                _ => flags.push_str(&escaped_pair),
            }
            j += 2;
            continue;
        }

        if ch == '/' {
            match state {
                0 => state = 1,
                1 => state = 2,
                _ => return None, // Extra delimiter in flags
            }
            j += 1;
            continue;
        }

        match state {
            0 => pattern.push(ch),
            1 => replacement.push(ch),
            _ => flags.push(ch),
        }
        j += 1;
    }

    // Must have found all three parts
    if state != 2 {
        return None;
    }

    // Validate flags — only allow safe substitution flags
    let valid_flags = regex::Regex::new(r"^[gpimIM1-9]*$").unwrap();
    if !valid_flags.is_match(&flags) {
        return None;
    }

    Some(SedEditInfo {
        file_path,
        pattern,
        replacement,
        flags,
        extended_regex,
    })
}

/// Apply a sed substitution to file content.
/// Returns the new content after applying the substitution.
pub fn apply_sed_substitution(content: &str, sed_info: &SedEditInfo) -> String {
    // Build regex flags
    let mut regex_pattern = String::new();
    let mut case_insensitive = false;
    let mut multiline = false;
    let global = sed_info.flags.contains('g');

    if sed_info.flags.contains('i') || sed_info.flags.contains('I') {
        case_insensitive = true;
    }
    if sed_info.flags.contains('m') || sed_info.flags.contains('M') {
        multiline = true;
    }

    // Convert sed pattern to Rust regex pattern
    let mut js_pattern = sed_info.pattern.replace("\\/", "/");

    // In BRE mode (no -E flag), convert escaping
    if !sed_info.extended_regex {
        js_pattern = convert_bre_to_ere(&js_pattern);
    }

    // Build regex flags string
    let mut flags_str = String::from("(?");
    if case_insensitive {
        flags_str.push('i');
    }
    if multiline {
        flags_str.push('m');
    }
    flags_str.push(')');

    let full_pattern = if flags_str == "(?)" {
        js_pattern
    } else {
        format!("{}{}", flags_str, js_pattern)
    };

    // Build replacement string
    let js_replacement = sed_info
        .replacement
        .replace("\\/", "/")
        .replace("\\&", "\x00ESCAPED_AMP\x00")
        .replace('&', "${0}")
        .replace("\x00ESCAPED_AMP\x00", "&");

    match regex::Regex::new(&full_pattern) {
        Ok(re) => {
            if global {
                re.replace_all(content, js_replacement.as_str()).to_string()
            } else {
                re.replace(content, js_replacement.as_str()).to_string()
            }
        }
        Err(_) => content.to_string(),
    }
}

/// Convert BRE (Basic Regular Expression) escaping to ERE for Rust regex.
fn convert_bre_to_ere(pattern: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            match next {
                '+' | '?' | '|' | '(' | ')' => {
                    // In BRE, \+ means "one or more" — in ERE just +
                    result.push(next);
                    i += 2;
                }
                '\\' => {
                    result.push_str("\\\\");
                    i += 2;
                }
                _ => {
                    result.push('\\');
                    result.push(next);
                    i += 2;
                }
            }
        } else {
            let c = chars[i];
            match c {
                '+' | '?' | '|' | '(' | ')' => {
                    // In BRE, bare + is literal — escape it for ERE
                    result.push('\\');
                    result.push(c);
                }
                _ => result.push(c),
            }
            i += 1;
        }
    }
    result
}

/// Simple shell argument tokenizer.
/// Handles single quotes, double quotes, and backslash escaping.
fn tokenize_shell_args(input: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escape_next = false;
    let mut has_content = false;

    for ch in input.chars() {
        if escape_next {
            current.push(ch);
            escape_next = false;
            has_content = true;
            continue;
        }

        if ch == '\\' && !in_single_quote {
            escape_next = true;
            continue;
        }

        if ch == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            has_content = true;
            continue;
        }

        if ch == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            has_content = true;
            continue;
        }

        if ch.is_whitespace() && !in_single_quote && !in_double_quote {
            if has_content || !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
                has_content = false;
            }
            continue;
        }

        current.push(ch);
        has_content = true;
    }

    // Unmatched quotes means parse failure
    if in_single_quote || in_double_quote {
        return None;
    }

    if has_content || !current.is_empty() {
        tokens.push(current);
    }

    Some(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_sed() {
        let info = parse_sed_edit_command("sed -i 's/foo/bar/g' file.txt").unwrap();
        assert_eq!(info.file_path, "file.txt");
        assert_eq!(info.pattern, "foo");
        assert_eq!(info.replacement, "bar");
        assert_eq!(info.flags, "g");
        assert!(!info.extended_regex);
    }

    #[test]
    fn test_parse_extended_regex() {
        let info = parse_sed_edit_command("sed -E -i '' 's/foo+/bar/' file.txt").unwrap();
        assert_eq!(info.pattern, "foo+");
        assert!(info.extended_regex);
    }

    #[test]
    fn test_not_in_place() {
        assert!(parse_sed_edit_command("sed 's/foo/bar/g' file.txt").is_none());
    }

    #[test]
    fn test_no_file() {
        assert!(parse_sed_edit_command("sed -i 's/foo/bar/g'").is_none());
    }

    #[test]
    fn test_apply_substitution_global() {
        let info = SedEditInfo {
            file_path: "test.txt".to_string(),
            pattern: "hello".to_string(),
            replacement: "world".to_string(),
            flags: "g".to_string(),
            extended_regex: true,
        };
        let result = apply_sed_substitution("hello hello hello", &info);
        assert_eq!(result, "world world world");
    }

    #[test]
    fn test_apply_substitution_first_only() {
        let info = SedEditInfo {
            file_path: "test.txt".to_string(),
            pattern: "hello".to_string(),
            replacement: "world".to_string(),
            flags: String::new(),
            extended_regex: true,
        };
        let result = apply_sed_substitution("hello hello hello", &info);
        assert_eq!(result, "world hello hello");
    }
}
