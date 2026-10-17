//! SVG and PNG export of a scene. The PNG is the SVG rendered with resvg, using the
//! same embedded font as the canvas.

use std::fmt::Write;
use std::sync::{Arc, OnceLock};

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
        if let Some((dash, gap)) = st.dash {
            let _ = write!(s, r#" stroke-dasharray="{dash} {gap}""#);
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
        Shape::Look {
            rect,
            svg,
            size,
            key,
            angle,
            stretch,
        } => {
            let turned = angle.abs() > 1e-4;
            if turned {
                let c = rect.center();
                let _ = write!(
                    out,
                    r#"<g transform="rotate({:.2} {:.1} {:.1})">"#,
                    angle.to_degrees(),
                    c.x,
                    c.y
                );
            }
            out.push_str(&crate::look::embed(
                svg,
                *size,
                *rect,
                &format!("look{key}-"),
                *stretch,
            ));
            if turned {
                out.push_str("</g>");
            }
            out.push('\n');
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

/// How resvg reads our pictures: with the one embedded font, and never loading
/// anything a picture names (no files, no embedded images).
pub(crate) fn options() -> resvg::usvg::Options<'static> {
    static FONTS: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    let fontdb = FONTS
        .get_or_init(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            db.load_font_data(text::font_data().to_vec());
            Arc::new(db)
        })
        .clone();
    resvg::usvg::Options {
        fontdb,
        font_family: text::FAMILY.to_owned(),
        resources_dir: None,
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..resvg::usvg::Options::default()
    }
}

/// A look rendered at `width` pixels wide, keeping its proportions.
pub fn rasterize(svg: &str, width: u32) -> Option<resvg::tiny_skia::Pixmap> {
    let tree = resvg::usvg::Tree::from_str(svg, &options()).ok()?;
    let size = tree.size();
    let scale = width as f32 / size.width();
    let height = (size.height() * scale).round().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width.max(1), height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(pixmap)
}

/// Longest side of an exported PNG, in pixels.
pub const MAX_PNG_SIDE: f32 = 8192.0;

/// Renders the scene to PNG bytes at `scale` (1.0 = one pixel per diagram unit).
pub fn to_png(scene: &Scene, scale: f32) -> Result<Vec<u8>, ExportError> {
    let svg = to_svg(scene);
    let tree = resvg::usvg::Tree::from_str(&svg, &options())
        .map_err(|e| ExportError::Render(e.to_string()))?;
    let size = tree.size();
    // Keep the image within MAX_PNG_SIDE pixels, however large the diagram.
    let longest = size.width().max(size.height()).max(1.0);
    let scale = scale.min(MAX_PNG_SIDE / longest);
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
