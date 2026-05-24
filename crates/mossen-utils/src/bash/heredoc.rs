//! Heredoc extraction and restoration utilities.
//!
//! Translated from `heredoc.ts` (734 lines).

use rand::Rng;
use regex::Regex;
use std::collections::HashMap;

const HEREDOC_PLACEHOLDER_PREFIX: &str = "__HEREDOC_";
const HEREDOC_PLACEHOLDER_SUFFIX: &str = "__";

/// Generates a random hex string for placeholder uniqueness.
fn generate_placeholder_salt() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 8] = rng.gen();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Heredoc info captured during extraction.
#[derive(Debug, Clone)]
pub struct HeredocInfo {
    /// The full heredoc text including << operator, delimiter, content, and closing delimiter
    pub full_text: String,
    /// The delimiter word (without quotes)
    pub delimiter: String,
    /// Start position of the << operator in the original command
    pub operator_start_index: usize,
    /// End position of the << operator (exclusive)
    pub operator_end_index: usize,
    /// Start position of heredoc content (the newline before content)
    pub content_start_index: usize,
    /// End position of heredoc content including closing delimiter (exclusive)
    pub content_end_index: usize,
}

/// Result of heredoc extraction.
#[derive(Debug, Clone)]
pub struct HeredocExtractionResult {
    /// The command with heredocs replaced by placeholders
    pub processed_command: String,
    /// Map of placeholder string to original heredoc info
    pub heredocs: HashMap<String, HeredocInfo>,
}

/// Options for heredoc extraction.
#[derive(Debug, Clone, Default)]
pub struct ExtractHeredocsOptions {
    pub quoted_only: bool,
}

/// Extracts heredocs from a command string and replaces them with placeholders.
pub fn extract_heredocs(
    command: &str,
    options: Option<&ExtractHeredocsOptions>,
) -> HeredocExtractionResult {
    let mut heredocs = HashMap::new();
    let quoted_only = options.map_or(false, |o| o.quoted_only);

    // Quick check: if no << present, skip processing
    if !command.contains("<<") {
        return HeredocExtractionResult {
            processed_command: command.to_string(),
            heredocs,
        };
    }

    // Security: bail if $' or $" present (ANSI-C / locale quoting)
    if command.contains("$'") || command.contains("$\"") {
        return HeredocExtractionResult {
            processed_command: command.to_string(),
            heredocs,
        };
    }

    // Check for backticks before the first <<
    let first_heredoc_pos = command.find("<<").unwrap();
    if first_heredoc_pos > 0 && command[..first_heredoc_pos].contains('`') {
        return HeredocExtractionResult {
            processed_command: command.to_string(),
            heredocs,
        };
    }

    // Security: Check for arithmetic evaluation context before first <<
    if first_heredoc_pos > 0 {
        let before_heredoc = &command[..first_heredoc_pos];
        let open_arith = before_heredoc.matches("((").count();
        let close_arith = before_heredoc.matches("))").count();
        if open_arith > close_arith {
            return HeredocExtractionResult {
                processed_command: command.to_string(),
                heredocs,
            };
        }
    }

    // Regex for heredoc start pattern
    let heredoc_start_re =
        Regex::new(r#"(?<![<])<<(?![<])(-)?[ \t]*(?:(['"])(\\?\w+)\2|\\?(\w+))"#).unwrap();

    let mut heredoc_matches: Vec<HeredocInfo> = Vec::new();
    let mut skipped_heredoc_ranges: Vec<(usize, usize)> = Vec::new();

    // Incremental quote/comment scanner state
    let chars: Vec<char> = command.chars().collect();
    let char_to_byte: Vec<usize> = {
        let mut map = Vec::with_capacity(chars.len());
        let mut byte_pos = 0;
        for &ch in &chars {
            map.push(byte_pos);
            byte_pos += ch.len_utf8();
        }
        map
    };
    let total_bytes = command.len();

    let mut scan_pos: usize = 0; // char index
    let mut scan_in_single_quote = false;
    let mut scan_in_double_quote = false;
    let mut scan_in_comment = false;
    let mut scan_dq_escape_next = false;
    let mut scan_pending_backslashes: usize = 0;

    // Convert byte offset to char index
    let byte_to_char_idx = |byte_offset: usize| -> usize {
        char_to_byte
            .partition_point(|&b| b < byte_offset)
            .min(chars.len())
    };

    let advance_scan = |target_char: usize,
                        scan_pos: &mut usize,
                        in_sq: &mut bool,
                        in_dq: &mut bool,
                        in_comment: &mut bool,
                        dq_escape: &mut bool,
                        pending_bs: &mut usize| {
        for i in *scan_pos..target_char {
            let ch = chars[i];
            if ch == '\n' {
                *in_comment = false;
            }
            if *in_sq {
                if ch == '\'' {
                    *in_sq = false;
                }
                continue;
            }
            if *in_dq {
                if *dq_escape {
                    *dq_escape = false;
                    continue;
                }
                if ch == '\\' {
                    *dq_escape = true;
                    continue;
                }
                if ch == '"' {
                    *in_dq = false;
                }
                continue;
            }
            // Unquoted context
            if ch == '\\' {
                *pending_bs += 1;
                continue;
            }
            let escaped = *pending_bs % 2 == 1;
            *pending_bs = 0;
            if escaped {
                continue;
            }
            if ch == '\'' {
                *in_sq = true;
            } else if ch == '"' {
                *in_dq = true;
            } else if !*in_comment && ch == '#' {
                *in_comment = true;
            }
        }
        *scan_pos = target_char;
    };

    for cap in heredoc_start_re.find_iter(command) {
        let start_byte = cap.start();
        let start_char = byte_to_char_idx(start_byte);

        advance_scan(
            start_char,
            &mut scan_pos,
            &mut scan_in_single_quote,
            &mut scan_in_double_quote,
            &mut scan_in_comment,
            &mut scan_dq_escape_next,
            &mut scan_pending_backslashes,
        );

        if scan_in_single_quote || scan_in_double_quote {
            continue;
        }
        if scan_in_comment {
            continue;
        }
        if scan_pending_backslashes % 2 == 1 {
            continue;
        }

        // Check if inside a skipped heredoc range
        let mut inside_skipped = false;
        for &(sk_start, sk_end) in &skipped_heredoc_ranges {
            if start_byte > sk_start && start_byte < sk_end {
                inside_skipped = true;
                break;
            }
        }
        if inside_skipped {
            continue;
        }

        // Parse the match using regex captures
        let caps = match heredoc_start_re.captures(&command[start_byte..]) {
            Some(c) => c,
            None => continue,
        };
        let full_match = caps.get(0).unwrap();
        let full_match_str = full_match.as_str();
        let is_dash = caps.get(1).map_or(false, |m| m.as_str() == "-");
        let quote_char = caps.get(2).map(|m| m.as_str());
        let delimiter = caps.get(3).or_else(|| caps.get(4)).map(|m| m.as_str());
        let delimiter = match delimiter {
            Some(d) => d.to_string(),
            None => continue,
        };

        let operator_end_byte = start_byte + full_match_str.len();

        // Check closing quote was matched
        if let Some(qc) = quote_char {
            let last_char = &command[operator_end_byte - 1..operator_end_byte];
            if last_char != qc {
                continue;
            }
        }

        // Security: determine if quoted/escaped
        let is_escaped_delimiter = full_match_str.contains('\\');
        let is_quoted_or_escaped = quote_char.is_some() || is_escaped_delimiter;

        // Check next char is a bash word terminator
        if operator_end_byte < total_bytes {
            let next_byte = command.as_bytes()[operator_end_byte];
            if !matches!(
                next_byte,
                b' ' | b'\t' | b'\n' | b'|' | b'&' | b';' | b'(' | b')' | b'<' | b'>'
            ) {
                continue;
            }
        }

        // Find first unquoted newline after the operator
        let mut first_newline_offset: Option<usize> = None;
        {
            let mut in_sq = false;
            let mut in_dq = false;
            let after = &command[operator_end_byte..];
            let after_chars: Vec<char> = after.chars().collect();
            let mut byte_off = 0usize;
            for &ch in &after_chars {
                if in_sq {
                    if ch == '\'' {
                        in_sq = false;
                    }
                    byte_off += ch.len_utf8();
                    continue;
                }
                if in_dq {
                    if ch == '\\' {
                        byte_off += ch.len_utf8();
                        // skip next char
                        // Need to handle this properly
                        continue;
                    }
                    if ch == '"' {
                        in_dq = false;
                    }
                    byte_off += ch.len_utf8();
                    continue;
                }
                if ch == '\n' {
                    first_newline_offset = Some(byte_off);
                    break;
                }
                if ch == '\'' {
                    in_sq = true;
                } else if ch == '"' {
                    in_dq = true;
                }
                byte_off += ch.len_utf8();
            }
        }

        let first_newline_offset = match first_newline_offset {
            Some(off) => off,
            None => continue, // No newline found
        };

        // Check for backslash-newline continuation
        let same_line_content =
            &command[operator_end_byte..operator_end_byte + first_newline_offset];
        let mut trailing_backslashes = 0;
        for ch in same_line_content.chars().rev() {
            if ch == '\\' {
                trailing_backslashes += 1;
            } else {
                break;
            }
        }
        if trailing_backslashes % 2 == 1 {
            continue; // Line continuation, bail
        }

        let content_start_index = operator_end_byte + first_newline_offset;
        let after_newline = &command[content_start_index + 1..]; // +1 to skip the newline
        let content_lines: Vec<&str> = after_newline.split('\n').collect();

        // Find the closing delimiter
        let mut closing_line_index: Option<usize> = None;
        for (idx, line) in content_lines.iter().enumerate() {
            if is_dash {
                let stripped = line.trim_start_matches('\t');
                if stripped == delimiter {
                    closing_line_index = Some(idx);
                    break;
                }
            } else if *line == delimiter {
                closing_line_index = Some(idx);
                break;
            }

            // Security: check for PST_EOFTOKEN-like early closure
            let eof_check_line = if is_dash {
                line.trim_start_matches('\t')
            } else {
                line
            };
            if eof_check_line.len() > delimiter.len() && eof_check_line.starts_with(&delimiter) {
                let char_after = eof_check_line.as_bytes()[delimiter.len()];
                if matches!(
                    char_after,
                    b')' | b'}' | b'`' | b'|' | b'&' | b';' | b'(' | b'<' | b'>'
                ) {
                    closing_line_index = None;
                    break;
                }
            }
        }

        // Handle quotedOnly mode for unquoted heredocs
        if quoted_only && !is_quoted_or_escaped {
            let skip_content_end = if closing_line_index.is_none() {
                total_bytes
            } else {
                let lines_up = &content_lines[..closing_line_index.unwrap() + 1];
                let content_len: usize = lines_up.join("\n").len();
                content_start_index + 1 + content_len
            };
            skipped_heredoc_ranges.push((content_start_index, skip_content_end));
            continue;
        }

        let closing_line_index = match closing_line_index {
            Some(idx) => idx,
            None => continue, // Malformed, skip
        };

        let lines_up_to_closing = &content_lines[..closing_line_index + 1];
        let content_length = lines_up_to_closing.join("\n").len();
        let content_end_index = content_start_index + 1 + content_length;

        // Check overlap with skipped ranges
        let mut overlaps_skipped = false;
        for &(sk_start, sk_end) in &skipped_heredoc_ranges {
            if content_start_index < sk_end && sk_start < content_end_index {
                overlaps_skipped = true;
                break;
            }
        }
        if overlaps_skipped {
            continue;
        }

        let operator_text = &command[start_byte..operator_end_byte];
        let content_text = &command[content_start_index..content_end_index];
        let full_text = format!("{}{}", operator_text, content_text);

        heredoc_matches.push(HeredocInfo {
            full_text,
            delimiter: delimiter.clone(),
            operator_start_index: start_byte,
            operator_end_index: operator_end_byte,
            content_start_index,
            content_end_index,
        });
    }

    if heredoc_matches.is_empty() {
        return HeredocExtractionResult {
            processed_command: command.to_string(),
            heredocs,
        };
    }

    // Filter out nested heredocs
    let top_level: Vec<HeredocInfo> = heredoc_matches
        .iter()
        .filter(|candidate| {
            !heredoc_matches.iter().any(|other| {
                !std::ptr::eq(*candidate, other)
                    && candidate.operator_start_index > other.content_start_index
                    && candidate.operator_start_index < other.content_end_index
            })
        })
        .cloned()
        .collect();

    if top_level.is_empty() {
        return HeredocExtractionResult {
            processed_command: command.to_string(),
            heredocs,
        };
    }

    // Check for duplicate content start positions
    let mut content_starts = std::collections::HashSet::new();
    for h in &top_level {
        content_starts.insert(h.content_start_index);
    }
    if content_starts.len() < top_level.len() {
        return HeredocExtractionResult {
            processed_command: command.to_string(),
            heredocs,
        };
    }

    // Sort by content end position descending
    let mut sorted_heredocs = top_level;
    sorted_heredocs.sort_by(|a, b| b.content_end_index.cmp(&a.content_end_index));

    let salt = generate_placeholder_salt();
    let mut processed_command = command.to_string();

    for (index, info) in sorted_heredocs.iter().enumerate() {
        let placeholder_index = sorted_heredocs.len() - 1 - index;
        let placeholder = format!(
            "{}{}_{}{}",
            HEREDOC_PLACEHOLDER_PREFIX, placeholder_index, salt, HEREDOC_PLACEHOLDER_SUFFIX
        );

        heredocs.insert(placeholder.clone(), info.clone());

        processed_command = format!(
            "{}{}{}{}",
            &processed_command[..info.operator_start_index],
            placeholder,
            &processed_command[info.operator_end_index..info.content_start_index],
            &processed_command[info.content_end_index..]
        );
    }

    HeredocExtractionResult {
        processed_command,
        heredocs,
    }
}

/// Restores heredoc placeholders in a single string.
fn restore_heredocs_in_string(text: &str, heredocs: &HashMap<String, HeredocInfo>) -> String {
    let mut result = text.to_string();
    for (placeholder, info) in heredocs {
        result = result.replace(placeholder, &info.full_text);
    }
    result
}

/// Restores heredoc placeholders in an array of strings.
pub fn restore_heredocs(parts: &[String], heredocs: &HashMap<String, HeredocInfo>) -> Vec<String> {
    if heredocs.is_empty() {
        return parts.to_vec();
    }
    parts
        .iter()
        .map(|part| restore_heredocs_in_string(part, heredocs))
        .collect()
}

/// Checks if a command contains heredoc syntax.
pub fn contains_heredoc(command: &str) -> bool {
    let re = Regex::new(r#"(?<![<])<<(?![<])(-)?[ \t]*(?:(['"])(\\?\w+)\2|\\?(\w+))"#).unwrap();
    re.is_match(command)
}
