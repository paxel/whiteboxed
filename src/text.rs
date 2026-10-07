//! Text measurement with the one font every output uses: egui's default proportional
//! font, embedded here so the layout, the canvas and the SVG/PNG export agree.

use std::sync::OnceLock;

/// Font family name to put into SVG files.
pub const FAMILY: &str = "Ubuntu";
pub const WEIGHT: u16 = 300;

pub fn font_data() -> &'static [u8] {
    epaint_default_fonts::UBUNTU_LIGHT
}

struct Metrics {
    units_per_em: f32,
    advances: Vec<(char, f32)>,
    fallback: f32,
}

fn metrics() -> Option<&'static Metrics> {
    static METRICS: OnceLock<Option<Metrics>> = OnceLock::new();
    METRICS
        .get_or_init(|| {
            let face = ttf_parser::Face::parse(font_data(), 0).ok()?;
            let units_per_em = f32::from(face.units_per_em());
            let advance = |c: char| {
                face.glyph_index(c)
                    .and_then(|g| face.glyph_hor_advance(g))
                    .map(f32::from)
            };
            let fallback = advance('n').unwrap_or(units_per_em * 0.5);
            let advances = (' '..='~')
                .chain('\u{a0}'..='\u{17f}')
                .filter_map(|c| advance(c).map(|a| (c, a)))
                .collect();
            Some(Metrics {
                units_per_em,
                advances,
                fallback,
            })
        })
        .as_ref()
}

/// Width of `text` in pixels at font size `size`.
pub fn width(text: &str, size: f32) -> f32 {
    let Some(m) = metrics() else {
        return text.chars().count() as f32 * size * 0.5;
    };
    let units: f32 = text
        .chars()
        .map(|c| {
            m.advances
                .binary_search_by(|(k, _)| k.cmp(&c))
                .map_or(m.fallback, |i| m.advances[i].1)
        })
        .sum();
    units * size / m.units_per_em
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wider_text_measures_wider() {
        let narrow = width("iii", 14.0);
        let wide = width("WWW", 14.0);
        assert!(narrow > 0.0);
        assert!(wide > narrow * 2.0);
        assert!((width("ab", 28.0) - 2.0 * width("ab", 14.0)).abs() < 0.01);
        assert_eq!(width("", 14.0), 0.0);
    }
}
