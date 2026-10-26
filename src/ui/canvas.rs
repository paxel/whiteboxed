//! Painting a scene onto the egui canvas and converting between screen and
//! diagram coordinates.

use std::sync::Arc;

use egui::{
    Align2, Color32, FontId, Painter, Pos2, Stroke as EStroke, StrokeKind, TextureHandle, TextureId,
};

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

fn dashed(painter: &Painter, points: &[Pos2], s: EStroke, (dash, gap): (f32, f32), zoom: f32) {
    painter.extend(egui::Shape::dashed_line(points, s, dash * zoom, gap * zoom));
}

/// Looks rendered for the canvas, kept while it shows them.
#[derive(Default)]
pub struct LookTextures {
    /// By the look's text (held, so its address stays unique) and pixel width.
    entries: Vec<LookTexture>,
}

struct LookTexture {
    svg: Arc<str>,
    width: u32,
    texture: TextureHandle,
    used: bool,
}

/// Widest a look is rendered on the canvas, in pixels.
const MAX_LOOK_PIXELS: u32 = 2048;

impl LookTextures {
    pub fn get(&mut self, ctx: &egui::Context, svg: &Arc<str>, pixels: f32) -> Option<TextureId> {
        // Widths in steps of 16 pixels: sharp at every zoom without rendering anew
        // on every frame of a zoom.
        let width = ((pixels / 16.0).ceil().max(1.0) as u32)
            .saturating_mul(16)
            .min(MAX_LOOK_PIXELS);
        // Tinted end pictures are made anew with every scene: the same text counts.
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.width == width && (Arc::ptr_eq(&e.svg, svg) || e.svg == *svg))
        {
            e.used = true;
            return Some(e.texture.id());
        }
        let pixmap = crate::export::rasterize(svg, width)?;
        let image = egui::ColorImage::from_rgba_premultiplied(
            [pixmap.width() as usize, pixmap.height() as usize],
            pixmap.data(),
        );
        let texture = ctx.load_texture("look", image, egui::TextureOptions::LINEAR);
        let id = texture.id();
        self.entries.push(LookTexture {
            svg: svg.clone(),
            width,
            texture,
            used: true,
        });
        Some(id)
    }

    /// Forgets the textures not painted since the last sweep.
    pub fn sweep(&mut self) {
        self.entries.retain(|e| e.used);
        for e in &mut self.entries {
            e.used = false;
        }
    }
}

pub fn paint(painter: &Painter, view: View, scene: &Scene, looks: &mut LookTextures) {
    let z = view.zoom;
    for shape in &scene.shapes {
        match shape {
            Shape::Look {
                rect, svg, angle, ..
            } => {
                let r = view.rect(*rect);
                let pixels = r.width() * painter.ctx().pixels_per_point();
                if let Some(texture) = looks.get(painter.ctx(), svg, pixels) {
                    let uv = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                    if angle.abs() > 1e-4 {
                        let mut mesh = egui::Mesh::with_texture(texture);
                        mesh.add_rect_with_uv(r, uv, Color32::WHITE);
                        mesh.rotate(egui::emath::Rot2::from_angle(*angle), r.center());
                        painter.add(egui::Shape::mesh(mesh));
                    } else {
                        painter.image(texture, r, uv, Color32::WHITE);
                    }
                }
            }
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
                    if let Some(dash) = st.dash {
                        let pts = [
                            r.left_top(),
                            r.right_top(),
                            r.right_bottom(),
                            r.left_bottom(),
                            r.left_top(),
                        ];
                        dashed(painter, &pts, stroke(*st, z), dash, z);
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
                if let Some(dash) = st.dash {
                    dashed(painter, &pts, stroke(*st, z), dash, z);
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
    looks.sweep();
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
