//! Layout node (node.ts).
use super::geometry::{Edges, Rect, Size};

/// Layout edge identifiers matching the TS `LayoutEdge` const object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutEdge {
    All, Horizontal, Vertical,
    Left, Right, Top, Bottom,
    Start, End,
}

/// Layout gutter directions for `setGap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutGutter { All, Column, Row }

/// Layout display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutDisplay { Flex, None }

/// Layout flex direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutFlexDirection { Row, RowReverse, Column, ColumnReverse }

/// Layout cross-axis alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutAlign { Auto, Stretch, FlexStart, Center, FlexEnd }

/// Layout main-axis justification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutJustify {
    FlexStart, Center, FlexEnd,
    SpaceBetween, SpaceAround, SpaceEvenly,
}

/// Layout wrap mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutWrap { NoWrap, Wrap, WrapReverse }

/// Layout position type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutPositionType { Relative, Absolute }

/// Layout overflow mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutOverflow { Visible, Hidden, Scroll }

/// Layout measure mode (Yoga semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMeasureMode { Undefined, Exactly, AtMost }

/// Result of a measure callback: width/height in cells.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutMeasureResult { pub width: f32, pub height: f32 }

/// Boxed measure callback used by the Yoga adapter.
pub type LayoutMeasureFunc = Box<dyn Fn(f32, LayoutMeasureMode) -> LayoutMeasureResult + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection { Row, Column, RowReverse, ColumnReverse }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexWrap { NoWrap, Wrap, WrapReverse }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems { Stretch, FlexStart, FlexEnd, Center, Baseline }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent { FlexStart, FlexEnd, Center, SpaceBetween, SpaceAround, SpaceEvenly }

#[derive(Debug, Clone)]
pub struct LayoutNode {
    pub id: usize,
    pub rect: Rect,
    pub content_size: Size,
    pub padding: Edges,
    pub margin: Edges,
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub align_items: AlignItems,
    pub justify_content: JustifyContent,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
}

impl LayoutNode {
    pub fn new(id: usize) -> Self {
        Self { id, rect: Rect::default(), content_size: Size::default(), padding: Edges::default(), margin: Edges::default(), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::NoWrap, flex_grow: 0.0, flex_shrink: 1.0, align_items: AlignItems::Stretch, justify_content: JustifyContent::FlexStart, children: Vec::new(), parent: None }
    }
    pub fn inner_rect(&self) -> Rect {
        Rect { x: self.rect.x + self.padding.left, y: self.rect.y + self.padding.top, width: self.rect.width - self.padding.horizontal(), height: self.rect.height - self.padding.vertical() }
    }
}
