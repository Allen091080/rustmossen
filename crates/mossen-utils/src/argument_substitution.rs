use regex::Regex;

/// Parse an arguments string into an array of individual arguments.
/// Uses shell-style argument parsing including quoted strings.
///
/// Examples:
/// - "foo bar baz" => ["foo", "bar", "baz"]
/// - 'foo "hello world" baz' => ["foo", "hello world", "baz"]
/// - "foo 'hello world' baz" => ["foo", "hello world", "baz"]
pub fn parse_arguments(args: &str) -> Vec<String> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // Simple shell-style parsing: split by whitespace but respect quotes
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut chars = trimmed.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            '\\' if in_double_quote => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ' ' | '\t' if !in_single_quote && !in_double_quote => {
                if !current.is_empty() {
                    result.push(current.clone());
                    current.clear();
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if !current.is_empty() {
        result.push(current);
    }

    result
}

/// Parse argument names from the frontmatter 'arguments' field.
/// Accepts either a space-separated string or a vec of strings.
///
/// Examples:
/// - "foo bar baz" => ["foo", "bar", "baz"]
pub fn parse_argument_names(argument_names: Option<&ArgumentNames>) -> Vec<String> {
    let argument_names = match argument_names {
        Some(names) => names,
        None => return Vec::new(),
    };

    let is_valid_name = |name: &str| -> bool {
        let trimmed = name.trim();
        !trimmed.is_empty() && trimmed.parse::<u64>().is_err()
    };

    match argument_names {
        ArgumentNames::Single(s) => s
            .split_whitespace()
            .filter(|name| is_valid_name(name))
            .map(|s| s.to_string())
            .collect(),
        ArgumentNames::List(list) => list
            .iter()
            .filter(|name| is_valid_name(name))
            .cloned()
            .collect(),
    }
}

/// Argument names can be a single space-separated string or a list
#[derive(Debug, Clone)]
pub enum ArgumentNames {
    Single(String),
    List(Vec<String>),
}

/// Generate argument hint showing remaining unfilled args.
/// Returns hint string like "[arg2] [arg3]" or None if all filled.
pub fn generate_progressive_argument_hint(
    arg_names: &[String],
    typed_args: &[String],
) -> Option<String> {
    let remaining = &arg_names[typed_args.len()..];
    if remaining.is_empty() {
        return None;
    }
    Some(
        remaining
            .iter()
            .map(|name| format!("[{}]", name))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// Substitute $ARGUMENTS placeholders in content with actual argument values.
///
/// Supports:
/// - $ARGUMENTS - replaced with the full arguments string
/// - $ARGUMENTS[0], $ARGUMENTS[1], etc. - replaced with individual indexed arguments
/// - $0, $1, etc. - shorthand for $ARGUMENTS[0], $ARGUMENTS[1]
/// - Named arguments (e.g., $foo, $bar) - when argument names are defined in frontmatter
pub fn substitute_arguments(
    content: &str,
    args: Option<&str>,
    append_if_no_placeholder: bool,
    argument_names: &[String],
) -> String {
    // undefined/null means no args provided - return content unchanged
    let args = match args {
        Some(a) => a,
        None => return content.to_string(),
    };

    let parsed_args = parse_arguments(args);
    let original_content = content.to_string();
    let mut result = content.to_string();

    // Replace named arguments (e.g., $foo, $bar) with their values
    for (i, name) in argument_names.iter().enumerate() {
        if name.is_empty() {
            continue;
        }
        // Match $name but not $name[...] or $nameXxx (word chars)
        let pattern = format!(r"\${}(?![\[\w])", regex::escape(name));
        if let Ok(re) = Regex::new(&pattern) {
            let replacement = parsed_args.get(i).map(|s| s.as_str()).unwrap_or("");
            result = re.replace_all(&result, replacement).to_string();
        }
    }

    // Replace indexed arguments ($ARGUMENTS[0], $ARGUMENTS[1], etc.)
    let indexed_re = Regex::new(r"\$ARGUMENTS\[(\d+)\]").unwrap();
    result = indexed_re
        .replace_all(&result, |caps: &regex::Captures| {
            let index: usize = caps[1].parse().unwrap_or(0);
            parsed_args.get(index).map(|s| s.as_str()).unwrap_or("").to_string()
        })
        .to_string();

    // Replace shorthand indexed arguments ($0, $1, etc.)
    let shorthand_re = Regex::new(r"\$(\d+)(?!\w)").unwrap();
    result = shorthand_re
        .replace_all(&result, |caps: &regex::Captures| {
            let index: usize = caps[1].parse().unwrap_or(0);
            parsed_args.get(index).map(|s| s.as_str()).unwrap_or("").to_string()
        })
        .to_string();

    // Replace $ARGUMENTS with the full arguments string
    result = result.replace("$ARGUMENTS", args);

    // If no placeholders were found and append_if_no_placeholder is true, append
    if result == original_content && append_if_no_placeholder && !args.is_empty() {
        result = format!("{}\n\nARGUMENTS: {}", result, args);
    }

    result
}
