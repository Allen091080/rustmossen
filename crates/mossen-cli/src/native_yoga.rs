//! Pure-Rust port of yoga-layout (Meta's flexbox engine).
//!
//! Matches the `yoga-layout/load` API surface used by ink layout.
//! Covers the subset of flexbox features Ink uses:
//!   - flex-direction (row/column + reverse)
//!   - flex-grow / flex-shrink / flex-basis
//!   - align-items / align-self (stretch, flex-start, center, flex-end, baseline)
//!   - justify-content (all six values)
//!   - margin / padding / border / gap
//!   - width / height / min / max (point, percent, auto)
//!   - position: relative / absolute
//!   - display: flex / none / contents
//!   - measure functions (for text nodes)
//!   - flex-wrap: wrap / wrap-reverse (multi-line flex)
//!   - align-content, baseline alignment
//!
//! Upstream: <https://github.com/facebook/yoga>

#![allow(
    clippy::float_cmp,
    clippy::many_single_char_names,
    clippy::too_many_arguments,
    clippy::needless_range_loop
)]

use std::cell::Cell;
use std::sync::atomic::{AtomicI64, Ordering};

// ---------------------------------------------------------------------------
// Enums (from enums.ts)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Align {
    Auto = 0,
    FlexStart = 1,
    Center = 2,
    FlexEnd = 3,
    Stretch = 4,
    Baseline = 5,
    SpaceBetween = 6,
    SpaceAround = 7,
    SpaceEvenly = 8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BoxSizing {
    BorderBox = 0,
    ContentBox = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Dimension {
    Width = 0,
    Height = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Direction {
    Inherit = 0,
    Ltr = 1,
    Rtl = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Display {
    Flex = 0,
    None = 1,
    Contents = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Edge {
    Left = 0,
    Top = 1,
    Right = 2,
    Bottom = 3,
    Start = 4,
    End = 5,
    Horizontal = 6,
    Vertical = 7,
    All = 8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Errata {
    None = 0,
    StretchFlexBasis = 1,
    AbsolutePositionWithoutInsetsExcludesPadding = 2,
    AbsolutePercentAgainstInnerSize = 4,
    All = 2_147_483_647,
    Classic = 2_147_483_646,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ExperimentalFeature {
    WebFlexBasis = 0,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlexDirection {
    Column = 0,
    ColumnReverse = 1,
    Row = 2,
    RowReverse = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Gutter {
    Column = 0,
    Row = 1,
    All = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Justify {
    FlexStart = 0,
    Center = 1,
    FlexEnd = 2,
    SpaceBetween = 3,
    SpaceAround = 4,
    SpaceEvenly = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MeasureMode {
    Undefined = 0,
    Exactly = 1,
    AtMost = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Overflow {
    Visible = 0,
    Hidden = 1,
    Scroll = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PositionType {
    Static = 0,
    Relative = 1,
    Absolute = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Unit {
    Undefined = 0,
    Point = 1,
    Percent = 2,
    Auto = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Wrap {
    NoWrap = 0,
    Wrap = 1,
    WrapReverse = 2,
}

// ---------------------------------------------------------------------------
// Value types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct Value {
    pub unit: Unit,
    pub value: f64,
}

const UNDEFINED_VALUE: Value = Value {
    unit: Unit::Undefined,
    value: f64::NAN,
};
const AUTO_VALUE: Value = Value {
    unit: Unit::Auto,
    value: f64::NAN,
};

fn point_value(v: f64) -> Value {
    Value {
        unit: Unit::Point,
        value: v,
    }
}

fn percent_value(v: f64) -> Value {
    Value {
        unit: Unit::Percent,
        value: v,
    }
}

fn resolve_value(v: &Value, owner_size: f64) -> f64 {
    match v.unit {
        Unit::Point => v.value,
        Unit::Percent => {
            if owner_size.is_nan() {
                f64::NAN
            } else {
                (v.value * owner_size) / 100.0
            }
        }
        _ => f64::NAN,
    }
}

fn is_defined(n: f64) -> bool {
    !n.is_nan()
}

/// NaN-safe equality for layout-cache input comparison.
fn same_float(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

// ---------------------------------------------------------------------------
// Layout result (computed values)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Layout {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    /// Computed per-edge values (left, top, right, bottom)
    pub border: [f64; 4],
    pub padding: [f64; 4],
    pub margin: [f64; 4],
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            width: 0.0,
            height: 0.0,
            border: [0.0; 4],
            padding: [0.0; 4],
            margin: [0.0; 4],
        }
    }
}

// ---------------------------------------------------------------------------
// Style (input values)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Style {
    pub direction: Direction,
    pub flex_direction: FlexDirection,
    pub justify_content: Justify,
    pub align_items: Align,
    pub align_self: Align,
    pub align_content: Align,
    pub flex_wrap: Wrap,
    pub overflow: Overflow,
    pub display: Display,
    pub position_type: PositionType,
    pub flex_grow: f64,
    pub flex_shrink: f64,
    pub flex_basis: Value,
    /// 9-edge arrays indexed by Edge enum
    pub margin: [Value; 9],
    pub padding: [Value; 9],
    pub border: [Value; 9],
    pub position: [Value; 9],
    /// 3-gutter array indexed by Gutter enum
    pub gap: [Value; 3],
    pub width: Value,
    pub height: Value,
    pub min_width: Value,
    pub min_height: Value,
    pub max_width: Value,
    pub max_height: Value,
}

fn default_style() -> Style {
    Style {
        direction: Direction::Inherit,
        flex_direction: FlexDirection::Column,
        justify_content: Justify::FlexStart,
        align_items: Align::Stretch,
        align_self: Align::Auto,
        align_content: Align::FlexStart,
        flex_wrap: Wrap::NoWrap,
        overflow: Overflow::Visible,
        display: Display::Flex,
        position_type: PositionType::Relative,
        flex_grow: 0.0,
        flex_shrink: 0.0,
        flex_basis: AUTO_VALUE,
        margin: [UNDEFINED_VALUE; 9],
        padding: [UNDEFINED_VALUE; 9],
        border: [UNDEFINED_VALUE; 9],
        position: [UNDEFINED_VALUE; 9],
        gap: [UNDEFINED_VALUE; 3],
        width: AUTO_VALUE,
        height: AUTO_VALUE,
        min_width: UNDEFINED_VALUE,
        min_height: UNDEFINED_VALUE,
        max_width: UNDEFINED_VALUE,
        max_height: UNDEFINED_VALUE,
    }
}

// ---------------------------------------------------------------------------
// Edge resolution — yoga's 9-edge model collapsed to 4 physical edges
// ---------------------------------------------------------------------------

const EDGE_LEFT: usize = 0;
const EDGE_TOP: usize = 1;
const EDGE_RIGHT: usize = 2;
const EDGE_BOTTOM: usize = 3;

fn resolve_edge(edges: &[Value; 9], physical_edge: usize, owner_size: f64, allow_auto: bool) -> f64 {
    let mut v = edges[physical_edge];
    if matches!(v.unit, Unit::Undefined) {
        v = if physical_edge == EDGE_LEFT || physical_edge == EDGE_RIGHT {
            edges[Edge::Horizontal as usize]
        } else {
            edges[Edge::Vertical as usize]
        };
    }
    if matches!(v.unit, Unit::Undefined) {
        v = edges[Edge::All as usize];
    }
    if matches!(v.unit, Unit::Undefined) {
        if physical_edge == EDGE_LEFT {
            v = edges[Edge::Start as usize];
        }
        if physical_edge == EDGE_RIGHT {
            v = edges[Edge::End as usize];
        }
    }
    if matches!(v.unit, Unit::Undefined) {
        return 0.0;
    }
    if matches!(v.unit, Unit::Auto) {
        return if allow_auto { f64::NAN } else { 0.0 };
    }
    resolve_value(&v, owner_size)
}

fn resolve_edge_no_auto(edges: &[Value; 9], physical_edge: usize, owner_size: f64) -> f64 {
    resolve_edge(edges, physical_edge, owner_size, false)
}

fn resolve_edge_raw(edges: &[Value; 9], physical_edge: usize) -> Value {
    let mut v = edges[physical_edge];
    if matches!(v.unit, Unit::Undefined) {
        v = if physical_edge == EDGE_LEFT || physical_edge == EDGE_RIGHT {
            edges[Edge::Horizontal as usize]
        } else {
            edges[Edge::Vertical as usize]
        };
    }
    if matches!(v.unit, Unit::Undefined) {
        v = edges[Edge::All as usize];
    }
    if matches!(v.unit, Unit::Undefined) {
        if physical_edge == EDGE_LEFT {
            v = edges[Edge::Start as usize];
        }
        if physical_edge == EDGE_RIGHT {
            v = edges[Edge::End as usize];
        }
    }
    v
}

fn is_margin_auto(edges: &[Value; 9], physical_edge: usize) -> bool {
    matches!(resolve_edge_raw(edges, physical_edge).unit, Unit::Auto)
}

fn has_any_auto_edge(edges: &[Value; 9]) -> bool {
    edges.iter().any(|e| matches!(e.unit, Unit::Auto))
}

fn has_any_defined_edge(edges: &[Value; 9]) -> bool {
    edges.iter().any(|e| !matches!(e.unit, Unit::Undefined))
}

/// Hot path: resolve all 4 physical edges in one pass.
fn resolve_edges4_into(edges: &[Value; 9], owner_size: f64, out: &mut [f64; 4]) {
    let e_h = edges[6]; // Edge::Horizontal
    let e_v = edges[7]; // Edge::Vertical
    let e_a = edges[8]; // Edge::All
    let e_s = edges[4]; // Edge::Start
    let e_e = edges[5]; // Edge::End
    let pct_denom = if owner_size.is_nan() {
        f64::NAN
    } else {
        owner_size / 100.0
    };

    // Left: edges[0] → Horizontal → All → Start
    let mut v = edges[0];
    if v.unit as u8 == 0 { v = e_h; }
    if v.unit as u8 == 0 { v = e_a; }
    if v.unit as u8 == 0 { v = e_s; }
    out[0] = if v.unit as u8 == 1 {
        v.value
    } else if v.unit as u8 == 2 {
        v.value * pct_denom
    } else {
        0.0
    };

    // Top: edges[1] → Vertical → All
    v = edges[1];
    if v.unit as u8 == 0 { v = e_v; }
    if v.unit as u8 == 0 { v = e_a; }
    out[1] = if v.unit as u8 == 1 {
        v.value
    } else if v.unit as u8 == 2 {
        v.value * pct_denom
    } else {
        0.0
    };

    // Right: edges[2] → Horizontal → All → End
    v = edges[2];
    if v.unit as u8 == 0 { v = e_h; }
    if v.unit as u8 == 0 { v = e_a; }
    if v.unit as u8 == 0 { v = e_e; }
    out[2] = if v.unit as u8 == 1 {
        v.value
    } else if v.unit as u8 == 2 {
        v.value * pct_denom
    } else {
        0.0
    };

    // Bottom: edges[3] → Vertical → All
    v = edges[3];
    if v.unit as u8 == 0 { v = e_v; }
    if v.unit as u8 == 0 { v = e_a; }
    out[3] = if v.unit as u8 == 1 {
        v.value
    } else if v.unit as u8 == 2 {
        v.value * pct_denom
    } else {
        0.0
    };
}

// ---------------------------------------------------------------------------
// Axis helpers
// ---------------------------------------------------------------------------

fn is_row(dir: FlexDirection) -> bool {
    matches!(dir, FlexDirection::Row | FlexDirection::RowReverse)
}

fn is_reverse(dir: FlexDirection) -> bool {
    matches!(dir, FlexDirection::RowReverse | FlexDirection::ColumnReverse)
}

fn cross_axis(dir: FlexDirection) -> FlexDirection {
    if is_row(dir) {
        FlexDirection::Column
    } else {
        FlexDirection::Row
    }
}

fn leading_edge(dir: FlexDirection) -> usize {
    match dir {
        FlexDirection::Row => EDGE_LEFT,
        FlexDirection::RowReverse => EDGE_RIGHT,
        FlexDirection::Column => EDGE_TOP,
        FlexDirection::ColumnReverse => EDGE_BOTTOM,
    }
}

fn trailing_edge(dir: FlexDirection) -> usize {
    match dir {
        FlexDirection::Row => EDGE_RIGHT,
        FlexDirection::RowReverse => EDGE_LEFT,
        FlexDirection::Column => EDGE_BOTTOM,
        FlexDirection::ColumnReverse => EDGE_TOP,
    }
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Measure function type: (width, widthMode, height, heightMode) -> Size
pub type MeasureFunction = Box<dyn Fn(f64, MeasureMode, f64, MeasureMode) -> Size>;

#[derive(Debug, Clone, Copy)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

pub struct Config {
    pub point_scale_factor: f64,
    pub errata: Errata,
    pub use_web_defaults: bool,
}

impl Config {
    pub fn create() -> Self {
        Self {
            point_scale_factor: 1.0,
            errata: Errata::None,
            use_web_defaults: false,
        }
    }

    pub fn free(&self) {}

    pub fn is_experimental_feature_enabled(&self, _feature: ExperimentalFeature) -> bool {
        false
    }

    pub fn set_experimental_feature_enabled(
        &mut self,
        _feature: ExperimentalFeature,
        _enabled: bool,
    ) {
    }

    pub fn set_point_scale_factor(&mut self, factor: f64) {
        self.point_scale_factor = factor;
    }

    pub fn get_errata(&self) -> Errata {
        self.errata
    }

    pub fn set_errata(&mut self, errata: Errata) {
        self.errata = errata;
    }

    pub fn set_use_web_defaults(&mut self, v: bool) {
        self.use_web_defaults = v;
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::create()
    }
}

// ---------------------------------------------------------------------------
// Profiling counters
// ---------------------------------------------------------------------------

thread_local! {
    static GENERATION: Cell<i64> = const { Cell::new(0) };
    static YOGA_NODES_VISITED: Cell<u64> = const { Cell::new(0) };
    static YOGA_MEASURE_CALLS: Cell<u64> = const { Cell::new(0) };
    static YOGA_CACHE_HITS: Cell<u64> = const { Cell::new(0) };
}

static YOGA_LIVE_NODES: AtomicI64 = AtomicI64::new(0);

#[derive(Debug, Clone, Copy)]
pub struct YogaCounters {
    pub visited: u64,
    pub measured: u64,
    pub cache_hits: u64,
    pub live: i64,
}

pub fn get_yoga_counters() -> YogaCounters {
    YogaCounters {
        visited: YOGA_NODES_VISITED.with(|c| c.get()),
        measured: YOGA_MEASURE_CALLS.with(|c| c.get()),
        cache_hits: YOGA_CACHE_HITS.with(|c| c.get()),
        live: YOGA_LIVE_NODES.load(Ordering::Relaxed),
    }
}

const CACHE_SLOTS: usize = 4;

// ---------------------------------------------------------------------------
// Node implementation
// ---------------------------------------------------------------------------

pub struct Node {
    pub style: Style,
    pub layout: Layout,
    pub parent: Option<*mut Node>,
    pub children: Vec<*mut Node>,
    pub measure_func: Option<MeasureFunction>,
    pub config_psf: f64,
    pub is_dirty: bool,
    pub is_reference_baseline: bool,

    // Per-layout scratch
    _flex_basis: f64,
    _main_size: f64,
    _cross_size: f64,
    _line_index: usize,

    // Fast-path flags
    _has_auto_margin: bool,
    _has_position: bool,
    _has_padding: bool,
    _has_border: bool,
    _has_margin: bool,

    // Dirty-flag layout cache (2-slot)
    _l_w: f64,
    _l_h: f64,
    _l_wm: MeasureMode,
    _l_hm: MeasureMode,
    _l_ow: f64,
    _l_oh: f64,
    _l_fw: bool,
    _l_fh: bool,
    _l_out_w: f64,
    _l_out_h: f64,
    _has_l: bool,

    _m_w: f64,
    _m_h: f64,
    _m_wm: MeasureMode,
    _m_hm: MeasureMode,
    _m_ow: f64,
    _m_oh: f64,
    _m_out_w: f64,
    _m_out_h: f64,
    _has_m: bool,

    // Cached computeFlexBasis result
    _fb_basis: f64,
    _fb_owner_w: f64,
    _fb_owner_h: f64,
    _fb_avail_main: f64,
    _fb_avail_cross: f64,
    _fb_cross_mode: MeasureMode,
    _fb_gen: i64,

    // Multi-entry layout cache
    _c_in: Option<Vec<f64>>,
    _c_out: Option<Vec<f64>>,
    _c_gen: i64,
    _c_n: usize,
    _c_wr: usize,
}

impl Node {
    pub fn create(config: Option<&Config>) -> Box<Self> {
        let psf = config.map_or(1.0, |c| c.point_scale_factor);
        YOGA_LIVE_NODES.fetch_add(1, Ordering::Relaxed);
        Box::new(Self {
            style: default_style(),
            layout: Layout::default(),
            parent: None,
            children: Vec::new(),
            measure_func: None,
            config_psf: psf,
            is_dirty: true,
            is_reference_baseline: false,
            _flex_basis: 0.0,
            _main_size: 0.0,
            _cross_size: 0.0,
            _line_index: 0,
            _has_auto_margin: false,
            _has_position: false,
            _has_padding: false,
            _has_border: false,
            _has_margin: false,
            _l_w: f64::NAN,
            _l_h: f64::NAN,
            _l_wm: MeasureMode::Undefined,
            _l_hm: MeasureMode::Undefined,
            _l_ow: f64::NAN,
            _l_oh: f64::NAN,
            _l_fw: false,
            _l_fh: false,
            _l_out_w: f64::NAN,
            _l_out_h: f64::NAN,
            _has_l: false,
            _m_w: f64::NAN,
            _m_h: f64::NAN,
            _m_wm: MeasureMode::Undefined,
            _m_hm: MeasureMode::Undefined,
            _m_ow: f64::NAN,
            _m_oh: f64::NAN,
            _m_out_w: f64::NAN,
            _m_out_h: f64::NAN,
            _has_m: false,
            _fb_basis: f64::NAN,
            _fb_owner_w: f64::NAN,
            _fb_owner_h: f64::NAN,
            _fb_avail_main: f64::NAN,
            _fb_avail_cross: f64::NAN,
            _fb_cross_mode: MeasureMode::Undefined,
            _fb_gen: -1,
            _c_in: None,
            _c_out: None,
            _c_gen: -1,
            _c_n: 0,
            _c_wr: 0,
        })
    }

    pub fn create_default() -> Box<Self> {
        Self::create(None)
    }

    // -- Tree operations

    pub fn insert_child(&mut self, child: &mut Node, index: usize) {
        child.parent = Some(self as *mut Node);
        let ptr = child as *mut Node;
        if index >= self.children.len() {
            self.children.push(ptr);
        } else {
            self.children.insert(index, ptr);
        }
        self.mark_dirty();
    }

    pub fn remove_child(&mut self, child: &mut Node) {
        let ptr = child as *mut Node;
        if let Some(idx) = self.children.iter().position(|&c| c == ptr) {
            self.children.remove(idx);
            child.parent = None;
            self.mark_dirty();
        }
    }

    pub fn get_child(&self, index: usize) -> Option<&Node> {
        self.children.get(index).map(|&ptr| unsafe { &*ptr })
    }

    pub fn get_child_mut(&mut self, index: usize) -> Option<&mut Node> {
        self.children.get(index).map(|&ptr| unsafe { &mut *ptr })
    }

    pub fn get_child_count(&self) -> usize {
        self.children.len()
    }

    pub fn get_parent(&self) -> Option<&Node> {
        self.parent.map(|ptr| unsafe { &*ptr })
    }

    // -- Lifecycle

    pub fn free(&mut self) {
        self.parent = None;
        self.children.clear();
        self.measure_func = None;
        self._c_in = None;
        self._c_out = None;
        YOGA_LIVE_NODES.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn free_recursive(&mut self) {
        let children: Vec<*mut Node> = self.children.clone();
        for &c in &children {
            unsafe { (*c).free_recursive() };
        }
        self.free();
    }

    pub fn reset(&mut self) {
        self.style = default_style();
        self.children.clear();
        self.parent = None;
        self.measure_func = None;
        self.is_dirty = true;
        self._has_auto_margin = false;
        self._has_position = false;
        self._has_padding = false;
        self._has_border = false;
        self._has_margin = false;
        self._has_l = false;
        self._has_m = false;
        self._c_n = 0;
        self._c_wr = 0;
        self._fb_basis = f64::NAN;
    }

    // -- Dirty tracking

    pub fn mark_dirty(&mut self) {
        self.is_dirty = true;
        if let Some(parent) = self.parent {
            let p = unsafe { &mut *parent };
            if !p.is_dirty {
                p.mark_dirty();
            }
        }
    }

    pub fn is_dirty_flag(&self) -> bool {
        self.is_dirty
    }

    pub fn has_new_layout(&self) -> bool {
        true
    }

    pub fn mark_layout_seen(&self) {}

    // -- Measure function

    pub fn set_measure_func(&mut self, f: MeasureFunction) {
        self.measure_func = Some(f);
        self.mark_dirty();
    }

    pub fn unset_measure_func(&mut self) {
        self.measure_func = None;
        self.mark_dirty();
    }

    // -- Computed layout getters

    pub fn get_computed_left(&self) -> f64 {
        self.layout.left
    }

    pub fn get_computed_top(&self) -> f64 {
        self.layout.top
    }

    pub fn get_computed_width(&self) -> f64 {
        self.layout.width
    }

    pub fn get_computed_height(&self) -> f64 {
        self.layout.height
    }

    pub fn get_computed_right(&self) -> f64 {
        if let Some(p) = self.parent {
            let p = unsafe { &*p };
            p.layout.width - self.layout.left - self.layout.width
        } else {
            0.0
        }
    }

    pub fn get_computed_bottom(&self) -> f64 {
        if let Some(p) = self.parent {
            let p = unsafe { &*p };
            p.layout.height - self.layout.top - self.layout.height
        } else {
            0.0
        }
    }

    pub fn get_computed_layout(&self) -> ComputedLayout {
        ComputedLayout {
            left: self.layout.left,
            top: self.layout.top,
            right: self.get_computed_right(),
            bottom: self.get_computed_bottom(),
            width: self.layout.width,
            height: self.layout.height,
        }
    }

    pub fn get_computed_border(&self, edge: Edge) -> f64 {
        self.layout.border[physical_edge(edge)]
    }

    pub fn get_computed_padding(&self, edge: Edge) -> f64 {
        self.layout.padding[physical_edge(edge)]
    }

    pub fn get_computed_margin(&self, edge: Edge) -> f64 {
        self.layout.margin[physical_edge(edge)]
    }

    // -- Style setters: dimensions

    pub fn set_width(&mut self, v: DimensionValue) {
        self.style.width = parse_dimension(v);
        self.mark_dirty();
    }

    pub fn set_width_percent(&mut self, v: f64) {
        self.style.width = percent_value(v);
        self.mark_dirty();
    }

    pub fn set_width_auto(&mut self) {
        self.style.width = AUTO_VALUE;
        self.mark_dirty();
    }

    pub fn set_height(&mut self, v: DimensionValue) {
        self.style.height = parse_dimension(v);
        self.mark_dirty();
    }

    pub fn set_height_percent(&mut self, v: f64) {
        self.style.height = percent_value(v);
        self.mark_dirty();
    }

    pub fn set_height_auto(&mut self) {
        self.style.height = AUTO_VALUE;
        self.mark_dirty();
    }

    pub fn set_min_width(&mut self, v: DimensionValue) {
        self.style.min_width = parse_dimension(v);
        self.mark_dirty();
    }

    pub fn set_min_width_percent(&mut self, v: f64) {
        self.style.min_width = percent_value(v);
        self.mark_dirty();
    }

    pub fn set_min_height(&mut self, v: DimensionValue) {
        self.style.min_height = parse_dimension(v);
        self.mark_dirty();
    }

    pub fn set_min_height_percent(&mut self, v: f64) {
        self.style.min_height = percent_value(v);
        self.mark_dirty();
    }

    pub fn set_max_width(&mut self, v: DimensionValue) {
        self.style.max_width = parse_dimension(v);
        self.mark_dirty();
    }

    pub fn set_max_width_percent(&mut self, v: f64) {
        self.style.max_width = percent_value(v);
        self.mark_dirty();
    }

    pub fn set_max_height(&mut self, v: DimensionValue) {
        self.style.max_height = parse_dimension(v);
        self.mark_dirty();
    }

    pub fn set_max_height_percent(&mut self, v: f64) {
        self.style.max_height = percent_value(v);
        self.mark_dirty();
    }

    // -- Style setters: flex

    pub fn set_flex_direction(&mut self, dir: FlexDirection) {
        self.style.flex_direction = dir;
        self.mark_dirty();
    }

    pub fn set_flex_grow(&mut self, v: Option<f64>) {
        self.style.flex_grow = v.unwrap_or(0.0);
        self.mark_dirty();
    }

    pub fn set_flex_shrink(&mut self, v: Option<f64>) {
        self.style.flex_shrink = v.unwrap_or(0.0);
        self.mark_dirty();
    }

    pub fn set_flex(&mut self, v: Option<f64>) {
        match v {
            None => {
                self.style.flex_grow = 0.0;
                self.style.flex_shrink = 0.0;
            }
            Some(val) if val.is_nan() => {
                self.style.flex_grow = 0.0;
                self.style.flex_shrink = 0.0;
            }
            Some(val) if val > 0.0 => {
                self.style.flex_grow = val;
                self.style.flex_shrink = 1.0;
                self.style.flex_basis = point_value(0.0);
            }
            Some(val) if val < 0.0 => {
                self.style.flex_grow = 0.0;
                self.style.flex_shrink = -val;
            }
            _ => {
                self.style.flex_grow = 0.0;
                self.style.flex_shrink = 0.0;
            }
        }
        self.mark_dirty();
    }

    pub fn set_flex_basis(&mut self, v: DimensionValue) {
        self.style.flex_basis = parse_dimension(v);
        self.mark_dirty();
    }

    pub fn set_flex_basis_percent(&mut self, v: f64) {
        self.style.flex_basis = percent_value(v);
        self.mark_dirty();
    }

    pub fn set_flex_basis_auto(&mut self) {
        self.style.flex_basis = AUTO_VALUE;
        self.mark_dirty();
    }

    pub fn set_flex_wrap(&mut self, wrap: Wrap) {
        self.style.flex_wrap = wrap;
        self.mark_dirty();
    }

    // -- Style setters: alignment

    pub fn set_align_items(&mut self, a: Align) {
        self.style.align_items = a;
        self.mark_dirty();
    }

    pub fn set_align_self(&mut self, a: Align) {
        self.style.align_self = a;
        self.mark_dirty();
    }

    pub fn set_align_content(&mut self, a: Align) {
        self.style.align_content = a;
        self.mark_dirty();
    }

    pub fn set_justify_content(&mut self, j: Justify) {
        self.style.justify_content = j;
        self.mark_dirty();
    }

    // -- Style setters: display / position / overflow

    pub fn set_display(&mut self, d: Display) {
        self.style.display = d;
        self.mark_dirty();
    }

    pub fn get_display(&self) -> Display {
        self.style.display
    }

    pub fn set_position_type(&mut self, t: PositionType) {
        self.style.position_type = t;
        self.mark_dirty();
    }

    pub fn set_position(&mut self, edge: Edge, v: DimensionValue) {
        self.style.position[edge as usize] = parse_dimension(v);
        self._has_position = has_any_defined_edge(&self.style.position);
        self.mark_dirty();
    }

    pub fn set_position_percent(&mut self, edge: Edge, v: f64) {
        self.style.position[edge as usize] = percent_value(v);
        self._has_position = true;
        self.mark_dirty();
    }

    pub fn set_position_auto(&mut self, edge: Edge) {
        self.style.position[edge as usize] = AUTO_VALUE;
        self._has_position = true;
        self.mark_dirty();
    }

    pub fn set_overflow(&mut self, o: Overflow) {
        self.style.overflow = o;
        self.mark_dirty();
    }

    pub fn set_direction(&mut self, d: Direction) {
        self.style.direction = d;
        self.mark_dirty();
    }

    pub fn set_box_sizing(&mut self, _bs: BoxSizing) {
        // Not implemented — Ink doesn't use content-box
    }

    // -- Style setters: spacing

    pub fn set_margin(&mut self, edge: Edge, v: DimensionValue) {
        let val = parse_dimension(v);
        self.style.margin[edge as usize] = val;
        if matches!(val.unit, Unit::Auto) {
            self._has_auto_margin = true;
        } else {
            self._has_auto_margin = has_any_auto_edge(&self.style.margin);
        }
        self._has_margin = self._has_auto_margin || has_any_defined_edge(&self.style.margin);
        self.mark_dirty();
    }

    pub fn set_margin_percent(&mut self, edge: Edge, v: f64) {
        self.style.margin[edge as usize] = percent_value(v);
        self._has_auto_margin = has_any_auto_edge(&self.style.margin);
        self._has_margin = true;
        self.mark_dirty();
    }

    pub fn set_margin_auto(&mut self, edge: Edge) {
        self.style.margin[edge as usize] = AUTO_VALUE;
        self._has_auto_margin = true;
        self._has_margin = true;
        self.mark_dirty();
    }

    pub fn set_padding(&mut self, edge: Edge, v: DimensionValue) {
        self.style.padding[edge as usize] = parse_dimension(v);
        self._has_padding = has_any_defined_edge(&self.style.padding);
        self.mark_dirty();
    }

    pub fn set_padding_percent(&mut self, edge: Edge, v: f64) {
        self.style.padding[edge as usize] = percent_value(v);
        self._has_padding = true;
        self.mark_dirty();
    }

    pub fn set_border_edge(&mut self, edge: Edge, v: Option<f64>) {
        self.style.border[edge as usize] = match v {
            Some(val) => point_value(val),
            None => UNDEFINED_VALUE,
        };
        self._has_border = has_any_defined_edge(&self.style.border);
        self.mark_dirty();
    }

    pub fn set_gap(&mut self, gutter: Gutter, v: DimensionValue) {
        self.style.gap[gutter as usize] = parse_dimension(v);
        self.mark_dirty();
    }

    pub fn set_gap_percent(&mut self, gutter: Gutter, v: f64) {
        self.style.gap[gutter as usize] = percent_value(v);
        self.mark_dirty();
    }

    // -- Style getters

    pub fn get_flex_direction(&self) -> FlexDirection {
        self.style.flex_direction
    }

    pub fn get_justify_content(&self) -> Justify {
        self.style.justify_content
    }

    pub fn get_align_items(&self) -> Align {
        self.style.align_items
    }

    pub fn get_align_self(&self) -> Align {
        self.style.align_self
    }

    pub fn get_align_content(&self) -> Align {
        self.style.align_content
    }

    pub fn get_flex_grow(&self) -> f64 {
        self.style.flex_grow
    }

    pub fn get_flex_shrink(&self) -> f64 {
        self.style.flex_shrink
    }

    pub fn get_flex_basis(&self) -> Value {
        self.style.flex_basis
    }

    pub fn get_flex_wrap(&self) -> Wrap {
        self.style.flex_wrap
    }

    pub fn get_width(&self) -> Value {
        self.style.width
    }

    pub fn get_height(&self) -> Value {
        self.style.height
    }

    pub fn get_overflow(&self) -> Overflow {
        self.style.overflow
    }

    pub fn get_position_type(&self) -> PositionType {
        self.style.position_type
    }

    pub fn get_direction(&self) -> Direction {
        self.style.direction
    }

    // -- Unused API stubs (present for API parity)

    pub fn copy_style(&mut self, _other: &Node) {}

    pub fn set_dirtied_func(&mut self) {}

    pub fn unset_dirtied_func(&mut self) {}

    pub fn set_is_reference_baseline(&mut self, v: bool) {
        self.is_reference_baseline = v;
        self.mark_dirty();
    }

    pub fn is_reference_baseline_flag(&self) -> bool {
        self.is_reference_baseline
    }

    pub fn set_aspect_ratio(&mut self, _v: Option<f64>) {}

    pub fn get_aspect_ratio(&self) -> f64 {
        f64::NAN
    }

    pub fn set_always_forms_containing_block(&mut self, _v: bool) {}

    // -- Layout entry point

    pub fn calculate_layout(
        &mut self,
        owner_width: Option<f64>,
        owner_height: Option<f64>,
        _direction: Option<Direction>,
    ) {
        YOGA_NODES_VISITED.with(|c| c.set(0));
        YOGA_MEASURE_CALLS.with(|c| c.set(0));
        YOGA_CACHE_HITS.with(|c| c.set(0));
        GENERATION.with(|c| c.set(c.get() + 1));

        let w = owner_width.unwrap_or(f64::NAN);
        let h = owner_height.unwrap_or(f64::NAN);

        layout_node(
            self,
            w,
            h,
            if is_defined(w) { MeasureMode::Exactly } else { MeasureMode::Undefined },
            if is_defined(h) { MeasureMode::Exactly } else { MeasureMode::Undefined },
            w,
            h,
            true,
            false,
            false,
        );

        // Root position = margin + position insets
        let mar = &self.layout.margin;
        let pos_l = resolve_value(
            &resolve_edge_raw(&self.style.position, EDGE_LEFT),
            if is_defined(w) { w } else { 0.0 },
        );
        let pos_t = resolve_value(
            &resolve_edge_raw(&self.style.position, EDGE_TOP),
            if is_defined(w) { w } else { 0.0 },
        );
        self.layout.left = mar[EDGE_LEFT] + if is_defined(pos_l) { pos_l } else { 0.0 };
        self.layout.top = mar[EDGE_TOP] + if is_defined(pos_t) { pos_t } else { 0.0 };

        let psf = self.config_psf;
        round_layout(self, psf, 0.0, 0.0);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ComputedLayout {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub width: f64,
    pub height: f64,
}

/// Dimension value that can be a number, "auto", a percent string, or undefined.
pub enum DimensionValue {
    Point(f64),
    Auto,
    Percent(f64),
    Undefined,
}

impl From<f64> for DimensionValue {
    fn from(v: f64) -> Self {
        if v.is_finite() {
            DimensionValue::Point(v)
        } else {
            DimensionValue::Undefined
        }
    }
}

fn parse_dimension(v: DimensionValue) -> Value {
    match v {
        DimensionValue::Undefined => UNDEFINED_VALUE,
        DimensionValue::Auto => AUTO_VALUE,
        DimensionValue::Point(n) => {
            if n.is_finite() {
                point_value(n)
            } else {
                UNDEFINED_VALUE
            }
        }
        DimensionValue::Percent(n) => percent_value(n),
    }
}

fn physical_edge(edge: Edge) -> usize {
    match edge {
        Edge::Left | Edge::Start => EDGE_LEFT,
        Edge::Top => EDGE_TOP,
        Edge::Right | Edge::End => EDGE_RIGHT,
        Edge::Bottom => EDGE_BOTTOM,
        _ => EDGE_LEFT,
    }
}


// ---------------------------------------------------------------------------
// Cache helpers
// ---------------------------------------------------------------------------

fn cache_write(
    node: &mut Node,
    a_w: f64,
    a_h: f64,
    w_m: MeasureMode,
    h_m: MeasureMode,
    o_w: f64,
    o_h: f64,
    f_w: bool,
    f_h: bool,
    was_dirty: bool,
) {
    let gen = GENERATION.with(|c| c.get());
    if node._c_in.is_none() {
        node._c_in = Some(vec![0.0; CACHE_SLOTS * 8]);
        node._c_out = Some(vec![0.0; CACHE_SLOTS * 2]);
    }
    if was_dirty && node._c_gen != gen {
        node._c_n = 0;
        node._c_wr = 0;
    }
    let i = node._c_wr % CACHE_SLOTS;
    node._c_wr += 1;
    if node._c_n < CACHE_SLOTS {
        node._c_n = node._c_wr;
    }
    let o = i * 8;
    let c_in = node._c_in.as_mut().unwrap();
    c_in[o] = a_w;
    c_in[o + 1] = a_h;
    c_in[o + 2] = w_m as u8 as f64;
    c_in[o + 3] = h_m as u8 as f64;
    c_in[o + 4] = o_w;
    c_in[o + 5] = o_h;
    c_in[o + 6] = if f_w { 1.0 } else { 0.0 };
    c_in[o + 7] = if f_h { 1.0 } else { 0.0 };
    let c_out = node._c_out.as_mut().unwrap();
    c_out[i * 2] = node.layout.width;
    c_out[i * 2 + 1] = node.layout.height;
    node._c_gen = gen;
}

fn commit_cache_outputs(node: &mut Node, perform_layout: bool) {
    if perform_layout {
        node._l_out_w = node.layout.width;
        node._l_out_h = node.layout.height;
    } else {
        node._m_out_w = node.layout.width;
        node._m_out_h = node.layout.height;
    }
}

// ---------------------------------------------------------------------------
// Core flexbox algorithm
// ---------------------------------------------------------------------------

fn layout_node(
    node: &mut Node,
    available_width: f64,
    available_height: f64,
    width_mode: MeasureMode,
    height_mode: MeasureMode,
    owner_width: f64,
    owner_height: f64,
    perform_layout: bool,
    force_width: bool,
    force_height: bool,
) {
    YOGA_NODES_VISITED.with(|c| c.set(c.get() + 1));
    let gen = GENERATION.with(|c| c.get());

    // Dirty-flag skip: clean subtree + matching inputs
    let same_gen = node._c_gen == gen && !perform_layout;
    if !node.is_dirty || same_gen {
        // Single-slot layout cache
        if !node.is_dirty
            && node._has_l
            && node._l_wm as u8 == width_mode as u8
            && node._l_hm as u8 == height_mode as u8
            && node._l_fw == force_width
            && node._l_fh == force_height
            && same_float(node._l_w, available_width)
            && same_float(node._l_h, available_height)
            && same_float(node._l_ow, owner_width)
            && same_float(node._l_oh, owner_height)
        {
            YOGA_CACHE_HITS.with(|c| c.set(c.get() + 1));
            node.layout.width = node._l_out_w;
            node.layout.height = node._l_out_h;
            return;
        }
        // Multi-entry cache
        if node._c_n > 0 && (same_gen || !node.is_dirty) {
            let c_in = node._c_in.as_ref().unwrap();
            let c_out = node._c_out.as_ref().unwrap();
            for i in 0..node._c_n {
                let o = i * 8;
                if c_in[o + 2] as u8 == width_mode as u8
                    && c_in[o + 3] as u8 == height_mode as u8
                    && c_in[o + 6] == (if force_width { 1.0 } else { 0.0 })
                    && c_in[o + 7] == (if force_height { 1.0 } else { 0.0 })
                    && same_float(c_in[o], available_width)
                    && same_float(c_in[o + 1], available_height)
                    && same_float(c_in[o + 4], owner_width)
                    && same_float(c_in[o + 5], owner_height)
                {
                    node.layout.width = c_out[i * 2];
                    node.layout.height = c_out[i * 2 + 1];
                    YOGA_CACHE_HITS.with(|c| c.set(c.get() + 1));
                    return;
                }
            }
        }
        // Single-slot measure cache
        if !node.is_dirty
            && !perform_layout
            && node._has_m
            && node._m_wm as u8 == width_mode as u8
            && node._m_hm as u8 == height_mode as u8
            && same_float(node._m_w, available_width)
            && same_float(node._m_h, available_height)
            && same_float(node._m_ow, owner_width)
            && same_float(node._m_oh, owner_height)
        {
            node.layout.width = node._m_out_w;
            node.layout.height = node._m_out_h;
            YOGA_CACHE_HITS.with(|c| c.set(c.get() + 1));
            return;
        }
    }

    // Commit cache inputs
    let was_dirty = node.is_dirty;
    if perform_layout {
        node._l_w = available_width;
        node._l_h = available_height;
        node._l_wm = width_mode;
        node._l_hm = height_mode;
        node._l_ow = owner_width;
        node._l_oh = owner_height;
        node._l_fw = force_width;
        node._l_fh = force_height;
        node._has_l = true;
        node.is_dirty = false;
        if was_dirty {
            node._has_m = false;
        }
    } else {
        node._m_w = available_width;
        node._m_h = available_height;
        node._m_wm = width_mode;
        node._m_hm = height_mode;
        node._m_ow = owner_width;
        node._m_oh = owner_height;
        node._has_m = true;
        if was_dirty {
            node._has_l = false;
        }
    }

    // Resolve padding/border/margin
    if node._has_padding {
        resolve_edges4_into(&node.style.padding, owner_width, &mut node.layout.padding);
    } else {
        node.layout.padding = [0.0; 4];
    }
    if node._has_border {
        resolve_edges4_into(&node.style.border, owner_width, &mut node.layout.border);
    } else {
        node.layout.border = [0.0; 4];
    }
    if node._has_margin {
        resolve_edges4_into(&node.style.margin, owner_width, &mut node.layout.margin);
    } else {
        node.layout.margin = [0.0; 4];
    }

    let pad = node.layout.padding;
    let bor = node.layout.border;

    let padding_border_width = pad[0] + pad[2] + bor[0] + bor[2];
    let padding_border_height = pad[1] + pad[3] + bor[1] + bor[3];

    // Resolve style dimensions
    let style_width = if force_width {
        f64::NAN
    } else {
        resolve_value(&node.style.width, owner_width)
    };
    let style_height = if force_height {
        f64::NAN
    } else {
        resolve_value(&node.style.height, owner_height)
    };

    let mut width = available_width;
    let mut height = available_height;
    let mut w_mode = width_mode;
    let mut h_mode = height_mode;

    if is_defined(style_width) {
        width = style_width;
        w_mode = MeasureMode::Exactly;
    }
    if is_defined(style_height) {
        height = style_height;
        h_mode = MeasureMode::Exactly;
    }

    // Apply min/max constraints
    width = bound_axis(&node.style, true, width, owner_width, owner_height);
    height = bound_axis(&node.style, false, height, owner_width, owner_height);

    // Measure-func leaf node
    if node.measure_func.is_some() && node.children.is_empty() {
        let inner_w = if w_mode == MeasureMode::Undefined {
            f64::NAN
        } else {
            (width - padding_border_width).max(0.0)
        };
        let inner_h = if h_mode == MeasureMode::Undefined {
            f64::NAN
        } else {
            (height - padding_border_height).max(0.0)
        };
        YOGA_MEASURE_CALLS.with(|c| c.set(c.get() + 1));
        let measured = (node.measure_func.as_ref().unwrap())(inner_w, w_mode, inner_h, h_mode);
        node.layout.width = if w_mode == MeasureMode::Exactly {
            width
        } else {
            bound_axis(
                &node.style,
                true,
                measured.width + padding_border_width,
                owner_width,
                owner_height,
            )
        };
        node.layout.height = if h_mode == MeasureMode::Exactly {
            height
        } else {
            bound_axis(
                &node.style,
                false,
                measured.height + padding_border_height,
                owner_width,
                owner_height,
            )
        };
        commit_cache_outputs(node, perform_layout);
        cache_write(
            node,
            available_width,
            available_height,
            width_mode,
            height_mode,
            owner_width,
            owner_height,
            force_width,
            force_height,
            was_dirty,
        );
        return;
    }

    // Leaf node with no children and no measure func
    if node.children.is_empty() {
        node.layout.width = if w_mode == MeasureMode::Exactly {
            width
        } else {
            bound_axis(&node.style, true, padding_border_width, owner_width, owner_height)
        };
        node.layout.height = if h_mode == MeasureMode::Exactly {
            height
        } else {
            bound_axis(&node.style, false, padding_border_height, owner_width, owner_height)
        };
        commit_cache_outputs(node, perform_layout);
        cache_write(
            node,
            available_width,
            available_height,
            width_mode,
            height_mode,
            owner_width,
            owner_height,
            force_width,
            force_height,
            was_dirty,
        );
        return;
    }

    // Container with children — run flexbox algorithm
    let main_axis = node.style.flex_direction;
    let cross_ax = cross_axis(main_axis);
    let is_main_row = is_row(main_axis);

    let main_size = if is_main_row { width } else { height };
    let cross_size = if is_main_row { height } else { width };
    let main_mode = if is_main_row { w_mode } else { h_mode };
    let cross_mode = if is_main_row { h_mode } else { w_mode };
    let main_pad_border = if is_main_row { padding_border_width } else { padding_border_height };
    let cross_pad_border = if is_main_row { padding_border_height } else { padding_border_width };

    let inner_main_size = if is_defined(main_size) {
        (main_size - main_pad_border).max(0.0)
    } else {
        f64::NAN
    };
    let inner_cross_size = if is_defined(cross_size) {
        (cross_size - cross_pad_border).max(0.0)
    } else {
        f64::NAN
    };

    // Resolve gap
    let gap_main = resolve_gap(
        &node.style,
        if is_main_row { Gutter::Column } else { Gutter::Row },
        inner_main_size,
    );

    // Partition children into flow vs absolute
    let children_ptrs: Vec<*mut Node> = node.children.clone();
    let mut flow_children: Vec<*mut Node> = Vec::new();
    let mut abs_children: Vec<*mut Node> = Vec::new();
    collect_layout_children_ptrs(&children_ptrs, &mut flow_children, &mut abs_children);

    let owner_w = if is_defined(width) { width } else { f64::NAN };
    let owner_h = if is_defined(height) { height } else { f64::NAN };
    let is_wrap = node.style.flex_wrap != Wrap::NoWrap;
    let gap_cross = resolve_gap(
        &node.style,
        if is_main_row { Gutter::Row } else { Gutter::Column },
        inner_cross_size,
    );

    // STEP 1: Compute flex-basis for each flow child
    for &c_ptr in &flow_children {
        let c = unsafe { &mut *c_ptr };
        c._flex_basis = compute_flex_basis(
            c,
            main_axis,
            inner_main_size,
            inner_cross_size,
            cross_mode,
            owner_w,
            owner_h,
        );
    }

    // Break into lines
    let mut lines: Vec<Vec<*mut Node>> = Vec::new();
    if !is_wrap || !is_defined(inner_main_size) || flow_children.is_empty() {
        for &c_ptr in &flow_children {
            unsafe { (*c_ptr)._line_index = 0 };
        }
        lines.push(flow_children.clone());
    } else {
        let mut line_start = 0;
        let mut line_len = 0.0_f64;
        for i in 0..flow_children.len() {
            let c = unsafe { &mut *flow_children[i] };
            let hypo = bound_axis(&c.style, is_main_row, c._flex_basis, owner_w, owner_h);
            let outer = hypo.max(0.0) + child_margin_for_axis(c, main_axis, owner_w);
            let with_gap = if i > line_start { gap_main } else { 0.0 };
            if i > line_start && line_len + with_gap + outer > inner_main_size {
                lines.push(flow_children[line_start..i].to_vec());
                line_start = i;
                line_len = outer;
            } else {
                line_len += with_gap + outer;
            }
            c._line_index = lines.len();
        }
        lines.push(flow_children[line_start..].to_vec());
    }

    let line_count = lines.len();
    let node_style = node.style.clone();
    let is_baseline = is_baseline_layout(&node_style, &flow_children);

    // STEP 2+3: For each line, resolve flexible lengths and lay out children
    let mut line_consumed_main: Vec<f64> = vec![0.0; line_count];
    let mut line_cross_sizes: Vec<f64> = vec![0.0; line_count];
    let mut line_max_ascent: Vec<f64> = if is_baseline {
        vec![0.0; line_count]
    } else {
        Vec::new()
    };
    let mut max_line_main = 0.0_f64;
    let mut total_lines_cross = 0.0_f64;

    for li in 0..line_count {
        let line = &lines[li];
        let line_gap = if line.len() > 1 {
            gap_main * (line.len() as f64 - 1.0)
        } else {
            0.0
        };
        let mut line_basis = line_gap;
        for &c_ptr in line {
            let c = unsafe { &*c_ptr };
            line_basis += c._flex_basis + child_margin_for_axis_immut(c, main_axis, owner_w);
        }

        let mut avail_main = inner_main_size;
        if !is_defined(avail_main) {
            let main_owner = if is_main_row { owner_width } else { owner_height };
            let min_m = resolve_value(
                if is_main_row { &node_style.min_width } else { &node_style.min_height },
                main_owner,
            );
            let max_m = resolve_value(
                if is_main_row { &node_style.max_width } else { &node_style.max_height },
                main_owner,
            );
            if is_defined(max_m) && line_basis > max_m - main_pad_border {
                avail_main = (max_m - main_pad_border).max(0.0);
            } else if is_defined(min_m) && line_basis < min_m - main_pad_border {
                avail_main = (min_m - main_pad_border).max(0.0);
            }
        }

        resolve_flexible_lengths(line, avail_main, line_basis, is_main_row, owner_w, owner_h);

        // Lay out each child in this line to measure cross
        let mut line_cross = 0.0_f64;
        for &c_ptr in line {
            let c = unsafe { &mut *c_ptr };
            let c_align = if c.style.align_self == Align::Auto {
                node_style.align_items
            } else {
                c.style.align_self
            };
            let c_margin_cross = child_margin_for_axis(c, cross_ax, owner_w);
            let mut child_cross_size = f64::NAN;
            let mut child_cross_mode = MeasureMode::Undefined;
            let resolved_cross_style = resolve_value(
                if is_main_row { &c.style.height } else { &c.style.width },
                if is_main_row { owner_h } else { owner_w },
            );
            let cross_lead_e = if is_main_row { EDGE_TOP } else { EDGE_LEFT };
            let cross_trail_e = if is_main_row { EDGE_BOTTOM } else { EDGE_RIGHT };
            let has_cross_auto_margin = c._has_auto_margin
                && (is_margin_auto(&c.style.margin, cross_lead_e)
                    || is_margin_auto(&c.style.margin, cross_trail_e));

            if is_defined(resolved_cross_style) {
                child_cross_size = resolved_cross_style;
                child_cross_mode = MeasureMode::Exactly;
            } else if c_align == Align::Stretch
                && !has_cross_auto_margin
                && !is_wrap
                && is_defined(inner_cross_size)
                && cross_mode == MeasureMode::Exactly
            {
                child_cross_size = (inner_cross_size - c_margin_cross).max(0.0);
                child_cross_mode = MeasureMode::Exactly;
            } else if !is_wrap && is_defined(inner_cross_size) {
                child_cross_size = (inner_cross_size - c_margin_cross).max(0.0);
                child_cross_mode = MeasureMode::AtMost;
            }

            let cw = if is_main_row { c._main_size } else { child_cross_size };
            let ch = if is_main_row { child_cross_size } else { c._main_size };
            layout_node(
                c,
                cw,
                ch,
                if is_main_row { MeasureMode::Exactly } else { child_cross_mode },
                if is_main_row { child_cross_mode } else { MeasureMode::Exactly },
                owner_w,
                owner_h,
                perform_layout,
                is_main_row,
                !is_main_row,
            );
            c._cross_size = if is_main_row { c.layout.height } else { c.layout.width };
            line_cross = line_cross.max(c._cross_size + c_margin_cross);
        }

        // Baseline layout
        if is_baseline {
            let mut max_ascent = 0.0_f64;
            let mut max_descent = 0.0_f64;
            for &c_ptr in line {
                let c = unsafe { &*c_ptr };
                if resolve_child_align(&node_style, c) != Align::Baseline {
                    continue;
                }
                let m_top = resolve_edge_no_auto(&c.style.margin, EDGE_TOP, owner_w);
                let m_bot = resolve_edge_no_auto(&c.style.margin, EDGE_BOTTOM, owner_w);
                let ascent = calculate_baseline(c) + m_top;
                let descent = c.layout.height + m_top + m_bot - ascent;
                if ascent > max_ascent { max_ascent = ascent; }
                if descent > max_descent { max_descent = descent; }
            }
            line_max_ascent[li] = max_ascent;
            if max_ascent + max_descent > line_cross {
                line_cross = max_ascent + max_descent;
            }
        }

        let main_lead = leading_edge(main_axis);
        let main_trail = trailing_edge(main_axis);
        let mut consumed = line_gap;
        for &c_ptr in line {
            let c = unsafe { &*c_ptr };
            consumed += c._main_size + c.layout.margin[main_lead] + c.layout.margin[main_trail];
        }
        line_consumed_main[li] = consumed;
        line_cross_sizes[li] = line_cross;
        max_line_main = max_line_main.max(consumed);
        total_lines_cross += line_cross;
    }

    let total_cross_gap = if line_count > 1 {
        gap_cross * (line_count as f64 - 1.0)
    } else {
        0.0
    };
    total_lines_cross += total_cross_gap;

    // STEP 4: Determine container dimensions
    let is_scroll = node_style.overflow == Overflow::Scroll;
    let content_main = max_line_main + main_pad_border;
    let final_main_size = if main_mode == MeasureMode::Exactly {
        main_size
    } else if main_mode == MeasureMode::AtMost && is_scroll {
        main_size.min(content_main).max(main_pad_border)
    } else if is_wrap && line_count > 1 && main_mode == MeasureMode::AtMost {
        main_size
    } else {
        content_main
    };
    let content_cross = total_lines_cross + cross_pad_border;
    let final_cross_size = if cross_mode == MeasureMode::Exactly {
        cross_size
    } else if cross_mode == MeasureMode::AtMost && is_scroll {
        cross_size.min(content_cross).max(cross_pad_border)
    } else {
        content_cross
    };

    node.layout.width = bound_axis(
        &node_style,
        true,
        if is_main_row { final_main_size } else { final_cross_size },
        owner_width,
        owner_height,
    );
    node.layout.height = bound_axis(
        &node_style,
        false,
        if is_main_row { final_cross_size } else { final_main_size },
        owner_width,
        owner_height,
    );
    commit_cache_outputs(node, perform_layout);
    cache_write(
        node,
        available_width,
        available_height,
        width_mode,
        height_mode,
        owner_width,
        owner_height,
        force_width,
        force_height,
        was_dirty,
    );

    if !perform_layout {
        return;
    }

    // STEP 5: Position lines and children
    let actual_inner_main =
        (if is_main_row { node.layout.width } else { node.layout.height }) - main_pad_border;
    let actual_inner_cross =
        (if is_main_row { node.layout.height } else { node.layout.width }) - cross_pad_border;
    let main_lead_edge_phys = leading_edge(main_axis);
    let main_trail_edge_phys = trailing_edge(main_axis);
    let cross_lead_edge_phys = if is_main_row { EDGE_TOP } else { EDGE_LEFT };
    let cross_trail_edge_phys = if is_main_row { EDGE_BOTTOM } else { EDGE_RIGHT };
    let reversed = is_reverse(main_axis);
    let main_container_size = if is_main_row { node.layout.width } else { node.layout.height };
    let cross_lead = pad[cross_lead_edge_phys] + bor[cross_lead_edge_phys];

    // Align-content
    let mut line_cross_offset = cross_lead;
    let mut between_lines = gap_cross;
    let free_cross = actual_inner_cross - total_lines_cross;

    if line_count == 1 && !is_wrap && !is_baseline {
        line_cross_sizes[0] = actual_inner_cross;
    } else {
        let rem_cross = free_cross.max(0.0);
        match node_style.align_content {
            Align::FlexStart => {}
            Align::Center => {
                line_cross_offset += free_cross / 2.0;
            }
            Align::FlexEnd => {
                line_cross_offset += free_cross;
            }
            Align::Stretch => {
                if line_count > 0 && rem_cross > 0.0 {
                    let add = rem_cross / line_count as f64;
                    for i in 0..line_count {
                        line_cross_sizes[i] += add;
                    }
                }
            }
            Align::SpaceBetween => {
                if line_count > 1 {
                    between_lines += rem_cross / (line_count as f64 - 1.0);
                }
            }
            Align::SpaceAround => {
                if line_count > 0 {
                    between_lines += rem_cross / line_count as f64;
                    line_cross_offset += rem_cross / line_count as f64 / 2.0;
                }
            }
            Align::SpaceEvenly => {
                if line_count > 0 {
                    between_lines += rem_cross / (line_count as f64 + 1.0);
                    line_cross_offset += rem_cross / (line_count as f64 + 1.0);
                }
            }
            _ => {}
        }
    }

    let wrap_reverse = node_style.flex_wrap == Wrap::WrapReverse;
    let cross_container_size = if is_main_row { node.layout.height } else { node.layout.width };
    let mut line_cross_pos = line_cross_offset;

    for li in 0..line_count {
        let line = &lines[li];
        let line_cross = line_cross_sizes[li];
        let consumed_main = line_consumed_main[li];
        let n = line.len();

        // Re-stretch children for wrap
        if is_wrap || cross_mode != MeasureMode::Exactly {
            for &c_ptr in line {
                let c = unsafe { &mut *c_ptr };
                let child_align = if c.style.align_self == Align::Auto {
                    node_style.align_items
                } else {
                    c.style.align_self
                };
                let cross_style_def = is_defined(resolve_value(
                    if is_main_row { &c.style.height } else { &c.style.width },
                    if is_main_row { owner_h } else { owner_w },
                ));
                let has_cross_auto_margin = c._has_auto_margin
                    && (is_margin_auto(&c.style.margin, cross_lead_edge_phys)
                        || is_margin_auto(&c.style.margin, cross_trail_edge_phys));
                if child_align == Align::Stretch && !cross_style_def && !has_cross_auto_margin {
                    let c_margin_cross = child_margin_for_axis(c, cross_ax, owner_w);
                    let target = (line_cross - c_margin_cross).max(0.0);
                    if c._cross_size != target {
                        let cw = if is_main_row { c._main_size } else { target };
                        let ch = if is_main_row { target } else { c._main_size };
                        layout_node(
                            c,
                            cw,
                            ch,
                            MeasureMode::Exactly,
                            MeasureMode::Exactly,
                            owner_w,
                            owner_h,
                            perform_layout,
                            is_main_row,
                            !is_main_row,
                        );
                        c._cross_size = target;
                    }
                }
            }
        }

        // Justify-content + auto margins
        let mut main_offset = pad[main_lead_edge_phys] + bor[main_lead_edge_phys];
        let mut between_main = gap_main;
        let mut num_auto_margins_main = 0;
        for &c_ptr in line {
            let c = unsafe { &*c_ptr };
            if !c._has_auto_margin {
                continue;
            }
            if is_margin_auto(&c.style.margin, main_lead_edge_phys) {
                num_auto_margins_main += 1;
            }
            if is_margin_auto(&c.style.margin, main_trail_edge_phys) {
                num_auto_margins_main += 1;
            }
        }
        let free_main = actual_inner_main - consumed_main;
        let remaining_main = free_main.max(0.0);
        let auto_margin_main_size = if num_auto_margins_main > 0 && remaining_main > 0.0 {
            remaining_main / num_auto_margins_main as f64
        } else {
            0.0
        };

        if num_auto_margins_main == 0 {
            match node_style.justify_content {
                Justify::FlexStart => {}
                Justify::Center => {
                    main_offset += free_main / 2.0;
                }
                Justify::FlexEnd => {
                    main_offset += free_main;
                }
                Justify::SpaceBetween => {
                    if n > 1 {
                        between_main += remaining_main / (n as f64 - 1.0);
                    }
                }
                Justify::SpaceAround => {
                    if n > 0 {
                        between_main += remaining_main / n as f64;
                        main_offset += remaining_main / n as f64 / 2.0;
                    }
                }
                Justify::SpaceEvenly => {
                    if n > 0 {
                        between_main += remaining_main / (n as f64 + 1.0);
                        main_offset += remaining_main / (n as f64 + 1.0);
                    }
                }
            }
        }

        let effective_line_cross_pos = if wrap_reverse {
            cross_container_size - line_cross_pos - line_cross
        } else {
            line_cross_pos
        };

        let mut pos = main_offset;
        for &c_ptr in line {
            let c = unsafe { &mut *c_ptr };
            let c_layout_margin = c.layout.margin;

            let (m_main_lead, m_main_trail, m_cross_lead, m_cross_trail, auto_cross_lead, auto_cross_trail);
            if c._has_auto_margin {
                let auto_main_lead_flag = is_margin_auto(&c.style.margin, main_lead_edge_phys);
                let auto_main_trail_flag = is_margin_auto(&c.style.margin, main_trail_edge_phys);
                auto_cross_lead = is_margin_auto(&c.style.margin, cross_lead_edge_phys);
                auto_cross_trail = is_margin_auto(&c.style.margin, cross_trail_edge_phys);
                m_main_lead = if auto_main_lead_flag {
                    auto_margin_main_size
                } else {
                    c_layout_margin[main_lead_edge_phys]
                };
                m_main_trail = if auto_main_trail_flag {
                    auto_margin_main_size
                } else {
                    c_layout_margin[main_trail_edge_phys]
                };
                m_cross_lead = if auto_cross_lead { 0.0 } else { c_layout_margin[cross_lead_edge_phys] };
                m_cross_trail = if auto_cross_trail { 0.0 } else { c_layout_margin[cross_trail_edge_phys] };
            } else {
                m_main_lead = c_layout_margin[main_lead_edge_phys];
                m_main_trail = c_layout_margin[main_trail_edge_phys];
                m_cross_lead = c_layout_margin[cross_lead_edge_phys];
                m_cross_trail = c_layout_margin[cross_trail_edge_phys];
                auto_cross_lead = false;
                auto_cross_trail = false;
            }

            let main_pos = if reversed {
                main_container_size - (pos + m_main_lead) - c._main_size
            } else {
                pos + m_main_lead
            };

            let child_align = if c.style.align_self == Align::Auto {
                node_style.align_items
            } else {
                c.style.align_self
            };
            let mut cross_pos = effective_line_cross_pos + m_cross_lead;
            let cross_free = line_cross - c._cross_size - m_cross_lead - m_cross_trail;

            if auto_cross_lead && auto_cross_trail {
                cross_pos += cross_free.max(0.0) / 2.0;
            } else if auto_cross_lead {
                cross_pos += cross_free.max(0.0);
            } else if auto_cross_trail {
                // stays at leading
            } else {
                match child_align {
                    Align::FlexStart | Align::Stretch => {
                        if wrap_reverse {
                            cross_pos += cross_free;
                        }
                    }
                    Align::Center => {
                        cross_pos += cross_free / 2.0;
                    }
                    Align::FlexEnd => {
                        if !wrap_reverse {
                            cross_pos += cross_free;
                        }
                    }
                    Align::Baseline => {
                        if is_baseline {
                            cross_pos = effective_line_cross_pos
                                + line_max_ascent[li]
                                - calculate_baseline(c);
                        }
                    }
                    _ => {}
                }
            }

            // Relative position offsets
            let (mut rel_x, mut rel_y) = (0.0_f64, 0.0_f64);
            if c._has_position {
                let rel_left = resolve_value(
                    &resolve_edge_raw(&c.style.position, EDGE_LEFT),
                    owner_w,
                );
                let rel_right = resolve_value(
                    &resolve_edge_raw(&c.style.position, EDGE_RIGHT),
                    owner_w,
                );
                let rel_top = resolve_value(
                    &resolve_edge_raw(&c.style.position, EDGE_TOP),
                    owner_w,
                );
                let rel_bottom = resolve_value(
                    &resolve_edge_raw(&c.style.position, EDGE_BOTTOM),
                    owner_w,
                );
                rel_x = if is_defined(rel_left) {
                    rel_left
                } else if is_defined(rel_right) {
                    -rel_right
                } else {
                    0.0
                };
                rel_y = if is_defined(rel_top) {
                    rel_top
                } else if is_defined(rel_bottom) {
                    -rel_bottom
                } else {
                    0.0
                };
            }

            if is_main_row {
                c.layout.left = main_pos + rel_x;
                c.layout.top = cross_pos + rel_y;
            } else {
                c.layout.left = cross_pos + rel_x;
                c.layout.top = main_pos + rel_y;
            }
            pos += c._main_size + m_main_lead + m_main_trail + between_main;
        }
        line_cross_pos += line_cross + between_lines;
    }

    // STEP 6: Absolute-positioned children
    let node_layout_width = node.layout.width;
    let node_layout_height = node.layout.height;
    let node_style_ref = node.style.clone();
    for &c_ptr in &abs_children {
        let c = unsafe { &mut *c_ptr };
        layout_absolute_child(
            &node_style_ref,
            c,
            node_layout_width,
            node_layout_height,
            &pad,
            &bor,
        );
    }
}


// ---------------------------------------------------------------------------
// layout_absolute_child
// ---------------------------------------------------------------------------

fn layout_absolute_child(
    parent_style: &Style,
    child: &mut Node,
    parent_width: f64,
    parent_height: f64,
    pad: &[f64; 4],
    bor: &[f64; 4],
) {
    let pos_left = resolve_edge_raw(&child.style.position, EDGE_LEFT);
    let pos_right = resolve_edge_raw(&child.style.position, EDGE_RIGHT);
    let pos_top = resolve_edge_raw(&child.style.position, EDGE_TOP);
    let pos_bottom = resolve_edge_raw(&child.style.position, EDGE_BOTTOM);

    let r_left = resolve_value(&pos_left, parent_width);
    let r_right = resolve_value(&pos_right, parent_width);
    let r_top = resolve_value(&pos_top, parent_height);
    let r_bottom = resolve_value(&pos_bottom, parent_height);

    let padding_box_w = parent_width - bor[0] - bor[2];
    let padding_box_h = parent_height - bor[1] - bor[3];
    let child_width_val = child.style.width;
    let child_height_val = child.style.height;
    let mut cw = resolve_value(&child_width_val, padding_box_w);
    let mut ch = resolve_value(&child_height_val, padding_box_h);

    if !is_defined(cw) && is_defined(r_left) && is_defined(r_right) {
        cw = padding_box_w - r_left - r_right;
    }
    if !is_defined(ch) && is_defined(r_top) && is_defined(r_bottom) {
        ch = padding_box_h - r_top - r_bottom;
    }

    // Extract style values before mutable borrow
    let child_margin = child.style.margin;
    let child_align_self = child.style.align_self;

    layout_node(
        child,
        cw,
        ch,
        if is_defined(cw) { MeasureMode::Exactly } else { MeasureMode::Undefined },
        if is_defined(ch) { MeasureMode::Exactly } else { MeasureMode::Undefined },
        padding_box_w,
        padding_box_h,
        true,
        false,
        false,
    );

    let m_l = resolve_edge_no_auto(&child_margin, EDGE_LEFT, parent_width);
    let m_t = resolve_edge_no_auto(&child_margin, EDGE_TOP, parent_width);
    let m_r = resolve_edge_no_auto(&child_margin, EDGE_RIGHT, parent_width);
    let m_b = resolve_edge_no_auto(&child_margin, EDGE_BOTTOM, parent_width);

    let main_axis = parent_style.flex_direction;
    let reversed = is_reverse(main_axis);
    let main_row = is_row(main_axis);
    let wrap_reverse = parent_style.flex_wrap == Wrap::WrapReverse;
    let alignment = if child_align_self == Align::Auto {
        parent_style.align_items
    } else {
        child_align_self
    };

    // Position left
    let left;
    if is_defined(r_left) {
        left = bor[0] + r_left + m_l;
    } else if is_defined(r_right) {
        left = parent_width - bor[2] - r_right - child.layout.width - m_r;
    } else if main_row {
        let lead = pad[0] + bor[0];
        let trail = parent_width - pad[2] - bor[2];
        left = if reversed {
            trail - child.layout.width - m_r
        } else {
            justify_absolute(parent_style.justify_content, lead, trail, child.layout.width) + m_l
        };
    } else {
        left = align_absolute(
            alignment,
            pad[0] + bor[0],
            parent_width - pad[2] - bor[2],
            child.layout.width,
            wrap_reverse,
        ) + m_l;
    }

    // Position top
    let top;
    if is_defined(r_top) {
        top = bor[1] + r_top + m_t;
    } else if is_defined(r_bottom) {
        top = parent_height - bor[3] - r_bottom - child.layout.height - m_b;
    } else if main_row {
        top = align_absolute(
            alignment,
            pad[1] + bor[1],
            parent_height - pad[3] - bor[3],
            child.layout.height,
            wrap_reverse,
        ) + m_t;
    } else {
        let lead = pad[1] + bor[1];
        let trail = parent_height - pad[3] - bor[3];
        top = if reversed {
            trail - child.layout.height - m_b
        } else {
            justify_absolute(parent_style.justify_content, lead, trail, child.layout.height) + m_t
        };
    }

    child.layout.left = left;
    child.layout.top = top;
}

fn justify_absolute(justify: Justify, lead_edge: f64, trail_edge: f64, child_size: f64) -> f64 {
    match justify {
        Justify::Center => lead_edge + (trail_edge - lead_edge - child_size) / 2.0,
        Justify::FlexEnd => trail_edge - child_size,
        _ => lead_edge,
    }
}

fn align_absolute(
    align: Align,
    lead_edge: f64,
    trail_edge: f64,
    child_size: f64,
    wrap_reverse: bool,
) -> f64 {
    match align {
        Align::Center => lead_edge + (trail_edge - lead_edge - child_size) / 2.0,
        Align::FlexEnd => {
            if wrap_reverse {
                lead_edge
            } else {
                trail_edge - child_size
            }
        }
        _ => {
            if wrap_reverse {
                trail_edge - child_size
            } else {
                lead_edge
            }
        }
    }
}

// ---------------------------------------------------------------------------
// compute_flex_basis
// ---------------------------------------------------------------------------

fn compute_flex_basis(
    child: &mut Node,
    main_axis: FlexDirection,
    available_main: f64,
    available_cross: f64,
    cross_mode: MeasureMode,
    owner_width: f64,
    owner_height: f64,
) -> f64 {
    let gen = GENERATION.with(|c| c.get());
    let same_gen = child._fb_gen == gen;
    if (same_gen || !child.is_dirty)
        && child._fb_cross_mode as u8 == cross_mode as u8
        && same_float(child._fb_owner_w, owner_width)
        && same_float(child._fb_owner_h, owner_height)
        && same_float(child._fb_avail_main, available_main)
        && same_float(child._fb_avail_cross, available_cross)
    {
        return child._fb_basis;
    }

    let is_main_row = is_row(main_axis);

    // Explicit flex-basis
    let basis = resolve_value(&child.style.flex_basis, available_main);
    if is_defined(basis) {
        let b = basis.max(0.0);
        child._fb_basis = b;
        child._fb_owner_w = owner_width;
        child._fb_owner_h = owner_height;
        child._fb_avail_main = available_main;
        child._fb_avail_cross = available_cross;
        child._fb_cross_mode = cross_mode;
        child._fb_gen = gen;
        return b;
    }

    // Style dimension on main axis
    let main_style_dim = if is_main_row {
        &child.style.width
    } else {
        &child.style.height
    };
    let main_owner = if is_main_row { owner_width } else { owner_height };
    let resolved = resolve_value(main_style_dim, main_owner);
    if is_defined(resolved) {
        let b = resolved.max(0.0);
        child._fb_basis = b;
        child._fb_owner_w = owner_width;
        child._fb_owner_h = owner_height;
        child._fb_avail_main = available_main;
        child._fb_avail_cross = available_cross;
        child._fb_cross_mode = cross_mode;
        child._fb_gen = gen;
        return b;
    }

    // Need to measure the child to get its natural size
    let cross_style_dim = if is_main_row {
        &child.style.height
    } else {
        &child.style.width
    };
    let cross_owner = if is_main_row { owner_height } else { owner_width };
    let mut cross_constraint = resolve_value(cross_style_dim, cross_owner);
    let mut cross_constraint_mode = if is_defined(cross_constraint) {
        MeasureMode::Exactly
    } else {
        MeasureMode::Undefined
    };
    if !is_defined(cross_constraint) && is_defined(available_cross) {
        cross_constraint = available_cross;
        cross_constraint_mode = if cross_mode == MeasureMode::Exactly && is_stretch_align(child) {
            MeasureMode::Exactly
        } else {
            MeasureMode::AtMost
        };
    }

    let mut main_constraint = f64::NAN;
    let mut main_constraint_mode = MeasureMode::Undefined;
    if is_main_row && is_defined(available_main) && has_measure_func_in_subtree(child) {
        main_constraint = available_main;
        main_constraint_mode = MeasureMode::AtMost;
    }

    let mw = if is_main_row { main_constraint } else { cross_constraint };
    let mh = if is_main_row { cross_constraint } else { main_constraint };
    let mw_mode = if is_main_row { main_constraint_mode } else { cross_constraint_mode };
    let mh_mode = if is_main_row { cross_constraint_mode } else { main_constraint_mode };

    layout_node(
        child,
        mw,
        mh,
        mw_mode,
        mh_mode,
        owner_width,
        owner_height,
        false,
        false,
        false,
    );
    let b = if is_main_row {
        child.layout.width
    } else {
        child.layout.height
    };
    child._fb_basis = b;
    child._fb_owner_w = owner_width;
    child._fb_owner_h = owner_height;
    child._fb_avail_main = available_main;
    child._fb_avail_cross = available_cross;
    child._fb_cross_mode = cross_mode;
    child._fb_gen = gen;
    b
}

fn has_measure_func_in_subtree(node: &Node) -> bool {
    if node.measure_func.is_some() {
        return true;
    }
    for &c_ptr in &node.children {
        let c = unsafe { &*c_ptr };
        if has_measure_func_in_subtree(c) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// resolve_flexible_lengths
// ---------------------------------------------------------------------------

fn resolve_flexible_lengths(
    children: &[*mut Node],
    available_inner_main: f64,
    total_flex_basis: f64,
    is_main_row: bool,
    owner_w: f64,
    owner_h: f64,
) {
    let n = children.len();
    let mut frozen = vec![false; n];
    let initial_free = if is_defined(available_inner_main) {
        available_inner_main - total_flex_basis
    } else {
        0.0
    };

    // Freeze inflexible items
    for i in 0..n {
        let c = unsafe { &mut *children[i] };
        let clamped = bound_axis(&c.style, is_main_row, c._flex_basis, owner_w, owner_h);
        let inflexible = !is_defined(available_inner_main)
            || (if initial_free >= 0.0 {
                c.style.flex_grow == 0.0
            } else {
                c.style.flex_shrink == 0.0
            });
        if inflexible {
            c._main_size = clamped.max(0.0);
            frozen[i] = true;
        } else {
            c._main_size = c._flex_basis;
        }
    }

    let mut unclamped = vec![0.0_f64; n];
    for _iter in 0..=n {
        let mut frozen_delta = 0.0_f64;
        let mut total_grow = 0.0_f64;
        let mut total_shrink_scaled = 0.0_f64;
        let mut unfrozen_count = 0;
        for i in 0..n {
            let c = unsafe { &*children[i] };
            if frozen[i] {
                frozen_delta += c._main_size - c._flex_basis;
            } else {
                total_grow += c.style.flex_grow;
                total_shrink_scaled += c.style.flex_shrink * c._flex_basis;
                unfrozen_count += 1;
            }
        }
        if unfrozen_count == 0 {
            break;
        }
        let mut remaining = initial_free - frozen_delta;
        if remaining > 0.0 && total_grow > 0.0 && total_grow < 1.0 {
            let scaled = initial_free * total_grow;
            if scaled < remaining {
                remaining = scaled;
            }
        } else if remaining < 0.0 && total_shrink_scaled > 0.0 {
            let mut total_shrink = 0.0;
            for i in 0..n {
                if !frozen[i] {
                    total_shrink += unsafe { &*children[i] }.style.flex_shrink;
                }
            }
            if total_shrink < 1.0 {
                let scaled = initial_free * total_shrink;
                if scaled > remaining {
                    remaining = scaled;
                }
            }
        }

        let mut total_violation = 0.0_f64;
        for i in 0..n {
            if frozen[i] {
                continue;
            }
            let c = unsafe { &mut *children[i] };
            let mut t = c._flex_basis;
            if remaining > 0.0 && total_grow > 0.0 {
                t += (remaining * c.style.flex_grow) / total_grow;
            } else if remaining < 0.0 && total_shrink_scaled > 0.0 {
                t += (remaining * (c.style.flex_shrink * c._flex_basis)) / total_shrink_scaled;
            }
            unclamped[i] = t;
            let clamped = bound_axis(&c.style, is_main_row, t, owner_w, owner_h).max(0.0);
            c._main_size = clamped;
            total_violation += clamped - t;
        }

        if total_violation == 0.0 {
            break;
        }
        let mut any_frozen = false;
        for i in 0..n {
            if frozen[i] {
                continue;
            }
            let v = unsafe { &*children[i] }._main_size - unclamped[i];
            if (total_violation > 0.0 && v > 0.0) || (total_violation < 0.0 && v < 0.0) {
                frozen[i] = true;
                any_frozen = true;
            }
        }
        if !any_frozen {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Misc helpers
// ---------------------------------------------------------------------------

fn is_stretch_align(child: &Node) -> bool {
    if let Some(p) = child.parent {
        let p = unsafe { &*p };
        let align = if child.style.align_self == Align::Auto {
            p.style.align_items
        } else {
            child.style.align_self
        };
        align == Align::Stretch
    } else {
        false
    }
}

fn resolve_child_align(parent_style: &Style, child: &Node) -> Align {
    if child.style.align_self == Align::Auto {
        parent_style.align_items
    } else {
        child.style.align_self
    }
}

fn calculate_baseline(node: &Node) -> f64 {
    let mut baseline_child: Option<&Node> = None;
    for &c_ptr in &node.children {
        let c = unsafe { &*c_ptr };
        if c._line_index > 0 {
            break;
        }
        if c.style.position_type == PositionType::Absolute {
            continue;
        }
        if c.style.display == Display::None {
            continue;
        }
        if let Some(p) = node.parent {
            let _p = unsafe { &*p };
            let parent_style = &node.style;
            if resolve_child_align(parent_style, c) == Align::Baseline || c.is_reference_baseline {
                baseline_child = Some(c);
                break;
            }
        } else if c.is_reference_baseline {
            baseline_child = Some(c);
            break;
        }
        if baseline_child.is_none() {
            baseline_child = Some(c);
        }
    }
    match baseline_child {
        None => node.layout.height,
        Some(bc) => calculate_baseline(bc) + bc.layout.top,
    }
}

fn is_baseline_layout(style: &Style, flow_children: &[*mut Node]) -> bool {
    if !is_row(style.flex_direction) {
        return false;
    }
    if style.align_items == Align::Baseline {
        return true;
    }
    for &c_ptr in flow_children {
        let c = unsafe { &*c_ptr };
        if c.style.align_self == Align::Baseline {
            return true;
        }
    }
    false
}

fn child_margin_for_axis(child: &mut Node, axis: FlexDirection, owner_width: f64) -> f64 {
    if !child._has_margin {
        return 0.0;
    }
    let lead = resolve_edge_no_auto(&child.style.margin, leading_edge(axis), owner_width);
    let trail = resolve_edge_no_auto(&child.style.margin, trailing_edge(axis), owner_width);
    lead + trail
}

fn child_margin_for_axis_immut(child: &Node, axis: FlexDirection, owner_width: f64) -> f64 {
    if !child._has_margin {
        return 0.0;
    }
    let lead = resolve_edge_no_auto(&child.style.margin, leading_edge(axis), owner_width);
    let trail = resolve_edge_no_auto(&child.style.margin, trailing_edge(axis), owner_width);
    lead + trail
}

fn resolve_gap(style: &Style, gutter: Gutter, owner_size: f64) -> f64 {
    let mut v = style.gap[gutter as usize];
    if matches!(v.unit, Unit::Undefined) {
        v = style.gap[Gutter::All as usize];
    }
    let r = resolve_value(&v, owner_size);
    if is_defined(r) { r.max(0.0) } else { 0.0 }
}

fn bound_axis(style: &Style, is_width: bool, value: f64, owner_width: f64, owner_height: f64) -> f64 {
    let min_v = if is_width { &style.min_width } else { &style.min_height };
    let max_v = if is_width { &style.max_width } else { &style.max_height };
    let min_u = min_v.unit as u8;
    let max_u = max_v.unit as u8;

    // Fast path: no min/max constraints set
    if min_u == 0 && max_u == 0 {
        return value;
    }

    let owner = if is_width { owner_width } else { owner_height };
    let mut v = value;

    // Inlined resolveValue: Unit::Point=1, Unit::Percent=2
    if max_u == 1 {
        if v > max_v.value {
            v = max_v.value;
        }
    } else if max_u == 2 {
        let m = (max_v.value * owner) / 100.0;
        if !m.is_nan() && v > m {
            v = m;
        }
    }
    if min_u == 1 {
        if v < min_v.value {
            v = min_v.value;
        }
    } else if min_u == 2 {
        let m = (min_v.value * owner) / 100.0;
        if !m.is_nan() && v < m {
            v = m;
        }
    }
    v
}

fn zero_layout_recursive(node: &mut Node) {
    let children: Vec<*mut Node> = node.children.clone();
    for &c_ptr in &children {
        let c = unsafe { &mut *c_ptr };
        c.layout.left = 0.0;
        c.layout.top = 0.0;
        c.layout.width = 0.0;
        c.layout.height = 0.0;
        c.is_dirty = true;
        c._has_l = false;
        c._has_m = false;
        zero_layout_recursive(c);
    }
}

fn collect_layout_children_ptrs(
    children: &[*mut Node],
    flow: &mut Vec<*mut Node>,
    abs: &mut Vec<*mut Node>,
) {
    for &c_ptr in children {
        let c = unsafe { &mut *c_ptr };
        match c.style.display {
            Display::None => {
                c.layout.left = 0.0;
                c.layout.top = 0.0;
                c.layout.width = 0.0;
                c.layout.height = 0.0;
                zero_layout_recursive(c);
            }
            Display::Contents => {
                c.layout.left = 0.0;
                c.layout.top = 0.0;
                c.layout.width = 0.0;
                c.layout.height = 0.0;
                let grandchildren: Vec<*mut Node> = c.children.clone();
                collect_layout_children_ptrs(&grandchildren, flow, abs);
            }
            _ => {
                if c.style.position_type == PositionType::Absolute {
                    abs.push(c_ptr);
                } else {
                    flow.push(c_ptr);
                }
            }
        }
    }
}

fn round_layout(node: &mut Node, scale: f64, abs_left: f64, abs_top: f64) {
    if scale == 0.0 {
        return;
    }
    let node_left = node.layout.left;
    let node_top = node.layout.top;
    let node_width = node.layout.width;
    let node_height = node.layout.height;

    let abs_node_left = abs_left + node_left;
    let abs_node_top = abs_top + node_top;

    let is_text = node.measure_func.is_some();
    node.layout.left = round_value(node_left, scale, false, is_text);
    node.layout.top = round_value(node_top, scale, false, is_text);

    let abs_right = abs_node_left + node_width;
    let abs_bottom = abs_node_top + node_height;
    let has_frac_w = !is_whole_number(node_width * scale);
    let has_frac_h = !is_whole_number(node_height * scale);
    node.layout.width = round_value(abs_right, scale, is_text && has_frac_w, is_text && !has_frac_w)
        - round_value(abs_node_left, scale, false, is_text);
    node.layout.height =
        round_value(abs_bottom, scale, is_text && has_frac_h, is_text && !has_frac_h)
            - round_value(abs_node_top, scale, false, is_text);

    let children: Vec<*mut Node> = node.children.clone();
    for &c_ptr in &children {
        let c = unsafe { &mut *c_ptr };
        round_layout(c, scale, abs_node_left, abs_node_top);
    }
}

fn is_whole_number(v: f64) -> bool {
    let frac = v - v.floor();
    frac < 0.0001 || frac > 0.9999
}

fn round_value(v: f64, scale: f64, force_ceil: bool, force_floor: bool) -> f64 {
    let mut scaled = v * scale;
    let mut frac = scaled - scaled.floor();
    if frac < 0.0 {
        frac += 1.0;
    }
    if frac < 0.0001 {
        scaled = scaled.floor();
    } else if frac > 0.9999 {
        scaled = scaled.ceil();
    } else if force_ceil {
        scaled = scaled.ceil();
    } else if force_floor {
        scaled = scaled.floor();
    } else {
        // Round half-up (>= 0.5 goes up)
        scaled = scaled.floor() + if frac >= 0.4999 { 1.0 } else { 0.0 };
    }
    scaled / scale
}

// ---------------------------------------------------------------------------
// Module API matching yoga-layout/load
// ---------------------------------------------------------------------------

/// Yoga instance providing Config and Node factories.
pub struct Yoga;

impl Yoga {
    pub fn config_create() -> Config {
        Config::create()
    }

    pub fn config_destroy(_config: Config) {}

    pub fn node_create(config: Option<&Config>) -> Box<Node> {
        Node::create(config)
    }

    pub fn node_create_default() -> Box<Node> {
        Node::create_default()
    }

    pub fn node_create_with_config(config: &Config) -> Box<Node> {
        Node::create(Some(config))
    }

    pub fn node_destroy(mut node: Box<Node>) {
        node.free();
    }
}

/// Load the yoga instance (async in TS, sync here).
pub fn load_yoga() -> Yoga {
    Yoga
}
