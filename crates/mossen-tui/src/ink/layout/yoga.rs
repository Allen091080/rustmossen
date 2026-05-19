//! Yoga-like layout helpers (yoga.ts).
use super::geometry::Edges;
use super::node::{AlignItems, FlexDirection, FlexWrap, JustifyContent, LayoutNode};

/// Parse flex shorthand into grow/shrink/basis.
pub fn parse_flex(flex: f32) -> (f32, f32, f32) {
    if flex <= 0.0 { (0.0, 1.0, 0.0) } else { (flex, 1.0, 0.0) }
}

/// Apply gap between children.
pub fn apply_gap(nodes: &mut [LayoutNode], parent_id: usize, gap: f32) {
    let children: Vec<usize> = nodes.iter().find(|n| n.id == parent_id).map(|n| n.children.clone()).unwrap_or_default();
    if children.len() < 2 { return; }
    let direction = nodes.iter().find(|n| n.id == parent_id).map(|n| n.flex_direction).unwrap_or(FlexDirection::Row);
    let is_row = matches!(direction, FlexDirection::Row | FlexDirection::RowReverse);
    for (i, &child_id) in children.iter().enumerate().skip(1) {
        if let Some(child) = nodes.iter_mut().find(|n| n.id == child_id) {
            if is_row { child.rect.x += gap * i as f32; }
            else { child.rect.y += gap * i as f32; }
        }
    }
}

/// Set padding from style values.
pub fn edges_from_style(top: Option<f32>, right: Option<f32>, bottom: Option<f32>, left: Option<f32>, x: Option<f32>, y: Option<f32>, all: Option<f32>) -> Edges {
    let base = all.unwrap_or(0.0);
    Edges { top: top.or(y).unwrap_or(base), right: right.or(x).unwrap_or(base), bottom: bottom.or(y).unwrap_or(base), left: left.or(x).unwrap_or(base) }
}

/// Wrapper for a Yoga-style layout node (subset of full LayoutNode used at the
/// yoga API boundary).
#[derive(Debug, Clone)]
pub struct YogaLayoutNode {
    pub id: usize,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: f32,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub padding: Edges,
    pub margin: Edges,
    pub direction: FlexDirection,
    pub align_items: AlignItems,
    pub justify_content: JustifyContent,
    pub flex_wrap: FlexWrap,
}

impl YogaLayoutNode {
    /// Apply flex shorthand to this node.
    pub fn set_flex(&mut self, flex: f32) {
        let (g, s, b) = parse_flex(flex);
        self.flex_grow = g;
        self.flex_shrink = s;
        self.flex_basis = b;
    }
}

/// Create a fresh yoga layout node with the given id.
pub fn create_yoga_layout_node(id: usize) -> YogaLayoutNode {
    YogaLayoutNode {
        id,
        flex_grow: 0.0,
        flex_shrink: 1.0,
        flex_basis: 0.0,
        width: None,
        height: None,
        padding: Edges::default(),
        margin: Edges::default(),
        direction: FlexDirection::Row,
        align_items: AlignItems::FlexStart,
        justify_content: JustifyContent::FlexStart,
        flex_wrap: FlexWrap::NoWrap,
    }
}
