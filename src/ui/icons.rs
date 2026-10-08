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
