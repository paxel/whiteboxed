//! A layout turned into plain drawing primitives. The canvas and the SVG export
//! both draw exactly this list, so what you see is what you export.

use crate::geom::{Pos, Rect};
use crate::layout::{KIND_SIZE, LABEL_SIZE, Layout, LineGeom, LineKind, NAME_SIZE, kind_caption};
use crate::model::{BlockKind, Rgb, Side};
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
    for line in &layout.lines {
        line_shapes(&mut shapes, line);
    }
    for line in &layout.lines {
        label_shapes(&mut shapes, line);
    }
    Scene {
        bounds: layout.bounds,
        shapes,
    }
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

fn line_shapes(shapes: &mut Vec<Shape>, line: &LineGeom) {
    let stroke = Stroke::solid(1.5, LINE);
    shapes.push(Shape::Polyline {
        points: line.points.clone(),
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

fn label_shapes(shapes: &mut Vec<Shape>, line: &LineGeom) {
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
    let w = text::width(&line.text, LABEL_SIZE);
    let left = match align {
        Align::Left => pos.x,
        Align::Center => pos.x - w / 2.0,
        Align::Right => pos.x - w,
    };
    shapes.push(Shape::Rect {
        rect: Rect::from_min_size(
            Pos::new(left - 3.0, pos.y - LABEL_SIZE / 2.0 - 2.0),
            w + 6.0,
            LABEL_SIZE + 4.0,
        ),
        radius: 3.0,
        fill: Some(PAPER),
        stroke: None,
    });
    shapes.push(Shape::Text {
        pos,
        text: line.text.clone(),
        size: LABEL_SIZE,
        color: INK,
        align,
    });
}
