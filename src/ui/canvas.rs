//! Painting a scene onto the egui canvas and converting between screen and
//! diagram coordinates.

use egui::{Align2, Color32, FontId, Painter, Pos2, Stroke as EStroke, StrokeKind};

use crate::geom::{Pos, Rect};
use crate::model::{Rgb, Side};
use crate::scene::{Align, Scene, Shape, Stroke};

pub const ACCENT: Color32 = Color32::from_rgb(0x1e, 0x88, 0xe5);

/// Screen = origin + diagram * zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub origin: Pos2,
    pub zoom: f32,
}

impl View {
    pub fn screen(&self, p: Pos) -> Pos2 {
        Pos2::new(
            self.origin.x + p.x * self.zoom,
            self.origin.y + p.y * self.zoom,
        )
    }

    pub fn diagram(&self, s: Pos2) -> Pos {
        Pos::new(
            (s.x - self.origin.x) / self.zoom,
            (s.y - self.origin.y) / self.zoom,
        )
    }

    pub fn rect(&self, r: Rect) -> egui::Rect {
        egui::Rect::from_min_max(self.screen(r.min), self.screen(r.max))
    }
}

pub fn color(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

fn stroke(s: Stroke, zoom: f32) -> EStroke {
    EStroke::new(s.width * zoom.max(0.5), color(s.color))
}

fn dashed(painter: &Painter, points: &[Pos2], s: EStroke, zoom: f32) {
    painter.extend(egui::Shape::dashed_line(points, s, 6.0 * zoom, 4.0 * zoom));
}

pub fn paint(painter: &Painter, view: View, scene: &Scene) {
    let z = view.zoom;
    for shape in &scene.shapes {
        match shape {
            Shape::Rect {
                rect,
                radius,
                fill,
                stroke: st,
            } => {
                let r = view.rect(*rect);
                let radius = radius * z;
                if let Some(f) = fill {
                    painter.rect_filled(r, radius, color(*f));
                }
                if let Some(st) = st {
                    if st.dashed {
                        let pts = [
                            r.left_top(),
                            r.right_top(),
                            r.right_bottom(),
                            r.left_bottom(),
                            r.left_top(),
                        ];
                        dashed(painter, &pts, stroke(*st, z), z);
                    } else {
                        painter.rect_stroke(r, radius, stroke(*st, z), StrokeKind::Middle);
                    }
                }
            }
            Shape::Polygon {
                points,
                fill,
                stroke: st,
            } => {
                let pts = points.iter().map(|p| view.screen(*p)).collect();
                painter.add(egui::Shape::convex_polygon(
                    pts,
                    fill.map_or(Color32::TRANSPARENT, color),
                    st.map_or(EStroke::NONE, |s| stroke(s, z)),
                ));
            }
            Shape::Polyline { points, stroke: st } => {
                let pts: Vec<Pos2> = points.iter().map(|p| view.screen(*p)).collect();
                if st.dashed {
                    dashed(painter, &pts, stroke(*st, z), z);
                } else {
                    painter.line(pts, stroke(*st, z));
                }
            }
            Shape::Circle {
                center,
                radius,
                fill,
                stroke: st,
            } => {
                let c = view.screen(*center);
                if let Some(f) = fill {
                    painter.circle_filled(c, radius * z, color(*f));
                }
                if let Some(st) = st {
                    painter.circle_stroke(c, radius * z, stroke(*st, z));
                }
            }
            Shape::Text {
                pos,
                text,
                size,
                color: c,
                align,
            } => {
                let anchor = match align {
                    Align::Left => Align2::LEFT_CENTER,
                    Align::Center => Align2::CENTER_CENTER,
                    Align::Right => Align2::RIGHT_CENTER,
                };
                painter.text(
                    view.screen(*pos),
                    anchor,
                    text,
                    FontId::proportional((size * z).max(1.0)),
                    color(*c),
                );
            }
        }
    }
}

/// The edge of a box rect on one side, in screen coordinates.
pub fn side_edge(r: egui::Rect, side: Side) -> [Pos2; 2] {
    match side {
        Side::Top => [r.left_top(), r.right_top()],
        Side::Right => [r.right_top(), r.right_bottom()],
        Side::Bottom => [r.left_bottom(), r.right_bottom()],
        Side::Left => [r.left_top(), r.left_bottom()],
    }
}
