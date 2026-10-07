//! SVG and PNG export of a scene. The PNG is the SVG rendered with resvg, using the
//! same embedded font as the canvas.

use std::fmt::Write;
use std::sync::Arc;

use crate::geom::Pos;
use crate::model::Rgb;
use crate::scene::{Align, PAPER, Scene, Shape, Stroke};
use crate::text;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("cannot render the diagram: {0}")]
    Render(String),
    #[error("cannot write the file: {0}")]
    Io(#[from] std::io::Error),
}

const MARGIN: f32 = 12.0;

pub fn to_svg(scene: &Scene) -> String {
    let b = scene.bounds;
    let (w, h) = (b.width() + 2.0 * MARGIN, b.height() + 2.0 * MARGIN);
    let mut out = String::new();
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="{:.1} {:.1} {w:.1} {h:.1}">"#,
        b.min.x - MARGIN,
        b.min.y - MARGIN,
    );
    let _ = writeln!(
        out,
        r#"<rect x="{:.1}" y="{:.1}" width="{w:.1}" height="{h:.1}" fill="{PAPER}"/>"#,
        b.min.x - MARGIN,
        b.min.y - MARGIN,
    );
    for shape in &scene.shapes {
        shape_svg(&mut out, shape);
    }
    out.push_str("</svg>\n");
    out
}

fn paint(fill: Option<Rgb>, stroke: Option<Stroke>) -> String {
    let mut s = match fill {
        Some(c) => format!(r#" fill="{c}""#),
        None => r#" fill="none""#.to_owned(),
    };
    if let Some(st) = stroke {
        let _ = write!(
            s,
            r#" stroke="{}" stroke-width="{}" stroke-linejoin="round""#,
            st.color, st.width
        );
        if st.dashed {
            s.push_str(r#" stroke-dasharray="6 4""#);
        }
    }
    s
}

fn points_attr(points: &[Pos]) -> String {
    points
        .iter()
        .map(|p| format!("{:.1},{:.1}", p.x, p.y))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shape_svg(out: &mut String, shape: &Shape) {
    match shape {
        Shape::Rect {
            rect,
            radius,
            fill,
            stroke,
        } => {
            let _ = writeln!(
                out,
                r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="{radius:.1}"{}/>"#,
                rect.min.x,
                rect.min.y,
                rect.width(),
                rect.height(),
                paint(*fill, *stroke)
            );
        }
        Shape::Polygon {
            points,
            fill,
            stroke,
        } => {
            let _ = writeln!(
                out,
                r#"<polygon points="{}"{}/>"#,
                points_attr(points),
                paint(*fill, *stroke)
            );
        }
        Shape::Polyline { points, stroke } => {
            let _ = writeln!(
                out,
                r#"<polyline points="{}"{}/>"#,
                points_attr(points),
                paint(None, Some(*stroke))
            );
        }
        Shape::Circle {
            center,
            radius,
            fill,
            stroke,
        } => {
            let _ = writeln!(
                out,
                r#"<circle cx="{:.1}" cy="{:.1}" r="{radius:.1}"{}/>"#,
                center.x,
                center.y,
                paint(*fill, *stroke)
            );
        }
        Shape::Text {
            pos,
            text: content,
            size,
            color,
            align,
        } => {
            let anchor = match align {
                Align::Left => "start",
                Align::Center => "middle",
                Align::Right => "end",
            };
            let _ = writeln!(
                out,
                r#"<text x="{:.1}" y="{:.1}" font-family="{}" font-weight="{}" font-size="{size}" text-anchor="{anchor}" fill="{color}">{}</text>"#,
                pos.x,
                baseline(pos.y, *size),
                text::FAMILY,
                text::WEIGHT,
                escape(content)
            );
        }
    }
}

/// Baseline that puts the text's x-height centre on `y`.
pub fn baseline(y: f32, size: f32) -> f32 {
    y + size * 0.35
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Renders the scene to PNG bytes at `scale` (1.0 = one pixel per diagram unit).
pub fn to_png(scene: &Scene, scale: f32) -> Result<Vec<u8>, ExportError> {
    let svg = to_svg(scene);
    let mut opt = resvg::usvg::Options::default();
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_font_data(text::font_data().to_vec());
    opt.fontdb = Arc::new(db);
    let tree =
        resvg::usvg::Tree::from_str(&svg, &opt).map_err(|e| ExportError::Render(e.to_string()))?;
    let size = tree.size();
    let (w, h) = (
        (size.width() * scale).ceil() as u32,
        (size.height() * scale).ceil() as u32,
    );
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1))
        .ok_or_else(|| ExportError::Render("image too large".into()))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|e| ExportError::Render(e.to_string()))
}
