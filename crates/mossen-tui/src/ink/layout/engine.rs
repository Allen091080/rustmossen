//! Layout engine (engine.ts) — computes flexbox layout.
use super::node::LayoutNode;
use super::geometry::Size;

/// Build a fresh layout node with the given id (matches the TS factory).
pub fn create_layout_node(id: usize) -> LayoutNode {
    LayoutNode::new(id)
}

/// Equal-distribution flex layout for the unused `ink::layout` parallel
/// engine. Currently has **no callers** — the production render path uses
/// ratatui's `Layout`/`Constraint` system directly. Kept as scaffolding for
/// any future port of Ink's full Yoga-based flex algorithm; until that
/// lands, the engine just splits inner space equally across children, which
/// matches the simplest possible flexbox behaviour (no grow/shrink, no
/// wrap, no main-axis distribution). Calling this in production would
/// produce visibly wrong layouts vs the React/Ink reference.
pub fn compute_layout(nodes: &mut [LayoutNode], root_id: usize, available: Size) {
    if let Some(root) = nodes.iter_mut().find(|n| n.id == root_id) {
        root.rect.width = available.width;
        root.rect.height = available.height;
    }
    layout_children(nodes, root_id);
}

fn layout_children(nodes: &mut [LayoutNode], parent_id: usize) {
    let (direction, inner_width, inner_height, children) = {
        let parent = nodes.iter().find(|n| n.id == parent_id).unwrap();
        let inner = parent.inner_rect();
        (parent.flex_direction, inner.width, inner.height, parent.children.clone())
    };
    if children.is_empty() { return; }
    let count = children.len() as f32;
    let is_row = matches!(direction, super::node::FlexDirection::Row | super::node::FlexDirection::RowReverse);
    let item_w = if is_row { inner_width / count } else { inner_width };
    let item_h = if is_row { inner_height } else { inner_height / count };
    for (i, &child_id) in children.iter().enumerate() {
        if let Some(child) = nodes.iter_mut().find(|n| n.id == child_id) {
            if is_row { child.rect.x = i as f32 * item_w; child.rect.y = 0.0; }
            else { child.rect.x = 0.0; child.rect.y = i as f32 * item_h; }
            child.rect.width = item_w; child.rect.height = item_h;
        }
        layout_children(nodes, child_id);
    }
}
