//! Geometry types (geometry.ts).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }
impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self { Self { x, y, width: w, height: h } }
    pub fn contains(&self, px: f32, py: f32) -> bool { px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height }
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.width && self.x + self.width > other.x && self.y < other.y + other.height && self.y + self.height > other.y
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Edges { pub top: f32, pub right: f32, pub bottom: f32, pub left: f32 }
impl Edges {
    pub fn uniform(v: f32) -> Self { Self { top: v, right: v, bottom: v, left: v } }
    pub fn horizontal(&self) -> f32 { self.left + self.right }
    pub fn vertical(&self) -> f32 { self.top + self.bottom }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Size { pub width: f32, pub height: f32 }

/// Simple `Point { x, y }` like the TS `Point` type.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Point { pub x: f32, pub y: f32 }

/// Alias mirroring the TS `Rectangle` shape (Point + Size flattened).
pub type Rectangle = Rect;

/// Zero edges constant.
pub const ZERO_EDGES: Edges = Edges { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 };

/// Create uniform edges (single value applied to all sides). The TS
/// `edges(all)` overload.
pub fn edges_uniform(all: f32) -> Edges {
    Edges { top: all, right: all, bottom: all, left: all }
}

/// Create symmetric edges (vertical, horizontal). The TS `edges(v, h)` overload.
pub fn edges_symmetric(vertical: f32, horizontal: f32) -> Edges {
    Edges { top: vertical, right: horizontal, bottom: vertical, left: horizontal }
}

/// Create explicit per-side edges. The TS `edges(t, r, b, l)` overload.
pub fn edges(top: f32, right: f32, bottom: f32, left: f32) -> Edges {
    Edges { top, right, bottom, left }
}

/// Add two edge values component-wise.
pub fn add_edges(a: Edges, b: Edges) -> Edges {
    Edges {
        top: a.top + b.top,
        right: a.right + b.right,
        bottom: a.bottom + b.bottom,
        left: a.left + b.left,
    }
}

/// Resolve an optional partial edges struct into a full edges struct, where
/// every missing component defaults to 0.
pub fn resolve_edges(partial: Option<Edges>) -> Edges {
    partial.unwrap_or(ZERO_EDGES)
}

/// Smallest rectangle that contains both inputs.
pub fn union_rect(a: Rect, b: Rect) -> Rect {
    let min_x = a.x.min(b.x);
    let min_y = a.y.min(b.y);
    let max_x = (a.x + a.width).max(b.x + b.width);
    let max_y = (a.y + a.height).max(b.y + b.height);
    Rect { x: min_x, y: min_y, width: max_x - min_x, height: max_y - min_y }
}

/// Clamp a rectangle to fit inside a size, mirroring the TS clampRect.
pub fn clamp_rect(rect: Rect, size: Size) -> Rect {
    let min_x = 0.0_f32.max(rect.x);
    let min_y = 0.0_f32.max(rect.y);
    let max_x = (size.width - 1.0).min(rect.x + rect.width - 1.0);
    let max_y = (size.height - 1.0).min(rect.y + rect.height - 1.0);
    let w = (max_x - min_x + 1.0).max(0.0);
    let h = (max_y - min_y + 1.0).max(0.0);
    Rect { x: min_x, y: min_y, width: w, height: h }
}

/// Is `point` strictly inside `size`?
pub fn within_bounds(size: Size, point: Point) -> bool {
    point.x >= 0.0 && point.y >= 0.0 && point.x < size.width && point.y < size.height
}

/// Clamp a scalar between optional min and max (None = unbounded).
pub fn clamp(value: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let mut v = value;
    if let Some(m) = min { if v < m { v = m; } }
    if let Some(m) = max { if v > m { v = m; } }
    v
}
