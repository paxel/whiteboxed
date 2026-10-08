//! Painted icon buttons. egui's default font has no arrow glyphs, so the undo and
//! redo arrows are drawn: an arc over the top with an arrowhead at one end.

use egui::{Pos2, Response, Sense, Shape, Stroke, Ui, WidgetInfo, WidgetType, vec2};

use super::canvas::ACCENT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum History {
    Undo,
    Redo,
}

pub fn history_button(ui: &mut Ui, which: History, enabled: bool) -> Response {
    let (label, tip) = match which {
        History::Undo => ("Undo", "Undo (Ctrl+Z)"),
        History::Redo => ("Redo", "Redo (Ctrl+Shift+Z)"),
    };
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 20.0), sense);
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, label));
    let resp = resp.on_hover_text(tip);
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let visuals = ui.visuals();
    let color = if !enabled {
        visuals.weak_text_color().gamma_multiply(0.6)
    } else if resp.hovered() {
        ACCENT
    } else {
        visuals.text_color()
    };
    if enabled && resp.hovered() {
        ui.painter()
            .rect_filled(rect, 4.0, visuals.widgets.hovered.weak_bg_fill);
    }
    let c = rect.center() + vec2(0.0, 2.5);
    let r = 6.0;
    // Screen angles: 0 = right, -90 = up. The arc runs over the top.
    let (from, to) = match which {
        History::Undo => (20.0_f32, -200.0_f32),
        History::Redo => (160.0_f32, 380.0_f32),
    };
    let steps = 16;
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let a = (from + (to - from) * i as f32 / steps as f32).to_radians();
            Pos2::new(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    let stroke = Stroke::new(1.8, color);
    ui.painter().add(Shape::line(points.clone(), stroke));
    if let [.., before, tip] = points.as_slice() {
        let dir = (*tip - *before).normalized();
        let side = vec2(-dir.y, dir.x);
        let base = *tip - dir * 1.0;
        ui.painter().add(Shape::convex_polygon(
            vec![*tip + dir * 3.5, base + side * 3.5, base - side * 3.5],
            color,
            Stroke::NONE,
        ));
    }
    resp
}

/// The panicked cat of the "unsaved changes" dialog, drawn into `rect` (square).
/// Coordinates follow a 120 × 120 design grid.
pub fn panic_cat(painter: &egui::Painter, rect: egui::Rect) {
    use egui::{Color32, Pos2};
    let s = rect.width().min(rect.height()) / 120.0;
    let o = rect.min;
    let p = |x: f32, y: f32| Pos2::new(o.x + x * s, o.y + y * s);
    let ink = Color32::from_rgb(0x1d, 0x27, 0x33);
    let fur = Color32::from_rgb(0xf2, 0xc2, 0x7a);
    let pink = Color32::from_rgb(0xf4, 0xa3, 0xa3);
    let paw = Color32::from_rgb(0xf7, 0xd6, 0xa2);
    let outline = Stroke::new(3.0 * s, ink);
    let thin = Stroke::new(2.0 * s, ink);
    let ellipse_points = |cx: f32, cy: f32, rx: f32, ry: f32, deg: f32| -> Vec<Pos2> {
        let (sin, cos) = deg.to_radians().sin_cos();
        (0..32)
            .map(|i| {
                let a = i as f32 / 32.0 * std::f32::consts::TAU;
                let (x, y) = (rx * a.cos(), ry * a.sin());
                p(cx + x * cos - y * sin, cy + x * sin + y * cos)
            })
            .collect()
    };
    // Ears behind the head.
    for (a, b, c, inner) in [
        (
            (26.0, 12.0),
            (20.0, 52.0),
            (50.0, 34.0),
            [(30.0, 22.0), (27.0, 40.0), (40.0, 33.0)],
        ),
        (
            (94.0, 12.0),
            (100.0, 52.0),
            (70.0, 34.0),
            [(90.0, 22.0), (93.0, 40.0), (80.0, 33.0)],
        ),
    ] {
        painter.add(Shape::convex_polygon(
            vec![p(a.0, a.1), p(b.0, b.1), p(c.0, c.1)],
            fur,
            outline,
        ));
        painter.add(Shape::convex_polygon(
            inner.iter().map(|(x, y)| p(*x, *y)).collect(),
            pink,
            Stroke::NONE,
        ));
    }
    painter.add(Shape::convex_polygon(
        ellipse_points(60.0, 64.0, 39.0, 34.0, 0.0),
        fur,
        outline,
    ));
    // Raised eyebrows.
    for (a, c, b) in [
        ((33.0, 40.0), (41.0, 32.0), (49.0, 38.0)),
        ((71.0, 38.0), (79.0, 32.0), (87.0, 40.0)),
    ] {
        painter.add(Shape::QuadraticBezier(
            egui::epaint::QuadraticBezierShape::from_points_stroke(
                [p(a.0, a.1), p(c.0, c.1), p(b.0, b.1)],
                false,
                Color32::TRANSPARENT,
                Stroke::new(3.0 * s, ink),
            ),
        ));
    }
    // Wide eyes, tiny pupils.
    for x in [43.0, 77.0] {
        painter.circle(p(x, 54.0), 11.0 * s, Color32::WHITE, outline);
        painter.circle_filled(p(x, 55.0), 3.0 * s, ink);
    }
    // Nose and open mouth.
    painter.add(Shape::convex_polygon(
        vec![p(56.0, 66.0), p(64.0, 66.0), p(60.0, 71.0)],
        Color32::from_rgb(0xe0, 0x70, 0x7a),
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        ellipse_points(60.0, 81.0, 7.0, 9.0, 0.0),
        Color32::from_rgb(0x5a, 0x1f, 0x2a),
        Stroke::new(2.5 * s, ink),
    ));
    // Whiskers.
    for (a, b) in [
        ((16.0, 64.0), (32.0, 67.0)),
        ((14.0, 73.0), (31.0, 72.0)),
        ((104.0, 64.0), (88.0, 67.0)),
        ((106.0, 73.0), (89.0, 72.0)),
    ] {
        painter.line_segment([p(a.0, a.1), p(b.0, b.1)], thin);
    }
    // Paws pressed against the cheeks.
    for (cx, deg, pads) in [
        (18.0, -18.0, [14.0, 19.5, 25.0]),
        (102.0, 18.0, [95.0, 100.5, 106.0]),
    ] {
        painter.add(Shape::convex_polygon(
            ellipse_points(cx, 88.0, 13.0, 16.0, deg),
            paw,
            outline,
        ));
        for (i, x) in pads.iter().enumerate() {
            let y = if i == 1 { 80.0 } else { 82.0 };
            painter.circle_filled(p(*x, y), 2.6 * s, pink);
        }
    }
    // A drop of sweat.
    painter.add(Shape::convex_polygon(
        vec![
            p(100.0, 28.0),
            p(104.0, 37.0),
            p(100.0, 42.0),
            p(96.0, 37.0),
        ],
        Color32::from_rgb(0x8c, 0xc9, 0xf2),
        Stroke::new(1.5 * s, ink),
    ));
}
