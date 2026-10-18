//! Deterministic grid layout: boxes sit in the cells of their diagram, columns and
//! rows take the size of their largest box, each box side grows with the lines
//! attached to it, and lines run orthogonally through the gaps between cells.

mod solve;

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::rc::Rc;
use std::sync::Arc;

use crate::geom::{Pos, Rect};
use crate::look::{Look, LookId, LookMode};
use crate::model::{BlockId, BlockKind, DiagramId, End, LineStyle, Project, RelationId, Rgb, Side};
use crate::text;
use crate::view::{DiagramView, OPEN_PARTNER, ViewEnd, ViewLine};

pub const GAP: f32 = 110.0;
pub const PORT_SPACING: f32 = 40.0;
/// How far a group's frame lies outside its cells, and how much further for every
/// group inside it.
const GROUP_INSET: f32 = 22.0;
const GROUP_STEP: f32 = 24.0;
/// Height of a cross-cutting band, and the space between two of them.
pub const BAND_H: f32 = 56.0;
const BAND_GAP: f32 = 12.0;
pub const MIN_W: f32 = 150.0;
pub const MIN_H: f32 = 80.0;
pub const PAD: f32 = 18.0;
/// Height of a look shown as an icon above the name.
pub const ICON_H: f32 = 32.0;
/// Between a look and the name next to it.
pub const LOOK_GAP: f32 = 6.0;
/// Height of a look that is the shape, with the name below it.
pub const LOOK_H: f32 = MIN_H;
pub const NAME_SIZE: f32 = 15.0;
pub const KIND_SIZE: f32 = 11.0;
pub const LABEL_SIZE: f32 = 12.0;
pub const STUB_LEN: f32 = 40.0;
/// Between lines that share a gap: as far apart as ports, so a line can leave a box
/// and take its lane without a step.
pub const LANE: f32 = PORT_SPACING;
const EMPTY_W: f32 = 520.0;
const EMPTY_H: f32 = 360.0;
const BEND: i64 = 60;
/// Cost of crossing another line: worth a detour of about one cell, no more.
const CROSS: i64 = ((MIN_H + GAP) * 10.0) as i64;
/// Cost of running along a gap beside another line, per line already there: a free
/// gap nearby wins over a slightly shorter route.
const SHARE: i64 = (LANE * 10.0) as i64;
/// How close the legs of an elbow may come to a box they do not end at.
const ELBOW_CLEARANCE: f32 = 12.0;
/// How far from its port a line runs before it goes into a mousehole.
pub const HOLE_IN: f32 = 26.0;
/// A straight line from end to end over more boxes than this goes through
/// mouseholes (unless told otherwise).
pub const HOLE_OVER: usize = 2;
/// What the readability measures cost each other in the drawing's price (see
/// [`solve`]): a line goes under only where that saves three crossings or more,
/// and running along another line is worse than crossing it.
const CROSSING_COST: i64 = 10;
const OVERLAP_COST: i64 = 20;
const HOLE_COST: i64 = 29;
/// A drawn segment through the whitebox title or a cross-cutting band.
const THROUGH_COST: i64 = 29;
/// A branch of a fanned-out relation on a frame point of its own, in the looks part
/// of the price: worth a few bends, never a crossing, so a fan-out keeps its one
/// frame point unless that tangles the picture.
const OWN_FRAME_POINT_COST: i64 = 100;

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
    /// A cross-cutting band across the bottom; it has no lines.
    pub band: bool,
    /// A custom look the box wears.
    pub look: Option<Look>,
    /// With the name below the picture: the picture, which lines from the left,
    /// right and top reach.
    pub picture: Option<Rect>,
    /// With the name below the picture: the frame around the name under it, which
    /// lines from below reach.
    pub caption: Option<Rect>,
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
    /// The text on the other side of the line (below a horizontal segment, left of
    /// a vertical one), where that keeps it off a box or another line.
    pub label_flipped: bool,
    /// The open end of a stub or the loose end of a dangling line.
    pub tip: Option<Pos>,
    /// Direction from the port toward `tip`.
    pub tip_dir: Option<Side>,
    /// Where the line crosses the whitebox frame, with the partner outside.
    pub frame_port: Option<(Pos, Side, String)>,
    /// For a frame end on top or bottom: whether the partner name sits before the
    /// end (left of it), in which row of names (0: next to the frame), and the width
    /// it wraps to.
    pub frame_name: (bool, usize, f32),
    /// Whether the partner name is written at this line's frame end: of the branches
    /// of a fanned-out relation, which share one frame port, only the first has it.
    pub frame_named: bool,
    /// Set when the line goes through mouseholes: only the pieces from the ports to
    /// the holes are drawn; `points` still holds the whole route, shown faintly on
    /// hover.
    pub mousehole: Option<Mousehole>,
    pub style: LineStyle,
    /// The relation's full text; `text` is what the line shows.
    pub full_text: String,
    /// For a line from the frame: the box inside it lands on. A relation can land on
    /// several boxes; each landing is a line of its own from the same frame port.
    pub landing: Option<BlockId>,
    /// Further relations drawn as this line, which then stands for all of them.
    pub bundle: Vec<RelationId>,
    pub pattern: crate::model::LinePattern,
    /// The relation's own colour (`None`: the default).
    pub color: Option<Rgb>,
    /// The line look the relation wears, resolved and tinted for drawing.
    pub look: Option<DrawnLook>,
}

/// A line look as drawn: the project's copy, its end pictures already in the line's
/// colour. A picture the project lacks is left out, and the end is plain.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnLook {
    pub id: LookId,
    pub width: f32,
    pub dash: crate::model::LinePattern,
    pub head: Option<EndPicture>,
    pub tail: Option<EndPicture>,
}

/// A picture at the end of a line.
#[derive(Debug, Clone, PartialEq)]
pub struct EndPicture {
    pub svg: Arc<str>,
    pub size: (f32, f32),
}

/// The project's portals for `line`, at hole a and hole b: the line goes in near its
/// tail and comes out near its head; without a head it goes in at end A.
fn portals(project: &Project, line: &ViewLine) -> (Option<EndPicture>, Option<EndPicture>) {
    let color = line.color.unwrap_or(crate::scene::LINE);
    let picture = |p: &Option<LookId>| {
        let picture = project.pictures.get(p.as_ref()?)?;
        Some(EndPicture {
            svg: crate::look::tinted(picture.svg(), color).into(),
            size: picture.size(),
        })
    };
    let (into, out) = (picture(&project.portal_in), picture(&project.portal_out));
    let heads_to_a = line.direction.arrow_at_a() && !line.direction.arrow_at_b();
    if heads_to_a { (out, into) } else { (into, out) }
}

/// The line look of `line` as the project keeps it, tinted in the line's colour.
fn drawn_look(project: &Project, line: &ViewLine) -> Option<DrawnLook> {
    look_in_color(
        project,
        line.look.as_ref()?,
        line.color.unwrap_or(crate::scene::LINE),
    )
}

/// A line look as the project keeps it, its pictures tinted in `color`.
fn look_in_color(project: &Project, id: &LookId, color: Rgb) -> Option<DrawnLook> {
    let look = project.line_looks.get(id)?;
    let picture = |p: Option<&LookId>| {
        let picture = project.pictures.get(p?)?;
        Some(EndPicture {
            svg: crate::look::tinted(picture.svg(), color).into(),
            size: picture.size(),
        })
    };
    Some(DrawnLook {
        id: id.clone(),
        width: look.width(),
        dash: look.dash(),
        head: picture(look.head()),
        tail: picture(look.tail()),
    })
}

/// A line through mouseholes: the hole near each end, the pieces drawn from the
/// ports to the holes, and the legend number(s) the holes carry.
#[derive(Debug, Clone, PartialEq)]
pub struct Mousehole {
    pub a: Pos,
    pub b: Pos,
    pub piece_a: Vec<Pos>,
    pub piece_b: Vec<Pos>,
    /// `2`, or `2,5` for a bundle.
    pub label: String,
    /// The portals drawn instead of the arches, where the project has them: at hole
    /// `a` and at hole `b`, already in the line's colour.
    pub portal_a: Option<EndPicture>,
    pub portal_b: Option<EndPicture>,
}

impl LineGeom {
    /// What is drawn of the line: the whole route, or the pieces to the mouseholes.
    pub fn drawn(&self) -> Vec<&[Pos]> {
        match &self.mousehole {
            Some(m) => vec![&m.piece_a, &m.piece_b],
            None => vec![&self.points],
        }
    }

    /// How far `p` is from what is drawn of the line (a mousehole counts as drawn).
    pub fn hit_dist(&self, p: Pos) -> f32 {
        let pieces = self
            .drawn()
            .into_iter()
            .map(|piece| crate::geom::polyline_dist(p, piece));
        let holes = self
            .mousehole
            .iter()
            .flat_map(|m| [m.a, m.b])
            .map(|hole| (hole.dist(p) - crate::scene::HOLE_R).max(0.0));
        pieces.chain(holes).fold(f32::MAX, f32::min)
    }

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
    /// The look the whitebox's own box wears, drawn as the frame.
    pub frame_look: Option<FrameLook>,
    /// With a look whose name sits below the picture: the frame around the title
    /// under the frame.
    pub frame_caption: Option<Rect>,
    pub bounds: Rect,
    /// The grid: cell -> rect, for drag-and-drop targets.
    pub cells: Vec<(crate::model::Cell, Rect)>,
    /// Shortened relation texts: key on the line (`[2]` or a short label) and the
    /// full text, already wrapped into lines.
    pub legend: Vec<LegendEntry>,
    /// The line looks used, by name, below the legend's entries.
    pub legend_looks: Vec<LookSample>,
    /// Where the legend is drawn (below the diagram), if there is one.
    pub legend_at: Option<Pos>,
    /// Whether boxes show their type («component») under the name.
    pub captions: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LegendEntry {
    pub key: String,
    pub lines: Vec<String>,
}

/// A look drawn as a whitebox frame: stretched as a nine-slice whose corners keep
/// `corner` (their size when the picture is drawn at a box's height).
#[derive(Debug, Clone, PartialEq)]
pub struct FrameLook {
    pub look: Look,
    pub corner: (f32, f32),
    /// The nine slices of the picture, row by row: corners, edges and the middle, cut
    /// at a quarter of the picture on each side.
    pub slices: Vec<Arc<str>>,
}

/// Height of the caption under a frame drawn from a look with the name below.
pub const FRAME_CAPTION_H: f32 = NAME_SIZE + 2.0 * 6.0;

/// A line look used on the diagram, shown in the legend as a sample with its name.
#[derive(Debug, Clone, PartialEq)]
pub struct LookSample {
    pub name: String,
    pub look: DrawnLook,
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
    line_crossings(&layout.lines)
}

fn line_crossings(lines: &[LineGeom]) -> Vec<Crossing> {
    let mut out = Vec::new();
    for (hl, h) in lines.iter().enumerate() {
        for (vl, v) in lines.iter().enumerate() {
            if h.relation == v.relation {
                continue;
            }
            for (hp, vp) in h
                .drawn()
                .into_iter()
                .flat_map(|hp| v.drawn().into_iter().map(move |vp| (hp, vp)))
            {
                for at in cross_points(hp, vp) {
                    out.push(Crossing {
                        horizontal: hl,
                        vertical: vl,
                        at,
                    });
                }
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

/// Where two lines of different relations run along each other: a stretch two
/// collinear segments share.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Overlap {
    /// Indices into `Layout::lines`, the lower first.
    pub a: usize,
    pub b: usize,
    pub from: Pos,
    pub to: Pos,
}

/// Every stretch where a segment of one relation lies on a segment of another. Worse
/// than a crossing: the reader cannot tell which line goes on where.
pub fn overlaps(layout: &Layout) -> Vec<Overlap> {
    line_overlaps(&layout.lines)
}

fn line_overlaps(lines: &[LineGeom]) -> Vec<Overlap> {
    let mut out = Vec::new();
    for (a, p) in lines.iter().enumerate() {
        for (b, q) in lines.iter().enumerate().skip(a + 1) {
            if p.relation == q.relation {
                continue;
            }
            for pp in p.drawn() {
                for qp in q.drawn() {
                    for (from, to) in overlap_stretches(pp, qp) {
                        out.push(Overlap { a, b, from, to });
                    }
                }
            }
        }
    }
    out
}

/// The stretches where a segment of `p` and a segment of `q` lie on one line and
/// share more than a point.
fn overlap_stretches(p: &[Pos], q: &[Pos]) -> Vec<(Pos, Pos)> {
    let horizontal = |w: &[Pos]| (w[0].y - w[1].y).abs() < 0.01;
    let vertical = |w: &[Pos]| (w[0].x - w[1].x).abs() < 0.01;
    let shared = |a0: f32, a1: f32, b0: f32, b1: f32| {
        let lo = a0.min(a1).max(b0.min(b1));
        let hi = a0.max(a1).min(b0.max(b1));
        (hi > lo + 0.5).then_some((lo, hi))
    };
    let mut out = Vec::new();
    for pw in p.windows(2) {
        for qw in q.windows(2) {
            if horizontal(pw) && horizontal(qw) && (pw[0].y - qw[0].y).abs() < 0.5 {
                if let Some((lo, hi)) = shared(pw[0].x, pw[1].x, qw[0].x, qw[1].x) {
                    out.push((Pos::new(lo, pw[0].y), Pos::new(hi, pw[0].y)));
                }
            } else if vertical(pw)
                && vertical(qw)
                && (pw[0].x - qw[0].x).abs() < 0.5
                && let Some((lo, hi)) = shared(pw[0].y, pw[1].y, qw[0].y, qw[1].y)
            {
                out.push((Pos::new(pw[0].x, lo), Pos::new(pw[0].x, hi)));
            }
        }
    }
    out
}

/// What two polylines cost each other: a crossing counts once, a stretch they share
/// twice.
fn tangles(p: &[Pos], q: &[Pos]) -> usize {
    cross_points(p, q).len() + cross_points(q, p).len() + 2 * overlap_stretches(p, q).len()
}

/// Where a relation wants its port on a side, the line ends that share it, and
/// whether further ends of the relation may join it (a branch that goes under
/// keeps a slot to itself).
type PortSlot = (f32, RelationId, Vec<(usize, End)>, bool);

/// Where a box was put: its rect, and the grid columns and rows it covers.
type Placed = (Rect, (usize, usize), (usize, usize));

/// A group on the page: its id, its box and its rectangle of cells.
type GroupRef<'a> = (
    BlockId,
    &'a crate::model::Block,
    (crate::model::Cell, crate::model::Cell),
);

/// A port on a side: sort key, relation, line index and which end.
type PortRef = (i64, RelationId, usize, End);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Owner {
    Block(BlockId),
    Frame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Chan {
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
    /// Vertical extent of every horizontal gap.
    hspan: Vec<(f32, f32)>,
    col_x: Vec<f32>,
    row_y: Vec<f32>,
    col_w: Vec<f32>,
    row_h: Vec<f32>,
    width: f32,
    height: f32,
}

pub fn layout(project: &Project, view: &DiagramView) -> Layout {
    let bundled;
    let view = if project.bundle {
        bundled = crate::view::bundle(view);
        &bundled
    } else {
        view
    };
    // The first pass finds out how wide the gaps have to be; only the later ones
    // turn labels with no room into numbers (never with shortening turned off).
    // Wider gaps change the routes, which can call for other widths again: built
    // once more while that happens, a few times at most.
    let (first, grid) = build(project, view, &[], &[], false);
    let (mut vgaps, mut hgaps) = widened_gaps(&first, &grid);
    let number = project.label_limit.is_some();
    let mut done = build(project, view, &vgaps, &hgaps, number);
    for _ in 0..2 {
        let (v, h) = widened_gaps(&done.0, &done.1);
        if v == vgaps && h == hgaps {
            break;
        }
        (vgaps, hgaps) = (v, h);
        done = build(project, view, &vgaps, &hgaps, number);
    }
    done.0
}

/// Gaps wide enough for the labels that sit in them and for the lines that run
/// along them, `LANE` apart: vertical gaps, then horizontal ones.
fn widened_gaps(layout: &Layout, grid: &Grid) -> (Vec<f32>, Vec<f32>) {
    let mut gaps: Vec<f32> = grid.vspan.iter().map(|(a, b)| b - a).collect();
    let mut hgaps: Vec<f32> = grid.hspan.iter().map(|(a, b)| b - a).collect();
    let within =
        |spans: &[(f32, f32)], c: f32| spans.iter().position(|(a, b)| c > *a + 0.5 && c < *b - 0.5);
    let mut along_v: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); grid.vspan.len()];
    let mut along_h: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); grid.hspan.len()];
    for (li, line) in layout.lines.iter().enumerate() {
        if line.mousehole.is_some() {
            continue;
        }
        for w in line.points.windows(2) {
            if (w[0].x - w[1].x).abs() < 0.01 {
                if let Some(v) = within(&grid.vspan, w[0].x) {
                    along_v[v].insert(li);
                }
            } else if (w[0].y - w[1].y).abs() < 0.01
                && let Some(h) = within(&grid.hspan, w[0].y)
            {
                along_h[h].insert(li);
            }
        }
    }
    for (v, lines) in along_v.iter().enumerate() {
        gaps[v] = gaps[v].max((lines.len() as f32 + 1.0) * LANE);
    }
    for (h, lines) in along_h.iter().enumerate() {
        hgaps[h] = hgaps[h].max((lines.len() as f32 + 1.0) * LANE);
    }
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
    (gaps, hgaps)
}

fn build(
    project: &Project,
    view: &DiagramView,
    vgaps: &[f32],
    hgaps: &[f32],
    number_labels: bool,
) -> (Layout, Grid) {
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

    // Box sizes; for a box with its name below its picture also the picture's and
    // the name frame's sizes.
    let mut sizes = BTreeMap::new();
    let mut look_parts: BTreeMap<BlockId, BelowParts> = BTreeMap::new();
    for id in &view.blocks {
        let Some(b) = project.blocks.get(id).filter(|b| !b.gridless()) else {
            continue;
        };
        let o = Owner::Block(*id);
        // The text: the name and its type caption, or a doodle shape's lines.
        let (text_w, text_h) = if project.is_doodle() {
            let w = b
                .name
                .lines()
                .map(|l| text::width(l, NAME_SIZE))
                .fold(0.0, f32::max);
            let n = b.name.lines().count().max(1) as f32;
            (w, n * crate::scene::NAME_LINE)
        } else {
            let w =
                text::width(&b.name, NAME_SIZE).max(text::width(&kind_caption(b.kind), KIND_SIZE));
            (w, 0.0)
        };
        // Round and pointed shapes need room around the text.
        let (fw, fh) = match b.kind {
            BlockKind::Diamond => (1.9, 2.0),
            BlockKind::Circle | BlockKind::Cloud => (1.45, 1.6),
            _ => (1.0, 1.0),
        };
        // Room for the lines on each side.
        let ports_w = (count(o, Side::Top).max(count(o, Side::Bottom)) + 1.0) * PORT_SPACING;
        let ports_h = (count(o, Side::Left).max(count(o, Side::Right)) + 1.0) * PORT_SPACING;
        let w = MIN_W.max((text_w + 2.0 * PAD) * fw).max(ports_w);
        let h = MIN_H.max((text_h + 2.0 * PAD) * fh).max(ports_h);
        let (w, h) = match b.look.as_ref().and_then(|l| project.looks.get(l)) {
            Some(look) => {
                let counts =
                    [Side::Top, Side::Right, Side::Bottom, Side::Left].map(|s| count(o, s));
                let (w, h, parts) =
                    look_box(look, (w, h), (text_w, text_block(project, b)), counts);
                if let Some(parts) = parts {
                    look_parts.insert(*id, parts);
                }
                (w, h)
            }
            None => (w, h),
        };
        sizes.insert(*id, (b.cell, w, h));
    }
    // A group's corners are columns and rows of the grid, even with no shape there.
    let mut groups: Vec<GroupRef> = view
        .blocks
        .iter()
        .filter_map(|id| {
            let b = project.blocks.get(id)?;
            Some((*id, b, b.group_cells()?))
        })
        .collect();
    // Outer groups first, so the inner ones are drawn on top of them.
    groups.sort_by_key(|(_, _, (near, far))| {
        std::cmp::Reverse((far.col - near.col + 1) * (far.row - near.row + 1))
    });
    let mut grid_cells = sizes.clone();
    for (k, (_, _, (near, far))) in groups.iter().enumerate() {
        for (j, corner) in [near, far].into_iter().enumerate() {
            let phantom = BlockId(u64::MAX - (2 * k + j) as u64);
            grid_cells.insert(phantom, (*corner, 0.0, 0.0));
        }
    }
    let grid = grid(&grid_cells, vgaps, hgaps);

    // Box rects, with the columns and rows each covers.
    let mut blocks = Vec::new();
    let mut rects: BTreeMap<BlockId, Placed> = BTreeMap::new();
    // Group frames first, so they lie behind the shapes. A frame sits in the gaps
    // around its cells, further out the more groups it holds.
    for (id, b, (near, far)) in &groups {
        let (c0, c1) = (grid.cols[&near.col], grid.cols[&far.col]);
        let (r0, r1) = (grid.rows[&near.row], grid.rows[&far.row]);
        let holds = groups
            .iter()
            .filter(|(o, _, cells)| {
                o != id
                    && crate::model::inside_cells(cells.0, (*near, *far))
                    && crate::model::inside_cells(cells.1, (*near, *far))
            })
            .count() as f32;
        let inset = (GROUP_INSET + holds * GROUP_STEP).min(GAP / 2.0 - 6.0);
        let rect = Rect {
            min: Pos::new(grid.col_x[c0] - inset, grid.row_y[r0] - inset),
            max: Pos::new(
                grid.col_x[c1] + grid.col_w[c1] + inset,
                grid.row_y[r1] + grid.row_h[r1] + inset,
            ),
        };
        rects.insert(*id, (rect, (c0, c1), (r0, r1)));
        blocks.push(BlockGeom {
            id: *id,
            rect,
            name: b.name.clone(),
            kind: b.kind,
            fill: b.color.unwrap_or(DEFAULT_FILL),
            has_content: false,
            band: false,
            look: None,
            picture: None,
            caption: None,
        });
    }
    // Where lines reach a box whose name sits below its picture: the picture, and
    // from below the name's frame.
    let mut look_rects: BTreeMap<BlockId, (Rect, Rect)> = BTreeMap::new();
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
        rects.insert(*id, (rect, (ci, ci), (ri, ri)));
        let (picture, caption) = match look_parts.get(id) {
            Some(&BelowParts {
                picture: (pw, ph),
                frame: (fw, fh),
            }) => {
                let picture =
                    Rect::from_min_size(Pos::new(center.x - pw / 2.0, rect.min.y), pw, ph);
                let caption =
                    Rect::from_min_size(Pos::new(center.x - fw / 2.0, rect.max.y - fh), fw, fh);
                look_rects.insert(*id, (picture, caption));
                (Some(picture), Some(caption))
            }
            None => (None, None),
        };
        let fill = b.color.unwrap_or_else(|| {
            b.tag
                .and_then(|t| project.tags.get(&t))
                .map(|t| t.color)
                .unwrap_or(if b.kind.is_neighbour() && !project.is_doodle() {
                    NEIGHBOUR_FILL
                } else {
                    DEFAULT_FILL
                })
        });
        blocks.push(BlockGeom {
            id: *id,
            rect,
            name: b.name.clone(),
            kind: b.kind,
            fill,
            has_content: project.has_content(*id),
            band: false,
            look: b.look.as_ref().and_then(|l| project.looks.get(l)).cloned(),
            picture,
            caption,
        });
    }

    // Cross-cutting bands: one below the other under the grid, as wide as the picture.
    let bands: Vec<(BlockId, &crate::model::Block)> = view
        .blocks
        .iter()
        .filter_map(|id| project.blocks.get(id).filter(|b| b.band).map(|b| (*id, b)))
        .collect();
    let mut width = grid.width;
    for (_, b) in &bands {
        let text_w = text::width(&b.name, NAME_SIZE)
            .max(text::width(&kind_caption(b.kind), KIND_SIZE))
            + 2.0 * PAD;
        width = width.max(text_w + GAP);
    }
    let mut y = grid.height;
    for (id, b) in &bands {
        let rect = Rect::from_min_size(Pos::new(GAP / 2.0, y), width - GAP, BAND_H);
        y += BAND_H + BAND_GAP;
        blocks.push(BlockGeom {
            id: *id,
            rect,
            name: b.name.clone(),
            kind: b.kind,
            fill: b
                .tag
                .and_then(|t| project.tags.get(&t))
                .map_or(DEFAULT_FILL, |t| t.color),
            has_content: project.has_content(*id),
            band: true,
            look: None,
            picture: None,
            caption: None,
        });
    }
    // Below the bands a full gap, so ends entering through the bottom of a whitebox
    // frame have room before they reach the bands.
    let height = if bands.is_empty() {
        grid.height
    } else {
        y - BAND_GAP + GAP
    };
    let bounds = Rect::from_min_size(Pos::new(0.0, 0.0), width, height);
    // A doodle page is no whitebox: no frame around it.
    let frame = view
        .diagram
        .filter(|_| !project.is_doodle())
        .map(|_| bounds);
    // The whitebox's own look is its frame; its corners keep the size they have on
    // the box drawn at the plain box height.
    let own = view.diagram.and_then(|d| project.blocks.get(&d));
    let frame_look = frame.and_then(|_| {
        let look = project.looks.get(own?.look.as_ref()?)?.clone();
        let (pw, ph) = look.size();
        let scale = MIN_H / ph;
        let parts = [0.0, 0.25, 0.75, 1.0];
        let slices = (0..3)
            .flat_map(|row| (0..3).map(move |col| (row, col)))
            .map(|(row, col)| {
                let part = (
                    parts[col],
                    parts[row],
                    parts[col + 1] - parts[col],
                    parts[row + 1] - parts[row],
                );
                crate::look::slice(look.svg(), part).into()
            })
            .collect();
        Some(FrameLook {
            corner: (pw * scale / 4.0, ph * scale / 4.0),
            slices,
            look,
        })
    });
    // With the name below the picture, the title goes into a caption under the
    // frame, and the picture grows by it.
    let caption_h = frame_look
        .as_ref()
        .filter(|fl| fl.look.mode() == LookMode::Below)
        .map(|_| FRAME_CAPTION_H + GAP / 4.0);
    let frame_caption = caption_h.zip(frame).map(|(_, f)| {
        let title = format!("{} (whitebox)", own.map_or("", |b| &b.name));
        let w = text::width(&title, NAME_SIZE) + 2.0 * PAD;
        Rect::from_min_size(
            Pos::new(f.center().x - w / 2.0, f.max.y + GAP / 8.0),
            w,
            FRAME_CAPTION_H,
        )
    });
    let bounds = match caption_h {
        Some(h) => Rect::from_min_size(bounds.min, bounds.width(), bounds.height() + h),
        None => bounds,
    };

    // Port positions: (line index, end) -> (point, channel entry). A port sits where
    // its partner is, so facing boxes get straight lines; ports on one side then keep
    // a minimum distance from each other.
    // What a port sits on: the box, or of a box with its name below its picture the
    // picture (from below: the name's frame).
    // The title is written inside the top left corner of the frame; frame points on
    // the top side start right of it.
    let title_end = match (frame, frame_caption, own) {
        (Some(f), None, Some(b)) => {
            let title = format!("{} (whitebox)", b.name);
            f.min.x + 16.0 + text::width(&title, NAME_SIZE) + 40.0
        }
        _ => f32::MIN,
    };
    let port_rect = |owner: Owner, side: Side| match owner {
        Owner::Block(id) => match look_rects.get(&id) {
            Some((picture, caption)) => Some(if side == Side::Bottom {
                *caption
            } else {
                *picture
            }),
            None => rects.get(&id).map(|(r, _, _)| *r),
        },
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
    // Where a port wants to be: in line with a facing partner, so the line runs
    // straight; otherwise in the middle of its side (a frame end around a corner
    // from its box: over that box).
    let ideal = |li: usize, end: End, owner: Owner, side: Side| -> Option<f32> {
        let own = range(port_rect(owner, side)?, side);
        let middle = (own.0 + own.1) / 2.0;
        let Some(&(o2, s2)) = ends.get(&(li, end.other())) else {
            return Some(middle);
        };
        let other_rect = port_rect(o2, s2)?;
        if side.is_horizontal() == s2.is_horizontal() {
            let other = range(other_rect, s2);
            let (lo, hi) = (own.0.max(other.0), own.1.min(other.1));
            Some(if lo <= hi { (lo + hi) / 2.0 } else { middle })
        } else if owner == Owner::Frame {
            let c = other_rect.center();
            let along = if side.is_horizontal() { c.y } else { c.x };
            Some(along.clamp(own.0, own.1))
        } else {
            Some(middle)
        }
    };
    // Place every side's ports for given wishes, in the order of `order` where given
    // (else of the wishes); returns the coordinate along the side.
    let place = |wish: &BTreeMap<(usize, End), f32>,
                 order: Option<&BTreeMap<(usize, End), OrderKey>>,
                 unshared: &BTreeSet<(usize, End)>|
     -> BTreeMap<(usize, End), f32> {
        let mut out = BTreeMap::new();
        for ((owner, side), list) in &ports {
            let Some(rect) = port_rect(*owner, *side) else {
                continue;
            };
            let (lo, hi) = range(rect, *side);
            let lo = if (*owner, *side) == (Owner::Frame, Side::Top) {
                lo.max(title_end - PORT_SPACING / 2.0).min(hi)
            } else {
                lo
            };
            // A relation that fans out to several boxes of a whitebox enters through
            // one frame port: its ends there share a slot, at the mean of their wishes.
            // Except the branches that go under (`unshared`): each needs a hole pair
            // of its own, so each gets its own frame point.
            let mut wanted: Vec<(OrderKey, PortSlot)> = Vec::new();
            for (_, rel, li, end) in list {
                let at = wish.get(&(*li, *end)).copied().unwrap_or((lo + hi) / 2.0);
                let key = order.and_then(|o| o.get(&(*li, *end)).copied()).unwrap_or((
                    0,
                    0,
                    (at * 10.0).round() as i64,
                ));
                let shared = *owner == Owner::Frame && !unshared.contains(&(*li, *end));
                match wanted
                    .iter_mut()
                    .find(|(_, w)| shared && w.3 && w.1 == *rel)
                {
                    Some((k, w)) => {
                        let n = w.2.len() as f32;
                        w.0 = (w.0 * n + at) / (n + 1.0);
                        *k = (*k).min(key);
                        w.2.push((*li, *end));
                    }
                    None => wanted.push((key, (at, *rel, vec![(*li, *end)], shared))),
                }
            }
            // In the order of their wishes; only slots that would collide take the
            // order their lines arrive in, so a slot far from the rest stays where
            // its box is instead of being dragged next to them.
            wanted.sort_by(|(_, a), (_, b)| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let mut start = 0;
            while start < wanted.len() {
                let mut end = start + 1;
                while end < wanted.len() && wanted[end].1.0 - wanted[end - 1].1.0 < PORT_SPACING {
                    end += 1;
                }
                wanted[start..end].sort_by(|(ka, a), (kb, b)| {
                    ka.cmp(kb)
                        .then(a.0.total_cmp(&b.0))
                        .then(a.1.cmp(&b.1))
                        .then(a.2.cmp(&b.2))
                });
                start = end;
            }
            let at: Vec<f32> = wanted.iter().map(|(_, w)| w.0).collect();
            let gaps = vec![PORT_SPACING; wanted.len()];
            let placed = spread_by(&at, lo + PORT_SPACING / 2.0, hi - PORT_SPACING / 2.0, &gaps);
            for ((_, (_, _, members, _)), v) in wanted.iter().zip(placed) {
                for (li, end) in members {
                    out.insert((*li, *end), v);
                }
            }
        }
        out
    };
    let busy = |owner: Owner, side: Side| ports.get(&(owner, side)).map_or(0, Vec::len);
    // Places every port: where it wants to be, then facing ends agree so the line
    // runs straight: a frame end follows its box, and of two boxes the one on the
    // busier side keeps its place. Where the spacing on the frame then pushed a
    // frame end a little off its box, the box end follows it back, so no line ends
    // in a step of a few pixels.
    let settle = |order: Option<&BTreeMap<(usize, End), OrderKey>>,
                  unshared: &BTreeSet<(usize, End)>|
     -> BTreeMap<(usize, End), f32> {
        let first_wish: BTreeMap<(usize, End), f32> = ends
            .iter()
            .filter_map(|(&(li, end), &(owner, side))| {
                ideal(li, end, owner, side).map(|v| ((li, end), v))
            })
            .collect();
        let first = place(&first_wish, order, unshared);
        let second_wish: BTreeMap<(usize, End), f32> = first
            .iter()
            .map(|(&(li, end), &v)| {
                let own = ends.get(&(li, end));
                let other = ends.get(&(li, end.other()));
                let follow = match (own, other, first.get(&(li, end.other()))) {
                    (Some(&(o1, s1)), Some(&(o2, s2)), Some(&w))
                        if s1.is_horizontal() == s2.is_horizontal()
                            && (o1 == Owner::Frame
                                || (o2 != Owner::Frame && busy(o2, s2) > busy(o1, s1))) =>
                    {
                        Some(w)
                    }
                    _ => None,
                };
                ((li, end), follow.unwrap_or(v))
            })
            .collect();
        let second = place(&second_wish, order, unshared);
        let third_wish: BTreeMap<(usize, End), f32> = second
            .iter()
            .map(|(&(li, end), &v)| {
                let own = ends.get(&(li, end));
                let other = ends.get(&(li, end.other()));
                let follow = match (own, other, second.get(&(li, end.other()))) {
                    (Some(&(o1 @ Owner::Block(_), s1)), Some(&(Owner::Frame, s2)), Some(&w))
                        if s1.is_horizontal() == s2.is_horizontal()
                            && (w - v).abs() > 0.01
                            && (w - v).abs() < PORT_SPACING =>
                    {
                        // Never past a neighbour on the side: the order of the ports
                        // there is what keeps their lines from crossing on the way in.
                        let keeps_order = ports
                            .get(&(o1, s1))
                            .into_iter()
                            .flatten()
                            .filter(|(_, _, l, e)| (*l, *e) != (li, end))
                            .filter_map(|(_, _, l, e)| second.get(&(*l, *e)))
                            .all(|q| (w - q).signum() == (v - q).signum());
                        keeps_order.then_some(w)
                    }
                    _ => None,
                };
                ((li, end), follow.unwrap_or(v))
            })
            .collect();
        place(&third_wish, order, unshared)
    };
    // The ports as points on their sides, with the channel each line leaves into.
    let ports_at = |along: &BTreeMap<(usize, End), f32>| -> PortAt {
        let mut port_at = PortAt::new();
        for ((owner, side), list) in &ports {
            let (rect, chan) = match owner {
                Owner::Block(id) => {
                    let Some((rect, (c0, c1), (r0, r1))) = rects.get(id) else {
                        continue;
                    };
                    let chan = match side {
                        Side::Right => Chan::V(c1 + 1),
                        Side::Left => Chan::V(*c0),
                        Side::Top => Chan::H(*r0),
                        Side::Bottom => Chan::H(r1 + 1),
                    };
                    (port_rect(*owner, *side).unwrap_or(*rect), chan)
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
        port_at
    };

    // The direction a line leaves an end in: away from a box, into the frame.
    let heading = |li: usize, end: End, side: Side| match ends.get(&(li, end)) {
        Some((Owner::Frame, _)) => side.opposite(),
        _ => side,
    };
    // Lines cross group frames freely.
    let boxes: Vec<Rect> = blocks
        .iter()
        .filter(|b| b.kind != BlockKind::Group)
        .map(|b| b.rect)
        .collect();
    // Route every line that has two ports, each against all the others. Then the
    // ports of a side take the order their lines arrive in, so no line crosses
    // another on its way in, and the lines are routed again from there.
    let mut port_at = ports_at(&settle(None, &BTreeSet::new()));
    // Lines through mouseholes: as told, or when a straight line from end to end
    // would run over more than HOLE_OVER boxes. Their route is not drawn, so the
    // other lines neither avoid it nor leave it a lane.
    let mut hidden: BTreeSet<usize> = (0..view.lines.len())
        .filter(|li| {
            let (Some(a), Some(b)) = (port_at.get(&(*li, End::A)), port_at.get(&(*li, End::B)))
            else {
                return false;
            };
            match view.lines[*li].mousehole {
                crate::model::MouseholeMode::Always => true,
                crate::model::MouseholeMode::Never => false,
                crate::model::MouseholeMode::Automatic => {
                    crossed_boxes(a.0, b.0, &boxes) > HOLE_OVER
                }
            }
        })
        .collect();
    let n = view.lines.len();
    // Settled: lines told to go under, and lines whose straight line runs over too
    // many boxes. Everything else the routing still decides is judged by one cost
    // over the drawing (see `solve`): which of a few candidate routes a line takes,
    // the order of the lanes in a channel, which automatic lines go under, and which
    // branches of a fanned-out relation leave the shared frame point for one of
    // their own, in line with their box.
    let forced = hidden.clone();
    let automatic: BTreeSet<usize> = (0..n)
        .filter(|li| {
            !forced.contains(li)
                && view.lines[*li].mousehole == crate::model::MouseholeMode::Automatic
                && port_at.contains_key(&(*li, End::A))
                && port_at.contains_key(&(*li, End::B))
        })
        .collect();
    // The frame end of a branch that still shares its frame point with another
    // branch of its relation.
    let shared_end = |li: usize, unshared: &BTreeSet<(usize, End)>| -> Option<(usize, End)> {
        let (l, end) = ends
            .iter()
            .find(|((l, _), (owner, _))| *l == li && *owner == Owner::Frame)
            .map(|(key, _)| *key)?;
        let ViewEnd::Frame { side, .. } = view.lines[li].end(end) else {
            return None;
        };
        let rel = view.lines[li].relation;
        let shares = !unshared.contains(&(l, end))
            && ends.iter().any(|((lj, ej), (owner, sj))| {
                *lj != li
                    && *owner == Owner::Frame
                    && sj == side
                    && view.lines[*lj].relation == rel
                    && !unshared.contains(&(*lj, *ej))
            });
        shares.then_some((l, end))
    };
    // What no line should run through: the title and the bands.
    let mut keep_off: Vec<Rect> = blocks.iter().filter(|b| b.band).map(|b| b.rect).collect();
    if let Some(f) = frame
        && title_end > f32::MIN
    {
        keep_off.push(Rect::from_min_size(f.min, title_end - f.min.x, 28.0));
    }
    // Ports and candidate routes for one set of frame ends on a frame point of their
    // own (a branch that goes under needs one for its holes); worked out once per
    // set, with every line routed, so that a route keeps its meaning whichever
    // lines go under.
    let mut prepared: PreparedFor = BTreeMap::new();
    let none: BTreeSet<usize> = BTreeSet::new();
    let mut prepare = |hidden: &BTreeSet<usize>, unshared: &BTreeSet<(usize, End)>| {
        let mut apart = unshared.clone();
        for ((li, end), (owner, _)) in &ends {
            if *owner == Owner::Frame && hidden.contains(li) {
                apart.insert((*li, *end));
            }
        }
        let key: Vec<(usize, End)> = apart.iter().copied().collect();
        if let Some(p) = prepared.get(&key) {
            return p.clone();
        }
        let hidden = &none;
        let mut port_at = ports_at(&settle(None, &apart));
        let mut routes: BTreeMap<usize, Route> = BTreeMap::new();
        let mut segments: Segments = BTreeMap::new();
        route_lines(
            &grid,
            &boxes,
            &port_at,
            &heading,
            n,
            2,
            hidden,
            &mut routes,
            &mut segments,
        );
        // The ports of a side take the order their lines arrive in, twice over.
        for _ in 0..2 {
            let lanes = lane_order(&grid, &routes, &port_at, hidden);
            let offsets = lane_offsets(&lanes, &grid);
            let drawn = points_of(&grid, &offsets, &port_at, &routes);
            let order = arrival_keys(&drawn, &ends);
            port_at = ports_at(&settle(Some(&order), &apart));
            route_lines(
                &grid,
                &boxes,
                &port_at,
                &heading,
                n,
                1,
                hidden,
                &mut routes,
                &mut segments,
            );
        }
        let candidates: Vec<Vec<Route>> = (0..n)
            .map(|li| alternatives(&grid, &boxes, &heading, &port_at, li, &segments))
            .collect();
        let p = Rc::new(Prepared {
            port_at,
            candidates,
        });
        prepared.insert(key, p.clone());
        p
    };
    let mut evaluate = |choice: &solve::Choice| -> (solve::Cost, Drawing) {
        let prepared = prepare(&choice.hidden, &choice.unshared);
        let routes: BTreeMap<usize, Route> = prepared
            .candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_empty())
            .map(|(li, c)| {
                let wanted = choice.route.get(li).and_then(|r| r.as_ref());
                let pick = wanted
                    .and_then(|r| c.iter().position(|k| k == r))
                    .unwrap_or(0);
                (li, c[pick].clone())
            })
            .collect();
        let lanes = solve::reconcile(
            &choice.lanes,
            &lane_order(&grid, &routes, &prepared.port_at, &choice.hidden),
        );
        let offsets = lane_offsets(&lanes, &grid);
        let points = points_of(&grid, &offsets, &prepared.port_at, &routes);
        // What is drawn: whole lines, and of a line under only the pieces to its
        // holes, which can still cross something.
        let mut drawn: Vec<(RelationId, Vec<Pos>)> = Vec::new();
        for (li, p) in &points {
            let rel = view.lines[*li].relation;
            if choice.hidden.contains(li) {
                let hole = mousehole_of(p, String::new());
                drawn.push((rel, hole.piece_a));
                drawn.push((rel, hole.piece_b));
            } else {
                drawn.push((rel, p.clone()));
            }
        }
        let whole: Vec<&[Pos]> = points.values().map(Vec::as_slice).collect();
        let mut cost = drawing_cost(&drawn, &whole, choice.hidden.len(), &keep_off);
        cost.cosmetic += choice.unshared.len() as i64 * OWN_FRAME_POINT_COST;
        (
            cost,
            Drawing {
                routes,
                lanes,
                offsets,
                points,
                prepared,
            },
        )
    };
    // The neighbours of a choice, cheapest remedies first: a branch on a frame point
    // of its own, another route, another lane, and last a line under or back up.
    let moves = |choice: &solve::Choice, drawing: &Drawing| -> Vec<solve::Choice> {
        let base = solve::Choice {
            lanes: drawing.lanes.clone(),
            ..choice.clone()
        };
        let mut out = Vec::new();
        for li in 0..n {
            if !base.hidden.contains(&li)
                && let Some(end) = shared_end(li, &base.unshared)
            {
                let mut c = base.clone();
                c.unshared.insert(end);
                out.push(c);
            }
        }
        for (li, candidates) in drawing.prepared.candidates.iter().enumerate() {
            if base.hidden.contains(&li) {
                continue;
            }
            let now = drawing.routes.get(&li);
            for candidate in candidates {
                if Some(candidate) != now {
                    let mut c = base.clone();
                    c.route[li] = Some(candidate.clone());
                    out.push(c);
                }
            }
        }
        for (chan, order) in solve::lane_moves(&drawing.lanes) {
            let mut c = base.clone();
            c.lanes.insert(chan, order);
            out.push(c);
        }
        for li in &automatic {
            let mut c = base.clone();
            if !c.hidden.remove(li) {
                c.hidden.insert(*li);
            }
            out.push(c);
        }
        out
    };
    let start = solve::Choice {
        route: vec![None; n],
        lanes: Lanes::new(),
        hidden: hidden.clone(),
        unshared: BTreeSet::new(),
    };
    let (choice, drawing) = solve::local_search(start, &mut evaluate, &moves, solve::BUDGET);
    hidden = choice.hidden;
    port_at = drawing.prepared.port_at.clone();
    let routes = drawing.routes;
    let offsets = drawing.offsets;
    let drawn = drawing.points;
    // Two lines of different relations that leave facing sides at one height toward
    // different lanes would run along each other between the boxes and the lanes:
    // the port of one of them moves half a lane along its side (where it has the
    // most room), and the lines are routed again from there.
    let mut nudged = BTreeSet::new();
    let indices: Vec<usize> = drawn
        .keys()
        .copied()
        .filter(|li| !hidden.contains(li))
        .collect();
    for (i, &li) in indices.iter().enumerate() {
        for &lj in &indices[i + 1..] {
            if view.lines[li].relation == view.lines[lj].relation {
                continue;
            }
            let (p, q) = (&drawn[&li], &drawn[&lj]);
            for (from, to) in overlap_stretches(p, q) {
                let on_stem = |pts: &[Pos], line: usize| -> Option<(usize, End)> {
                    let n = pts.len();
                    let holds =
                        |a: Pos, b: Pos| !overlap_stretches(&[a, b], &[from, to]).is_empty();
                    if n >= 2 && holds(pts[0], pts[1]) {
                        Some((line, End::A))
                    } else if n >= 2 && holds(pts[n - 2], pts[n - 1]) {
                        Some((line, End::B))
                    } else {
                        None
                    }
                };
                let Some((line, end)) = on_stem(p, li).or_else(|| on_stem(q, lj)) else {
                    continue;
                };
                let Some(&(owner, side)) = ends.get(&(line, end)) else {
                    continue;
                };
                // A frame port is shared by the branches of a fanned-out relation.
                if owner == Owner::Frame || !nudged.insert((line, end)) {
                    continue;
                }
                let (Some(&(pos, s, chan)), Some(rect)) =
                    (port_at.get(&(line, end)), port_rect(owner, side))
                else {
                    continue;
                };
                let (lo, hi) = range(rect, side);
                let coord = |p: Pos| if side.is_horizontal() { p.y } else { p.x };
                let others: Vec<f32> = ports
                    .get(&(owner, side))
                    .into_iter()
                    .flatten()
                    .filter(|(_, _, l, e)| (*l, *e) != (line, end))
                    .filter_map(|(_, _, l, e)| port_at.get(&(*l, *e)))
                    .map(|(p, _, _)| coord(*p))
                    .collect();
                let room = |v: f32| {
                    v >= lo + PORT_SPACING / 2.0 - 0.01 && v <= hi - PORT_SPACING / 2.0 + 0.01
                };
                let space = |v: f32| {
                    others
                        .iter()
                        .map(|o| (o - v).abs())
                        .fold(f32::MAX, f32::min)
                };
                let at = coord(pos);
                let best = [at - PORT_SPACING / 2.0, at + PORT_SPACING / 2.0]
                    .into_iter()
                    .filter(|v| room(*v))
                    .max_by(|a, b| space(*a).total_cmp(&space(*b)).then(b.total_cmp(a)));
                if let Some(v) = best {
                    let p = if side.is_horizontal() {
                        Pos::new(pos.x, v)
                    } else {
                        Pos::new(v, pos.y)
                    };
                    port_at.insert((line, end), (p, s, chan));
                }
            }
        }
    }

    let mut lines = Vec::new();
    let (shown, mut legend_keys, hole_labels) =
        shown_labels(project, view, project.label_limit, &hidden);
    let partners = short_partners(view, project.label_limit, &mut legend_keys);
    for (li, line) in view.lines.iter().enumerate() {
        let look = drawn_look(project, line);
        let pa = port_at.get(&(li, End::A)).copied();
        let pb = port_at.get(&(li, End::B)).copied();
        let geom = match (pa, pb) {
            (Some(a), Some(b)) => {
                let route = routes.get(&li).cloned().unwrap_or(Route::Straight);
                let mut points = route_points(&grid, &offsets, li, a.0, b.0, &route);
                // Round and pointed shapes: the line reaches their outline.
                let outline = |end: &ViewEnd, port: (Pos, Side, Chan)| match end {
                    ViewEnd::Block { block, .. } => {
                        let kind = project.blocks.get(block).map(|b| b.kind);
                        let rect = rects.get(block).map(|r| r.0);
                        match (kind, rect) {
                            (Some(k), Some(r)) => on_outline(k, r, port.1, port.0),
                            _ => port.0,
                        }
                    }
                    _ => port.0,
                };
                if let Some(first) = points.first_mut() {
                    *first = outline(&line.a, a);
                }
                if let Some(last) = points.last_mut() {
                    *last = outline(&line.b, b);
                }
                let mut geom = build_line(line, LineKind::Routed, points, None, None, look);
                if hidden.contains(&li) {
                    let mut hole = mousehole_of(&geom.points, hole_labels[li].clone());
                    (hole.portal_a, hole.portal_b) = portals(project, line);
                    geom.mousehole = Some(hole);
                    geom.label_at = geom.mousehole.as_ref().map_or(geom.label_at, |m| m.a);
                }
                geom
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
                build_line(line, kind, points, Some(tip), Some(dir), look)
            }
            (None, None) => continue,
        };
        let frame_port = [(End::A, pa), (End::B, pb)]
            .into_iter()
            .find_map(|(end, port)| match (line.end(end), port) {
                (ViewEnd::Frame { partner, side }, Some(p)) => Some((
                    p.0,
                    *side,
                    partners
                        .get(partner)
                        .cloned()
                        .unwrap_or_else(|| partner.clone()),
                )),
                _ => None,
            });
        lines.push(LineGeom { frame_port, ..geom });
    }
    // A line that still enters through the bottom, set so by hand, would cross the
    // cross-cutting bands on its way: it goes under them instead, a hole before it
    // meets the first band and one after it leaves the last, numbered per relation
    // (the branches of a fanned-out relation share the stem there, so they share
    // the number).
    let band_span = blocks
        .iter()
        .filter(|b| b.band)
        .map(|b| b.rect)
        .reduce(|a, b| a.union(b));
    if let Some(span) = band_span {
        let mut band_numbers: BTreeMap<RelationId, String> = BTreeMap::new();
        for (li, geom) in lines.iter_mut().enumerate() {
            if geom.mousehole.is_some() || geom.tip.is_some() {
                continue;
            }
            let (mut along, mut entry, mut exit) = (0.0, None, None);
            for w in geom.points.windows(2) {
                let len = w[0].dist(w[1]);
                if let Some((t0, t1)) = clip_params(w[0], w[1], &span) {
                    entry.get_or_insert(along + t0 * len);
                    exit = Some(along + t1 * len);
                }
                along += len;
            }
            let (Some(entry), Some(exit)) = (entry, exit) else {
                continue;
            };
            if exit - entry < 1.0 {
                continue;
            }
            // The holes sit just before and just after the bands, their arches
            // outside them.
            let (piece_a, _) = cut_at(&geom.points, entry - 2.0 * crate::scene::HOLE_R);
            let (_, piece_b) = cut_at(&geom.points, exit + 2.0 * crate::scene::HOLE_R);
            let numbered = legend_keys
                .iter()
                .filter(|(k, _)| k.starts_with('['))
                .count();
            let full = geom.full_text.trim().to_owned();
            let short = view.lines[li].short.clone();
            let key = band_numbers
                .entry(geom.relation)
                .or_insert_with(|| {
                    let key = format!("[{}]", numbered + 1);
                    // A short label's entry explains the number from now on.
                    let under_short = legend_keys
                        .iter_mut()
                        .find(|(k, text)| !short.is_empty() && *k == short && *text == full);
                    match under_short {
                        Some(entry) => entry.0 = key.clone(),
                        None => {
                            let entry = if full.is_empty() {
                                ends_of(project, &view.lines[li])
                            } else {
                                full
                            };
                            legend_keys.push((key.clone(), entry));
                        }
                    }
                    key
                })
                .clone();
            let mut hole = Mousehole {
                a: piece_a.last().copied().unwrap_or_default(),
                b: piece_b.first().copied().unwrap_or_default(),
                piece_a,
                piece_b,
                label: key.trim_matches(['[', ']']).to_owned(),
                portal_a: None,
                portal_b: None,
            };
            (hole.portal_a, hole.portal_b) = portals(project, &view.lines[li]);
            geom.label_at = hole.a;
            geom.text = view.lines[li].short.clone();
            geom.mousehole = Some(hole);
        }
    }
    // A fanned-out relation names its partner once per frame side: at the branch
    // in the middle of its frame points (they share one, or going under, each has
    // its own next to the others).
    let mut by_side: BTreeMap<(RelationId, Side), Vec<(usize, f32)>> = BTreeMap::new();
    for (li, g) in lines.iter_mut().enumerate() {
        if let Some((p, side, _)) = &g.frame_port {
            let along = if side.is_horizontal() { p.y } else { p.x };
            by_side
                .entry((g.relation, *side))
                .or_default()
                .push((li, along));
            g.frame_named = false;
        }
    }
    for members in by_side.values() {
        let mean = members.iter().map(|(_, v)| v).sum::<f32>() / members.len() as f32;
        let middle = members.iter().min_by(|a, b| {
            (a.1 - mean)
                .abs()
                .total_cmp(&(b.1 - mean).abs())
                .then(a.0.cmp(&b.0))
        });
        if let Some((li, _)) = middle {
            lines[*li].frame_named = true;
        }
    }
    // The partner names beside the frame ends on top and bottom: left of the end
    // when that stays inside the frame, else right of it, wrapped only when even
    // that is too little; in the first row of names where it overlaps no other.
    for side in [Side::Top, Side::Bottom] {
        let mut on_side: Vec<(f32, usize, String)> = lines
            .iter()
            .enumerate()
            .filter_map(|(li, g)| {
                let (p, s, name) = g.frame_port.as_ref()?;
                (*s == side && g.frame_named).then(|| (p.x, li, name.clone()))
            })
            .collect();
        on_side.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let (lo, hi) = (bounds.min.x + PAD, bounds.max.x - PAD);
        let mut rows: Vec<Vec<(f32, f32)>> = Vec::new();
        // The title sits in the first row at the top left; names keep off it.
        if side == Side::Top && title_end > f32::MIN {
            rows.push(vec![(bounds.min.x, title_end)]);
        }
        for (x, li, name) in on_side {
            let w = text::width(&name, LABEL_SIZE);
            let (room_left, room_right) = (x - 8.0 - lo, hi - x - 8.0);
            let (left, width) = if w <= room_left {
                (true, f32::MAX)
            } else if w <= room_right {
                (false, f32::MAX)
            } else if room_left >= room_right {
                (true, room_left.max(40.0))
            } else {
                (false, room_right.max(40.0))
            };
            let shown: Vec<String> = if width == f32::MAX {
                vec![name]
            } else {
                wrap(&name, width, LABEL_SIZE)
            };
            let shown_w = shown
                .iter()
                .map(|l| text::width(l, LABEL_SIZE))
                .fold(0.0, f32::max);
            let extent = if left {
                (x - 8.0 - shown_w, x - 8.0)
            } else {
                (x + 8.0, x + 8.0 + shown_w)
            };
            let free = |row: &Vec<(f32, f32)>| {
                row.iter()
                    .all(|(a, b)| extent.1 <= *a + 0.5 || extent.0 >= *b - 0.5)
            };
            let row = (0..)
                .find(|k| (*k..*k + shown.len()).all(|r| rows.get(r).is_none_or(free)))
                .unwrap_or(0);
            for r in row..row + shown.len() {
                if rows.len() <= r {
                    rows.resize_with(r + 1, Vec::new);
                }
                rows[r].push(extent);
            }
            lines[li].frame_name = (left, row, width);
        }
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

    // A relation that lands on several boxes is labelled once per frame point:
    // branches that share one share the label, a branch with a frame point of
    // its own is a line of its own to the eye and carries the text too.
    let mut labelled = BTreeSet::new();
    for (li, geom) in lines.iter_mut().enumerate() {
        if let Some(text) = shown.get(li) {
            let at = geom
                .frame_port
                .as_ref()
                .map(|(p, _, _)| ((p.x * 10.0).round() as i64, (p.y * 10.0).round() as i64));
            geom.text = if labelled.insert((geom.relation, at)) {
                text.clone()
            } else {
                String::new()
            };
        }
    }
    let area = frame.unwrap_or(bounds).expand(-8.0);
    place_labels(&mut lines, &boxes, area, number_labels, &mut legend_keys);
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
    // Every line look in use, once, by name, drawn in the plain line colour.
    let mut legend_looks: Vec<LookSample> = Vec::new();
    for line in &lines {
        if let Some(look) = &line.look
            && !legend_looks.iter().any(|s| s.look.id == look.id)
            && let Some(kept) = project.line_looks.get(&look.id)
        {
            legend_looks.push(LookSample {
                name: kept.name().to_owned(),
                look: look_in_color(project, &look.id, crate::scene::LINE)
                    .unwrap_or_else(|| look.clone()),
            });
        }
    }
    legend_looks.sort_by_key(|s| s.name.to_lowercase());
    let (bounds, legend_at) = if legend.is_empty() && legend_looks.is_empty() {
        (bounds, None)
    } else {
        let rows: usize =
            legend.iter().map(|e| e.lines.len().max(1)).sum::<usize>() + legend_looks.len();
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
            .filter(|_| !project.is_doodle())
            .and_then(|d| project.blocks.get(&d))
            .map(|b| b.name.clone()),
        captions: !project.is_doodle(),
        bounds,
        cells,
        legend,
        legend_looks,
        legend_at,
        frame_look,
        frame_caption,
    };
    (layout, grid)
}

/// What each line of the view shows, and the legend for whatever was shortened:
/// a short label if the relation has one, the full text up to `limit` characters,
/// otherwise a number `[n]` (numbered in relation order).
/// Partner names at frame ends that reach deep (`B › b1, b2`) and are longer than the
/// label limit: shown as `B › [n]`, with the full name added to the legend.
fn short_partners(
    view: &DiagramView,
    limit: Option<u32>,
    legend: &mut Vec<(String, String)>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(limit) = limit else { return out };
    let sep = " \u{203a} ";
    for line in &view.lines {
        for end in [&line.a, &line.b] {
            let ViewEnd::Frame { partner, .. } = end else {
                continue;
            };
            if out.contains_key(partner) || partner.chars().count() <= limit as usize {
                continue;
            }
            let Some((root, _)) = partner.split_once(sep) else {
                continue;
            };
            let numbered = legend.iter().filter(|(k, _)| k.starts_with('[')).count();
            let key = format!("[{}]", numbered + 1);
            legend.push((key.clone(), partner.clone()));
            out.insert(partner.clone(), format!("{root}{sep}{key}"));
        }
    }
    out
}

/// Besides the labels: for every line through mouseholes (`hidden`) the number(s)
/// its holes carry. Such a line always goes to the legend, one entry per relation
/// of a bundle; a relation without text is listed by its two ends. Of a fanned-out
/// relation every branch that goes under has a number of its own, so a reader can
/// pair its holes; the legend lists each with the box it reaches.
fn shown_labels(
    project: &Project,
    view: &DiagramView,
    limit: Option<u32>,
    hidden: &BTreeSet<usize>,
) -> (Vec<String>, Vec<(String, String)>, Vec<String>) {
    let mut shown = Vec::new();
    let mut holes = Vec::new();
    let mut legend: Vec<(String, String)> = Vec::new();
    let mut numbers: BTreeMap<RelationId, String> = BTreeMap::new();
    let mut issued = 0usize;
    let mut seen = BTreeSet::new();
    let mut under_per_relation: BTreeMap<RelationId, usize> = BTreeMap::new();
    for li in hidden {
        if let Some(line) = view.lines.get(*li) {
            *under_per_relation.entry(line.relation).or_default() += 1;
        }
    }
    let text_of = |rel: RelationId| {
        project
            .relations
            .get(&rel)
            .map(|r| r.text.trim().to_owned())
            .unwrap_or_default()
    };
    for (li, line) in view.lines.iter().enumerate() {
        if hidden.contains(&li) {
            let mut keys = Vec::new();
            let branched = under_per_relation
                .get(&line.relation)
                .is_some_and(|n| *n > 1);
            for rel in std::iter::once(line.relation).chain(line.bundle.iter().copied()) {
                let text = text_of(rel);
                let key = if branched && rel == line.relation {
                    issued += 1;
                    let key = format!("[{issued}]");
                    let entry = if text.is_empty() {
                        ends_of(project, line)
                    } else {
                        format!("{text} \u{203a} {}", landing_of(project, line))
                    };
                    legend.push((key.clone(), entry));
                    key
                } else {
                    let key = numbers
                        .entry(rel)
                        .or_insert_with(|| {
                            issued += 1;
                            format!("[{issued}]")
                        })
                        .clone();
                    if seen.insert(rel) {
                        let entry = if text.is_empty() {
                            ends_of(project, line)
                        } else {
                            text
                        };
                        legend.push((key.clone(), entry));
                    }
                    key
                };
                keys.push(key.trim_matches(['[', ']']).to_owned());
            }
            shown.push(line.short.clone());
            holes.push(keys.join(","));
            continue;
        }
        holes.push(String::new());
        let full = line.text.trim();
        let too_long = |n: u32| !full.is_empty() && full.chars().count() > n as usize;
        let label = if !line.short.is_empty() {
            if !full.is_empty() && full != line.short && seen.insert(line.relation) {
                legend.push((line.short.clone(), full.to_owned()));
            }
            line.short.clone()
        } else if limit.is_some_and(too_long) {
            let key = numbers
                .entry(line.relation)
                .or_insert_with(|| {
                    issued += 1;
                    format!("[{issued}]")
                })
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
    (shown, legend, holes)
}

/// The box a line from the frame lands on (else its ends).
fn landing_of(project: &Project, line: &ViewLine) -> String {
    match (&line.a, &line.b) {
        (ViewEnd::Block { block, .. }, _) | (_, ViewEnd::Block { block, .. }) => project
            .blocks
            .get(block)
            .map_or_else(|| "?".to_owned(), |b| b.name.replace('\n', " ")),
        _ => ends_of(project, line),
    }
}

/// The two ends of a line, with its direction between them: `A → B`.
fn ends_of(project: &Project, line: &ViewLine) -> String {
    let name = |e: &ViewEnd| match e {
        ViewEnd::Block { block, .. } => project
            .blocks
            .get(block)
            .map_or_else(|| "?".to_owned(), |b| b.name.replace('\n', " ")),
        ViewEnd::Frame { partner, .. } => partner.clone(),
        ViewEnd::Open => "open end".to_owned(),
        ViewEnd::Dangling => "not attached".to_owned(),
    };
    let arrow = match (line.direction.arrow_at_a(), line.direction.arrow_at_b()) {
        (true, true) => "\u{2194}",
        (false, true) => "\u{2192}",
        (true, false) => "\u{2190}",
        (false, false) => "\u{2013}",
    };
    format!("{} {arrow} {}", name(&line.a), name(&line.b))
}

/// Where a line through mouseholes goes under: the hole `HOLE_IN` from each port
/// (closer on a short line), with the drawn pieces up to them.
fn mousehole_of(points: &[Pos], label: String) -> Mousehole {
    let length: f32 = points.windows(2).map(|w| w[0].dist(w[1])).sum();
    let in_by = HOLE_IN.min(length / 3.0);
    let (piece_a, rest) = cut_at(points, in_by);
    let rest_len: f32 = rest.windows(2).map(|w| w[0].dist(w[1])).sum();
    let (_, piece_b) = cut_at(&rest, rest_len - in_by);
    Mousehole {
        a: piece_a.last().copied().unwrap_or_default(),
        b: piece_b.first().copied().unwrap_or_default(),
        piece_a,
        piece_b,
        label,
        portal_a: None,
        portal_b: None,
    }
}

/// Cuts a polyline `at` from its start: the part before the cut, ending at the cut
/// point, and the part after it, starting there.
fn cut_at(points: &[Pos], at: f32) -> (Vec<Pos>, Vec<Pos>) {
    let mut before = Vec::new();
    let mut left = at.max(0.0);
    for (i, w) in points.windows(2).enumerate() {
        let len = w[0].dist(w[1]);
        before.push(w[0]);
        if len >= left {
            let t = if len > 0.0 { left / len } else { 0.0 };
            let cut = Pos::new(
                w[0].x + (w[1].x - w[0].x) * t,
                w[0].y + (w[1].y - w[0].y) * t,
            );
            before.push(cut);
            let mut after = vec![cut];
            after.extend_from_slice(&points[i + 1..]);
            return (before, after);
        }
        left -= len;
    }
    let end = points.last().copied().unwrap_or_default();
    before.push(end);
    (before, vec![end])
}

/// How many boxes a straight line from `a` to `b` runs over, not counting the ones
/// it starts or ends on.
fn crossed_boxes(a: Pos, b: Pos, boxes: &[Rect]) -> usize {
    let on_border = |r: &Rect, p: Pos| {
        p.x >= r.min.x - 0.5 && p.x <= r.max.x + 0.5 && p.y >= r.min.y - 0.5 && p.y <= r.max.y + 0.5
    };
    boxes
        .iter()
        .filter(|r| !on_border(r, a) && !on_border(r, b) && segment_meets(a, b, &r.expand(-0.5)))
        .count()
}

/// Whether the segment `a`-`b` passes through `r` (Liang-Barsky).
pub fn segment_meets(a: Pos, b: Pos, r: &Rect) -> bool {
    clip_params(a, b, r).is_some()
}

/// Where the segment `a`-`b` enters and leaves `r`, as fractions of its length
/// (Liang-Barsky); `None` when it misses `r`.
fn clip_params(a: Pos, b: Pos, r: &Rect) -> Option<(f32, f32)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [
        (-dx, a.x - r.min.x),
        (dx, r.max.x - a.x),
        (-dy, a.y - r.min.y),
        (dy, r.max.y - a.y),
    ] {
        if p.abs() < f32::EPSILON {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
        if t0 > t1 {
            return None;
        }
    }
    Some((t0, t1))
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
/// Where a line that reaches a box at `port` on `side` meets the box's outline: the
/// port itself for boxes, further in along the line for round and pointed shapes.
fn on_outline(kind: BlockKind, r: Rect, side: Side, port: Pos) -> Pos {
    let c = r.center();
    let (a, b) = (r.width() / 2.0, r.height() / 2.0);
    // How far across the side the port lies, 0 at its middle and 1 at its corner.
    let across = if side.is_horizontal() {
        ((port.y - c.y) / b).abs().min(1.0)
    } else {
        ((port.x - c.x) / a).abs().min(1.0)
    };
    // How far in from the side the outline is, as a share of the half size.
    let inset = match kind {
        BlockKind::Circle | BlockKind::Cloud => 1.0 - (1.0 - across * across).sqrt(),
        BlockKind::Diamond => across,
        _ => return port,
    };
    match side {
        Side::Top => Pos::new(port.x, r.min.y + inset * b),
        Side::Bottom => Pos::new(port.x, r.max.y - inset * b),
        Side::Left => Pos::new(r.min.x + inset * a, port.y),
        Side::Right => Pos::new(r.max.x - inset * a, port.y),
    }
}

/// Height of a box's text: a doodle shape's lines, or the name and its type caption.
pub fn text_block(project: &Project, b: &crate::model::Block) -> f32 {
    if project.is_doodle() {
        b.name.lines().count().max(1) as f32 * crate::scene::NAME_LINE
    } else {
        CAPTIONED_H
    }
}

/// Height of a name with its type caption below.
pub const CAPTIONED_H: f32 = 34.0;

/// With the name below the picture: the picture's and the name frame's sizes.
#[derive(Debug, Clone, Copy)]
struct BelowParts {
    picture: (f32, f32),
    frame: (f32, f32),
}

/// The size of a box wearing a look: the box it would be, grown so the picture keeps
/// its proportions. `counts` are the lines on its top, right, bottom and left. With
/// the name below the picture, also the picture's and the name frame's sizes: the
/// picture grows as a whole for the lines on its sides, the frame for those from
/// below and for a long name.
fn look_box(
    look: &Look,
    (w, h): (f32, f32),
    (text_w, text_h): (f32, f32),
    counts: [f32; 4],
) -> (f32, f32, Option<BelowParts>) {
    let (pw, ph) = look.size();
    let aspect = pw / ph;
    let need = |n: f32| (n + 1.0) * PORT_SPACING;
    let [top, right, bottom, left] = counts;
    match look.mode() {
        LookMode::Inside => {
            if w / h < aspect {
                (h * aspect, h, None)
            } else {
                (w, w / aspect, None)
            }
        }
        LookMode::Below => {
            let (pw, ph) = (LOOK_H * aspect, LOOK_H);
            let scale = (need(top) / pw).max(need(left.max(right)) / ph).max(1.0);
            let (pw, ph) = (pw * scale, ph * scale);
            let (fw, fh) = ((text_w + PAD).max(need(bottom)), text_h + PAD);
            (
                pw.max(fw),
                ph + LOOK_GAP + fh,
                Some(BelowParts {
                    picture: (pw, ph),
                    frame: (fw, fh),
                }),
            )
        }
        LookMode::Icon => (
            w.max(ICON_H * aspect + 2.0 * PAD),
            h + ICON_H + LOOK_GAP,
            None,
        ),
    }
}

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

/// Places items in their given order as close to where they want to be as possible,
/// within [lo, hi], with at least `gaps[i]` between item `i - 1` and item `i`. Items
/// that would overlap are pushed apart evenly around where they wanted to be; when
/// they do not fit at all, they are spread evenly.
fn spread_by(wanted: &[f32], lo: f32, hi: f32, gaps: &[f32]) -> Vec<f32> {
    let n = wanted.len();
    if n == 0 {
        return Vec::new();
    }
    let gap = |i: usize| gaps.get(i).copied().unwrap_or(0.0);
    let needed: f32 = (1..n).map(gap).sum();
    if hi < lo || needed > hi - lo {
        let mid = (lo + hi) / 2.0;
        let step = if n > 1 {
            (hi - lo).max(0.0) / (n as f32 - 1.0)
        } else {
            0.0
        };
        let start = if n > 1 { lo.min(mid) } else { mid };
        return (0..n).map(|i| start + step * i as f32).collect();
    }
    // Clusters of items that touch: (first item, count, where the first one goes).
    // Each item's place relative to its cluster's first item is the sum of the gaps.
    let rel = |i: usize, first: usize| (first + 1..=i).map(gap).sum::<f32>();
    let mut clusters: Vec<(usize, usize, f32)> = Vec::new();
    for (i, w) in wanted.iter().enumerate() {
        clusters.push((i, 1, w.clamp(lo, hi)));
        // Merge with the cluster before while they overlap: the merged cluster sits
        // at the mean of its members' wishes, kept inside the range.
        while let [.., (f0, n0, start0), (f1, n1, start1)] = clusters[..] {
            let last0 = start0 + rel(f0 + n0 - 1, f0);
            if start1 >= last0 + gap(f1) - 0.01 {
                break;
            }
            let count = n0 + n1;
            let mean = (f0..f0 + count)
                .map(|j| wanted[j].clamp(lo, hi) - rel(j, f0))
                .sum::<f32>()
                / count as f32;
            let width = rel(f0 + count - 1, f0);
            let start = mean.clamp(lo, hi - width);
            clusters.truncate(clusters.len() - 2);
            clusters.push((f0, count, start));
        }
    }
    let mut out = vec![0.0; n];
    for (first, count, start) in clusters {
        for (k, slot) in out[first..first + count].iter_mut().enumerate() {
            *slot = start + rel(first + k, first);
        }
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

fn grid(
    sizes: &BTreeMap<BlockId, (crate::model::Cell, f32, f32)>,
    vgaps: &[f32],
    hgaps: &[f32],
) -> Grid {
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
    let (row_y, hspan, height) = axis(&row_h, hgaps, EMPTY_H);
    let center = |(a, b): &(f32, f32)| (a + b) / 2.0;
    Grid {
        cols,
        rows,
        vx: vspan.iter().map(center).collect(),
        hy: hspan.iter().map(center).collect(),
        vspan,
        hspan,
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

/// Whether two ends, each with the direction it leaves in, face each other on one
/// line with no box in between, so the line can run straight from one to the other.
fn straight(a: (Pos, Side), b: (Pos, Side), boxes: &[Rect]) -> bool {
    let (pa, pb) = (a.0, b.0);
    let ahead = match a.1 {
        Side::Right => pb.x > pa.x,
        Side::Left => pb.x < pa.x,
        Side::Bottom => pb.y > pa.y,
        Side::Top => pb.y < pa.y,
    };
    let aligned = if a.1.is_horizontal() {
        (pa.y - pb.y).abs() < 0.5
    } else {
        (pa.x - pb.x).abs() < 0.5
    };
    if a.1 != b.1.opposite() || !ahead || !aligned {
        return false;
    }
    // Ends lie on box borders; only a box the segment passes through blocks it.
    let lo = Pos::new(pa.x.min(pb.x), pa.y.min(pb.y));
    let hi = Pos::new(pa.x.max(pb.x), pa.y.max(pb.y));
    !boxes.iter().any(|r| {
        lo.x < r.max.x - 0.5 && hi.x > r.min.x + 0.5 && lo.y < r.max.y - 0.5 && hi.y > r.min.y + 0.5
    })
}

/// Straight segments of routed lines, by line index.
type Segments = BTreeMap<usize, Vec<(Pos, Pos)>>;

/// What a route should not cross: for every vertical channel the y of the
/// horizontal segments spanning it, and for every horizontal channel the x of the
/// vertical ones.
struct Obstacles {
    on_v: Vec<Vec<f32>>,
    on_h: Vec<Vec<f32>>,
    /// The stretches other lines run along each channel: lanes already taken.
    along_v: Vec<Vec<(f32, f32)>>,
    along_h: Vec<Vec<(f32, f32)>>,
}

/// The segments of every line except `skip`, as obstacles per channel.
fn obstacles(grid: &Grid, segments: &Segments, skip: usize) -> Obstacles {
    let mut out = Obstacles {
        on_v: vec![Vec::new(); grid.vx.len()],
        on_h: vec![Vec::new(); grid.hy.len()],
        along_v: vec![Vec::new(); grid.vx.len()],
        along_h: vec![Vec::new(); grid.hy.len()],
    };
    let spans = |c: f32, a: f32, b: f32| c > a.min(b) + 0.01 && c < a.max(b) - 0.01;
    let stretch = |a: f32, b: f32| (a.min(b), a.max(b));
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
                if let Some(h) = grid.hy.iter().position(|y| (y - p.y).abs() < 0.5) {
                    out.along_h[h].push(stretch(p.x, q.x));
                }
            } else if (p.x - q.x).abs() < 0.01 {
                for (h, y) in grid.hy.iter().enumerate() {
                    if spans(*y, p.y, q.y) {
                        out.on_h[h].push(p.x);
                    }
                }
                if let Some(v) = grid.vx.iter().position(|x| (x - p.x).abs() < 0.5) {
                    out.along_v[v].push(stretch(p.y, q.y));
                }
            }
        }
    }
    out
}

/// Whether two ends can be joined by one bend: the corner where their headings meet,
/// when it lies ahead of both and neither leg runs through or close to a box other
/// than the ones the ends sit on.
fn elbow(a: (Pos, Side), b: (Pos, Side), boxes: &[Rect]) -> Option<Pos> {
    let ((pa, ha), (pb, hb)) = (a, b);
    if ha.is_horizontal() == hb.is_horizontal() {
        return None;
    }
    let corner = if ha.is_horizontal() {
        Pos::new(pb.x, pa.y)
    } else {
        Pos::new(pa.x, pb.y)
    };
    let ahead = |p: Pos, h: Side, q: Pos| match h {
        Side::Right => q.x > p.x + 1.0,
        Side::Left => q.x < p.x - 1.0,
        Side::Bottom => q.y > p.y + 1.0,
        Side::Top => q.y < p.y - 1.0,
    };
    if !ahead(pa, ha, corner) || !ahead(pb, hb, corner) {
        return None;
    }
    let on_border = |r: &Rect, p: Pos| {
        p.x >= r.min.x - 0.5 && p.x <= r.max.x + 0.5 && p.y >= r.min.y - 0.5 && p.y <= r.max.y + 0.5
    };
    let clear = |p: Pos, q: Pos| {
        let lo = Pos::new(p.x.min(q.x), p.y.min(q.y));
        let hi = Pos::new(p.x.max(q.x), p.y.max(q.y));
        !boxes.iter().any(|r| {
            if on_border(r, pa) || on_border(r, pb) {
                return false;
            }
            let r = r.expand(ELBOW_CLEARANCE);
            lo.x < r.max.x && hi.x > r.min.x && lo.y < r.max.y && hi.y > r.min.y
        })
    };
    (clear(pa, corner) && clear(corner, pb)).then_some(corner)
}

/// How often `segs` cross the segments of every line but `skip`.
fn crossings_with(segs: &[(Pos, Pos)], segments: &Segments, skip: usize) -> usize {
    let mut n = 0;
    for (li, list) in segments {
        if *li == skip {
            continue;
        }
        for (p, q) in segs {
            for (r, s) in list {
                n += tangles(&[*p, *q], &[*r, *s]);
            }
        }
    }
    n
}

/// Ports, candidate routes and a first lane order for one set of hidden lines and
/// branches on their own frame point.
struct Prepared {
    port_at: PortAt,
    candidates: Vec<Vec<Route>>,
}

/// What was prepared for each set of frame ends on a frame point of their own.
type PreparedFor = BTreeMap<Vec<(usize, End)>, Rc<Prepared>>;

/// A choice as drawn: the routes taken, the lanes, their offsets and every line's
/// points, with what they were prepared from.
struct Drawing {
    routes: BTreeMap<usize, Route>,
    lanes: Lanes,
    offsets: BTreeMap<(usize, Chan), f32>,
    points: BTreeMap<usize, Vec<Pos>>,
    prepared: Rc<Prepared>,
}

/// Every routed line's points for the given routes and lane offsets.
fn points_of(
    grid: &Grid,
    offsets: &BTreeMap<(usize, Chan), f32>,
    port_at: &PortAt,
    routes: &BTreeMap<usize, Route>,
) -> BTreeMap<usize, Vec<Pos>> {
    routes
        .iter()
        .filter_map(|(li, route)| {
            let a = port_at.get(&(*li, End::A))?.0;
            let b = port_at.get(&(*li, End::B))?.0;
            Some((*li, route_points(grid, offsets, *li, a, b, route)))
        })
        .collect()
}

/// How many candidate routes a line gets at most.
const ROUTE_CHOICES: usize = 4;

/// The routes a line may take: straight across empty cells when it can (then
/// nothing else), else one bend where its headings meet, the cheapest run through
/// the channels, and runs that each avoid one channel of that.
fn alternatives(
    grid: &Grid,
    boxes: &[Rect],
    heading: &dyn Fn(usize, End, Side) -> Side,
    port_at: &PortAt,
    li: usize,
    segments: &Segments,
) -> Vec<Route> {
    let (Some(a), Some(b)) = (
        port_at.get(&(li, End::A)).copied(),
        port_at.get(&(li, End::B)).copied(),
    ) else {
        return Vec::new();
    };
    let fa = (a.0, heading(li, End::A, a.1));
    let fb = (b.0, heading(li, End::B, b.1));
    if straight(fa, fb, boxes) {
        return vec![Route::Straight];
    }
    let others = obstacles(grid, segments, li);
    let best = route(grid, (a.0, a.2), (b.0, b.2), &others, None);
    let mut out = Vec::new();
    if let Some(corner) = elbow(fa, fb, boxes) {
        out.push(Route::Elbow(corner));
    }
    out.push(Route::Chans(best.clone()));
    // Not the channels at the ports: without them there is no way out.
    let inner = best.len().saturating_sub(1);
    for chan in best.iter().copied().take(inner).skip(1) {
        if out.len() >= ROUTE_CHOICES {
            break;
        }
        let alt = Route::Chans(route(grid, (a.0, a.2), (b.0, b.2), &others, Some(chan)));
        if !out.contains(&alt) {
            out.push(alt);
        }
    }
    out
}

/// The price of a drawing: the lines as drawn (with their relations, since the
/// branches of one relation may touch), every line's whole route, how many go
/// under, and what they must keep off. Crossings, overlaps, holes and runs through
/// title or band make the first part; bends, steps shorter than half a lane and
/// length the second, counted on the whole routes, so that going under earns a
/// line nothing on looks.
fn drawing_cost(
    drawn: &[(RelationId, Vec<Pos>)],
    whole: &[&[Pos]],
    holes: usize,
    keep_off: &[Rect],
) -> solve::Cost {
    let mut tangle = holes as i64 * HOLE_COST;
    let mut cosmetic = 0i64;
    for p in whole {
        cosmetic += p.len().saturating_sub(2) as i64 * 10;
        cosmetic += (p.windows(2).map(|w| w[0].dist(w[1])).sum::<f32>() / 20.0) as i64;
        let steps = p
            .windows(2)
            .enumerate()
            .filter(|(i, w)| *i > 0 && *i + 2 < p.len() && w[0].dist(w[1]) < LANE / 2.0)
            .count();
        cosmetic += steps as i64 * 50;
    }
    for (i, (ra, p)) in drawn.iter().enumerate() {
        for (rb, q) in &drawn[i + 1..] {
            if ra == rb {
                continue;
            }
            let crossings = cross_points(p, q).len() + cross_points(q, p).len();
            tangle += crossings as i64 * CROSSING_COST;
            tangle += overlap_stretches(p, q).len() as i64 * OVERLAP_COST;
        }
        let through = p
            .windows(2)
            .filter(|w| {
                keep_off
                    .iter()
                    .any(|r| segment_meets(w[0], w[1], &r.expand(-0.5)))
            })
            .count();
        tangle += through as i64 * THROUGH_COST;
    }
    solve::Cost { tangle, cosmetic }
}

/// Routes every line with two ports, `passes` times over all of them: a line starts
/// with only its stems known, so the later passes let every line see the others.
#[expect(clippy::too_many_arguments, reason = "one pass over the routing state")]
fn route_lines(
    grid: &Grid,
    boxes: &[Rect],
    port_at: &PortAt,
    heading: &dyn Fn(usize, End, Side) -> Side,
    n: usize,
    passes: usize,
    hidden: &BTreeSet<usize>,
    routes: &mut BTreeMap<usize, Route>,
    segments: &mut Segments,
) {
    let both = |li: usize| Some((*port_at.get(&(li, End::A))?, *port_at.get(&(li, End::B))?));
    let stem = |(p, _, chan): (Pos, Side, Chan)| match chan {
        Chan::V(v) => (p, Pos::new(grid.vx[v], p.y)),
        Chan::H(h) => (p, Pos::new(p.x, grid.hy[h])),
    };
    segments.clear();
    for li in (0..n).filter(|li| !hidden.contains(li)) {
        if let Some((a, b)) = both(li) {
            segments.insert(li, vec![stem(a), stem(b)]);
        }
    }
    let pairs =
        |points: &[Pos]| -> Vec<(Pos, Pos)> { points.windows(2).map(|w| (w[0], w[1])).collect() };
    for _ in 0..passes {
        for li in 0..n {
            let Some((a, b)) = both(li) else { continue };
            let fa = (a.0, heading(li, End::A, a.1));
            let fb = (b.0, heading(li, End::B, b.1));
            // Ends that face each other across empty cells: one straight line. Ends
            // around a corner: one bend, unless the channels cross fewer lines.
            let route = if straight(fa, fb, boxes) {
                Route::Straight
            } else {
                let others = obstacles(grid, segments, li);
                let chans = Route::Chans(route(grid, (a.0, a.2), (b.0, b.2), &others, None));
                match elbow(fa, fb, boxes) {
                    Some(corner) => {
                        let bent = crossings_with(&[(a.0, corner), (corner, b.0)], segments, li);
                        let routed = route_points(grid, &BTreeMap::new(), li, a.0, b.0, &chans);
                        if bent <= crossings_with(&pairs(&routed), segments, li) {
                            Route::Elbow(corner)
                        } else {
                            chans
                        }
                    }
                    None => chans,
                }
            };
            let points = route_points(grid, &BTreeMap::new(), li, a.0, b.0, &route);
            if !hidden.contains(&li) {
                segments.insert(li, pairs(&points));
            }
            routes.insert(li, route);
        }
    }
}

/// The order of a port along its side: from which direction its line comes (before
/// the side, straight, after it), then how near the side the line's lane runs (of
/// lines from the same direction the nearest lane turns in first), then where it
/// comes from. Ports in this order never cross on their way in.
type OrderKey = (i8, i64, i64);

/// The order key of every port, from the lines as drawn.
fn arrival_keys(
    drawn: &BTreeMap<usize, Vec<Pos>>,
    ends: &BTreeMap<(usize, End), (Owner, Side)>,
) -> BTreeMap<(usize, End), OrderKey> {
    let mut out = BTreeMap::new();
    for (&(li, end), &(_, side)) in ends {
        let Some(points) = drawn.get(&li) else {
            continue;
        };
        let along = |p: &Pos| if side.is_horizontal() { p.y } else { p.x };
        let across = |p: &Pos| if side.is_horizontal() { p.x } else { p.y };
        let tenths = |v: f32| (v * 10.0).round() as i64;
        let seq: Vec<&Pos> = match end {
            End::A => points.iter().collect(),
            End::B => points.iter().rev().collect(),
        };
        let key = match seq[..] {
            [port, turn, far, ..] => {
                let class = (along(far) - along(port)).signum() as i8;
                let stem = tenths((across(turn) - across(port)).abs());
                (class, -i64::from(class) * stem, tenths(along(far)))
            }
            [_, other] => (0, 0, tenths(along(other))),
            _ => continue,
        };
        out.insert((li, end), key);
    }
    out
}

/// Shortest path with few bends and few crossings over the lattice of gap channels.
fn route(
    grid: &Grid,
    a: (Pos, Chan),
    b: (Pos, Chan),
    others: &Obstacles,
    banned: Option<Chan>,
) -> Vec<Chan> {
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
    // Other lines already running along `chan` between `from` and `to`.
    let shared = |chan: Chan, from: f32, to: f32| -> i64 {
        let list = match chan {
            Chan::V(v) => &others.along_v[v],
            Chan::H(h) => &others.along_h[h],
        };
        let (lo, hi) = (from.min(to), from.max(to));
        list.iter()
            .filter(|(a, b)| *a < hi - 0.5 && *b > lo + 0.5)
            .count() as i64
            * SHARE
    };
    let penalty = |chan: Chan, from: f32, to: f32| crossed(chan, from, to) + shared(chan, from, to);

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
                            let x = penalty(a.1, along, grid.hy[h]);
                            edges.push((v * nh + h, a.1, grid.hy[h] - along, x));
                        }
                    }
                    Chan::H(h) => {
                        for v in 0..nv {
                            let x = penalty(a.1, along, grid.vx[v]);
                            edges.push((v * nh + h, a.1, grid.vx[v] - along, x));
                        }
                    }
                }
                if a.1 == b.1 {
                    let to = coord(b.1, b.0);
                    edges.push((t, a.1, to - along, penalty(a.1, along, to)));
                }
            }
            Some((v, h)) => {
                let (y, x) = (grid.hy[h], grid.vx[v]);
                if h + 1 < nh {
                    let c = penalty(Chan::V(v), y, grid.hy[h + 1]);
                    edges.push((node + 1, Chan::V(v), grid.hy[h + 1] - y, c));
                }
                if h > 0 {
                    let c = penalty(Chan::V(v), y, grid.hy[h - 1]);
                    edges.push((node - 1, Chan::V(v), y - grid.hy[h - 1], c));
                }
                if v + 1 < nv {
                    let c = penalty(Chan::H(h), x, grid.vx[v + 1]);
                    edges.push((node + nh, Chan::H(h), grid.vx[v + 1] - x, c));
                }
                if v > 0 {
                    let c = penalty(Chan::H(h), x, grid.vx[v - 1]);
                    edges.push((node - nh, Chan::H(h), x - grid.vx[v - 1], c));
                }
                match b.1 {
                    Chan::V(bv) if bv == v => {
                        let x = penalty(b.1, grid.hy[h], b.0.y);
                        edges.push((t, b.1, b.0.y - grid.hy[h], x));
                    }
                    Chan::H(bh) if bh == h => {
                        let x = penalty(b.1, grid.vx[v], b.0.x);
                        edges.push((t, b.1, b.0.x - grid.vx[v], x));
                    }
                    _ => {}
                }
            }
        }
        for (next, chan, len, cross) in edges {
            if Some(chan) == banned {
                continue;
            }
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
pub(crate) type Lanes = BTreeMap<Chan, Vec<usize>>;

/// Only a line that runs a stretch along a channel takes a lane there: one that
/// merely turns in it, from its stem straight into the next channel, does not.
fn lane_order(
    grid: &Grid,
    routes: &BTreeMap<usize, Route>,
    port_at: &PortAt,
    hidden: &BTreeSet<usize>,
) -> Lanes {
    let mut users: Lanes = BTreeMap::new();
    let no_offsets = BTreeMap::new();
    for (li, route) in routes {
        let Route::Chans(chans) = route else { continue };
        if hidden.contains(li) {
            continue;
        }
        let (Some(a), Some(b)) = (port_at.get(&(*li, End::A)), port_at.get(&(*li, End::B))) else {
            continue;
        };
        let points = route_points(grid, &no_offsets, *li, a.0, b.0, route);
        let runs_along = |chan: Chan| {
            points.windows(2).any(|w| match chan {
                Chan::H(h) => {
                    (w[0].y - grid.hy[h]).abs() < 0.5
                        && (w[1].y - grid.hy[h]).abs() < 0.5
                        && (w[0].x - w[1].x).abs() > 0.5
                }
                Chan::V(v) => {
                    (w[0].x - grid.vx[v]).abs() < 0.5
                        && (w[1].x - grid.vx[v]).abs() < 0.5
                        && (w[0].y - w[1].y).abs() > 0.5
                }
            })
        };
        for chan in chans {
            if !runs_along(*chan) {
                continue;
            }
            let list = users.entry(*chan).or_default();
            if !list.contains(li) {
                list.push(*li);
            }
        }
    }
    users
}

/// Spreads lines that share a channel side by side, `LANE` apart, in their lane
/// order, as far as the gap allows.
fn lane_offsets(lanes: &Lanes, grid: &Grid) -> BTreeMap<(usize, Chan), f32> {
    let mut out = BTreeMap::new();
    for (chan, list) in lanes {
        let (lo, hi) = match chan {
            Chan::V(v) => grid.vspan[*v],
            Chan::H(h) => grid.hspan[*h],
        };
        let limit = ((hi - lo) / 2.0 - LANE / 2.0).max(0.0);
        let n = list.len() as f32;
        for (k, li) in list.iter().copied().enumerate() {
            let off = ((k as f32 - (n - 1.0) / 2.0) * LANE).clamp(-limit, limit);
            out.insert((li, *chan), off);
        }
    }
    out
}

/// Where a line runs between its two ports.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Route {
    /// One segment: the ports face each other.
    Straight,
    /// One bend at this corner, through free cells.
    Elbow(Pos),
    /// Along these gap channels.
    Chans(Vec<Chan>),
}

/// The ports of every line: point, side and the channel the line leaves into.
type PortAt = BTreeMap<(usize, End), (Pos, Side, Chan)>;

fn route_points(
    grid: &Grid,
    offsets: &BTreeMap<(usize, Chan), f32>,
    li: usize,
    pa: Pos,
    pb: Pos,
    route: &Route,
) -> Vec<Pos> {
    let off = |c: Chan| offsets.get(&(li, c)).copied().unwrap_or(0.0);
    let x = |v: usize| grid.vx[v] + off(Chan::V(v));
    let y = |h: usize| grid.hy[h] + off(Chan::H(h));
    let mut pts = vec![pa];
    let chans = match route {
        Route::Straight => &Vec::new(),
        Route::Elbow(corner) => {
            pts.push(*corner);
            pts.push(pb);
            return simplify(pts);
        }
        Route::Chans(chans) => chans,
    };
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
    simplify(unjog(pts))
}

/// A step across shorter than this, right after a line leaves its port and before
/// it runs on along the same axis, reads as a bow in the line, not as a turn.
const JOG: f32 = LANE / 2.0;

/// Takes the jog out of a line's ends: where the line leaves its port, steps aside
/// by less than `JOG` and goes on in the port's direction, the run keeps the port's
/// coordinate instead (the frame port of a fanned-out relation sits at the mean of
/// its branches, a few pixels off the lane of the branch that goes straight).
fn unjog(mut pts: Vec<Pos>) -> Vec<Pos> {
    let n = pts.len();
    if n < 5 {
        return pts;
    }
    let vertical = |a: Pos, b: Pos| (a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() >= 0.01;
    let horizontal = |a: Pos, b: Pos| (a.y - b.y).abs() < 0.01 && (a.x - b.x).abs() >= 0.01;
    // From each end: the port, the turn, the step and the next corner.
    for idx in [[0, 1, 2, 3], [n - 1, n - 2, n - 3, n - 4]] {
        let [p0, p1, p2, p3] = idx.map(|i| pts[i]);
        if vertical(p0, p1) && horizontal(p1, p2) && (p1.x - p2.x).abs() < JOG && vertical(p2, p3) {
            for i in &idx[1..] {
                pts[*i].x = p0.x;
            }
        } else if horizontal(p0, p1)
            && vertical(p1, p2)
            && (p1.y - p2.y).abs() < JOG
            && horizontal(p2, p3)
        {
            for i in &idx[1..] {
                pts[*i].y = p0.y;
            }
        }
    }
    pts
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
    look: Option<DrawnLook>,
) -> LineGeom {
    let (label_at, label_horizontal) = label_position(&points);
    LineGeom {
        look,
        relation: line.relation,
        kind,
        arrow_start: line.direction.arrow_at_a(),
        arrow_end: line.direction.arrow_at_b(),
        text: line.text.clone(),
        label_at,
        label_horizontal,
        label_flipped: false,
        points,
        tip,
        tip_dir,
        frame_port: None,
        frame_name: (true, 0, f32::MAX),
        frame_named: true,
        mousehole: None,
        style: line.style,
        full_text: line.text.clone(),
        bundle: line.bundle.clone(),
        pattern: line.pattern,
        color: line.color,
        landing: match (&line.a, &line.b) {
            (ViewEnd::Block { block, .. }, ViewEnd::Frame { .. })
            | (ViewEnd::Frame { .. }, ViewEnd::Block { block, .. }) => Some(*block),
            _ => None,
        },
    }
}

/// A place for a label: on a stretch of its line between two crossings (or the
/// segment's ends), and how good it is there.
struct LabelSpot {
    at: Pos,
    horizontal: bool,
    /// On the other side of the line: below it, or left of it.
    flipped: bool,
    /// Inside the frame (or the picture).
    inside: bool,
    /// Clear of every box and of every other relation's line.
    clear: bool,
    /// The stretch is long enough for the text.
    fits: bool,
    /// In the middle of the stretch, not slid toward one end.
    centered: bool,
    length: f32,
}

/// Puts every label of a routed line where it fits: on a stretch of the line
/// between crossings, inside `area`, that holds the text clear of boxes, of other
/// lines, of the pictures at line ends, of the labels at mouseholes and of the
/// labels placed before it; a stretch with room before one
/// without, a horizontal one before a vertical one, the middle of the stretch
/// before a spot slid toward one end, the usual side of the line before the other,
/// the longest first. When `number`, a label with no clear place takes a legend
/// number instead, and its text goes to the legend; when even the number fits
/// nowhere, it goes to the best place there is.
fn place_labels(
    lines: &mut [LineGeom],
    boxes: &[Rect],
    area: Rect,
    number: bool,
    legend: &mut Vec<(String, String)>,
) {
    let crossings = line_crossings(lines);
    let others: Vec<(RelationId, Vec<(Pos, Pos)>)> = lines
        .iter()
        .map(|g| {
            let segs = g
                .drawn()
                .into_iter()
                .flat_map(|piece| piece.windows(2).map(|w| (w[0], w[1])))
                .collect();
            (g.relation, segs)
        })
        .collect();
    // The pictures at the ends of lines with a look and the labels at mouseholes
    // take room like boxes do.
    let mut placed: Vec<Rect> = lines
        .iter()
        .flat_map(|g| {
            let mut taken = crate::scene::end_picture_rects(g);
            taken.extend(crate::scene::hole_label_rects(g));
            taken
        })
        .collect();
    for (li, geom) in lines.iter_mut().enumerate() {
        if geom.tip.is_some() || geom.mousehole.is_some() || geom.text.is_empty() {
            continue;
        }
        let marks: Vec<Pos> = crossings
            .iter()
            .filter(|c| c.horizontal == li || c.vertical == li)
            .map(|c| c.at)
            .collect();
        let spots = |text: &str| -> Vec<LabelSpot> {
            let mut out = Vec::new();
            for w in geom.points.windows(2) {
                let horizontal = (w[0].y - w[1].y).abs() < 0.01;
                let along = |p: Pos| if horizontal { p.x } else { p.y };
                let (lo, hi) = (along(w[0]).min(along(w[1])), along(w[0]).max(along(w[1])));
                let mut cuts = vec![lo];
                cuts.extend(
                    marks
                        .iter()
                        .filter(|m| {
                            (if horizontal {
                                m.y - w[0].y
                            } else {
                                m.x - w[0].x
                            })
                            .abs()
                                < 0.01
                                && along(**m) > lo
                                && along(**m) < hi
                        })
                        .map(|m| along(*m)),
                );
                cuts.push(hi);
                cuts.sort_by(f32::total_cmp);
                for (c, flipped) in cuts.windows(2).flat_map(|c| [(c, false), (c, true)]) {
                    let mid = (c[0] + c[1]) / 2.0;
                    let spot_at = |v: f32| {
                        if horizontal {
                            Pos::new(v, w[0].y)
                        } else {
                            Pos::new(w[0].x, v)
                        }
                    };
                    let rect = crate::scene::label_rect(spot_at(mid), horizontal, flipped, text);
                    let need = if horizontal {
                        rect.width()
                    } else {
                        rect.height()
                    };
                    // The middle of the stretch, then spots slid toward either end
                    // by half the label at a time, as long as the label stays on it.
                    let room = ((c[1] - c[0] - need) / 2.0).max(0.0);
                    let steps = (room / (need / 2.0)).floor().min(8.0) as i32;
                    let mut slid: Vec<(f32, bool)> = vec![(mid, true)];
                    for k in 1..=steps {
                        let d = (k as f32) * need / 2.0;
                        slid.push((mid - d, false));
                        slid.push((mid + d, false));
                    }
                    for (v, centered) in slid {
                        let at = spot_at(v);
                        let rect = crate::scene::label_rect(at, horizontal, flipped, text);
                        // Clear means with a little air around the paper.
                        let near = rect.expand(4.0);
                        let clear = !boxes.iter().any(|b| b.overlaps(&near))
                            && !placed.iter().any(|r| r.overlaps(&near))
                            && !others.iter().any(|(rel, segs)| {
                                *rel != geom.relation
                                    && segs.iter().any(|(a, b)| segment_meets(*a, *b, &near))
                            });
                        out.push(LabelSpot {
                            at,
                            horizontal,
                            flipped,
                            inside: area.holds(&rect),
                            clear,
                            fits: c[1] - c[0] >= need,
                            centered,
                            length: c[1] - c[0],
                        });
                    }
                }
            }
            out
        };
        let best = |spots: Vec<LabelSpot>| {
            spots.into_iter().max_by(|p, q| {
                (
                    p.inside,
                    p.clear,
                    p.fits,
                    p.horizontal,
                    p.centered,
                    !p.flipped,
                )
                    .cmp(&(
                        q.inside,
                        q.clear,
                        q.fits,
                        q.horizontal,
                        q.centered,
                        !q.flipped,
                    ))
                    .then(p.length.total_cmp(&q.length))
            })
        };
        let mut text = geom.text.clone();
        let mut spot = best(spots(&text));
        if number && spot.as_ref().is_some_and(|s| !s.clear) && !text.starts_with('[') {
            // Nowhere clear: the number instead, the text under it in the legend
            // (in place of the entry its short label had, if any).
            let full = if geom.full_text.trim().is_empty() {
                text.clone()
            } else {
                geom.full_text.trim().to_owned()
            };
            let numbered = legend.iter().filter(|(k, _)| k.starts_with('[')).count();
            let key = format!("[{}]", numbered + 1);
            match legend.iter_mut().find(|(k, _)| *k == text) {
                Some(entry) => entry.0 = key.clone(),
                None => legend.push((key.clone(), full)),
            }
            text = key;
            spot = best(spots(&text));
        }
        if let Some(s) = spot {
            placed.push(crate::scene::label_rect(
                s.at,
                s.horizontal,
                s.flipped,
                &text,
            ));
            geom.text = text;
            geom.label_at = s.at;
            geom.label_horizontal = s.horizontal;
            geom.label_flipped = s.flipped;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Direction, LinePattern, MouseholeMode};

    /// A vertical line of `text` between two boxes that leave no room beside it.
    fn boxed_in(text: &str) -> (Vec<LineGeom>, Vec<Rect>) {
        let view = ViewLine {
            relation: RelationId(1),
            a: ViewEnd::Open,
            b: ViewEnd::Open,
            direction: Direction::Out,
            text: text.to_owned(),
            style: LineStyle::Round12,
            short: String::new(),
            bundle: Vec::new(),
            pattern: LinePattern::Solid,
            color: None,
            mousehole: MouseholeMode::Automatic,
            look: None,
        };
        let points = vec![Pos::new(100.0, 0.0), Pos::new(100.0, 60.0)];
        let line = build_line(&view, LineKind::Routed, points, None, None, None);
        let boxes = vec![
            Rect::from_min_size(Pos::new(0.0, -50.0), 89.0, 160.0),
            Rect::from_min_size(Pos::new(111.0, -50.0), 89.0, 160.0),
        ];
        (vec![line], boxes)
    }

    #[test]
    fn a_label_with_no_clear_place_becomes_a_number() {
        let (mut lines, boxes) = boxed_in("dashboard");
        let area = Rect::from_min_size(Pos::new(-100.0, -100.0), 400.0, 400.0);
        let mut legend = Vec::new();
        place_labels(&mut lines, &boxes, area, true, &mut legend);
        assert_eq!(lines[0].text, "[1]");
        assert_eq!(legend, vec![("[1]".to_owned(), "dashboard".to_owned())]);
        // With shortening off the text stays, at the best place there is.
        let (mut lines, boxes) = boxed_in("dashboard");
        let mut legend = Vec::new();
        place_labels(&mut lines, &boxes, area, false, &mut legend);
        assert_eq!(lines[0].text, "dashboard");
        assert!(legend.is_empty());
        // A label that already has a legend entry under its short text keeps the
        // entry, now under the number.
        let (mut lines, boxes) = boxed_in("dash");
        lines[0].full_text = "dashboard of everything".to_owned();
        let mut legend = vec![("dash".to_owned(), "dashboard of everything".to_owned())];
        place_labels(&mut lines, &boxes, area, true, &mut legend);
        assert_eq!(lines[0].text, "[1]");
        assert_eq!(
            legend,
            vec![("[1]".to_owned(), "dashboard of everything".to_owned())]
        );
    }
}
