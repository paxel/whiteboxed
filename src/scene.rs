//! A layout turned into plain drawing primitives. The canvas and the SVG export
//! both draw exactly this list, so what you see is what you export.

use crate::geom::{Pos, Rect};
use crate::layout::{
    KIND_SIZE, LABEL_SIZE, LEGEND_LINE, Layout, LegendEntry, LineGeom, LineKind, NAME_SIZE,
    kind_caption,
};
use crate::model::{BlockKind, LineStyle, Rgb, Side};
use crate::text;

pub const INK: Rgb = Rgb(0x22, 0x22, 0x22);
pub const MUTED: Rgb = Rgb(0x66, 0x66, 0x66);
pub const LINE: Rgb = Rgb(0x44, 0x4b, 0x55);
pub const FRAME: Rgb = Rgb(0x8a, 0x93, 0x9e);
pub const WARN: Rgb = Rgb(0xe0, 0x8a, 0x00);
pub const PAPER: Rgb = Rgb(0xff, 0xff, 0xff);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub color: Rgb,
    pub dashed: bool,
}

impl Stroke {
    pub const fn solid(width: f32, color: Rgb) -> Self {
        Stroke {
            width,
            color,
            dashed: false,
        }
    }

    pub const fn dashed(width: f32, color: Rgb) -> Self {
        Stroke {
            width,
            color,
            dashed: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Rect {
        rect: Rect,
        radius: f32,
        fill: Option<Rgb>,
        stroke: Option<Stroke>,
    },
    /// A convex polygon.
    Polygon {
        points: Vec<Pos>,
        fill: Option<Rgb>,
        stroke: Option<Stroke>,
    },
    Polyline {
        points: Vec<Pos>,
        stroke: Stroke,
    },
    Circle {
        center: Pos,
        radius: f32,
        fill: Option<Rgb>,
        stroke: Option<Stroke>,
    },
    /// Single-line text, vertically centred on `pos`.
    Text {
        pos: Pos,
        text: String,
        size: f32,
        color: Rgb,
        align: Align,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub bounds: Rect,
    pub shapes: Vec<Shape>,
}

const OUTLINE: Stroke = Stroke::solid(1.5, INK);

pub fn scene(layout: &Layout) -> Scene {
    let mut shapes = Vec::new();
    if let Some(frame) = layout.frame {
        shapes.push(Shape::Rect {
            rect: frame.expand(-4.0),
            radius: 4.0,
            fill: None,
            stroke: Some(Stroke::dashed(1.5, FRAME)),
        });
        if let Some(title) = &layout.title {
            shapes.push(Shape::Text {
                pos: Pos::new(frame.min.x + 16.0, frame.min.y + 20.0),
                text: format!("{title} (whitebox)"),
                size: NAME_SIZE,
                color: FRAME,
                align: Align::Left,
            });
        }
    }
    for block in &layout.blocks {
        block_shapes(&mut shapes, block.kind, block.rect, block.fill);
        let c = block.rect.center();
        shapes.push(Shape::Text {
            pos: Pos::new(c.x, c.y - 7.0),
            text: block.name.clone(),
            size: NAME_SIZE,
            color: INK,
            align: Align::Center,
        });
        shapes.push(Shape::Text {
            pos: Pos::new(c.x, c.y + 12.0),
            text: kind_caption(block.kind),
            size: KIND_SIZE,
            color: MUTED,
            align: Align::Center,
        });
        if block.has_content {
            // A small "has a whitebox" mark in the corner.
            let r = block.rect;
            let m = Rect::from_min_size(Pos::new(r.max.x - 22.0, r.max.y - 18.0), 14.0, 10.0);
            shapes.push(Shape::Rect {
                rect: m,
                radius: 1.0,
                fill: None,
                stroke: Some(Stroke::solid(1.0, MUTED)),
            });
            shapes.push(Shape::Rect {
                rect: Rect::from_min_size(Pos::new(m.min.x + 3.0, m.min.y + 3.0), 4.0, 4.0),
                radius: 0.0,
                fill: Some(MUTED),
                stroke: None,
            });
        }
    }
    // Where lines cross, the horizontal one jumps over the vertical one.
    let crossings = crate::layout::crossings(layout);
    for (li, line) in layout.lines.iter().enumerate() {
        let jumps: Vec<Pos> = crossings
            .iter()
            .filter(|c| c.horizontal == li)
            .map(|c| c.at)
            .collect();
        let crossed: Vec<Pos> = crossings
            .iter()
            .filter(|c| c.horizontal == li || c.vertical == li)
            .map(|c| c.at)
            .collect();
        line_shapes(&mut shapes, line, &jumps, &crossed);
    }
    // Labels stay inside the frame of a whitebox, or inside the picture.
    let label_area = label_area(layout);
    for line in &layout.lines {
        label_shapes(&mut shapes, line, label_area);
    }
    if let Some(at) = layout.legend_at {
        legend_shapes(&mut shapes, at, &layout.legend);
    }
    // Safety net: the picture grows around any text that still sticks out.
    let bounds = match text_bounds(&shapes) {
        Some(t) => Rect {
            min: Pos::new(
                layout.bounds.min.x.min(t.min.x - 4.0),
                layout.bounds.min.y.min(t.min.y - 4.0),
            ),
            max: Pos::new(
                layout.bounds.max.x.max(t.max.x + 4.0),
                layout.bounds.max.y.max(t.max.y + 4.0),
            ),
        },
        None => layout.bounds,
    };
    Scene { bounds, shapes }
}

fn block_shapes(shapes: &mut Vec<Shape>, kind: BlockKind, r: Rect, fill: Rgb) {
    let fill = Some(fill);
    let stroke = Some(OUTLINE);
    match kind {
        BlockKind::Component => shapes.push(Shape::Rect {
            rect: r,
            radius: 4.0,
            fill,
            stroke,
        }),
        BlockKind::Database => {
            let ry = 10.0;
            let cx = r.center().x;
            let rx = r.width() / 2.0;
            let mut body = vec![Pos::new(r.min.x, r.min.y + ry)];
            body.extend(arc(Pos::new(cx, r.max.y - ry), rx, ry, 180.0, 0.0));
            body.push(Pos::new(r.max.x, r.min.y + ry));
            body.extend(arc(Pos::new(cx, r.min.y + ry), rx, ry, 0.0, -180.0));
            shapes.push(Shape::Polygon {
                points: body,
                fill,
                stroke,
            });
            let mut lid = arc(Pos::new(cx, r.min.y + ry), rx, ry, 0.0, 360.0);
            lid.push(Pos::new(r.max.x, r.min.y + ry));
            shapes.push(Shape::Polyline {
                points: lid,
                stroke: OUTLINE,
            });
        }
        BlockKind::Queue => {
            let rx = 10.0;
            let cy = r.center().y;
            let ry = r.height() / 2.0;
            let mut body = vec![Pos::new(r.min.x + rx, r.min.y)];
            body.extend(arc(Pos::new(r.max.x - rx, cy), rx, ry, -90.0, 90.0));
            body.push(Pos::new(r.min.x + rx, r.max.y));
            body.extend(arc(Pos::new(r.min.x + rx, cy), rx, ry, 90.0, 270.0));
            shapes.push(Shape::Polygon {
                points: body,
                fill,
                stroke,
            });
            shapes.push(Shape::Polyline {
                points: arc(Pos::new(r.max.x - rx, cy), rx, ry, 90.0, 270.0),
                stroke: OUTLINE,
            });
        }
        BlockKind::Cache => {
            shapes.push(Shape::Rect {
                rect: r,
                radius: 4.0,
                fill,
                stroke,
            });
            shapes.push(Shape::Rect {
                rect: r.expand(-4.0),
                radius: 2.0,
                fill: None,
                stroke: Some(Stroke::solid(1.0, INK)),
            });
        }
        BlockKind::FileStorage => {
            let tab = Rect::from_min_size(Pos::new(r.min.x, r.min.y), r.width() * 0.4, 10.0);
            shapes.push(Shape::Rect {
                rect: tab,
                radius: 3.0,
                fill,
                stroke,
            });
            shapes.push(Shape::Rect {
                rect: Rect {
                    min: Pos::new(r.min.x, r.min.y + 8.0),
                    max: r.max,
                },
                radius: 3.0,
                fill,
                stroke,
            });
        }
        BlockKind::Ui => {
            shapes.push(Shape::Rect {
                rect: r,
                radius: 4.0,
                fill,
                stroke,
            });
            let bar = r.min.y + 14.0;
            shapes.push(Shape::Polyline {
                points: vec![Pos::new(r.min.x, bar), Pos::new(r.max.x, bar)],
                stroke: Stroke::solid(1.0, INK),
            });
            for i in 0..3 {
                shapes.push(Shape::Circle {
                    center: Pos::new(r.min.x + 9.0 + 8.0 * i as f32, r.min.y + 7.0),
                    radius: 2.2,
                    fill: Some(INK),
                    stroke: None,
                });
            }
        }
        BlockKind::Person => {
            shapes.push(Shape::Rect {
                rect: r,
                radius: r.height() / 2.0,
                fill,
                stroke,
            });
            let head = Pos::new(r.min.x + 26.0, r.center().y - 9.0);
            shapes.push(Shape::Circle {
                center: head,
                radius: 6.0,
                fill: None,
                stroke: Some(Stroke::solid(1.2, INK)),
            });
            shapes.push(Shape::Polyline {
                points: arc(Pos::new(head.x, head.y + 20.0), 11.0, 10.0, 180.0, 360.0),
                stroke: Stroke::solid(1.2, INK),
            });
        }
        BlockKind::ExternalSystem => shapes.push(Shape::Rect {
            rect: r,
            radius: 0.0,
            fill,
            stroke: Some(Stroke::dashed(1.5, INK)),
        }),
    }
}

/// Points on an ellipse from `from` to `to` degrees (0 = right, 90 = down).
fn arc(c: Pos, rx: f32, ry: f32, from: f32, to: f32) -> Vec<Pos> {
    let steps = 24;
    (0..=steps)
        .map(|i| {
            let deg = from + (to - from) * i as f32 / steps as f32;
            let rad = deg.to_radians();
            Pos::new(c.x + rx * rad.cos(), c.y + ry * rad.sin())
        })
        .collect()
}

fn line_shapes(shapes: &mut Vec<Shape>, line: &LineGeom, jumps: &[Pos], crossed: &[Pos]) {
    let stroke = Stroke::solid(1.5, LINE);
    shapes.push(Shape::Polyline {
        points: with_jumps(shaped_around(&line.points, line.style, crossed), jumps),
        stroke,
    });
    let n = line.points.len();
    // A dangling end shows a warning marker instead of an arrowhead.
    let marked = |p: Pos| line.kind == LineKind::Dangling && line.tip == Some(p);
    if n >= 2 {
        if line.arrow_start && !marked(line.points[0]) {
            shapes.push(arrow_head(line.points[1], line.points[0]));
        }
        if line.arrow_end && !marked(line.points[n - 1]) {
            shapes.push(arrow_head(line.points[n - 2], line.points[n - 1]));
        }
    }
    if let Some((p, _, _)) = &line.frame_port {
        shapes.push(Shape::Rect {
            rect: Rect::from_center(*p, 8.0, 8.0),
            radius: 0.0,
            fill: Some(PAPER),
            stroke: Some(Stroke::solid(1.2, FRAME)),
        });
    }
    match (line.kind, line.tip) {
        (LineKind::Stub, Some(tip)) => shapes.push(Shape::Circle {
            center: tip,
            radius: 5.0,
            fill: Some(PAPER),
            stroke: Some(stroke),
        }),
        (LineKind::Dangling, Some(tip)) => {
            shapes.push(Shape::Polygon {
                points: vec![
                    Pos::new(tip.x, tip.y - 9.0),
                    Pos::new(tip.x + 9.0, tip.y + 7.0),
                    Pos::new(tip.x - 9.0, tip.y + 7.0),
                ],
                fill: Some(WARN),
                stroke: None,
            });
            shapes.push(Shape::Text {
                pos: Pos::new(tip.x, tip.y + 1.5),
                text: "!".into(),
                size: 11.0,
                color: PAPER,
                align: Align::Center,
            });
        }
        _ => {}
    }
}

fn arrow_head(from: Pos, to: Pos) -> Shape {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / len, dy / len);
    let (l, w) = (11.0, 5.0);
    let base = Pos::new(to.x - ux * l, to.y - uy * l);
    Shape::Polygon {
        points: vec![
            to,
            Pos::new(base.x - uy * w, base.y + ux * w),
            Pos::new(base.x + uy * w, base.y - ux * w),
        ],
        fill: Some(LINE),
        stroke: None,
    }
}

fn label_shapes(shapes: &mut Vec<Shape>, line: &LineGeom, area: Rect) {
    if let Some((p, side, partner)) = &line.frame_port {
        let (pos, align) = match side {
            Side::Left => (Pos::new(p.x + 8.0, p.y + 13.0), Align::Left),
            Side::Right => (Pos::new(p.x - 8.0, p.y + 13.0), Align::Right),
            Side::Top => (Pos::new(p.x - 8.0, p.y + 16.0), Align::Right),
            Side::Bottom => (Pos::new(p.x - 8.0, p.y - 14.0), Align::Right),
        };
        shapes.push(Shape::Text {
            pos,
            text: partner.clone(),
            size: LABEL_SIZE,
            color: FRAME,
            align,
        });
    }
    if line.text.is_empty() {
        return;
    }
    let (pos, align) = match (line.tip, line.tip_dir) {
        (Some(tip), Some(dir)) => match dir {
            Side::Right => (Pos::new(tip.x + 12.0, tip.y), Align::Left),
            Side::Left => (Pos::new(tip.x - 12.0, tip.y), Align::Right),
            Side::Top => (Pos::new(tip.x, tip.y - 16.0), Align::Center),
            Side::Bottom => (Pos::new(tip.x, tip.y + 18.0), Align::Center),
        },
        _ if line.label_horizontal => (
            Pos::new(line.label_at.x, line.label_at.y - 11.0),
            Align::Center,
        ),
        _ => (
            Pos::new(line.label_at.x + 8.0, line.label_at.y),
            Align::Left,
        ),
    };
    // Too wide for the area: wrap; then shift the block so it lies inside the area.
    let lines = if text::width(&line.text, LABEL_SIZE) > area.width() {
        crate::layout::wrap(&line.text, area.width(), LABEL_SIZE)
    } else {
        vec![line.text.clone()]
    };
    let w = lines
        .iter()
        .map(|l| text::width(l, LABEL_SIZE))
        .fold(0.0, f32::max);
    let h = LABEL_SIZE + 4.0 + (lines.len() as f32 - 1.0) * LINE_HEIGHT;
    let left = match align {
        Align::Left => pos.x,
        Align::Center => pos.x - w / 2.0,
        Align::Right => pos.x - w,
    };
    let left = left.min(area.max.x - w).max(area.min.x);
    let top = pos.y - LABEL_SIZE / 2.0 - 2.0;
    shapes.push(Shape::Rect {
        rect: Rect::from_min_size(Pos::new(left - 3.0, top), w + 6.0, h),
        radius: 3.0,
        fill: Some(PAPER),
        stroke: None,
    });
    for (i, text) in lines.into_iter().enumerate() {
        shapes.push(Shape::Text {
            pos: Pos::new(left, pos.y + i as f32 * LINE_HEIGHT),
            text,
            size: LABEL_SIZE,
            color: INK,
            align: Align::Left,
        });
    }
}

/// Where relation labels have to stay: inside the frame of a whitebox, or inside the
/// picture.
pub fn label_area(layout: &Layout) -> Rect {
    layout.frame.unwrap_or(layout.bounds).expand(-8.0)
}

/// The box each line's label takes, as drawn: line index and rect.
pub fn label_rects(layout: &Layout) -> Vec<(usize, Rect)> {
    let area = label_area(layout);
    let mut out = Vec::new();
    for (li, line) in layout.lines.iter().enumerate() {
        let mut shapes = Vec::new();
        label_shapes(&mut shapes, line, area);
        // The text's paper background is the only rect a label draws.
        let rect = shapes.iter().find_map(|s| match s {
            Shape::Rect { rect, .. } => Some(*rect),
            _ => None,
        });
        if let Some(rect) = rect {
            out.push((li, rect));
        }
    }
    out
}

/// Where the partner name next to each frame end is written: line index and rect.
pub fn frame_label_rects(layout: &Layout) -> Vec<(usize, Rect)> {
    let area = label_area(layout);
    let mut out = Vec::new();
    for (li, line) in layout.lines.iter().enumerate() {
        if line.frame_port.is_none() {
            continue;
        }
        let mut shapes = Vec::new();
        label_shapes(&mut shapes, line, area);
        // The partner name is the first text a frame end writes.
        if let Some(Shape::Text {
            pos,
            text,
            size,
            align,
            ..
        }) = shapes.first()
        {
            let w = text::width(text, *size);
            let left = match align {
                Align::Left => pos.x,
                Align::Center => pos.x - w / 2.0,
                Align::Right => pos.x - w,
            };
            out.push((
                li,
                Rect::from_min_size(Pos::new(left, pos.y - size * 0.7), w, size * 1.4),
            ));
        }
    }
    out
}

/// Distance between wrapped label lines.
const LINE_HEIGHT: f32 = LABEL_SIZE + 3.0;

/// The area every text shape covers, so nothing is drawn outside the picture.
fn text_bounds(shapes: &[Shape]) -> Option<Rect> {
    shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Text {
                pos,
                text: t,
                size,
                align,
                ..
            } => {
                let w = text::width(t, *size);
                let left = match align {
                    Align::Left => pos.x,
                    Align::Center => pos.x - w / 2.0,
                    Align::Right => pos.x - w,
                };
                Some(Rect::from_min_size(
                    Pos::new(left, pos.y - size * 0.7),
                    w,
                    size * 1.4,
                ))
            }
            _ => None,
        })
        .reduce(|a, b| Rect {
            min: Pos::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
            max: Pos::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y)),
        })
}

/// A routed polyline with its bends drawn in `style`. End segments stay straight, so
/// arrowheads keep their direction.
pub fn shaped(points: &[Pos], style: LineStyle) -> Vec<Pos> {
    shaped_around(points, style, &[])
}

/// Like [`shaped`], but bends stay clear of the points in `crossed` (where the line
/// crosses another), so the line runs straight there and a jump fits.
pub fn shaped_around(points: &[Pos], style: LineStyle, crossed: &[Pos]) -> Vec<Pos> {
    let radius = match style {
        LineStyle::Square => return points.to_vec(),
        LineStyle::Round6 => 6.0,
        LineStyle::Round12 => 12.0,
        LineStyle::Curved => f32::INFINITY,
    };
    let n = points.len();
    if n < 3 {
        return points.to_vec();
    }
    let mut out = vec![points[0]];
    for i in 1..n - 1 {
        let (prev, corner, next) = (points[i - 1], points[i], points[i + 1]);
        let (d_in, d_out) = (prev.dist(corner), corner.dist(next));
        // Curves reach to the middle of each segment; rounded corners take at most
        // their radius, and never more than half a segment, so short steps stay steps.
        let mut r = radius.min(d_in / 2.0).min(d_out / 2.0);
        for c in crossed {
            if on_segment(*c, prev, corner) || on_segment(*c, corner, next) {
                r = r.min(corner.dist(*c) - JUMP_RADIUS - 1.0);
            }
        }
        if r < 0.5 {
            out.push(corner);
            continue;
        }
        let toward = |from: Pos, to: Pos, len: f32| {
            let d = from.dist(to).max(0.001);
            Pos::new(
                from.x + (to.x - from.x) * len / d,
                from.y + (to.y - from.y) * len / d,
            )
        };
        let start = toward(corner, prev, r);
        let end = toward(corner, next, r);
        // Quadratic Bézier from `start` to `end` with the corner as control point.
        let steps = 10;
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            let u = 1.0 - t;
            out.push(Pos::new(
                u * u * start.x + 2.0 * u * t * corner.x + t * t * end.x,
                u * u * start.y + 2.0 * u * t * corner.y + t * t * end.y,
            ));
        }
    }
    out.push(points[n - 1]);
    out
}

/// Whether `c` lies on the straight segment from `a` to `b`.
fn on_segment(c: Pos, a: Pos, b: Pos) -> bool {
    let within = |v: f32, p: f32, q: f32| v >= p.min(q) - 0.5 && v <= p.max(q) + 0.5;
    within(c.x, a.x, b.x) && within(c.y, a.y, b.y) && {
        let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        cross.abs() <= 0.5 * a.dist(b).max(1.0)
    }
}

/// Radius of the small arc a line makes where it jumps over another.
pub const JUMP_RADIUS: f32 = 5.0;

/// Splices a small arc into horizontal stretches at every point in `jumps` that lies
/// on one, so the line visibly jumps over the line it crosses there.
pub fn with_jumps(points: Vec<Pos>, jumps: &[Pos]) -> Vec<Pos> {
    if jumps.is_empty() {
        return points;
    }
    let r = JUMP_RADIUS;
    let mut out = Vec::with_capacity(points.len());
    for (i, p) in points.iter().enumerate() {
        out.push(*p);
        let Some(q) = points.get(i + 1) else { break };
        if (p.y - q.y).abs() > 0.01 {
            continue;
        }
        let (lo, hi) = (p.x.min(q.x) + r, p.x.max(q.x) - r);
        let mut xs: Vec<f32> = jumps
            .iter()
            .filter(|j| (j.y - p.y).abs() < 0.5 && j.x > lo && j.x < hi)
            .map(|j| j.x)
            .collect();
        let rightward = q.x >= p.x;
        xs.sort_by(|a, b| {
            if rightward {
                a.total_cmp(b)
            } else {
                b.total_cmp(a)
            }
        });
        for x in xs {
            // Over the top: from the left (180°) through up (270°) to the right, or back.
            let (from, to) = if rightward {
                (180.0, 360.0)
            } else {
                (0.0, -180.0)
            };
            out.extend(arc(Pos::new(x, p.y), r, r, from, to));
        }
    }
    out
}

/// The legend below the diagram: "Legend", then one entry per shortened text.
fn legend_shapes(shapes: &mut Vec<Shape>, at: Pos, entries: &[LegendEntry]) {
    shapes.push(Shape::Text {
        pos: Pos::new(at.x, at.y + LEGEND_LINE / 2.0),
        text: "Legend".into(),
        size: LABEL_SIZE,
        color: MUTED,
        align: Align::Left,
    });
    let key_width = entries
        .iter()
        .map(|e| text::width(&e.key, LABEL_SIZE))
        .fold(0.0, f32::max)
        + 12.0;
    let mut y = at.y + LEGEND_LINE * 1.5;
    for entry in entries {
        shapes.push(Shape::Text {
            pos: Pos::new(at.x, y + LEGEND_LINE / 2.0),
            text: entry.key.clone(),
            size: LABEL_SIZE,
            color: INK,
            align: Align::Left,
        });
        for line in &entry.lines {
            shapes.push(Shape::Text {
                pos: Pos::new(at.x + key_width, y + LEGEND_LINE / 2.0),
                text: line.clone(),
                size: LABEL_SIZE,
                color: INK,
                align: Align::Left,
            });
            y += LEGEND_LINE;
        }
        if entry.lines.is_empty() {
            y += LEGEND_LINE;
        }
    }
}
