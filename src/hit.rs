//! What is under a point of the diagram: a box border side, a box, a line or an
//! open end. Pure geometry, so clicking can be tested without a window.

use crate::editor::OpenEnd;
use crate::geom::{Pos, polyline_dist};
use crate::layout::{GAP, Layout, LineKind};
use crate::model::{BlockId, Cell, End, Project, RelationId, Side};

/// How close (in diagram units at zoom 1) a click must be to count as on a border.
pub const BORDER: f32 = 9.0;
/// How close a click must be to a line or marker.
pub const NEAR: f32 = 7.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Side(BlockId, Side),
    Block(BlockId),
    OpenEnd(OpenEnd),
    /// A line, with the box it lands on when it comes in through the frame.
    Line(RelationId, Option<BlockId>),
    Empty,
}

/// `scale` is the screen zoom; tolerances stay constant on screen.
pub fn hit(project: &Project, layout: &Layout, p: Pos, scale: f32) -> Hit {
    let border = BORDER / scale;
    let near = NEAR / scale;
    for line in &layout.lines {
        let marker = line.tip.filter(|t| t.dist(p) <= near + 4.0 / scale);
        let open_port = line
            .frame_port
            .as_ref()
            .filter(|(fp, _, _)| line.leaves_open() && fp.dist(p) <= near + 4.0 / scale);
        if marker.is_none() && open_port.is_none() {
            continue;
        }
        match line.kind {
            LineKind::Dangling => {
                if let Some(end) = dangling_end(project, layout, line.relation) {
                    return Hit::OpenEnd(OpenEnd::Dangling(line.relation, end));
                }
            }
            _ => return Hit::OpenEnd(OpenEnd::Stub(line.relation)),
        }
    }
    for b in layout.blocks.iter().rev() {
        let r = b.rect;
        if !r.expand(border).contains(p) {
            continue;
        }
        // Bands have no lines, so no border to start one from.
        if b.band {
            return Hit::Block(b.id);
        }
        let distances = [
            (Side::Top, (p.y - r.min.y).abs()),
            (Side::Right, (p.x - r.max.x).abs()),
            (Side::Bottom, (p.y - r.max.y).abs()),
            (Side::Left, (p.x - r.min.x).abs()),
        ];
        let (side, d) = distances
            .into_iter()
            .fold((Side::Top, f32::INFINITY), |best, cur| {
                if cur.1 < best.1 { cur } else { best }
            });
        if d <= border {
            return Hit::Side(b.id, side);
        }
        return Hit::Block(b.id);
    }
    layout
        .lines
        .iter()
        .filter(|l| polyline_dist(p, &l.points) <= near)
        .min_by(|a, b| {
            polyline_dist(p, &a.points)
                .partial_cmp(&polyline_dist(p, &b.points))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map_or(Hit::Empty, |l| Hit::Line(l.relation, l.landing))
}

/// Which end of a relation dangles in the diagram of `layout`.
fn dangling_end(project: &Project, layout: &Layout, rel: RelationId) -> Option<End> {
    let owner = layout.diagram?;
    let r = project.relations.get(&rel)?;
    [End::A, End::B].into_iter().find(|end| {
        r.end(*end).position_of(owner).is_some() && project.landings(r.end(*end), owner).is_empty()
    })
}

/// The grid cell under a point, extending one cell beyond the used grid on every
/// side so boxes can be dragged outward.
pub fn cell_at(layout: &Layout, p: Pos) -> Option<Cell> {
    if layout.cells.is_empty() {
        return None;
    }
    let mut cols: Vec<(i32, f32)> = Vec::new();
    let mut rows: Vec<(i32, f32)> = Vec::new();
    for (cell, rect) in &layout.cells {
        let c = rect.center();
        if !cols.iter().any(|(k, _)| *k == cell.col) {
            cols.push((cell.col, c.x));
        }
        if !rows.iter().any(|(k, _)| *k == cell.row) {
            rows.push((cell.row, c.y));
        }
    }
    cols.sort_by_key(|(k, _)| *k);
    rows.sort_by_key(|(k, _)| *k);
    let extend = |list: &mut Vec<(i32, f32)>| {
        let step = GAP + 150.0;
        if let (Some(first), Some(last)) = (list.first().copied(), list.last().copied()) {
            list.insert(0, (first.0 - 1, first.1 - step));
            list.push((last.0 + 1, last.1 + step));
        }
    };
    extend(&mut cols);
    extend(&mut rows);
    let nearest = |list: &[(i32, f32)], v: f32| {
        list.iter()
            .min_by(|a, b| {
                (a.1 - v)
                    .abs()
                    .partial_cmp(&(b.1 - v).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(k, _)| *k)
    };
    Some(Cell::new(nearest(&cols, p.x)?, nearest(&rows, p.y)?))
}
