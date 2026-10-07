//! Deterministic grid layout: boxes sit in the cells of their diagram, columns and
//! rows take the size of their largest box, each box side grows with the lines
//! attached to it, and lines run orthogonally through the gaps between cells.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use crate::geom::{Pos, Rect};
use crate::model::{BlockId, BlockKind, DiagramId, End, Project, RelationId, Rgb, Side};
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
}

impl Layout {
    pub fn block(&self, id: BlockId) -> Option<&BlockGeom> {
        self.blocks.iter().find(|b| b.id == id)
    }
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

    // Port positions: (line index, end) -> (point, channel entry).
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
        let n = list.len() as f32;
        for (i, (_, _, li, end)) in list.iter().enumerate() {
            let t = (i as f32 + 1.0) / (n + 1.0);
            let p = match side {
                Side::Top => Pos::new(rect.min.x + t * rect.width(), rect.min.y),
                Side::Bottom => Pos::new(rect.min.x + t * rect.width(), rect.max.y),
                Side::Left => Pos::new(rect.min.x, rect.min.y + t * rect.height()),
                Side::Right => Pos::new(rect.max.x, rect.min.y + t * rect.height()),
            };
            port_at.insert((*li, *end), (p, *side, chan));
        }
    }

    // Route every line that has two ports.
    let mut routes: BTreeMap<usize, Vec<Chan>> = BTreeMap::new();
    for li in 0..view.lines.len() {
        if let (Some(a), Some(b)) = (port_at.get(&(li, End::A)), port_at.get(&(li, End::B))) {
            routes.insert(li, route(&grid, (a.0, a.2), (b.0, b.2)));
        }
    }
    let offsets = lane_offsets(&routes);

    let mut lines = Vec::new();
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
    };
    (layout, grid)
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

/// Shortest path with few bends over the lattice of gap channels.
fn route(grid: &Grid, a: (Pos, Chan), b: (Pos, Chan)) -> Vec<Chan> {
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
        let mut edges: Vec<(usize, Chan, f32)> = Vec::new();
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
                            edges.push((v * nh + h, a.1, grid.hy[h] - along));
                        }
                    }
                    Chan::H(h) => {
                        for v in 0..nv {
                            edges.push((v * nh + h, a.1, grid.vx[v] - along));
                        }
                    }
                }
                if a.1 == b.1 {
                    edges.push((t, a.1, coord(b.1, b.0) - along));
                }
            }
            Some((v, h)) => {
                if h + 1 < nh {
                    edges.push((node + 1, Chan::V(v), grid.hy[h + 1] - grid.hy[h]));
                }
                if h > 0 {
                    edges.push((node - 1, Chan::V(v), grid.hy[h] - grid.hy[h - 1]));
                }
                if v + 1 < nv {
                    edges.push((node + nh, Chan::H(h), grid.vx[v + 1] - grid.vx[v]));
                }
                if v > 0 {
                    edges.push((node - nh, Chan::H(h), grid.vx[v] - grid.vx[v - 1]));
                }
                match b.1 {
                    Chan::V(bv) if bv == v => edges.push((t, b.1, b.0.y - grid.hy[h])),
                    Chan::H(bh) if bh == h => edges.push((t, b.1, b.0.x - grid.vx[v])),
                    _ => {}
                }
            }
        }
        for (next, chan, len) in edges {
            let next_axis = match chan {
                Chan::V(_) => 1,
                Chan::H(_) => 0,
            };
            let bend = if next_axis == axis { 0 } else { BEND };
            let nd = d + cost(len) + bend;
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

/// Spreads lines that share a channel side by side.
fn lane_offsets(routes: &BTreeMap<usize, Vec<Chan>>) -> BTreeMap<(usize, Chan), f32> {
    let mut users: BTreeMap<Chan, Vec<usize>> = BTreeMap::new();
    for (li, chans) in routes {
        for chan in chans {
            let list = users.entry(*chan).or_default();
            if !list.contains(li) {
                list.push(*li);
            }
        }
    }
    let limit = GAP / 2.0 - 12.0;
    let mut out = BTreeMap::new();
    for (chan, list) in users {
        let n = list.len() as f32;
        for (k, li) in list.into_iter().enumerate() {
            let off = ((k as f32 - (n - 1.0) / 2.0) * LANE).clamp(-limit, limit);
            out.insert((li, chan), off);
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
