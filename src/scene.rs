//! A layout turned into plain drawing primitives. The canvas and the SVG export
//! both draw exactly this list, so what you see is what you export.

use std::sync::Arc;

use crate::geom::{Pos, Rect};
use crate::layout::{
    BlockGeom, CAPTIONED_H, DrawnLook, EndPicture, ICON_H, KIND_SIZE, LABEL_SIZE, LEGEND_LINE,
    LOOK_GAP, Layout, LegendEntry, LineGeom, LineKind, LookSample, NAME_SIZE, PAD, kind_caption,
};
use crate::look::LookMode;
use crate::model::{BlockKind, LineStyle, Rgb, Side};
use crate::text;

pub const INK: Rgb = Rgb(0x22, 0x22, 0x22);
pub const MUTED: Rgb = Rgb(0x66, 0x66, 0x66);
pub const LINE: Rgb = Rgb(0x44, 0x4b, 0x55);
pub const FRAME: Rgb = Rgb(0x8a, 0x93, 0x9e);
pub const WARN: Rgb = Rgb(0xe0, 0x8a, 0x00);
pub const PAPER: Rgb = Rgb(0xff, 0xff, 0xff);
/// Radius of a mousehole's arch.
pub const HOLE_R: f32 = 8.0;
/// Height of a picture at the end of a line; its width follows from the picture.
pub const END_H: f32 = 12.0;
/// Keys of the pictures on lines start here, above every box id.
const LINE_KEYS: u64 = 1 << 32;
/// Size of the number at a mousehole.
const HOLE_SIZE: f32 = 11.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub color: Rgb,
    /// Dash and gap length, or `None` for a solid line.
    pub dash: Option<(f32, f32)>,
}

impl Stroke {
    pub const fn solid(width: f32, color: Rgb) -> Self {
        Stroke {
            width,
            color,
            dash: None,
        }
    }

    pub const fn dashed(width: f32, color: Rgb) -> Self {
        Stroke {
            width,
            color,
            dash: Some((6.0, 4.0)),
        }
    }

    pub const fn dotted(width: f32, color: Rgb) -> Self {
        Stroke {
            width,
            color,
            dash: Some((1.5, 3.5)),
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
    /// A custom look, scaled into `rect` without stretching (or stretched to fill it,
    /// for a slice of a frame), then turned by `angle` (radians, clockwise) about the
    /// rect's centre. `key` is unique per picture drawn, so ids inside never clash
    /// with another look in the same document.
    Look {
        rect: Rect,
        svg: Arc<str>,
        size: (f32, f32),
        key: u64,
        angle: f32,
        stretch: bool,
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

/// Height of the icon next to a whitebox's title.
pub const FRAME_ICON_H: f32 = 20.0;
/// Keys of the slices of a frame drawn from a look.
const FRAME_KEYS: u64 = 3 << 32;

pub fn scene(layout: &Layout) -> Scene {
    let mut shapes = Vec::new();
    if let Some(frame) = layout.frame {
        frame_shapes(&mut shapes, layout, frame);
    }
    for block in &layout.blocks {
        if block.kind == BlockKind::Group {
            group_shapes(&mut shapes, block);
            continue;
        }
        let (picture, c) = look_place(block, layout.captions);
        if block
            .look
            .as_ref()
            .is_none_or(|l| l.mode() == LookMode::Icon)
        {
            block_shapes(&mut shapes, block.kind, block.rect, block.fill);
        }
        // The frame around the name under a picture: where lines from below land.
        if let Some(frame) = block.caption {
            shapes.push(Shape::Rect {
                rect: frame,
                radius: 4.0,
                fill: Some(block.fill),
                stroke: Some(OUTLINE),
            });
        }
        if let (Some(look), Some(area)) = (&block.look, picture) {
            shapes.push(Shape::Look {
                rect: crate::look::fit(look.size(), area),
                svg: look.svg().clone(),
                size: look.size(),
                key: block.id.0,
                angle: 0.0,
                stretch: false,
            });
        }
        if layout.captions {
            shapes.push(Shape::Text {
                pos: Pos::new(c.x, c.y - 7.0),
                text: block.name.clone(),
                size: NAME_SIZE,
                color: INK,
                align: Align::Center,
            });
        } else {
            // Doodle shapes: every line of the text, centred.
            let lines: Vec<&str> = block.name.lines().collect();
            let n = lines.len() as f32;
            for (k, line) in lines.into_iter().enumerate() {
                shapes.push(Shape::Text {
                    pos: Pos::new(c.x, c.y + (k as f32 - (n - 1.0) / 2.0) * NAME_LINE),
                    text: line.to_owned(),
                    size: NAME_SIZE,
                    color: INK,
                    align: Align::Center,
                });
            }
        }
        if layout.captions {
            shapes.push(Shape::Text {
                pos: Pos::new(c.x, c.y + 12.0),
                text: kind_caption(block.kind),
                size: KIND_SIZE,
                color: MUTED,
                align: Align::Center,
            });
        }
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
        line_shapes(
            &mut shapes,
            line,
            &jumps,
            &crossed,
            LINE_KEYS + 4 * li as u64,
        );
    }
    // Labels stay inside the frame of a whitebox, or inside the picture.
    let label_area = label_area(layout);
    for line in &layout.lines {
        frame_name_shapes(&mut shapes, line);
        label_shapes(&mut shapes, line, label_area);
    }
    if let Some(at) = layout.legend_at {
        legend_shapes(&mut shapes, at, &layout.legend, &layout.legend_looks);
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

/// The whitebox frame: the dashed rectangle with the title inside, or the box's own
/// look stretched as a nine-slice (corners keep their size), with the title inside
/// or, for a look with the name below, in the caption under the frame. An icon look
/// leaves the frame plain and sits next to the title.
fn frame_shapes(shapes: &mut Vec<Shape>, layout: &Layout, frame: Rect) {
    let title = layout
        .title
        .as_ref()
        .map(|t| format!("{t} (whitebox)"))
        .unwrap_or_default();
    let mut title_at = Pos::new(frame.min.x + 16.0, frame.min.y + 20.0);
    match &layout.frame_look {
        Some(fl) if fl.look.mode() != LookMode::Icon => {
            let (cw, ch) = fl.corner;
            let (cw, ch) = (cw.min(frame.width() / 2.0), ch.min(frame.height() / 2.0));
            let xs = [frame.min.x, frame.min.x + cw, frame.max.x - cw, frame.max.x];
            let ys = [frame.min.y, frame.min.y + ch, frame.max.y - ch, frame.max.y];
            for row in 0..3 {
                for col in 0..3 {
                    let rect = Rect {
                        min: Pos::new(xs[col], ys[row]),
                        max: Pos::new(xs[col + 1], ys[row + 1]),
                    };
                    if rect.width() <= 0.0 || rect.height() <= 0.0 {
                        continue;
                    }
                    shapes.push(Shape::Look {
                        rect,
                        svg: fl.slices[row * 3 + col].clone(),
                        size: (rect.width(), rect.height()),
                        key: FRAME_KEYS + (row * 3 + col) as u64,
                        angle: 0.0,
                        stretch: true,
                    });
                }
            }
            if let Some(caption) = layout.frame_caption {
                shapes.push(Shape::Rect {
                    rect: caption,
                    radius: 4.0,
                    fill: Some(PAPER),
                    stroke: Some(Stroke::solid(1.5, FRAME)),
                });
                shapes.push(Shape::Text {
                    pos: caption.center(),
                    text: title,
                    size: NAME_SIZE,
                    color: FRAME,
                    align: Align::Center,
                });
                return;
            }
        }
        other => {
            shapes.push(Shape::Rect {
                rect: frame.expand(-4.0),
                radius: 4.0,
                fill: None,
                stroke: Some(Stroke::dashed(1.5, FRAME)),
            });
            if let Some(fl) = other {
                let (w, h) = fl.look.size();
                let icon = Rect::from_min_size(
                    Pos::new(frame.min.x + 16.0, frame.min.y + 10.0),
                    FRAME_ICON_H * w / h,
                    FRAME_ICON_H,
                );
                shapes.push(Shape::Look {
                    rect: icon,
                    svg: fl.look.svg().clone(),
                    size: (w, h),
                    key: FRAME_KEYS,
                    angle: 0.0,
                    stretch: false,
                });
                title_at.x = icon.max.x + 8.0;
            }
        }
    }
    if layout.title.is_some() {
        shapes.push(Shape::Text {
            pos: title_at,
            text: title,
            size: NAME_SIZE,
            color: FRAME,
            align: Align::Left,
        });
    }
}

/// Where a box's look goes, and the centre of its text.
fn look_place(block: &BlockGeom, captions: bool) -> (Option<Rect>, Pos) {
    let r = block.rect;
    let Some(look) = &block.look else {
        return (None, r.center());
    };
    if let (Some(picture), Some(caption)) = (block.picture, block.caption) {
        return (Some(picture), caption.center());
    }
    let text_h = if captions {
        CAPTIONED_H
    } else {
        block.name.lines().count().max(1) as f32 * NAME_LINE
    };
    match look.mode() {
        LookMode::Inside => (Some(r), r.center()),
        LookMode::Below => {
            let text_y = r.max.y - PAD / 2.0 - text_h / 2.0;
            let picture = Rect {
                min: r.min,
                max: Pos::new(r.max.x, r.max.y - PAD / 2.0 - text_h - LOOK_GAP),
            };
            (Some(picture), Pos::new(r.center().x, text_y))
        }
        LookMode::Icon => {
            let icon = Rect::from_min_size(
                Pos::new(r.min.x + PAD, r.min.y + PAD / 2.0),
                r.width() - 2.0 * PAD,
                ICON_H,
            );
            let below = icon.max.y + LOOK_GAP;
            (Some(icon), Pos::new(r.center().x, (below + r.max.y) / 2.0))
        }
    }
}

fn block_shapes(shapes: &mut Vec<Shape>, kind: BlockKind, r: Rect, fill: Rgb) {
    let fill = Some(fill);
    let stroke = Some(OUTLINE);
    match kind {
        BlockKind::Rounded => shapes.push(Shape::Rect {
            rect: r,
            radius: (r.height() / 2.0).min(18.0),
            fill,
            stroke,
        }),
        BlockKind::Circle | BlockKind::Cloud => {
            let c = r.center();
            let (rx, ry) = (r.width() / 2.0, r.height() / 2.0);
            let points = if kind == BlockKind::Circle {
                arc(c, rx, ry, 0.0, 360.0)
            } else {
                cloud(c, rx, ry)
            };
            shapes.push(Shape::Polygon {
                points,
                fill,
                stroke,
            });
        }
        BlockKind::Diamond => {
            let c = r.center();
            shapes.push(Shape::Polygon {
                points: vec![
                    Pos::new(c.x, r.min.y),
                    Pos::new(r.max.x, c.y),
                    Pos::new(c.x, r.max.y),
                    Pos::new(r.min.x, c.y),
                ],
                fill,
                stroke,
            });
        }
        BlockKind::Document => {
            // A sheet whose lower edge waves.
            let wave = 8.0;
            let mut points = vec![
                Pos::new(r.min.x, r.min.y),
                Pos::new(r.max.x, r.min.y),
                Pos::new(r.max.x, r.max.y - wave),
            ];
            let steps = 24;
            for i in 0..=steps {
                let t = i as f32 / steps as f32;
                let x = r.max.x - t * r.width();
                let y = r.max.y - wave + wave * (t * std::f32::consts::TAU).sin();
                points.push(Pos::new(x, y));
            }
            shapes.push(Shape::Polygon {
                points,
                fill,
                stroke,
            });
        }
        // No border: a sticky note in its colour (white on the white page by default).
        BlockKind::Note => shapes.push(Shape::Rect {
            rect: r,
            radius: 2.0,
            fill,
            stroke: None,
        }),
        // A group's frame is drawn on its own, behind the shapes.
        BlockKind::Group => {}
        BlockKind::Component | BlockKind::Page => shapes.push(Shape::Rect {
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

/// A group: a rounded frame (tinted when it has a colour) and its name at the top
/// left, inside the frame.
fn group_shapes(shapes: &mut Vec<Shape>, block: &crate::layout::BlockGeom) {
    let tinted = block.fill != crate::layout::DEFAULT_FILL;
    shapes.push(Shape::Rect {
        rect: block.rect,
        radius: 10.0,
        fill: tinted.then_some(block.fill),
        stroke: Some(Stroke::solid(1.2, FRAME)),
    });
    let r = group_label_rect(block);
    // The name sits on the frame's top edge, which it interrupts.
    shapes.push(Shape::Rect {
        rect: r.expand(2.0),
        radius: 3.0,
        fill: Some(if tinted { block.fill } else { PAPER }),
        stroke: None,
    });
    shapes.push(Shape::Text {
        pos: Pos::new(r.min.x, r.center().y),
        text: block.name.lines().next().unwrap_or_default().to_owned(),
        size: LABEL_SIZE,
        color: MUTED,
        align: Align::Left,
    });
}

/// Where a group's name is written: the place to click to select the group.
pub fn group_label_rect(block: &crate::layout::BlockGeom) -> Rect {
    let name = block.name.lines().next().unwrap_or_default();
    let w = text::width(name, LABEL_SIZE).max(20.0);
    let h = LABEL_SIZE + 6.0;
    Rect::from_min_size(
        Pos::new(block.rect.min.x + 12.0, block.rect.min.y - h / 2.0),
        w,
        h,
    )
}

/// The outline of a cloud: an ellipse with bumps.
fn cloud(c: Pos, rx: f32, ry: f32) -> Vec<Pos> {
    let steps = 96;
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32 * std::f32::consts::TAU;
            // Bumps outward, but the whole cloud stays inside its box.
            let bump = (1.0 + 0.09 * (t * 7.0).sin().abs()) / 1.09;
            Pos::new(c.x + rx * bump * t.cos(), c.y + ry * bump * t.sin())
        })
        .collect()
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

/// The stroke along a line: its look's width and dash, or the plain ones.
fn along(look: Option<&DrawnLook>, pattern: crate::model::LinePattern, color: Rgb) -> Stroke {
    use crate::model::LinePattern;
    let (pattern, width) = match look {
        Some(look) => (look.dash, look.width),
        None => (
            pattern,
            if pattern == LinePattern::Dotted {
                1.8
            } else {
                1.5
            },
        ),
    };
    match pattern {
        LinePattern::Solid => Stroke::solid(width, color),
        LinePattern::Dashed => Stroke::dashed(width, color),
        LinePattern::Dotted => Stroke::dotted(width, color),
    }
}

/// Where the pictures of a line's look sit at its ends, as drawn by [`end_shapes`]:
/// the area each covers (unrotated: as wide as the picture is long along the line).
pub fn end_picture_rects(line: &LineGeom) -> Vec<Rect> {
    let Some(look) = &line.look else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // A whole line has both ends; a piece to a mousehole only the end at its port.
    let pieces: Vec<(&[Pos], bool, bool)> = match &line.mousehole {
        Some(m) => vec![(&m.piece_a[..], true, false), (&m.piece_b[..], false, true)],
        None => vec![(&line.points[..], true, true)],
    };
    for (piece, at_a, at_b) in pieces {
        let n = piece.len();
        if n < 2 {
            continue;
        }
        for (at_start, arrow, drawn) in [
            (true, line.arrow_start, at_a),
            (false, line.arrow_end, at_b),
        ] {
            if !drawn {
                continue;
            }
            let picture = if arrow {
                look.head.as_ref()
            } else {
                look.tail.as_ref()
            };
            let Some(picture) = picture else { continue };
            let (from, tip) = if at_start {
                (piece[1], piece[0])
            } else {
                (piece[n - 2], piece[n - 1])
            };
            let (dx, dy) = (tip.x - from.x, tip.y - from.y);
            let len = (dx * dx + dy * dy).sqrt().max(0.001);
            let (ux, uy) = (dx / len, dy / len);
            let width = END_H * picture.size.0 / picture.size.1;
            let center = Pos::new(tip.x - ux * width / 2.0, tip.y - uy * width / 2.0);
            let reach = width.max(END_H);
            out.push(Rect::from_center(center, reach, reach));
        }
    }
    out
}

/// The ends of a line (or of a piece of one): the picture of its look, with the line
/// cut back to where the picture begins, or the plain arrowhead. `arrows` says which
/// ends point into their box; `ends` which ends are drawn at all (a piece drawn to a
/// mousehole has no end there). `key` is for the pictures' ids.
fn end_shapes(
    shapes: &mut Vec<Shape>,
    points: &mut [Pos],
    arrows: (bool, bool),
    ends: (bool, bool),
    look: Option<&DrawnLook>,
    color: Rgb,
    key: u64,
) {
    if points.len() < 2 {
        return;
    }
    for (at_start, arrow, drawn) in [(true, arrows.0, ends.0), (false, arrows.1, ends.1)] {
        if !drawn {
            continue;
        }
        let picture = look.and_then(|l| {
            if arrow {
                l.head.as_ref()
            } else {
                l.tail.as_ref()
            }
        });
        let n = points.len();
        let (from, tip) = if at_start {
            (points[1], points[0])
        } else {
            (points[n - 2], points[n - 1])
        };
        match picture {
            Some(picture) => {
                let (dx, dy) = (tip.x - from.x, tip.y - from.y);
                let len = (dx * dx + dy * dy).sqrt().max(0.001);
                let (ux, uy) = (dx / len, dy / len);
                let width = END_H * picture.size.0 / picture.size.1;
                let back = width.min(len);
                let cut = Pos::new(tip.x - ux * back, tip.y - uy * back);
                if at_start {
                    points[0] = cut;
                } else {
                    points[n - 1] = cut;
                }
                let center = Pos::new(tip.x - ux * width / 2.0, tip.y - uy * width / 2.0);
                shapes.push(Shape::Look {
                    rect: Rect::from_center(center, width, END_H),
                    svg: picture.svg.clone(),
                    size: picture.size,
                    key: key + u64::from(!at_start),
                    angle: uy.atan2(ux),
                    stretch: false,
                });
            }
            None if arrow => shapes.push(arrow_head(from, tip, color)),
            None => {}
        }
    }
}

fn line_shapes(shapes: &mut Vec<Shape>, line: &LineGeom, jumps: &[Pos], crossed: &[Pos], key: u64) {
    let color = line.color.unwrap_or(LINE);
    let along = along(line.look.as_ref(), line.pattern, color);
    // A dangling end shows a warning marker instead of an arrowhead.
    let marked = |p: Pos| line.kind == LineKind::Dangling && line.tip == Some(p);
    if let Some(m) = &line.mousehole {
        // Only the pieces from the ports to the holes, each with its end; the holes
        // carry the legend number and the short label.
        for (piece, arrows, ends) in [
            (&m.piece_a, (line.arrow_start, false), (true, false)),
            (&m.piece_b, (false, line.arrow_end), (false, true)),
        ] {
            let mut points = piece.clone();
            end_shapes(
                shapes,
                &mut points,
                arrows,
                ends,
                line.look.as_ref(),
                color,
                key,
            );
            shapes.push(Shape::Polyline {
                points: with_jumps(shaped_around(&points, line.style, crossed), jumps),
                stroke: along,
            });
        }
        let label = if line.text.is_empty() {
            m.label.clone()
        } else {
            format!("{} {}", m.label, line.text)
        };
        let n = m.piece_a.len();
        let before_a = m.piece_a.get(n.wrapping_sub(2)).copied().unwrap_or(m.a);
        let before_b = m.piece_b.get(1).copied().unwrap_or(m.b);
        hole_shapes(
            shapes,
            m.a,
            before_a,
            &label,
            color,
            m.portal_a.as_ref(),
            key + 2,
        );
        hole_shapes(
            shapes,
            m.b,
            before_b,
            &label,
            color,
            m.portal_b.as_ref(),
            key + 3,
        );
    } else {
        let mut points = line.points.clone();
        let n = points.len();
        let ends = (
            n >= 2 && !marked(points[0]),
            n >= 2 && !marked(points[n - 1]),
        );
        end_shapes(
            shapes,
            &mut points,
            (line.arrow_start, line.arrow_end),
            ends,
            line.look.as_ref(),
            color,
            key,
        );
        shapes.push(Shape::Polyline {
            points: with_jumps(shaped_around(&points, line.style, crossed), jumps),
            stroke: along,
        });
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
            stroke: Some(Stroke::solid(1.5, color)),
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

/// Where the label of a hole at `c` is written: past the hole in the line's
/// direction `(ux, uy)`, beyond what the hole reaches.
fn hole_label_anchor(c: Pos, (ux, uy): (f32, f32), reach: f32) -> (Pos, Align) {
    if ux.abs() > uy.abs() {
        if ux > 0.0 {
            (Pos::new(c.x + reach + 5.0, c.y), Align::Left)
        } else {
            (Pos::new(c.x - reach - 5.0, c.y), Align::Right)
        }
    } else if uy > 0.0 {
        (Pos::new(c.x, c.y + reach + 10.0), Align::Center)
    } else {
        (Pos::new(c.x, c.y - reach - 9.0), Align::Center)
    }
}

/// What the labels at a line's two holes cover, as [`hole_shapes`] writes them;
/// nothing for a line that is not through mouseholes.
pub fn hole_label_rects(line: &LineGeom) -> Vec<Rect> {
    let Some(m) = &line.mousehole else {
        return Vec::new();
    };
    let label = if line.text.is_empty() {
        m.label.clone()
    } else {
        format!("{} {}", m.label, line.text)
    };
    let w = text::width(&label, HOLE_SIZE);
    let n = m.piece_a.len();
    let before_a = m.piece_a.get(n.wrapping_sub(2)).copied().unwrap_or(m.a);
    let before_b = m.piece_b.get(1).copied().unwrap_or(m.b);
    [(m.a, before_a, &m.portal_a), (m.b, before_b, &m.portal_b)]
        .into_iter()
        .map(|(c, from, portal)| {
            let (dx, dy) = (c.x - from.x, c.y - from.y);
            let len = (dx * dx + dy * dy).sqrt().max(0.001);
            let reach = match portal {
                Some(p) => (HOLE_R * p.size.0 / p.size.1).max(HOLE_R),
                None => HOLE_R,
            };
            let (pos, align) = hole_label_anchor(c, (dx / len, dy / len), reach);
            let left = match align {
                Align::Left => pos.x,
                Align::Center => pos.x - w / 2.0,
                Align::Right => pos.x - w,
            };
            Rect::from_min_size(Pos::new(left, pos.y - HOLE_SIZE / 2.0), w, HOLE_SIZE)
        })
        .collect()
}

/// A mousehole at `c` for a line coming from `from`: a filled arch, its round side
/// up like a hole in a skirting board whichever way the line runs, or the project's
/// portal picture turned the way of the line; and the label past the hole.
fn hole_shapes(
    shapes: &mut Vec<Shape>,
    c: Pos,
    from: Pos,
    label: &str,
    color: Rgb,
    portal: Option<&EndPicture>,
    key: u64,
) {
    let (dx, dy) = (c.x - from.x, c.y - from.y);
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / len, dy / len);
    let theta = uy.atan2(ux);
    // How far the hole reaches ahead of `c`, where the label starts.
    let reach = match portal {
        Some(p) => {
            let width = 2.0 * HOLE_R * p.size.0 / p.size.1;
            shapes.push(Shape::Look {
                rect: Rect::from_center(c, width, 2.0 * HOLE_R),
                svg: p.svg.clone(),
                size: p.size,
                key,
                angle: theta,
                stretch: false,
            });
            (width / 2.0).max(HOLE_R)
        }
        None => {
            shapes.push(Shape::Polygon {
                points: arc(c, HOLE_R, HOLE_R, 180.0, 360.0),
                fill: Some(color),
                stroke: None,
            });
            HOLE_R
        }
    };
    let (pos, align) = hole_label_anchor(c, (ux, uy), reach);
    shapes.push(Shape::Text {
        pos,
        text: label.to_owned(),
        size: HOLE_SIZE,
        color,
        align,
    });
}

fn arrow_head(from: Pos, to: Pos, color: Rgb) -> Shape {
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
        fill: Some(color),
        stroke: None,
    }
}

/// The partner name at a frame end: its lines with where each is written, their
/// alignment, and the rect they cover. Beside the end on the left and right of a
/// frame; on top and bottom where and how the layout decided (see
/// [`LineGeom::frame_name`]), the rows further from the frame stacked inward.
fn frame_name(line: &LineGeom) -> Option<FrameName> {
    let (p, side, partner) = line.frame_port.as_ref().filter(|_| line.frame_named)?;
    let (left, row, width) = line.frame_name;
    let (pos, align) = match side {
        Side::Left => (Pos::new(p.x + 8.0, p.y + 13.0), Align::Left),
        Side::Right => (Pos::new(p.x - 8.0, p.y + 13.0), Align::Right),
        Side::Top | Side::Bottom => {
            let inward = row as f32 * NAME_ROW;
            let y = if *side == Side::Top {
                p.y + 16.0 + inward
            } else {
                p.y - 14.0 - inward
            };
            if left {
                (Pos::new(p.x - 8.0, y), Align::Right)
            } else {
                (Pos::new(p.x + 8.0, y), Align::Left)
            }
        }
    };
    let lines = if text::width(partner, LABEL_SIZE) > width {
        crate::layout::wrap(partner, width, LABEL_SIZE)
    } else {
        vec![partner.clone()]
    };
    // Above the frame's bottom the lines of one name stack upward.
    let top = if *side == Side::Bottom {
        pos.y - (lines.len() as f32 - 1.0) * NAME_ROW
    } else {
        pos.y
    };
    let w = lines
        .iter()
        .map(|l| text::width(l, LABEL_SIZE))
        .fold(0.0, f32::max);
    let x = match align {
        Align::Left => pos.x,
        Align::Center => pos.x - w / 2.0,
        Align::Right => pos.x - w,
    };
    let rect = name_rows_rect(*p, *side, row, lines.len(), x, x + w);
    let texts = lines
        .into_iter()
        .enumerate()
        .map(|(i, text)| (Pos::new(pos.x, top + i as f32 * NAME_ROW), text))
        .collect();
    Some(FrameName { texts, align, rect })
}

/// What rows `row..row + n` of a partner name beside the frame end `p` on `side`
/// cover between `x0` and `x1`: the rect of a name as drawn, or of a placement
/// the layout weighs against the lines.
pub fn name_rows_rect(p: Pos, side: Side, row: usize, n: usize, x0: f32, x1: f32) -> Rect {
    let depth = (row + n - 1) as f32 * NAME_ROW;
    let top = if side == Side::Top {
        p.y + 16.0 + row as f32 * NAME_ROW
    } else {
        p.y - 14.0 - depth
    };
    let h = LABEL_SIZE * 1.4 + (n as f32 - 1.0) * NAME_ROW;
    Rect::from_min_size(Pos::new(x0, top - LABEL_SIZE * 0.7), x1 - x0, h)
}

/// A partner name as written at its frame end.
struct FrameName {
    /// Each line of the name and where it is written.
    texts: Vec<(Pos, String)>,
    align: Align,
    /// What the name covers.
    rect: Rect,
}

/// The partner name at a frame end, on paper.
fn frame_name_shapes(shapes: &mut Vec<Shape>, line: &LineGeom) {
    if let Some(name) = frame_name(line) {
        // On paper, since a name in a further row lies over neighbouring lines.
        shapes.push(Shape::Rect {
            rect: name.rect,
            radius: 0.0,
            fill: Some(PAPER),
            stroke: None,
        });
        for (pos, text) in name.texts {
            shapes.push(Shape::Text {
                pos,
                text,
                size: LABEL_SIZE,
                color: FRAME,
                align: name.align,
            });
        }
    }
}

/// The line's label on paper: at its tip for a stub, else where the layout put it.
fn label_shapes(shapes: &mut Vec<Shape>, line: &LineGeom, area: Rect) {
    // A line through mouseholes writes its short label at the holes.
    if line.text.is_empty() || line.mousehole.is_some() {
        return;
    }
    let (pos, align) = match (line.tip, line.tip_dir) {
        (Some(tip), Some(dir)) => match dir {
            Side::Right => (Pos::new(tip.x + 12.0, tip.y), Align::Left),
            Side::Left => (Pos::new(tip.x - 12.0, tip.y), Align::Right),
            Side::Top => (Pos::new(tip.x, tip.y - 16.0), Align::Center),
            Side::Bottom => (Pos::new(tip.x, tip.y + 18.0), Align::Center),
        },
        _ => label_anchor(line.label_at, line.label_horizontal, line.label_flipped),
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

/// Where a label is written for a line at `at`: above a horizontal segment, centred,
/// or right of a vertical one; `flipped`, below or left of it.
fn label_anchor(at: Pos, horizontal: bool, flipped: bool) -> (Pos, Align) {
    match (horizontal, flipped) {
        (true, false) => (Pos::new(at.x, at.y - 11.0), Align::Center),
        (true, true) => (Pos::new(at.x, at.y + 11.0), Align::Center),
        (false, false) => (Pos::new(at.x + 8.0, at.y), Align::Left),
        (false, true) => (Pos::new(at.x - 8.0, at.y), Align::Right),
    }
}

/// The paper a one-line label of `text` takes when written for a line at `at`, as
/// [`label_shapes`] draws it (before wrapping or shifting it into the picture).
pub fn label_rect(at: Pos, horizontal: bool, flipped: bool, text: &str) -> Rect {
    let (pos, align) = label_anchor(at, horizontal, flipped);
    let w = text::width(text, LABEL_SIZE);
    let left = match align {
        Align::Left => pos.x,
        Align::Center => pos.x - w / 2.0,
        Align::Right => pos.x - w,
    };
    let top = pos.y - LABEL_SIZE / 2.0 - 2.0;
    Rect::from_min_size(Pos::new(left - 3.0, top), w + 6.0, LABEL_SIZE + 4.0)
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
    layout
        .lines
        .iter()
        .enumerate()
        .filter_map(|(li, line)| frame_name(line).map(|name| (li, name.rect)))
        .collect()
}

/// Distance between the lines of a doodle shape's text.
pub const NAME_LINE: f32 = 19.0;

/// Distance between wrapped label lines.
const LINE_HEIGHT: f32 = LABEL_SIZE + 3.0;
/// Between rows of partner names at a frame, so their papers do not touch.
const NAME_ROW: f32 = LABEL_SIZE * 1.5;

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
/// Length of a line look's sample in the legend.
pub const SAMPLE_W: f32 = 48.0;

fn legend_shapes(shapes: &mut Vec<Shape>, at: Pos, entries: &[LegendEntry], looks: &[LookSample]) {
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
    // A sample of every line look in use, pointing right, with its name.
    for (i, sample) in looks.iter().enumerate() {
        let mid = y + LEGEND_LINE / 2.0;
        let mut points = vec![Pos::new(at.x, mid), Pos::new(at.x + SAMPLE_W, mid)];
        end_shapes(
            shapes,
            &mut points,
            (false, true),
            (true, true),
            Some(&sample.look),
            LINE,
            LINE_KEYS / 2 + 2 * i as u64,
        );
        shapes.push(Shape::Polyline {
            points,
            stroke: along(Some(&sample.look), Default::default(), LINE),
        });
        shapes.push(Shape::Text {
            pos: Pos::new(at.x + SAMPLE_W + 12.0, mid),
            text: sample.name.clone(),
            size: LABEL_SIZE,
            color: INK,
            align: Align::Left,
        });
        y += LEGEND_LINE;
    }
}
