/// Number of context lines in diffs.
pub const CONTEXT_LINES: usize = 3;

/// Diff timeout in milliseconds.
pub const DIFF_TIMEOUT_MS: u64 = 5_000;

/// Represents a structured patch hunk.
#[derive(Debug, Clone)]
pub struct StructuredPatchHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<String>,
}

/// Represents a file edit operation.
#[derive(Debug, Clone)]
pub struct FileEdit {
    pub old_string: String,
    pub new_string: String,
    pub replace_all: bool,
}

/// Token to escape ampersands during diff.
const AMPERSAND_TOKEN: &str = "<<:AMPERSAND_TOKEN:>>";
/// Token to escape dollar signs during diff.
const DOLLAR_TOKEN: &str = "<<:DOLLAR_TOKEN:>>";

fn escape_for_diff(s: &str) -> String {
    s.replace('&', AMPERSAND_TOKEN).replace('$', DOLLAR_TOKEN)
}

fn unescape_from_diff(s: &str) -> String {
    s.replace(AMPERSAND_TOKEN, "&").replace(DOLLAR_TOKEN, "$")
}

/// Shifts hunk line numbers by offset.
pub fn adjust_hunk_line_numbers(
    hunks: &[StructuredPatchHunk],
    offset: usize,
) -> Vec<StructuredPatchHunk> {
    if offset == 0 {
        return hunks.to_vec();
    }
    hunks
        .iter()
        .map(|h| StructuredPatchHunk {
            old_start: h.old_start + offset,
            old_lines: h.old_lines,
            new_start: h.new_start + offset,
            new_lines: h.new_lines,
            lines: h.lines.clone(),
        })
        .collect()
}

/// Count lines added and removed in a patch.
pub fn count_lines_changed(
    patch: &[StructuredPatchHunk],
    new_file_content: Option<&str>,
) -> (usize, usize) {
    if patch.is_empty() {
        if let Some(content) = new_file_content {
            let num_additions = content.lines().count();
            return (num_additions, 0);
        }
        return (0, 0);
    }

    let num_additions = patch
        .iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.starts_with('+'))
        .count();

    let num_removals = patch
        .iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.starts_with('-'))
        .count();

    (num_additions, num_removals)
}

/// Get a patch from old and new contents using the `similar` crate.
pub fn get_patch_from_contents(
    _file_path: &str,
    old_content: &str,
    new_content: &str,
    _ignore_whitespace: bool,
    single_hunk: bool,
) -> Vec<StructuredPatchHunk> {
    let escaped_old = escape_for_diff(old_content);
    let escaped_new = escape_for_diff(new_content);

    let context_size = if single_hunk { 100_000 } else { CONTEXT_LINES };

    let diff = similar::TextDiff::configure()
        .algorithm(similar::Algorithm::Myers)
        .timeout(std::time::Duration::from_millis(DIFF_TIMEOUT_MS))
        .diff_lines(&escaped_old, &escaped_new);

    let mut hunks = Vec::new();

    for group in diff.grouped_ops(context_size) {
        let mut lines = Vec::new();
        let mut old_start = 0;
        let mut old_lines_count = 0;
        let mut new_start = 0;
        let mut new_lines_count = 0;
        let mut first = true;

        for op in &group {
            if first {
                old_start = op.old_range().start + 1;
                new_start = op.new_range().start + 1;
                first = false;
            }

            match op.tag() {
                similar::DiffTag::Equal => {
                    for change in diff.iter_changes(op) {
                        lines.push(format!(
                            " {}",
                            unescape_from_diff(change.value().trim_end_matches('\n'))
                        ));
                        old_lines_count += 1;
                        new_lines_count += 1;
                    }
                }
                similar::DiffTag::Delete => {
                    for change in diff.iter_changes(op) {
                        lines.push(format!(
                            "-{}",
                            unescape_from_diff(change.value().trim_end_matches('\n'))
                        ));
                        old_lines_count += 1;
                    }
                }
                similar::DiffTag::Insert => {
                    for change in diff.iter_changes(op) {
                        lines.push(format!(
                            "+{}",
                            unescape_from_diff(change.value().trim_end_matches('\n'))
                        ));
                        new_lines_count += 1;
                    }
                }
                similar::DiffTag::Replace => {
                    for change in diff.iter_changes(op) {
                        match change.tag() {
                            similar::ChangeTag::Delete => {
                                lines.push(format!(
                                    "-{}",
                                    unescape_from_diff(change.value().trim_end_matches('\n'))
                                ));
                                old_lines_count += 1;
                            }
                            similar::ChangeTag::Insert => {
                                lines.push(format!(
                                    "+{}",
                                    unescape_from_diff(change.value().trim_end_matches('\n'))
                                ));
                                new_lines_count += 1;
                            }
                            similar::ChangeTag::Equal => {
                                lines.push(format!(
                                    " {}",
                                    unescape_from_diff(change.value().trim_end_matches('\n'))
                                ));
                                old_lines_count += 1;
                                new_lines_count += 1;
                            }
                        }
                    }
                }
            }
        }

        hunks.push(StructuredPatchHunk {
            old_start,
            old_lines: old_lines_count,
            new_start,
            new_lines: new_lines_count,
            lines,
        });
    }

    hunks
}

/// Convert leading tabs to spaces for display.
fn convert_leading_tabs_to_spaces(content: &str) -> String {
    if !content.contains('\t') {
        return content.to_string();
    }
    content
        .lines()
        .map(|line| {
            let tab_count = line.chars().take_while(|&c| c == '\t').count();
            if tab_count > 0 {
                format!("{}{}", "  ".repeat(tab_count), &line[tab_count..])
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Get a patch for display with edits applied.
pub fn get_patch_for_display(
    file_path: &str,
    file_contents: &str,
    edits: &[FileEdit],
    ignore_whitespace: bool,
) -> Vec<StructuredPatchHunk> {
    let prepared = escape_for_diff(&convert_leading_tabs_to_spaces(file_contents));

    let new_content = edits.iter().fold(prepared.clone(), |acc, edit| {
        let escaped_old = escape_for_diff(&convert_leading_tabs_to_spaces(&edit.old_string));
        let escaped_new = escape_for_diff(&convert_leading_tabs_to_spaces(&edit.new_string));

        if edit.replace_all {
            acc.replace(&escaped_old, &escaped_new)
        } else {
            acc.replacen(&escaped_old, &escaped_new, 1)
        }
    });

    let unescaped_old = unescape_from_diff(&prepared);
    let unescaped_new = unescape_from_diff(&new_content);

    get_patch_from_contents(
        file_path,
        &unescaped_old,
        &unescaped_new,
        ignore_whitespace,
        false,
    )
}
