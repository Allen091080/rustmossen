use std::collections::HashMap;
use std::path::Path;

/// LSP SymbolKind numeric values to readable strings.
fn symbol_kind_to_string(kind: u32) -> &'static str {
    match kind {
        1 => "File",
        2 => "Module",
        3 => "Namespace",
        4 => "Package",
        5 => "Class",
        6 => "Method",
        7 => "Property",
        8 => "Field",
        9 => "Constructor",
        10 => "Enum",
        11 => "Interface",
        12 => "Function",
        13 => "Variable",
        14 => "Constant",
        15 => "String",
        16 => "Number",
        17 => "Boolean",
        18 => "Array",
        19 => "Object",
        20 => "Key",
        21 => "Null",
        22 => "EnumMember",
        23 => "Struct",
        24 => "Event",
        25 => "Operator",
        26 => "TypeParameter",
        _ => "Unknown",
    }
}

/// Represents an LSP position.
#[derive(Debug, Clone)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// Represents an LSP range.
#[derive(Debug, Clone)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// Represents an LSP Location.
#[derive(Debug, Clone)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

/// Represents a DocumentSymbol from LSP.
#[derive(Debug, Clone)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: u32,
    pub detail: Option<String>,
    pub range: Range,
    pub children: Vec<DocumentSymbol>,
}

/// Represents a SymbolInformation from LSP.
#[derive(Debug, Clone)]
pub struct SymbolInformation {
    pub name: String,
    pub kind: u32,
    pub location: Location,
    pub container_name: Option<String>,
}

/// Represents hover result.
#[derive(Debug, Clone)]
pub struct HoverResult {
    pub contents: String,
    pub range: Option<Range>,
}

/// Represents a CallHierarchyItem.
#[derive(Debug, Clone)]
pub struct CallHierarchyItem {
    pub name: String,
    pub kind: u32,
    pub uri: String,
    pub range: Range,
    pub detail: Option<String>,
}

/// Represents an incoming call.
#[derive(Debug, Clone)]
pub struct IncomingCall {
    pub from: CallHierarchyItem,
    pub from_ranges: Vec<Range>,
}

/// Represents an outgoing call.
#[derive(Debug, Clone)]
pub struct OutgoingCall {
    pub to: CallHierarchyItem,
    pub from_ranges: Vec<Range>,
}

/// Format a URI by converting to relative path if possible.
fn format_uri(uri: &str, cwd: Option<&str>) -> String {
    if uri.is_empty() {
        return "<unknown location>".to_string();
    }
    let mut file_path = uri.replace("file://", "");
    // Windows drive letter paths.
    if file_path.starts_with('/')
        && file_path
            .chars()
            .nth(1)
            .map(|c| c.is_ascii_alphabetic())
            .unwrap_or(false)
        && file_path.chars().nth(2) == Some(':')
    {
        file_path = file_path[1..].to_string();
    }
    // Simplified URI decode for %XX sequences.
    file_path = uri_decode(&file_path);

    if let Some(cwd) = cwd {
        if let Some(rel) = make_relative(&file_path, cwd) {
            if rel.len() < file_path.len() && !rel.starts_with("../../") {
                return rel;
            }
        }
    }
    file_path.replace('\\', "/")
}

/// Simple percent-decode for URI paths.
fn uri_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                result.push((hi << 4 | lo) as char);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Make a path relative to cwd.
fn make_relative(path: &str, cwd: &str) -> Option<String> {
    let path = Path::new(path);
    let base = Path::new(cwd);
    path.strip_prefix(base)
        .ok()
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
}

/// Format a Location with file path and line:character.
fn format_location(location: &Location, cwd: Option<&str>) -> String {
    let file_path = format_uri(&location.uri, cwd);
    let line = location.range.start.line + 1;
    let character = location.range.start.character + 1;
    format!("{}:{}:{}", file_path, line, character)
}

/// Format go-to-definition result.
pub fn format_go_to_definition_result(locations: &[Location], cwd: Option<&str>) -> String {
    if locations.is_empty() {
        return "No definition found. This may occur if the cursor is not on a symbol, or if \
                the definition is in an external library not indexed by the LSP server."
            .to_string();
    }
    let valid: Vec<&Location> = locations.iter().filter(|l| !l.uri.is_empty()).collect();
    if valid.is_empty() {
        return "No definition found. This may occur if the cursor is not on a symbol, or if \
                the definition is in an external library not indexed by the LSP server."
            .to_string();
    }
    if valid.len() == 1 {
        return format!("Defined in {}", format_location(valid[0], cwd));
    }
    let list: Vec<String> = valid
        .iter()
        .map(|l| format!("  {}", format_location(l, cwd)))
        .collect();
    format!("Found {} definitions:\n{}", valid.len(), list.join("\n"))
}

/// Format find-references result.
pub fn format_find_references_result(locations: &[Location], cwd: Option<&str>) -> String {
    if locations.is_empty() {
        return "No references found. This may occur if the symbol has no usages, or if the \
                LSP server has not fully indexed the workspace."
            .to_string();
    }
    let valid: Vec<&Location> = locations.iter().filter(|l| !l.uri.is_empty()).collect();
    if valid.is_empty() {
        return "No references found.".to_string();
    }
    if valid.len() == 1 {
        return format!("Found 1 reference:\n  {}", format_location(valid[0], cwd));
    }
    let mut by_file: HashMap<String, Vec<&Location>> = HashMap::new();
    for loc in &valid {
        let fp = format_uri(&loc.uri, cwd);
        by_file.entry(fp).or_default().push(loc);
    }
    let mut lines = vec![format!(
        "Found {} references across {} files:",
        valid.len(),
        by_file.len()
    )];
    for (file_path, locs) in &by_file {
        lines.push(format!("\n{}:", file_path));
        for loc in locs {
            let line = loc.range.start.line + 1;
            let ch = loc.range.start.character + 1;
            lines.push(format!("  Line {}:{}", line, ch));
        }
    }
    lines.join("\n")
}

/// Format hover result.
pub fn format_hover_result(result: Option<&HoverResult>) -> String {
    match result {
        None => "No hover information available. This may occur if the cursor is not on a \
                 symbol, or if the LSP server has not fully indexed the file."
            .to_string(),
        Some(hover) => {
            if let Some(ref range) = hover.range {
                let line = range.start.line + 1;
                let ch = range.start.character + 1;
                format!("Hover info at {}:{}:\n\n{}", line, ch, hover.contents)
            } else {
                hover.contents.clone()
            }
        }
    }
}

/// Format document-symbol result.
pub fn format_document_symbol_result(symbols: &[DocumentSymbol]) -> String {
    if symbols.is_empty() {
        return "No symbols found in document.".to_string();
    }
    let mut lines = vec!["Document symbols:".to_string()];
    for sym in symbols {
        format_document_symbol_node(sym, 0, &mut lines);
    }
    lines.join("\n")
}

fn format_document_symbol_node(symbol: &DocumentSymbol, indent: usize, lines: &mut Vec<String>) {
    let prefix = "  ".repeat(indent);
    let kind = symbol_kind_to_string(symbol.kind);
    let mut line = format!("{}{} ({})", prefix, symbol.name, kind);
    if let Some(ref detail) = symbol.detail {
        line.push_str(&format!(" {}", detail));
    }
    line.push_str(&format!(" - Line {}", symbol.range.start.line + 1));
    lines.push(line);
    for child in &symbol.children {
        format_document_symbol_node(child, indent + 1, lines);
    }
}

/// Format workspace-symbol result.
pub fn format_workspace_symbol_result(symbols: &[SymbolInformation], cwd: Option<&str>) -> String {
    if symbols.is_empty() {
        return "No symbols found in workspace.".to_string();
    }
    let valid: Vec<&SymbolInformation> = symbols
        .iter()
        .filter(|s| !s.location.uri.is_empty())
        .collect();
    if valid.is_empty() {
        return "No symbols found in workspace.".to_string();
    }
    let mut by_file: HashMap<String, Vec<&SymbolInformation>> = HashMap::new();
    for sym in &valid {
        let fp = format_uri(&sym.location.uri, cwd);
        by_file.entry(fp).or_default().push(sym);
    }
    let mut lines = vec![format!(
        "Found {} {} in workspace:",
        valid.len(),
        plural(valid.len(), "symbol")
    )];
    for (file_path, syms) in &by_file {
        lines.push(format!("\n{}:", file_path));
        for sym in syms {
            let kind = symbol_kind_to_string(sym.kind);
            let line = sym.location.range.start.line + 1;
            let mut sym_line = format!("  {} ({}) - Line {}", sym.name, kind, line);
            if let Some(ref container) = sym.container_name {
                sym_line.push_str(&format!(" in {}", container));
            }
            lines.push(sym_line);
        }
    }
    lines.join("\n")
}

/// Format prepare-call-hierarchy result.
pub fn format_prepare_call_hierarchy_result(
    items: &[CallHierarchyItem],
    cwd: Option<&str>,
) -> String {
    if items.is_empty() {
        return "No call hierarchy item found at this position".to_string();
    }
    if items.len() == 1 {
        return format!(
            "Call hierarchy item: {}",
            format_call_hierarchy_item(&items[0], cwd)
        );
    }
    let mut lines = vec![format!("Found {} call hierarchy items:", items.len())];
    for item in items {
        lines.push(format!("  {}", format_call_hierarchy_item(item, cwd)));
    }
    lines.join("\n")
}

fn format_call_hierarchy_item(item: &CallHierarchyItem, cwd: Option<&str>) -> String {
    if item.uri.is_empty() {
        return format!(
            "{} ({}) - <unknown location>",
            item.name,
            symbol_kind_to_string(item.kind)
        );
    }
    let file_path = format_uri(&item.uri, cwd);
    let line = item.range.start.line + 1;
    let kind = symbol_kind_to_string(item.kind);
    let mut result = format!("{} ({}) - {}:{}", item.name, kind, file_path, line);
    if let Some(ref detail) = item.detail {
        result.push_str(&format!(" [{}]", detail));
    }
    result
}

/// Format incoming-calls result.
pub fn format_incoming_calls_result(calls: &[IncomingCall], cwd: Option<&str>) -> String {
    if calls.is_empty() {
        return "No incoming calls found (nothing calls this function)".to_string();
    }
    let mut by_file: HashMap<String, Vec<&IncomingCall>> = HashMap::new();
    for call in calls {
        let fp = format_uri(&call.from.uri, cwd);
        by_file.entry(fp).or_default().push(call);
    }
    let mut lines = vec![format!(
        "Found {} incoming {}:",
        calls.len(),
        plural(calls.len(), "call")
    )];
    for (file_path, file_calls) in &by_file {
        lines.push(format!("\n{}:", file_path));
        for call in file_calls {
            let kind = symbol_kind_to_string(call.from.kind);
            let line = call.from.range.start.line + 1;
            let mut call_line = format!("  {} ({}) - Line {}", call.from.name, kind, line);
            if !call.from_ranges.is_empty() {
                let sites: Vec<String> = call
                    .from_ranges
                    .iter()
                    .map(|r| format!("{}:{}", r.start.line + 1, r.start.character + 1))
                    .collect();
                call_line.push_str(&format!(" [calls at: {}]", sites.join(", ")));
            }
            lines.push(call_line);
        }
    }
    lines.join("\n")
}

/// Format outgoing-calls result.
pub fn format_outgoing_calls_result(calls: &[OutgoingCall], cwd: Option<&str>) -> String {
    if calls.is_empty() {
        return "No outgoing calls found (this function calls nothing)".to_string();
    }
    let mut by_file: HashMap<String, Vec<&OutgoingCall>> = HashMap::new();
    for call in calls {
        let fp = format_uri(&call.to.uri, cwd);
        by_file.entry(fp).or_default().push(call);
    }
    let mut lines = vec![format!(
        "Found {} outgoing {}:",
        calls.len(),
        plural(calls.len(), "call")
    )];
    for (file_path, file_calls) in &by_file {
        lines.push(format!("\n{}:", file_path));
        for call in file_calls {
            let kind = symbol_kind_to_string(call.to.kind);
            let line = call.to.range.start.line + 1;
            let mut call_line = format!("  {} ({}) - Line {}", call.to.name, kind, line);
            if !call.from_ranges.is_empty() {
                let sites: Vec<String> = call
                    .from_ranges
                    .iter()
                    .map(|r| format!("{}:{}", r.start.line + 1, r.start.character + 1))
                    .collect();
                call_line.push_str(&format!(" [called from: {}]", sites.join(", ")));
            }
            lines.push(call_line);
        }
    }
    lines.join("\n")
}

/// Helper: pluralize a word.
fn plural(count: usize, word: &str) -> String {
    if count == 1 {
        word.to_string()
    } else {
        format!("{}s", word)
    }
}
