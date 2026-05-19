//! Dom (dom.ts) — a lightweight virtual-DOM mirror.
//!
//! The TypeScript version is built on top of Yoga layout nodes and React
//! reconciliation. In Rust we keep an arena-based tree that exposes the same
//! public API: createNode/appendChildNode/insertBeforeNode/etc.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Names of element nodes that can appear in the Ink tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementNames {
    InkRoot,
    InkBox,
    InkText,
    InkVirtualText,
    InkLink,
    InkProgress,
    InkRawAnsi,
}

impl ElementNames {
    pub fn as_str(self) -> &'static str {
        match self {
            ElementNames::InkRoot => "ink-root",
            ElementNames::InkBox => "ink-box",
            ElementNames::InkText => "ink-text",
            ElementNames::InkVirtualText => "ink-virtual-text",
            ElementNames::InkLink => "ink-link",
            ElementNames::InkProgress => "ink-progress",
            ElementNames::InkRawAnsi => "ink-raw-ansi",
        }
    }
}

/// Text-node name marker.
pub type TextName = &'static str;
pub const TEXT_NAME: &str = "#text";

/// Node names cover both elements and the text-name sentinel.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeNames {
    Element(ElementNames),
    Text,
}

/// Attribute values supported on DOM elements.
#[derive(Debug, Clone, PartialEq)]
pub enum DOMNodeAttribute {
    Bool(bool),
    Str(String),
    Num(f64),
}

impl DOMNodeAttribute {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            DOMNodeAttribute::Num(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            DOMNodeAttribute::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            DOMNodeAttribute::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// Styles applied to a DOM node (kept as a generic attribute map).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Styles {
    pub props: HashMap<String, DOMNodeAttribute>,
}

impl Styles {
    pub fn position(&self) -> Option<&str> {
        self.props.get("position").and_then(|v| v.as_str())
    }
}

/// Text styling separated from layout styles (mirrors TextStyles in TS).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextStyles {
    pub props: HashMap<String, DOMNodeAttribute>,
}

/// Identifier for an arena-managed DOM node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

/// A DOM element node.
#[derive(Debug, Clone)]
pub struct DOMElement {
    pub id: NodeId,
    pub node_name: ElementNames,
    pub attributes: HashMap<String, DOMNodeAttribute>,
    pub child_nodes: Vec<NodeId>,
    pub parent_node: Option<NodeId>,
    pub style: Styles,
    pub text_styles: Option<TextStyles>,
    pub dirty: bool,
    pub is_hidden: bool,
    pub has_rendered_content: bool,
    pub scroll_top: Option<f32>,
    pub pending_scroll_delta: Option<f32>,
    pub scroll_clamp_min: Option<f32>,
    pub scroll_clamp_max: Option<f32>,
    pub scroll_height: Option<f32>,
    pub scroll_viewport_height: Option<f32>,
    pub scroll_viewport_top: Option<f32>,
    pub sticky_scroll: bool,
    pub debug_owner_chain: Option<Vec<String>>,
    pub event_handlers: HashMap<String, String>,
    pub yoga_index: Option<u32>,
}

/// A DOM text node.
#[derive(Debug, Clone)]
pub struct TextNode {
    pub id: NodeId,
    pub node_value: String,
    pub parent_node: Option<NodeId>,
    pub style: Styles,
}

/// Polymorphic DOM node — element or text.
#[derive(Debug, Clone)]
pub enum DOMNode {
    Element(DOMElement),
    Text(TextNode),
}

impl DOMNode {
    pub fn id(&self) -> NodeId {
        match self {
            DOMNode::Element(e) => e.id,
            DOMNode::Text(t) => t.id,
        }
    }
    pub fn parent_node(&self) -> Option<NodeId> {
        match self {
            DOMNode::Element(e) => e.parent_node,
            DOMNode::Text(t) => t.parent_node,
        }
    }
    pub fn set_parent(&mut self, parent: Option<NodeId>) {
        match self {
            DOMNode::Element(e) => e.parent_node = parent,
            DOMNode::Text(t) => t.parent_node = parent,
        }
    }
    pub fn is_text(&self) -> bool { matches!(self, DOMNode::Text(_)) }
    pub fn is_element(&self) -> bool { matches!(self, DOMNode::Element(_)) }
}

/// Arena that owns all DOM nodes. Cheap to clone (Arc-wrapped).
#[derive(Debug, Default, Clone)]
pub struct DomArena {
    pub nodes: Vec<Option<DOMNode>>,
    pub next_id: u32,
}

impl DomArena {
    pub fn new() -> Self { Self { nodes: Vec::new(), next_id: 0 } }
    pub fn get(&self, id: NodeId) -> Option<&DOMNode> {
        self.nodes.get(id.0 as usize).and_then(|s| s.as_ref())
    }
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut DOMNode> {
        self.nodes.get_mut(id.0 as usize).and_then(|s| s.as_mut())
    }
    pub fn get_element(&self, id: NodeId) -> Option<&DOMElement> {
        match self.get(id)? {
            DOMNode::Element(e) => Some(e),
            _ => None,
        }
    }
    pub fn get_element_mut(&mut self, id: NodeId) -> Option<&mut DOMElement> {
        match self.get_mut(id)? {
            DOMNode::Element(e) => Some(e),
            _ => None,
        }
    }
    fn alloc(&mut self, node: DOMNode) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        self.nodes.push(Some(node));
        id
    }
    fn free(&mut self, id: NodeId) {
        if let Some(slot) = self.nodes.get_mut(id.0 as usize) {
            *slot = None;
        }
    }
}

/// Thread-safe shared arena handle for use across functions.
pub type SharedArena = Arc<Mutex<DomArena>>;

pub fn new_shared_arena() -> SharedArena {
    Arc::new(Mutex::new(DomArena::new()))
}

/// Create an empty element node in `arena` and return its id.
pub fn create_node(arena: &mut DomArena, node_name: ElementNames) -> NodeId {
    let id = NodeId(arena.next_id);
    let elem = DOMElement {
        id,
        node_name,
        attributes: HashMap::new(),
        child_nodes: Vec::new(),
        parent_node: None,
        style: Styles::default(),
        text_styles: None,
        dirty: false,
        is_hidden: false,
        has_rendered_content: false,
        scroll_top: None,
        pending_scroll_delta: None,
        scroll_clamp_min: None,
        scroll_clamp_max: None,
        scroll_height: None,
        scroll_viewport_height: None,
        scroll_viewport_top: None,
        sticky_scroll: false,
        debug_owner_chain: None,
        event_handlers: HashMap::new(),
        yoga_index: None,
    };
    arena.alloc(DOMNode::Element(elem))
}

/// Append `child_id` to `parent_id`. Detaches from any previous parent.
pub fn append_child_node(arena: &mut DomArena, parent_id: NodeId, child_id: NodeId) {
    if let Some(prev_parent) = arena.get(child_id).and_then(|c| c.parent_node()) {
        remove_child_node(arena, prev_parent, child_id);
    }
    if let Some(child) = arena.get_mut(child_id) {
        child.set_parent(Some(parent_id));
    }
    if let Some(p) = arena.get_element_mut(parent_id) {
        p.child_nodes.push(child_id);
    }
    mark_dirty(arena, Some(parent_id));
}

/// Insert `new_child` before `before` under `parent`.
pub fn insert_before_node(
    arena: &mut DomArena,
    parent_id: NodeId,
    new_child_id: NodeId,
    before_id: NodeId,
) {
    if let Some(prev_parent) = arena.get(new_child_id).and_then(|c| c.parent_node()) {
        remove_child_node(arena, prev_parent, new_child_id);
    }
    if let Some(child) = arena.get_mut(new_child_id) {
        child.set_parent(Some(parent_id));
    }
    let index = arena
        .get_element(parent_id)
        .and_then(|p| p.child_nodes.iter().position(|&id| id == before_id));
    match index {
        Some(i) => {
            if let Some(p) = arena.get_element_mut(parent_id) {
                p.child_nodes.insert(i, new_child_id);
            }
        }
        None => {
            if let Some(p) = arena.get_element_mut(parent_id) {
                p.child_nodes.push(new_child_id);
            }
        }
    }
    mark_dirty(arena, Some(parent_id));
}

/// Remove `child_id` from `parent_id`. Frees the child slot afterward.
pub fn remove_child_node(arena: &mut DomArena, parent_id: NodeId, child_id: NodeId) {
    if let Some(p) = arena.get_element_mut(parent_id) {
        if let Some(pos) = p.child_nodes.iter().position(|&id| id == child_id) {
            p.child_nodes.remove(pos);
        }
    }
    if let Some(child) = arena.get_mut(child_id) {
        child.set_parent(None);
    }
    crate::ink::node_cache::add_pending_clear(parent_id, child_id);
    mark_dirty(arena, Some(parent_id));
}

/// Update an attribute on an element.
pub fn set_attribute(
    arena: &mut DomArena,
    node_id: NodeId,
    key: &str,
    value: DOMNodeAttribute,
) {
    if key == "children" {
        return;
    }
    let mut changed = false;
    if let Some(e) = arena.get_element_mut(node_id) {
        let existing = e.attributes.get(key);
        if existing != Some(&value) {
            e.attributes.insert(key.to_string(), value);
            changed = true;
        }
    }
    if changed {
        mark_dirty(arena, Some(node_id));
    }
}

/// Replace the style block on a node if it differs.
pub fn set_style(arena: &mut DomArena, node_id: NodeId, style: Styles) {
    let changed = match arena.get(node_id) {
        Some(DOMNode::Element(e)) => e.style != style,
        Some(DOMNode::Text(t)) => t.style != style,
        None => false,
    };
    if !changed { return; }
    match arena.get_mut(node_id) {
        Some(DOMNode::Element(e)) => e.style = style,
        Some(DOMNode::Text(t)) => t.style = style,
        None => {}
    }
    mark_dirty(arena, Some(node_id));
}

/// Replace the text styles on an element if they differ.
pub fn set_text_styles(arena: &mut DomArena, node_id: NodeId, text_styles: TextStyles) {
    let same = arena
        .get_element(node_id)
        .map(|e| e.text_styles.as_ref() == Some(&text_styles))
        .unwrap_or(false);
    if same { return; }
    if let Some(e) = arena.get_element_mut(node_id) {
        e.text_styles = Some(text_styles);
    }
    mark_dirty(arena, Some(node_id));
}

/// Create a text node.
pub fn create_text_node(arena: &mut DomArena, text: impl Into<String>) -> NodeId {
    let id = NodeId(arena.next_id);
    let node = TextNode {
        id,
        node_value: text.into(),
        parent_node: None,
        style: Styles::default(),
    };
    arena.alloc(DOMNode::Text(node))
}

/// Update a text node's value.
pub fn set_text_node_value(arena: &mut DomArena, node_id: NodeId, text: &str) {
    let changed = match arena.get(node_id) {
        Some(DOMNode::Text(t)) => t.node_value != text,
        _ => false,
    };
    if !changed { return; }
    if let Some(DOMNode::Text(t)) = arena.get_mut(node_id) {
        t.node_value = text.to_string();
    }
    mark_dirty(arena, Some(node_id));
}

/// Mark a node and all its ancestors as dirty for re-rendering.
pub fn mark_dirty(arena: &mut DomArena, node_id: Option<NodeId>) {
    let mut current = node_id;
    while let Some(id) = current {
        let parent = match arena.get_mut(id) {
            Some(DOMNode::Element(e)) => {
                e.dirty = true;
                e.parent_node
            }
            Some(DOMNode::Text(t)) => t.parent_node,
            None => None,
        };
        current = parent;
    }
}

/// Walk to the root and run its `onRender` hook. We don't have closures in
/// nodes here — instead we expose a registry consumers can drive.
pub fn schedule_render_from(arena: &DomArena, node_id: Option<NodeId>) -> Option<NodeId> {
    let mut current = node_id?;
    loop {
        let parent = arena.get(current).and_then(|n| n.parent_node());
        match parent {
            Some(p) => current = p,
            None => break,
        }
    }
    Some(current)
}

/// Walk a subtree and forget every yoga node id reference.
pub fn clear_yoga_node_references(arena: &mut DomArena, node_id: NodeId) {
    let kids = arena.get_element(node_id).map(|e| e.child_nodes.clone()).unwrap_or_default();
    for c in kids {
        clear_yoga_node_references(arena, c);
    }
    if let Some(e) = arena.get_element_mut(node_id) {
        e.yoga_index = None;
    }
}

/// Find the deepest debug owner chain whose layout box contains row `y`.
pub fn find_owner_chain_at_row(arena: &DomArena, root: NodeId, y: i32) -> Vec<String> {
    let mut best: Vec<String> = Vec::new();
    walk(arena, root, 0, y, &mut best);
    return best;

    fn walk(arena: &DomArena, id: NodeId, offset_y: i32, target: i32, best: &mut Vec<String>) {
        let elem = match arena.get_element(id) {
            Some(e) => e,
            None => return,
        };
        let top = offset_y
            + elem
                .scroll_viewport_top
                .map(|v| v as i32)
                .unwrap_or(0);
        let height = elem.scroll_viewport_height.map(|v| v as i32).unwrap_or(0);
        // Without a true layout pass, we still walk children using ScrollBox
        // viewports as a proxy.
        if height > 0 && (target < top || target >= top + height) {
            return;
        }
        if let Some(chain) = &elem.debug_owner_chain {
            *best = chain.clone();
        }
        for &c in &elem.child_nodes {
            walk(arena, c, top, target, best);
        }
    }
}
