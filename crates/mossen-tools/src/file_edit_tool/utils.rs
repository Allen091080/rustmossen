use std::collections::HashMap;
use std::sync::LazyLock;

use super::types::{FileEdit, StructuredPatchHunk};

pub const LEFT_SINGLE_CURLY_QUOTE: char = '\u{2018}';
pub const RIGHT_SINGLE_CURLY_QUOTE: char = '\u{2019}';
pub const LEFT_DOUBLE_CURLY_QUOTE: char = '\u{201C}';
pub const RIGHT_DOUBLE_CURLY_QUOTE: char = '\u{201D}';

/// Normalizes quotes in a string by converting curly quotes to straight quotes.
pub fn normalize_quotes(s: &str) -> String {
    s.replace(LEFT_SINGLE_CURLY_QUOTE, "'")
        .replace(RIGHT_SINGLE_CURLY_QUOTE, "'")
        .replace(LEFT_DOUBLE_CURLY_QUOTE, "\"")
        .replace(RIGHT_DOUBLE_CURLY_QUOTE, "\"")
}

/// Strips trailing whitespace from each line while preserving line endings.
pub fn strip_trailing_whitespace(s: &str) -> String {
    let parts = split_keeping_separators(s);
    let mut result = String::with_capacity(s.len());
    for (i, part) in parts.iter().enumerate() {
        if i % 2 == 0 {
            result.push_str(part.trim_end());
        } else {
            result.push_str(part);
        }
    }
    result
}

fn split_keeping_separators(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'\r' && i + 1 < len && bytes[i + 1] == b'\n' {
            parts.push(&s[start..i]);
            parts.push(&s[i..i + 2]);
            start = i + 2;
            i += 2;
        } else if bytes[i] == b'\n' || bytes[i] == b'\r' {
            parts.push(&s[start..i]);
            parts.push(&s[i..i + 1]);
            start = i + 1;
            i += 1;
        } else {
            i += 1;
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Finds the actual string in the file content that matches the search string.
pub fn find_actual_string<'a>(file_content: &'a str, search_string: &str) -> Option<&'a str> {
    if let Some(idx) = file_content.find(search_string) {
        return Some(&file_content[idx..idx + search_string.len()]);
    }
    let normalized_search = normalize_quotes(search_string);
    let normalized_file = normalize_quotes(file_content);
    if let Some(search_index) = normalized_file.find(&normalized_search) {
        let char_offset = normalized_file[..search_index].chars().count();
        let char_len = normalized_search.chars().count();
        let start_byte = file_content
            .char_indices()
            .nth(char_offset)
            .map(|(i, _)| i)?;
        let end_byte = file_content
            .char_indices()
            .nth(char_offset + char_len)
            .map(|(i, _)| i)
            .unwrap_or(file_content.len());
        return Some(&file_content[start_byte..end_byte]);
    }
    None
}

/// Preserve curly quote style from old_string in new_string.
pub fn preserve_quote_style(old_string: &str, actual_old_string: &str, new_string: &str) -> String {
    if old_string == actual_old_string {
        return new_string.to_string();
    }
    let has_double = actual_old_string.contains(LEFT_DOUBLE_CURLY_QUOTE)
        || actual_old_string.contains(RIGHT_DOUBLE_CURLY_QUOTE);
    let has_single = actual_old_string.contains(LEFT_SINGLE_CURLY_QUOTE)
        || actual_old_string.contains(RIGHT_SINGLE_CURLY_QUOTE);
    if !has_double && !has_single {
        return new_string.to_string();
    }
    let mut result = new_string.to_string();
    if has_double {
        result = apply_curly_double_quotes(&result);
    }
    if has_single {
        result = apply_curly_single_quotes(&result);
    }
    result
}

fn is_opening_context(chars: &[char], index: usize) -> bool {
    if index == 0 {
        return true;
    }
    matches!(
        chars[index - 1],
        ' ' | '\t' | '\n' | '\r' | '(' | '[' | '{' | '\u{2014}' | '\u{2013}'
    )
}

fn apply_curly_double_quotes(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::with_capacity(s.len());
    for (i, &ch) in chars.iter().enumerate() {
        if ch == '"' {
            result.push(if is_opening_context(&chars, i) {
                LEFT_DOUBLE_CURLY_QUOTE
            } else {
                RIGHT_DOUBLE_CURLY_QUOTE
            });
        } else {
            result.push(ch);
        }
    }
    result
}

fn apply_curly_single_quotes(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::with_capacity(s.len());
    for (i, &ch) in chars.iter().enumerate() {
        if ch == '\'' {
            let prev = if i > 0 { Some(chars[i - 1]) } else { None };
            let next = chars.get(i + 1).copied();
            let prev_is_letter = prev.map(|c| c.is_alphabetic()).unwrap_or(false);
            let next_is_letter = next.map(|c| c.is_alphabetic()).unwrap_or(false);
            if prev_is_letter && next_is_letter {
                result.push(RIGHT_SINGLE_CURLY_QUOTE);
            } else if is_opening_context(&chars, i) {
                result.push(LEFT_SINGLE_CURLY_QUOTE);
            } else {
                result.push(RIGHT_SINGLE_CURLY_QUOTE);
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Apply an edit to file content.
pub fn apply_edit_to_file(
    original_content: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
) -> String {
    let do_replace = |content: &str, search: &str, replace: &str| -> String {
        if replace_all {
            content.replace(search, replace)
        } else {
            content.replacen(search, replace, 1)
        }
    };
    if !new_string.is_empty() {
        return do_replace(original_content, old_string, new_string);
    }
    let old_with_newline = format!("{}\n", old_string);
    let strip_trailing =
        !old_string.ends_with('\n') && original_content.contains(&old_with_newline);
    if strip_trailing {
        do_replace(original_content, &old_with_newline, new_string)
    } else {
        do_replace(original_content, old_string, new_string)
    }
}

/// Applies an edit and returns (patch, updated_file).
pub fn get_patch_for_edit(
    file_path: &str,
    file_contents: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
) -> Result<(Vec<StructuredPatchHunk>, String), String> {
    get_patch_for_edits(
        file_path,
        file_contents,
        &[FileEdit {
            old_string: old_string.to_string(),
            new_string: new_string.to_string(),
            replace_all,
        }],
    )
}

/// Applies a list of edits and returns (patch, updated_file).
pub fn get_patch_for_edits(
    _file_path: &str,
    file_contents: &str,
    edits: &[FileEdit],
) -> Result<(Vec<StructuredPatchHunk>, String), String> {
    let mut updated = file_contents.to_string();
    let mut applied_new: Vec<String> = Vec::new();

    if file_contents.is_empty()
        && edits.len() == 1
        && edits[0].old_string.is_empty()
        && edits[0].new_string.is_empty()
    {
        return Ok((compute_structured_patch(file_contents, ""), String::new()));
    }

    for edit in edits {
        let check = edit.old_string.trim_end_matches('\n');
        for prev in &applied_new {
            if !check.is_empty() && prev.contains(check) {
                return Err("Cannot edit file: old_string is a substring of a new_string from a previous edit.".to_string());
            }
        }
        let prev_content = updated.clone();
        if edit.old_string.is_empty() {
            updated = edit.new_string.clone();
        } else {
            updated = apply_edit_to_file(
                &updated,
                &edit.old_string,
                &edit.new_string,
                edit.replace_all,
            );
        }
        if updated == prev_content {
            return Err("String not found in file. Failed to apply edit.".to_string());
        }
        applied_new.push(edit.new_string.clone());
    }

    if updated == file_contents {
        return Err("Original and edited file match exactly. Failed to apply edit.".to_string());
    }

    let patch = compute_structured_patch(file_contents, &updated);
    Ok((patch, updated))
}

fn compute_structured_patch(old_content: &str, new_content: &str) -> Vec<StructuredPatchHunk> {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();
    let mut hunks = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < old_lines.len() || j < new_lines.len() {
        if i < old_lines.len() && j < new_lines.len() && old_lines[i] == new_lines[j] {
            i += 1;
            j += 1;
            continue;
        }
        let ctx_start = i.saturating_sub(3);
        let old_start = ctx_start + 1;
        let mut hunk_lines: Vec<String> = Vec::new();
        for k in ctx_start..i {
            hunk_lines.push(format!(" {}", old_lines[k]));
        }
        let di = i;
        let dj = j;
        while i < old_lines.len() && (j >= new_lines.len() || old_lines[i] != new_lines[j]) {
            i += 1;
        }
        while j < new_lines.len() && (i >= old_lines.len() || old_lines.get(i) != new_lines.get(j))
        {
            j += 1;
        }
        for k in di..i {
            hunk_lines.push(format!("-{}", old_lines[k]));
        }
        for k in dj..j {
            hunk_lines.push(format!("+{}", new_lines[k]));
        }
        let trailing = std::cmp::min(
            old_lines.len().saturating_sub(i),
            new_lines.len().saturating_sub(j),
        )
        .min(3);
        for k in 0..trailing {
            if i + k < old_lines.len() {
                hunk_lines.push(format!(" {}", old_lines[i + k]));
            }
        }
        let old_line_count = (i - ctx_start) + trailing;
        let new_line_count = (j - (dj - (di - ctx_start))) + trailing;
        hunks.push(StructuredPatchHunk {
            old_start,
            old_lines: old_line_count,
            new_start: old_start,
            new_lines: new_line_count,
            lines: hunk_lines,
        });
        i += trailing;
        j += trailing;
    }
    hunks
}

/// Gets a snippet showing context around a patch.
pub fn get_snippet_for_patch(patch: &[StructuredPatchHunk], new_file: &str) -> (String, usize) {
    if patch.is_empty() {
        return (String::new(), 1);
    }
    let mut min_line = usize::MAX;
    let mut max_line = 0usize;
    for hunk in patch {
        min_line = min_line.min(hunk.old_start);
        max_line = max_line.max(hunk.old_start + hunk.new_lines.saturating_sub(1));
    }
    let start = min_line.saturating_sub(4).max(1);
    let end = max_line + 4;
    let lines: Vec<&str> = new_file.lines().collect();
    let snippet: Vec<&str> = lines
        .iter()
        .skip(start - 1)
        .take(end - start + 1)
        .copied()
        .collect();
    let formatted = add_line_numbers(&snippet.join("\n"), start);
    (formatted, start)
}

/// Gets a snippet around a single edit.
pub fn get_snippet(original: &str, old_str: &str, new_str: &str, ctx: usize) -> (String, usize) {
    let before = original.split(old_str).next().unwrap_or("");
    let repl_line = before.lines().count().saturating_sub(1);
    let new_file = apply_edit_to_file(original, old_str, new_str, false);
    let lines: Vec<&str> = new_file.lines().collect();
    let start = repl_line.saturating_sub(ctx);
    let end = repl_line + ctx + new_str.lines().count();
    let snip: Vec<&str> = lines
        .iter()
        .skip(start)
        .take(end - start)
        .copied()
        .collect();
    (snip.join("\n"), start + 1)
}

/// Extract edits from a structured patch.
pub fn get_edits_for_patch(patch: &[StructuredPatchHunk]) -> Vec<FileEdit> {
    patch
        .iter()
        .map(|hunk| {
            let mut old_lines = Vec::new();
            let mut new_lines = Vec::new();
            for line in &hunk.lines {
                if let Some(rest) = line.strip_prefix(' ') {
                    old_lines.push(rest.to_string());
                    new_lines.push(rest.to_string());
                } else if let Some(rest) = line.strip_prefix('-') {
                    old_lines.push(rest.to_string());
                } else if let Some(rest) = line.strip_prefix('+') {
                    new_lines.push(rest.to_string());
                }
            }
            FileEdit {
                old_string: old_lines.join("\n"),
                new_string: new_lines.join("\n"),
                replace_all: false,
            }
        })
        .collect()
}

/// Desanitization map.
static DESANITIZATIONS: LazyLock<Vec<(&str, &str)>> = LazyLock::new(|| {
    vec![
        ("<fnr>", "<function_results>"),
        ("<n>", "<name>"),
        ("</n>", "</name>"),
        ("<o>", "<output>"),
        ("</o>", "</output>"),
        ("<e>", "<error>"),
        ("</e>", "</error>"),
        ("<s>", "<system>"),
        ("</s>", "</system>"),
        ("<r>", "<result>"),
        ("</r>", "</result>"),
        ("\n\nH:", "\n\nHuman:"),
        ("\n\nA:", "\n\nAssistant:"),
    ]
});

fn desanitize_match_string(match_string: &str) -> (String, Vec<(String, String)>) {
    let mut result = match_string.to_string();
    let mut applied = Vec::new();
    for &(from, to) in DESANITIZATIONS.iter() {
        let before = result.clone();
        result = result.replace(from, to);
        if before != result {
            applied.push((from.to_string(), to.to_string()));
        }
    }
    (result, applied)
}

/// Normalize the input for the FileEditTool.
pub fn normalize_file_edit_input(
    file_path: &str,
    edits: &[(String, String, Option<bool>)],
) -> (String, Vec<(String, String, Option<bool>)>) {
    if edits.is_empty() {
        return (file_path.to_string(), edits.to_vec());
    }
    let is_markdown = file_path.ends_with(".md") || file_path.ends_with(".mdx");
    let full_path = expand_path(file_path);
    let file_content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(_) => return (file_path.to_string(), edits.to_vec()),
    };
    let normalized_edits: Vec<(String, String, Option<bool>)> = edits
        .iter()
        .map(|(old, new, replace_all)| {
            let norm_new = if is_markdown {
                new.clone()
            } else {
                strip_trailing_whitespace(new)
            };
            if file_content.contains(old.as_str()) {
                return (old.clone(), norm_new, *replace_all);
            }
            let (desanitized_old, applied) = desanitize_match_string(old);
            if file_content.contains(&desanitized_old) {
                let mut desanitized_new = norm_new.clone();
                for (from, to) in &applied {
                    desanitized_new = desanitized_new.replace(from.as_str(), to.as_str());
                }
                return (desanitized_old, desanitized_new, *replace_all);
            }
            (old.clone(), norm_new, *replace_all)
        })
        .collect();
    (file_path.to_string(), normalized_edits)
}

/// Compare two sets of edits for equivalence.
pub fn are_file_edits_equivalent(
    edits1: &[FileEdit],
    edits2: &[FileEdit],
    original_content: &str,
) -> bool {
    if edits1.len() == edits2.len()
        && edits1.iter().zip(edits2.iter()).all(|(a, b)| {
            a.old_string == b.old_string
                && a.new_string == b.new_string
                && a.replace_all == b.replace_all
        })
    {
        return true;
    }
    let r1 = get_patch_for_edits("temp", original_content, edits1);
    let r2 = get_patch_for_edits("temp", original_content, edits2);
    match (r1, r2) {
        (Ok((_, u1)), Ok((_, u2))) => u1 == u2,
        (Err(e1), Err(e2)) => e1 == e2,
        _ => false,
    }
}

/// Check if two file edit inputs are equivalent.
pub fn are_file_edits_inputs_equivalent(
    file_path1: &str,
    edits1: &[FileEdit],
    file_path2: &str,
    edits2: &[FileEdit],
) -> bool {
    if file_path1 != file_path2 {
        return false;
    }
    if edits1.len() == edits2.len()
        && edits1.iter().zip(edits2.iter()).all(|(a, b)| {
            a.old_string == b.old_string
                && a.new_string == b.new_string
                && a.replace_all == b.replace_all
        })
    {
        return true;
    }
    let file_content = std::fs::read_to_string(file_path1).unwrap_or_default();
    are_file_edits_equivalent(edits1, edits2, &file_content)
}

/// Add line numbers to content.
pub fn add_line_numbers(content: &str, start_line: usize) -> String {
    content
        .lines()
        .enumerate()
        .map(|(i, line)| format!("{:>6}\u{2192}{}", start_line + i, line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Expand ~ in file paths.
fn expand_path(path: &str) -> String {
    if path.starts_with('~') {
        if let Ok(home) = std::env::var("HOME") {
            return path.replacen('~', &home, 1);
        }
    }
    path.to_string()
}

/// Get snippet for two-file diff.
pub fn get_snippet_for_two_file_diff(file_a: &str, file_b: &str) -> String {
    let patch = compute_structured_patch(file_a, file_b);
    if patch.is_empty() {
        return String::new();
    }
    let full: String = patch
        .iter()
        .map(|hunk| {
            let content: String = hunk
                .lines
                .iter()
                .filter(|l| !l.starts_with('-') && !l.starts_with('\\'))
                .map(|l| if l.len() > 1 { &l[1..] } else { "" })
                .collect::<Vec<_>>()
                .join("\n");
            add_line_numbers(&content, hunk.old_start)
        })
        .collect::<Vec<_>>()
        .join("\n...\n");
    const MAX_BYTES: usize = 8192;
    if full.len() <= MAX_BYTES {
        return full;
    }
    let cutoff = full[..MAX_BYTES].rfind('\n').unwrap_or(MAX_BYTES);
    let kept = &full[..cutoff];
    let remaining = full[cutoff..].lines().count();
    format!("{}\n\n... [{} lines truncated] ...", kept, remaining)
}
