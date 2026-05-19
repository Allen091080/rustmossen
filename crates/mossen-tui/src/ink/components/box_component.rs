//! Box component — flex container (Box.tsx).
use crate::ink::layout::{FlexDirection, FlexWrap, LayoutNode, Edges};

/// Box style properties.
#[derive(Debug, Clone)]
pub struct BoxStyle {
    pub flex_direction: FlexDirection,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_wrap: FlexWrap,
    pub padding: Edges,
    pub margin: Edges,
    pub gap: f32,
    pub column_gap: f32,
    pub row_gap: f32,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub min_width: Option<u16>,
    pub min_height: Option<u16>,
    pub border_style: Option<BorderStyle>,
    pub border_color: Option<String>,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle { Single, Double, Round, Bold, SingleDouble, DoubleSingle, Classic }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow { Visible, Hidden }

impl Default for BoxStyle {
    fn default() -> Self {
        Self {
            flex_direction: FlexDirection::Row, flex_grow: 0.0, flex_shrink: 1.0, flex_wrap: FlexWrap::NoWrap,
            padding: Edges::default(), margin: Edges::default(), gap: 0.0, column_gap: 0.0, row_gap: 0.0,
            width: None, height: None, min_width: None, min_height: None,
            border_style: None, border_color: None, overflow_x: Overflow::Visible, overflow_y: Overflow::Visible,
        }
    }
}

/// State for a Box component instance.
#[derive(Debug, Clone)]
pub struct BoxComponentState {
    pub id: usize,
    pub style: BoxStyle,
    pub tab_index: Option<i32>,
    pub auto_focus: bool,
    pub children: Vec<usize>,
}

impl BoxComponentState {
    pub fn new(id: usize) -> Self {
        Self { id, style: BoxStyle::default(), tab_index: None, auto_focus: false, children: Vec::new() }
    }
    pub fn set_style(&mut self, style: BoxStyle) { self.style = style; }
    pub fn add_child(&mut self, child_id: usize) { self.children.push(child_id); }
    pub fn is_focusable(&self) -> bool { self.tab_index.map_or(false, |i| i >= 0) }
}

/// TS `Box` exports `type Props`. Alias to the existing component state shape.
pub type Props = BoxComponentState;
