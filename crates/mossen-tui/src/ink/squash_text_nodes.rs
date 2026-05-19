//! Squash Text Nodes (squash-text-nodes.ts).

/// One styled segment after collapsing adjacent same-style text runs.
#[derive(Debug, Clone, Default)]
pub struct StyledSegment {
    pub text: String,
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl StyledSegment {
    pub fn same_style(&self, other: &StyledSegment) -> bool {
        self.fg == other.fg
            && self.bg == other.bg
            && self.bold == other.bold
            && self.italic == other.italic
            && self.underline == other.underline
    }
}

/// Collapse adjacent text nodes that share styling into single segments.
pub fn squash_text_nodes_to_segments(nodes: &[StyledSegment]) -> Vec<StyledSegment> {
    let mut out: Vec<StyledSegment> = Vec::with_capacity(nodes.len());
    for node in nodes {
        if let Some(prev) = out.last_mut() {
            if prev.same_style(node) {
                prev.text.push_str(&node.text);
                continue;
            }
        }
        out.push(node.clone());
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct SquashTextNodesState {
    pub initialized: bool,
}

impl SquashTextNodesState {
    pub fn new() -> Self { Self { initialized: false } }
    pub fn initialize(&mut self) { self.initialized = true; }
}
