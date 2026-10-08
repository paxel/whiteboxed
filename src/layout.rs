//! Deterministic grid layout: boxes sit in the cells of their diagram, columns and
//! rows take the size of their largest box, each box side grows with the lines
//! attached to it, and lines run orthogonally through the gaps between cells.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use crate::geom::{Pos, Rect};
use crate::model::{BlockId, BlockKind, DiagramId, End, LineStyle, Project, RelationId, Rgb, Side};
use crate::text;
use crate::view::{DiagramView, OPEN_PARTNER, ViewEnd, ViewLine};

pub const GAP: f32 = 110.0;
pub const PORT_SPACING: f32 = 26.0;
pub const MIN_W: f32 = 150.0;
pub const MIN_H: f32 = 80.0;
pub const PAD: f32 = 18.0;
pub const NAME_SIZE: f32 = 15.0;
pub const KIND_SIZE: f32 = 11.0;
pub const LABEL_SIZE: f32 = 12.0;
pub const STUB_LEN: f32 = 40.0;
pub const LANE: f32 = 8.0;
const EMPTY_W: f32 = 520.0;
const EMPTY_H: f32 = 360.0;
const BEND: i64 = 60;
/// Cost of crossing a line routed earlier: worth a detour of about 150 px.
const CROSS: i64 = 1500;

pub const DEFAULT_FILL: Rgb = Rgb(0xff, 0xff, 0xff);
pub const NEIGHBOUR_FILL: Rgb = Rgb(0xe6, 0xe6, 0xe6);

#[derive(Debug, Clone, PartialEq)]
pub struct BlockGeom {
    pub id: BlockId,
    pub rect: Rect,
    pub name: String,
    pub kind: BlockKind,
    pub fill: Rgb,
    pub has_content: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Between two ports (boxes or the frame).
    Routed,
    /// From a box to an open end on this level.
    Stub,
    /// From the frame to an inherited end with no box yet.
    Dangling,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineGeom {
    pub relation: RelationId,
    pub kind: LineKind,
    /// From end `a` to end `b`.
    pub points: Vec<Pos>,
    pub arrow_start: bool,
    pub arrow_end: bool,
    pub text: String,
    pub label_at: Pos,
    /// Whether `label_at` sits on a horizontal segment (text goes above it) or a
    /// vertical one (text goes right of it).
    pub label_horizontal: bool,
    /// The open end of a stub or the loose end of a dangling line.
    pub tip: Option<Pos>,
    /// Direction from the port toward `tip`.
    pub tip_dir: Option<Side>,
    /// Where the line crosses the whitebox frame, with the partner outside.
    pub frame_port: Option<(Pos, Side, String)>,
    pub style: LineStyle,
    /// The relation's full text; `text` is what the line shows.
    pub full_text: String,
}

impl LineGeom {
    /// A line that leaves this whitebox toward an open end can be connected here.
    pub fn leaves_open(&self) -> bool {
        self.frame_port
            .as_ref()
            .is_some_and(|(_, _, partner)| partner == OPEN_PARTNER)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub diagram: DiagramId,
    pub blocks: Vec<BlockGeom>,
    pub lines: Vec<LineGeom>,
    pub frame: Option<Rect>,
    pub title: Option<String>,
    pub bounds: Rect,
    /// The grid: cell -> rect, for drag-and-drop targets.
    pub cells: Vec<(crate::model::Cell, Rect)>,
    /// Shortened relation texts: key on the line (`[2]` or a short label) and the
    /// full text, already wrapped into lines.
    pub legend: Vec<LegendEntry>,
    /// Where the legend is drawn (below the diagram), if there is one.
    pub legend_at: Option<Pos>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LegendEntry {
    pub key: String,
    pub lines: Vec<String>,
}

pub const LEGEND_LINE: f32 = 17.0;
const LEGEND_GAP: f32 = 18.0;
const LEGEND_MIN_WIDTH: f32 = 420.0;

impl Layout {
    pub fn block(&self, id: BlockId) -> Option<&BlockGeom> {
        self.blocks.iter().find(|b| b.id == id)
    }
}

/// Where two lines of different relations cross.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crossing {
    /// Index into `Layout::lines` of the line running horizontally there.
    pub horizontal: usize,
    /// Index into `Layout::lines` of the line running vertically there.
    pub vertical: usize,
    pub at: Pos,
}

/// Every point where a horizontal segment of one relation crosses a vertical
/// segment of another, away from both segments' ends.
pub fn crossings(layout: &Layout) -> Vec<Crossing> {
    let mut out = Vec::new();
    for (hl, h) in layout.lines.iter().enumerate() {
        for (vl, v) in layout.lines.iter().enumerate() {
            if h.relation == v.relation {
                continue;
            }
            for at in cross_points(&h.points, &v.points) {
                out.push(Crossing {
                    horizontal: hl,
                    vertical: vl,
                    at,
                });
            }
        }
    }
    out
}

/// Where a horizontal segment of `h` crosses a vertical segment of `v`, away from
/// both segments' ends.
fn cross_points(h: &[Pos], v: &[Pos]) -> Vec<Pos> {
    let inside = |c: f32, a: f32, b: f32| c > a.min(b) + 0.5 && c < a.max(b) - 0.5;
    let mut out = Vec::new();
    for hw in h.windows(2) {
        let (hp, hq) = (hw[0], hw[1]);
        if (hp.y - hq.y).abs() >= 0.01 {
            continue;
        }
        for vw in v.windows(2) {
            let (vp, vq) = (vw[0], vw[1]);
            if (vp.x - vq.x).abs() >= 0.01 {
                continue;
            }
            if inside(vp.x, hp.x, hq.x) && inside(hp.y, vp.y, vq.y) {
                out.push(Pos::new(vp.x, hp.y));
            }
        }
    }
    out
}

/// A port on a side: sort key, relation, line index and which end.
type PortRef = (i64, RelationId, usize, End);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Owner {
    Block(BlockId),
    Frame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Chan {
    V(usize),
    H(usize),
}

struct Grid {
    cols: BTreeMap<i32, usize>,
    rows: BTreeMap<i32, usize>,
    vx: Vec<f32>,
    hy: Vec<f32>,
    /// Horizontal extent of every vertical gap.
    vspan: Vec<(f32, f32)>,
    col_x: Vec<f32>,
    row_y: Vec<f32>,
    col_w: Vec<f32>,
    row_h: Vec<f32>,
    width: f32,
    height: f32,
}

pub fn layout(project: &Project, view: &DiagramView) -> Layout {
    let (first, grid) = build(project, view, &[]);
    let gaps = widened_gaps(&first, &grid);
    build(project, view, &gaps).0
}

/// Vertical gaps wide enough for the labels that sit in them.
fn widened_gaps(layout: &Layout, grid: &Grid) -> Vec<f32> {
    let mut gaps: Vec<f32> = grid.vspan.iter().map(|(a, b)| b - a).collect();
    let mut need = |x: f32, width: f32| {
        if let Some(v) = grid
            .vspan
            .iter()
            .position(|(a, b)| x >= *a - 0.5 && x <= *b + 0.5)
        {
            gaps[v] = gaps[v].max(width);
        }
    };
    for line in &layout.lines {
        let w = text::width(&line.text, LABEL_SIZE);
        match (line.kind, line.tip, line.tip_dir) {
            (LineKind::Routed, _, _) => {
                if line.label_horizontal && w > 0.0 {
                    need(line.label_at.x, w + 56.0);
                }
            }
            (_, Some(tip), Some(dir)) if dir.is_horizontal() => {
                need(tip.x, STUB_LEN + w + 40.0);
            }
            _ => {}
        }
        if let Some((p, side, partner)) = &line.frame_port
            && side.is_horizontal()
        {
            need(p.x, text::width(partner, LABEL_SIZE) + 24.0);
        }
    }
    gaps
}

fn build(project: &Project, view: &DiagramView, vgaps: &[f32]) -> (Layout, Grid) {
    // Ports per box side and per frame side.
    let mut ports: BTreeMap<(Owner, Side), Vec<PortRef>> = BTreeMap::new();
    let cell_of = |id: BlockId| project.blocks.get(&id).map(|b| b.cell);
    for (li, line) in view.lines.iter().enumerate() {
        for end in [End::A, End::B] {
            let (owner, side) = match line.end(end) {
                ViewEnd::Block { block, side } => (Owner::Block(*block), *side),
                ViewEnd::Frame { side, .. } => (Owner::Frame, *side),
                _ => continue,
            };
            let key = sort_key(line.end(end.other()), side, &cell_of);
            ports
                .entry((owner, side))
                .or_default()
                .push((key, line.relation, li, end));
        }
    }
    for list in ports.values_mut() {
        list.sort();
    }
    let count = |owner: Owner, side: Side| ports.get(&(owner, side)).map_or(0, Vec::len) as f32;

    // Box sizes.
    let mut sizes = BTreeMap::new();
    for id in &view.blocks {
        let Some(b) = project.blocks.get(id) else {
            continue;
        };
        let o = Owner::Block(*id);
        let text_w = text::width(&b.name, NAME_SIZE)
            .max(text::width(&kind_caption(b.kind), KIND_SIZE))
            + 2.0 * PAD;
        let w = MIN_W
            .max(text_w)
            .max((count(o, Side::Top) + 1.0) * PORT_SPACING)
            .max((count(o, Side::Bottom) + 1.0) * PORT_SPACING);
        let h = MIN_H
            .max((count(o, Side::Left) + 1.0) * PORT_SPACING)
            .max((count(o, Side::Right) + 1.0) * PORT_SPACING);
        sizes.insert(*id, (b.cell, w, h));
    }
    let grid = grid(&sizes, vgaps);

    // Box rects.
    let mut blocks = Vec::new();
    let mut rects = BTreeMap::new();
    for id in &view.blocks {
        let (Some(b), Some((cell, w, h))) = (project.blocks.get(id), sizes.get(id)) else {
            continue;
        };
        let (ci, ri) = (grid.cols[&cell.col], grid.rows[&cell.row]);
        let center = Pos::new(
            grid.col_x[ci] + grid.col_w[ci] / 2.0,
            grid.row_y[ri] + grid.row_h[ri] / 2.0,
        );
        let rect = Rect::from_center(center, *w, *h);
        rects.insert(*id, (rect, ci, ri));
        let fill = b
            .tag
            .and_then(|t| project.tags.get(&t))
            .map(|t| t.color)
            .unwrap_or(if b.kind.is_neighbour() {
                NEIGHBOUR_FILL
            } else {
                DEFAULT_FILL
            });
        blocks.push(BlockGeom {
            id: *id,
            rect,
            name: b.name.clone(),
            kind: b.kind,
            fill,
            has_content: project.has_content(*id),
        });
    }

    let bounds = Rect::from_min_size(Pos::new(0.0, 0.0), grid.width, grid.height);
    let frame = view.diagram.map(|_| bounds);

    // Port positions: (line index, end) -> (point, channel entry). A port sits where
    // its partner is, so facing boxes get straight lines; ports on one side then keep
    // a minimum distance from each other.
    let owner_rect = |owner: Owner| match owner {
        Owner::Block(id) => rects.get(&id).map(|(r, _, _)| *r),
        Owner::Frame => Some(bounds),
    };
    let range = |rect: Rect, side: Side| {
        if side.is_horizontal() {
            (rect.min.y, rect.max.y)
        } else {
            (rect.min.x, rect.max.x)
        }
    };
    let mut ends: BTreeMap<(usize, End), (Owner, Side)> = BTreeMap::new();
    for ((owner, side), list) in &ports {
        for (_, _, li, end) in list {
            ends.insert((*li, *end), (*owner, *side));
        }
    }
    let ideal = |li: usize, end: End, owner: Owner, side: Side| -> Option<f32> {
        let own = range(owner_rect(owner)?, side);
        let Some(&(o2, s2)) = ends.get(&(li, end.other())) else {
            return Some((own.0 + own.1) / 2.0);
        };
        let other_rect = owner_rect(o2)?;
        if side.is_horizontal() == s2.is_horizontal() {
            let other = range(other_rect, s2);
            let (lo, hi) = (own.0.max(other.0), own.1.min(other.1));
            // No overlap: as close to the partner as this side allows.
            Some(if lo <= hi {
                (lo + hi) / 2.0
            } else {
                ((other.0 + other.1) / 2.0).clamp(own.0, own.1)
            })
        } else {
            let c = other_rect.center();
            let along = if side.is_horizontal() { c.y } else { c.x };
            Some(along.clamp(own.0, own.1))
        }
    };
    // Place every side's ports for given wishes; returns the coordinate along the side.
    let place = |wish: &BTreeMap<(usize, End), f32>| -> BTreeMap<(usize, End), f32> {
        let mut out = BTreeMap::new();
        for ((owner, side), list) in &ports {
            let Some(rect) = owner_rect(*owner) else {
                continue;
            };
            let (lo, hi) = range(rect, *side);
            let mut wanted: Vec<(f32, RelationId, usize, End)> = list
                .iter()
                .map(|(_, rel, li, end)| {
                    let at = wish.get(&(*li, *end)).copied().unwrap_or((lo + hi) / 2.0);
                    (at, *rel, *li, *end)
                })
                .collect();
            wanted.sort_by(|a, b| {
                a.0.partial_cmp(&b.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.1.cmp(&b.1))
                    .then(a.3.cmp(&b.3))
            });
            let at: Vec<f32> = wanted.iter().map(|w| w.0).collect();
            let placed = spread(
                &at,
                lo + PORT_SPACING / 2.0,
                hi - PORT_SPACING / 2.0,
                PORT_SPACING,
            );
            for ((_, _, li, end), v) in wanted.iter().zip(placed) {
                out.insert((*li, *end), v);
            }
        }
        out
    };
    let first_wish: BTreeMap<(usize, End), f32> = ends
        .iter()
        .filter_map(|(&(li, end), &(owner, side))| {
            ideal(li, end, owner, side).map(|v| ((li, end), v))
        })
        .collect();
    let first = place(&first_wish);
    // Facing ends must agree: the end on the busier side keeps its place, and the
    // other one takes the same coordinate.
    let busy = |owner: Owner, side: Side| ports.get(&(owner, side)).map_or(0, Vec::len);
    let second_wish: BTreeMap<(usize, End), f32> = first
        .iter()
        .map(|(&(li, end), &v)| {
            let own = ends.get(&(li, end));
            let other = ends.get(&(li, end.other()));
            let follow = match (own, other, first.get(&(li, end.other()))) {
                (Some(&(o1, s1)), Some(&(o2, s2)), Some(&w))
                    if s1.is_horizontal() == s2.is_horizontal() && busy(o2, s2) > busy(o1, s1) =>
                {
                    Some(w)
                }
                _ => None,
            };
            ((li, end), follow.unwrap_or(v))
        })
        .collect();
    let along = place(&second_wish);
    let mut port_at: BTreeMap<(usize, End), (Pos, Side, Chan)> = BTreeMap::new();
    for ((owner, side), list) in &ports {
        let (rect, chan) = match owner {
            Owner::Block(id) => {
                let Some((rect, ci, ri)) = rects.get(id) else {
                    continue;
                };
                let chan = match side {
                    Side::Right => Chan::V(ci + 1),
                    Side::Left => Chan::V(*ci),
                    Side::Top => Chan::H(*ri),
                    Side::Bottom => Chan::H(ri + 1),
                };
                (*rect, chan)
            }
            Owner::Frame => {
                let chan = match side {
                    Side::Left => Chan::V(0),
                    Side::Right => Chan::V(grid.vx.len() - 1),
                    Side::Top => Chan::H(0),
                    Side::Bottom => Chan::H(grid.hy.len() - 1),
                };
                (bounds, chan)
            }
        };
        for (_, _, li, end) in list {
            let Some(&v) = along.get(&(*li, *end)) else {
                continue;
            };
            let p = match side {
                Side::Top => Pos::new(v, rect.min.y),
                Side::Bottom => Pos::new(v, rect.max.y),
                Side::Left => Pos::new(rect.min.x, v),
                Side::Right => Pos::new(rect.max.x, v),
            };
            port_at.insert((*li, *end), (p, *side, chan));
        }
    }

    // Route every line that has two ports.
    // Lines avoid crossing the ones routed before them.
    let mut routes: BTreeMap<usize, Vec<Chan>> = BTreeMap::new();
    // Every line starts with only its stems known; then each line is routed against
    // all the others, twice, so the first lines also see the later ones.
    let both = |li: usize| Some((*port_at.get(&(li, End::A))?, *port_at.get(&(li, End::B))?));
    let stem = |(p, _, chan): (Pos, Side, Chan)| match chan {
        Chan::V(v) => (p, Pos::new(grid.vx[v], p.y)),
        Chan::H(h) => (p, Pos::new(p.x, grid.hy[h])),
    };
    let mut segments: Segments = BTreeMap::new();
    for li in 0..view.lines.len() {
        if let Some((a, b)) = both(li) {
            segments.insert(li, vec![stem(a), stem(b)]);
        }
    }
    for _ in 0..2 {
        for li in 0..view.lines.len() {
            let Some((a, b)) = both(li) else { continue };
            let others = obstacles(&grid, &segments, li);
            let chans = route(&grid, (a.0, a.2), (b.0, b.2), &others);
            let points = route_points(&grid, &BTreeMap::new(), li, a.0, b.0, &chans);
            segments.insert(li, points.windows(2).map(|w| (w[0], w[1])).collect());
            routes.insert(li, chans);
        }
    }
    let mut lanes = lane_order(&routes);
    let ends: BTreeMap<usize, (Pos, Pos)> = routes
        .keys()
        .filter_map(|li| {
            Some((
                *li,
                (
                    port_at.get(&(*li, End::A))?.0,
                    port_at.get(&(*li, End::B))?.0,
                ),
            ))
        })
        .collect();
    untangle(&grid, &routes, &ends, &mut lanes);
    let offsets = lane_offsets(&lanes);

    let mut lines = Vec::new();
    let (shown, legend_keys) = shown_labels(view, project.label_limit);
    for (li, line) in view.lines.iter().enumerate() {
        let pa = port_at.get(&(li, End::A)).copied();
        let pb = port_at.get(&(li, End::B)).copied();
        let geom = match (pa, pb) {
            (Some(a), Some(b)) => {
                let chans = routes.get(&li).cloned().unwrap_or_default();
                let points = route_points(&grid, &offsets, li, a.0, b.0, &chans);
                build_line(line, LineKind::Routed, points, None, None)
            }
            (Some(a), None) | (None, Some(a)) => {
                let inward = line_is_frame_inward(line);
                let dir = if inward { a.1.opposite() } else { a.1 };
                let tip = step_out(a.0, dir, STUB_LEN);
                let kind = if line.a == ViewEnd::Dangling || line.b == ViewEnd::Dangling {
                    LineKind::Dangling
                } else {
                    LineKind::Stub
                };
                let points = if pa.is_some() {
                    vec![a.0, tip]
                } else {
                    vec![tip, a.0]
                };
                build_line(line, kind, points, Some(tip), Some(dir))
            }
            (None, None) => continue,
        };
        let frame_port = [(End::A, pa), (End::B, pb)]
            .into_iter()
            .find_map(|(end, port)| match (line.end(end), port) {
                (ViewEnd::Frame { partner, side }, Some(p)) => Some((p.0, *side, partner.clone())),
                _ => None,
            });
        lines.push(LineGeom { frame_port, ..geom });
    }

    let mut cells = Vec::new();
    for (col, ci) in &grid.cols {
        for (row, ri) in &grid.rows {
            cells.push((
                crate::model::Cell::new(*col, *row),
                Rect::from_min_size(
                    Pos::new(grid.col_x[*ci], grid.row_y[*ri]),
                    grid.col_w[*ci],
                    grid.row_h[*ri],
                ),
            ));
        }
    }

    for (li, geom) in lines.iter_mut().enumerate() {
        if let Some(text) = shown.get(li) {
            geom.text = text.clone();
        }
    }
    // The legend goes below everything, as wide as the diagram (or a readable
    // minimum), with long texts wrapped.
    let legend_width = bounds.width().max(LEGEND_MIN_WIDTH) - 2.0 * PAD;
    let legend: Vec<LegendEntry> = legend_keys
        .into_iter()
        .map(|(key, full)| {
            let indent = text::width(&format!("{key}  "), LABEL_SIZE);
            LegendEntry {
                lines: wrap(&full, (legend_width - indent).max(120.0), LABEL_SIZE),
                key,
            }
        })
        .collect();
    let (bounds, legend_at) = if legend.is_empty() {
        (bounds, None)
    } else {
        let rows: usize = legend.iter().map(|e| e.lines.len().max(1)).sum();
        let at = Pos::new(PAD, bounds.max.y + LEGEND_GAP);
        let height = LEGEND_GAP + LEGEND_LINE * (rows as f32 + 1.0) + LEGEND_GAP;
        (
            Rect::from_min_size(
                bounds.min,
                bounds.width().max(LEGEND_MIN_WIDTH),
                bounds.height() + height,
            ),
            Some(at),
        )
    };
    let layout = Layout {
        diagram: view.diagram,
        blocks,
        lines,
        frame,
        title: view
            .diagram
            .and_then(|d| project.blocks.get(&d))
            .map(|b| b.name.clone()),
        bounds,
        cells,
        legend,
        legend_at,
    };
    (layout, grid)
}

/// What each line of the view shows, and the legend for whatever was shortened:
/// a short label if the relation has one, the full text up to `limit` characters,
/// otherwise a number `[n]` (numbered in relation order).
fn shown_labels(view: &DiagramView, limit: Option<u32>) -> (Vec<String>, Vec<(String, String)>) {
    let mut shown = Vec::new();
    let mut legend: Vec<(String, String)> = Vec::new();
    let mut numbers: BTreeMap<RelationId, String> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for line in &view.lines {
        let full = line.text.trim();
        let too_long = |n: u32| !full.is_empty() && full.chars().count() > n as usize;
        let label = if !line.short.is_empty() {
            if !full.is_empty() && full != line.short && seen.insert(line.relation) {
                legend.push((line.short.clone(), full.to_owned()));
            }
            line.short.clone()
        } else if limit.is_some_and(too_long) {
            let next = numbers.len() + 1;
            let key = numbers
                .entry(line.relation)
                .or_insert_with(|| format!("[{next}]"))
                .clone();
            if seen.insert(line.relation) {
                legend.push((key.clone(), full.to_owned()));
            }
            key
        } else {
            full.to_owned()
        };
        shown.push(label);
    }
    (shown, legend)
}

/// Splits `text` into lines no wider than `width` at `size`, at spaces where possible.
pub fn wrap(text: &str, width: f32, size: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && text::width(&candidate, size) > width {
                lines.push(std::mem::take(&mut line));
                line = word.to_owned();
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    lines
}

/// The caption under a box name, e.g. `«database»`.
pub fn kind_caption(kind: BlockKind) -> String {
    format!("\u{ab}{}\u{bb}", kind.label())
}

fn line_is_frame_inward(line: &ViewLine) -> bool {
    matches!(line.a, ViewEnd::Frame { .. }) || matches!(line.b, ViewEnd::Frame { .. })
}

/// Point `len` away from `p` in direction `side`.
fn step_out(p: Pos, side: Side, len: f32) -> Pos {
    match side {
        Side::Top => Pos::new(p.x, p.y - len),
        Side::Bottom => Pos::new(p.x, p.y + len),
        Side::Left => Pos::new(p.x - len, p.y),
        Side::Right => Pos::new(p.x + len, p.y),
    }
}

/// Positions as close to `wanted` (sorted) as possible, at least `gap` apart and
/// within `lo..=hi`. When they do not fit, they are spread evenly.
fn spread(wanted: &[f32], lo: f32, hi: f32, gap: f32) -> Vec<f32> {
    let n = wanted.len();
    if n == 0 {
        return Vec::new();
    }
    if hi < lo || (n as f32 - 1.0) * gap > hi - lo {
        let mid = (lo + hi) / 2.0;
        let step = if n > 1 {
            (hi - lo).max(0.0) / (n as f32 - 1.0)
        } else {
            0.0
        };
        let start = if n > 1 { lo.min(mid) } else { mid };
        return (0..n).map(|i| start + step * i as f32).collect();
    }
    let mut out: Vec<f32> = Vec::with_capacity(n);
    for (i, w) in wanted.iter().enumerate() {
        let min = if i == 0 { lo } else { out[i - 1] + gap };
        out.push(w.clamp(lo, hi).max(min));
    }
    // Pull back from the far end where the forward pass ran over.
    let mut max = hi;
    for v in out.iter_mut().rev() {
        if *v > max {
            *v = max;
        }
        max = *v - gap;
    }
    out
}

/// Orders ports along a side by where their partner is, to avoid crossings.
fn sort_key(
    partner: &ViewEnd,
    side: Side,
    cell_of: &impl Fn(BlockId) -> Option<crate::model::Cell>,
) -> i64 {
    const FAR: i64 = 1_000_000;
    let along =
        |c: crate::model::Cell| i64::from(if side.is_horizontal() { c.row } else { c.col }) * 1000;
    match partner {
        ViewEnd::Block { block, .. } => cell_of(*block).map_or(0, along),
        ViewEnd::Frame { side: fs, .. } => match (side.is_horizontal(), fs) {
            (true, Side::Top) | (false, Side::Left) => -FAR,
            (true, Side::Bottom) | (false, Side::Right) => FAR,
            _ => 0,
        },
        ViewEnd::Open | ViewEnd::Dangling => 0,
    }
}

fn grid(sizes: &BTreeMap<BlockId, (crate::model::Cell, f32, f32)>, vgaps: &[f32]) -> Grid {
    let cols: BTreeSet<i32> = sizes.values().map(|(c, _, _)| c.col).collect();
    let rows: BTreeSet<i32> = sizes.values().map(|(c, _, _)| c.row).collect();
    let cols: BTreeMap<i32, usize> = cols.into_iter().enumerate().map(|(i, c)| (c, i)).collect();
    let rows: BTreeMap<i32, usize> = rows.into_iter().enumerate().map(|(i, r)| (r, i)).collect();
    let mut col_w = vec![0.0f32; cols.len()];
    let mut row_h = vec![0.0f32; rows.len()];
    for (cell, w, h) in sizes.values() {
        let ci = cols[&cell.col];
        let ri = rows[&cell.row];
        col_w[ci] = col_w[ci].max(*w);
        row_h[ri] = row_h[ri].max(*h);
    }
    let (col_x, vspan, width) = axis(&col_w, vgaps, EMPTY_W);
    let (row_y, hspan, height) = axis(&row_h, &[], EMPTY_H);
    let center = |(a, b): &(f32, f32)| (a + b) / 2.0;
    Grid {
        cols,
        rows,
        vx: vspan.iter().map(center).collect(),
        hy: hspan.iter().map(center).collect(),
        vspan,
        col_x,
        row_y,
        col_w,
        row_h,
        width,
        height,
    }
}

/// Starts of the cells, extents of the gaps around them, and the total length.
/// `gaps` overrides the default gap width per gap where given.
fn axis(sizes: &[f32], gaps: &[f32], empty: f32) -> (Vec<f32>, Vec<(f32, f32)>, f32) {
    let gap = |i: usize| gaps.get(i).copied().unwrap_or(GAP).max(GAP);
    if sizes.is_empty() {
        let w = empty.max(gap(0));
        return (Vec::new(), vec![(0.0, w)], w);
    }
    let mut starts = Vec::new();
    let mut spans = vec![(0.0, gap(0))];
    let mut x = gap(0);
    for (i, s) in sizes.iter().enumerate() {
        starts.push(x);
        x += s;
        let g = gap(i + 1);
        spans.push((x, x + g));
        x += g;
    }
    (starts, spans, x)
}

/// Straight segments of routed lines, by line index.
type Segments = BTreeMap<usize, Vec<(Pos, Pos)>>;

/// What a route should not cross: for every vertical channel the y of the
/// horizontal segments spanning it, and for every horizontal channel the x of the
/// vertical ones.
struct Obstacles {
    on_v: Vec<Vec<f32>>,
    on_h: Vec<Vec<f32>>,
}

/// The segments of every line except `skip`, as obstacles per channel.
fn obstacles(grid: &Grid, segments: &Segments, skip: usize) -> Obstacles {
    let mut out = Obstacles {
        on_v: vec![Vec::new(); grid.vx.len()],
        on_h: vec![Vec::new(); grid.hy.len()],
    };
    let spans = |c: f32, a: f32, b: f32| c > a.min(b) + 0.01 && c < a.max(b) - 0.01;
    for (li, list) in segments {
        if *li == skip {
            continue;
        }
        for (p, q) in list {
            if (p.y - q.y).abs() < 0.01 {
                for (v, x) in grid.vx.iter().enumerate() {
                    if spans(*x, p.x, q.x) {
                        out.on_v[v].push(p.y);
                    }
                }
            } else if (p.x - q.x).abs() < 0.01 {
                for (h, y) in grid.hy.iter().enumerate() {
                    if spans(*y, p.y, q.y) {
                        out.on_h[h].push(p.x);
                    }
                }
            }
        }
    }
    out
}

/// Shortest path with few bends and few crossings over the lattice of gap channels.
fn route(grid: &Grid, a: (Pos, Chan), b: (Pos, Chan), others: &Obstacles) -> Vec<Chan> {
    let nv = grid.vx.len();
    let nh = grid.hy.len();
    let s = nv * nh;
    let t = s + 1;
    let node_count = s + 2;
    let coord = |chan: Chan, p: Pos| match chan {
        Chan::V(_) => p.y,
        Chan::H(_) => p.x,
    };
    let cost = |d: f32| (d.abs() * 10.0).round() as i64;
    // Crossings when running along `chan` from `from` to `to`: other lines met past
    // `from`, up to and including `to`.
    let crossed = |chan: Chan, from: f32, to: f32| -> i64 {
        let list = match chan {
            Chan::V(v) => &others.on_v[v],
            Chan::H(h) => &others.on_h[h],
        };
        let met = |c: &&f32| {
            if to >= from {
                **c > from + 0.01 && **c <= to + 0.01
            } else {
                **c < from - 0.01 && **c >= to - 0.01
            }
        };
        list.iter().filter(met).count() as i64 * CROSS
    };

    // state = node * 2 + axis (0: moved along H, 1: moved along V)
    let mut dist = vec![i64::MAX; node_count * 2];
    let mut prev: Vec<Option<(usize, Chan)>> = vec![None; node_count * 2];
    let mut heap = BinaryHeap::new();
    let start_axis = match a.1 {
        Chan::V(_) => 0,
        Chan::H(_) => 1,
    };
    dist[s * 2 + start_axis] = 0;
    heap.push(Reverse((0i64, s * 2 + start_axis)));

    let mut target_state = None;
    while let Some(Reverse((d, state))) = heap.pop() {
        if d > dist[state] {
            continue;
        }
        let (node, axis) = (state / 2, state % 2);
        if node == t {
            target_state = Some(state);
            break;
        }
        // (next node, channel, length, crossing cost)
        let mut edges: Vec<(usize, Chan, f32, i64)> = Vec::new();
        let here = if node == s {
            None
        } else {
            Some((node / nh, node % nh))
        };
        match here {
            None => {
                let along = coord(a.1, a.0);
                match a.1 {
                    Chan::V(v) => {
                        for h in 0..nh {
                            let x = crossed(a.1, along, grid.hy[h]);
                            edges.push((v * nh + h, a.1, grid.hy[h] - along, x));
                        }
                    }
                    Chan::H(h) => {
                        for v in 0..nv {
                            let x = crossed(a.1, along, grid.vx[v]);
                            edges.push((v * nh + h, a.1, grid.vx[v] - along, x));
                        }
                    }
                }
                if a.1 == b.1 {
                    let to = coord(b.1, b.0);
                    edges.push((t, a.1, to - along, crossed(a.1, along, to)));
                }
            }
            Some((v, h)) => {
                let (y, x) = (grid.hy[h], grid.vx[v]);
                if h + 1 < nh {
                    let c = crossed(Chan::V(v), y, grid.hy[h + 1]);
                    edges.push((node + 1, Chan::V(v), grid.hy[h + 1] - y, c));
                }
                if h > 0 {
                    let c = crossed(Chan::V(v), y, grid.hy[h - 1]);
                    edges.push((node - 1, Chan::V(v), y - grid.hy[h - 1], c));
                }
                if v + 1 < nv {
                    let c = crossed(Chan::H(h), x, grid.vx[v + 1]);
                    edges.push((node + nh, Chan::H(h), grid.vx[v + 1] - x, c));
                }
                if v > 0 {
                    let c = crossed(Chan::H(h), x, grid.vx[v - 1]);
                    edges.push((node - nh, Chan::H(h), x - grid.vx[v - 1], c));
                }
                match b.1 {
                    Chan::V(bv) if bv == v => {
                        let x = crossed(b.1, grid.hy[h], b.0.y);
                        edges.push((t, b.1, b.0.y - grid.hy[h], x));
                    }
                    Chan::H(bh) if bh == h => {
                        let x = crossed(b.1, grid.vx[v], b.0.x);
                        edges.push((t, b.1, b.0.x - grid.vx[v], x));
                    }
                    _ => {}
                }
            }
        }
        for (next, chan, len, cross) in edges {
            let next_axis = match chan {
                Chan::V(_) => 1,
                Chan::H(_) => 0,
            };
            let bend = if next_axis == axis { 0 } else { BEND };
            let nd = d + cost(len) + bend + cross;
            let ns = next * 2 + next_axis;
            if nd < dist[ns] {
                dist[ns] = nd;
                prev[ns] = Some((state, chan));
                heap.push(Reverse((nd, ns)));
            }
        }
    }

    let Some(mut state) = target_state else {
        return vec![a.1];
    };
    let mut chans = Vec::new();
    while let Some((p, chan)) = prev[state] {
        chans.push(chan);
        state = p;
    }
    chans.reverse();
    chans.dedup();
    chans
}

/// Lines that share a channel, in the order of their lanes across it.
type Lanes = BTreeMap<Chan, Vec<usize>>;

/// Above this many routed lines, lanes keep their first order (untangling would
/// get slow).
const UNTANGLE_LIMIT: usize = 60;

fn lane_order(routes: &BTreeMap<usize, Vec<Chan>>) -> Lanes {
    let mut users: Lanes = BTreeMap::new();
    for (li, chans) in routes {
        for chan in chans {
            let list = users.entry(*chan).or_default();
            if !list.contains(li) {
                list.push(*li);
            }
        }
    }
    users
}

/// Swaps neighbouring lanes wherever that leaves fewer crossings, until nothing
/// improves.
fn untangle(
    grid: &Grid,
    routes: &BTreeMap<usize, Vec<Chan>>,
    ends: &BTreeMap<usize, (Pos, Pos)>,
    lanes: &mut Lanes,
) {
    if routes.len() > UNTANGLE_LIMIT {
        return;
    }
    let draw = |offsets: &BTreeMap<(usize, Chan), f32>, li: usize| -> Vec<Pos> {
        match (ends.get(&li), routes.get(&li)) {
            (Some((a, b)), Some(chans)) => route_points(grid, offsets, li, *a, *b, chans),
            _ => Vec::new(),
        }
    };
    let offsets = lane_offsets(lanes);
    let mut points: BTreeMap<usize, Vec<Pos>> =
        routes.keys().map(|li| (*li, draw(&offsets, *li))).collect();
    // Lines whose boxes do not overlap cannot cross.
    let extent = |p: &[Pos]| {
        p.iter().fold(
            (f32::MAX, f32::MAX, f32::MIN, f32::MIN),
            |(x0, y0, x1, y1), q| (x0.min(q.x), y0.min(q.y), x1.max(q.x), y1.max(q.y)),
        )
    };
    let apart = |a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)| {
        a.2 < b.0 || b.2 < a.0 || a.3 < b.1 || b.3 < a.1
    };
    // Crossings that involve at least one of `users`.
    let count = |points: &BTreeMap<usize, Vec<Pos>>, users: &[usize]| -> usize {
        let mut n = 0;
        for u in users {
            let q = &points[u];
            let eq = extent(q);
            for (li, p) in points {
                // Pairs within `users` are counted once.
                if li == u || (users.contains(li) && li < u) || apart(extent(p), eq) {
                    continue;
                }
                n += cross_points(p, q).len() + cross_points(q, p).len();
            }
        }
        n
    };
    let chans: Vec<Chan> = lanes.keys().copied().collect();
    for _ in 0..4 {
        let mut improved = false;
        for chan in &chans {
            let n = lanes.get(chan).map_or(0, Vec::len);
            for k in 0..n.saturating_sub(1) {
                // A swap moves only the two lines swapped.
                let Some(users) = lanes.get(chan).map(|l| vec![l[k], l[k + 1]]) else {
                    break;
                };
                let before = count(&points, &users);
                if before == 0 {
                    continue;
                }
                if let Some(list) = lanes.get_mut(chan) {
                    list.swap(k, k + 1);
                }
                let offsets = lane_offsets(lanes);
                let old: Vec<(usize, Vec<Pos>)> = users
                    .iter()
                    .map(|li| {
                        (
                            *li,
                            points.insert(*li, draw(&offsets, *li)).unwrap_or_default(),
                        )
                    })
                    .collect();
                if count(&points, &users) < before {
                    improved = true;
                } else {
                    points.extend(old);
                    if let Some(list) = lanes.get_mut(chan) {
                        list.swap(k, k + 1);
                    }
                }
            }
        }
        if !improved {
            break;
        }
    }
}

/// Spreads lines that share a channel side by side, in their lane order.
fn lane_offsets(lanes: &Lanes) -> BTreeMap<(usize, Chan), f32> {
    let limit = GAP / 2.0 - 12.0;
    let mut out = BTreeMap::new();
    for (chan, list) in lanes {
        let n = list.len() as f32;
        for (k, li) in list.iter().copied().enumerate() {
            let off = ((k as f32 - (n - 1.0) / 2.0) * LANE).clamp(-limit, limit);
            out.insert((li, *chan), off);
        }
    }
    out
}

fn route_points(
    grid: &Grid,
    offsets: &BTreeMap<(usize, Chan), f32>,
    li: usize,
    pa: Pos,
    pb: Pos,
    chans: &[Chan],
) -> Vec<Pos> {
    let off = |c: Chan| offsets.get(&(li, c)).copied().unwrap_or(0.0);
    let x = |v: usize| grid.vx[v] + off(Chan::V(v));
    let y = |h: usize| grid.hy[h] + off(Chan::H(h));
    let mut pts = vec![pa];
    let (Some(first), Some(last)) = (chans.first(), chans.last()) else {
        pts.push(pb);
        return pts;
    };
    pts.push(match *first {
        Chan::V(v) => Pos::new(x(v), pa.y),
        Chan::H(h) => Pos::new(pa.x, y(h)),
    });
    for w in chans.windows(2) {
        match (w[0], w[1]) {
            (Chan::V(v), Chan::H(h)) | (Chan::H(h), Chan::V(v)) => pts.push(Pos::new(x(v), y(h))),
            _ => {}
        }
    }
    pts.push(match *last {
        Chan::V(v) => Pos::new(x(v), pb.y),
        Chan::H(h) => Pos::new(pb.x, y(h)),
    });
    pts.push(pb);
    simplify(pts)
}

/// Drops repeated points and points in the middle of a straight run.
fn simplify(points: Vec<Pos>) -> Vec<Pos> {
    let mut out: Vec<Pos> = Vec::with_capacity(points.len());
    for p in points {
        if out.last().is_some_and(|q| q.dist(p) < 0.01) {
            continue;
        }
        if out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            let straight = ((a.x - b.x).abs() < 0.01 && (b.x - p.x).abs() < 0.01)
                || ((a.y - b.y).abs() < 0.01 && (b.y - p.y).abs() < 0.01);
            if straight {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

fn build_line(
    line: &ViewLine,
    kind: LineKind,
    points: Vec<Pos>,
    tip: Option<Pos>,
    tip_dir: Option<Side>,
) -> LineGeom {
    let (label_at, label_horizontal) = label_position(&points);
    LineGeom {
        relation: line.relation,
        kind,
        arrow_start: line.direction.arrow_at_a(),
        arrow_end: line.direction.arrow_at_b(),
        text: line.text.clone(),
        label_at,
        label_horizontal,
        points,
        tip,
        tip_dir,
        frame_port: None,
        style: line.style,
        full_text: line.text.clone(),
    }
}

/// Middle of the longest horizontal segment, else of the longest vertical one.
fn label_position(points: &[Pos]) -> (Pos, bool) {
    let longest = |horizontal: bool| {
        points
            .windows(2)
            .filter(|w| ((w[0].y - w[1].y).abs() < 0.01) == horizontal)
            .max_by(|p, q| {
                p[0].dist(p[1])
                    .partial_cmp(&q[0].dist(q[1]))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|w| Pos::new((w[0].x + w[1].x) / 2.0, (w[0].y + w[1].y) / 2.0))
    };
    match longest(true) {
        Some(p) => (p, true),
        None => (longest(false).unwrap_or_default(), false),
    }
}
