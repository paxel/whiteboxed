//! Custom looks: SVG pictures a box can wear instead of (or above) its own shape,
//! pictures for the ends of lines and for mouseholes, and line looks (width, dash and
//! end pictures a relation wears).
//!
//! Looks come from the AI, from imported files and from project files pulled from
//! anywhere, and they end up in SVG, PNG and HTML exports. So every look is checked
//! against a strict subset of SVG (shapes, paths, groups, gradients, text) and rebuilt
//! from the parsed tree; anything outside the subset is refused with its name, never
//! silently dropped.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::geom::Rect;
use crate::model::LinePattern;

/// Largest SVG accepted, in bytes.
pub const MAX_SVG_BYTES: usize = 512 * 1024;
/// Most elements accepted in one SVG.
pub const MAX_ELEMENTS: usize = 5000;
/// Longest look name, in characters.
pub const MAX_NAME: usize = 60;
/// Thinnest and thickest stroke a line look may have.
pub const MIN_WIDTH: f32 = 0.5;
pub const MAX_WIDTH: f32 = 8.0;

const SVG_NS: &str = "http://www.w3.org/2000/svg";

const ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "defs",
    "title",
    "desc",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "text",
    "tspan",
    "linearGradient",
    "radialGradient",
    "stop",
];

const ATTRIBUTES: &[&str] = &[
    // The picture.
    "viewBox",
    "width",
    "height",
    "preserveAspectRatio",
    "version",
    "id",
    "transform",
    // Geometry.
    "x",
    "y",
    "dx",
    "dy",
    "rx",
    "ry",
    "cx",
    "cy",
    "r",
    "x1",
    "y1",
    "x2",
    "y2",
    "points",
    "d",
    // Paint.
    "color",
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-opacity",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "opacity",
    "visibility",
    // Text.
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "text-anchor",
    "dominant-baseline",
    "letter-spacing",
    // Gradients.
    "offset",
    "stop-color",
    "stop-opacity",
    "gradientUnits",
    "gradientTransform",
    "spreadMethod",
    "fx",
    "fy",
    "fr",
];

/// Elements whose text content is kept.
const TEXT_ELEMENTS: &[&str] = &["text", "tspan", "title", "desc"];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LookError {
    #[error("not a readable SVG: {0}")]
    Parse(String),
    #[error("the SVG is larger than {} KiB", MAX_SVG_BYTES / 1024)]
    TooLarge,
    #[error("the SVG has more than {MAX_ELEMENTS} elements")]
    TooManyElements,
    #[error("the outermost element must be <svg>")]
    NotSvg,
    #[error("<{0}> is not allowed in a look (allowed: shapes, paths, groups, gradients, text)")]
    Element(String),
    #[error("the attribute {0} is not allowed in a look")]
    Attribute(String),
    #[error("{0}=\"{1}\" is not allowed: only references inside the look, like url(#name)")]
    Reference(String, String),
    #[error("the SVG needs a viewBox, or a width and height")]
    NoSize,
    #[error("a look needs a name")]
    NoName,
    #[error("a look name has at most {MAX_NAME} characters")]
    NameTooLong,
    #[error("a line is between {MIN_WIDTH} and {MAX_WIDTH} wide")]
    Width,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LookMode {
    /// The picture is the shape; the name is written inside it.
    #[default]
    Inside,
    /// The picture is the shape; the name is written below it.
    Below,
    /// The box keeps its shape and shows the picture above its name.
    Icon,
}

impl LookMode {
    pub const ALL: [LookMode; 3] = [LookMode::Inside, LookMode::Below, LookMode::Icon];

    pub fn label(self) -> &'static str {
        match self {
            LookMode::Inside => "shape, name inside",
            LookMode::Below => "shape, name below",
            LookMode::Icon => "icon above the name",
        }
    }
}

/// Identifies a look across renames, in the library and in every project.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LookId(pub String);

impl LookId {
    pub fn fresh() -> Self {
        LookId(uuid::Uuid::new_v4().to_string())
    }
}

/// A checked look. Its SVG is rebuilt from the parsed tree, never the text given.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    name: String,
    mode: LookMode,
    svg: Arc<str>,
    /// Grows with every change, so the newer of two copies is known.
    revision: u64,
    /// The picture's size from its viewBox (or width and height).
    size: (f32, f32),
}

// The size is always a finite number, so equality is reflexive.
impl Eq for Look {}

impl Look {
    pub fn new(name: &str, mode: LookMode, svg: &str, revision: u64) -> Result<Self, LookError> {
        let name = check_name(name)?;
        let (svg, size) = sanitize(svg)?;
        Ok(Look {
            name,
            mode,
            svg: svg.into(),
            revision,
            size,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn mode(&self) -> LookMode {
        self.mode
    }

    pub fn svg(&self) -> &Arc<str> {
        &self.svg
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Width and height of the picture, for scaling it without stretching.
    pub fn size(&self) -> (f32, f32) {
        self.size
    }

    pub fn rename(&mut self, name: &str) -> Result<(), LookError> {
        self.name = check_name(name)?;
        self.revision += 1;
        Ok(())
    }
}

/// A picture: an SVG without a mode, drawn at the ends of lines and as a portal (the
/// picture of a mousehole). Not worn by a box. On a line it points right with its tip
/// at the right edge of its viewBox; its `currentColor` parts take the line's colour.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    name: String,
    svg: Arc<str>,
    revision: u64,
    size: (f32, f32),
}

impl Eq for Picture {}

impl Picture {
    pub fn new(name: &str, svg: &str, revision: u64) -> Result<Self, LookError> {
        let name = check_name(name)?;
        let (svg, size) = sanitize(svg)?;
        Ok(Picture {
            name,
            svg: svg.into(),
            revision,
            size,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn svg(&self) -> &Arc<str> {
        &self.svg
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn size(&self) -> (f32, f32) {
        self.size
    }

    pub fn rename(&mut self, name: &str) -> Result<(), LookError> {
        self.name = check_name(name)?;
        self.revision += 1;
        Ok(())
    }
}

/// How a line is drawn: its stroke width, its dash, and the pictures at its ends. The
/// line's direction decides which ends get the head; the others get the tail. Without
/// a head picture the ends with an arrow get the plain arrowhead.
#[derive(Debug, Clone, PartialEq)]
pub struct LineLook {
    name: String,
    width: f32,
    dash: LinePattern,
    head: Option<LookId>,
    tail: Option<LookId>,
    revision: u64,
}

impl Eq for LineLook {}

/// The settable part of a line look.
#[derive(Debug, Clone, PartialEq)]
pub struct LineLookSpec {
    pub width: f32,
    pub dash: LinePattern,
    pub head: Option<LookId>,
    pub tail: Option<LookId>,
}

impl Default for LineLookSpec {
    fn default() -> Self {
        LineLookSpec {
            width: 1.5,
            dash: LinePattern::Solid,
            head: None,
            tail: None,
        }
    }
}

impl LineLook {
    pub fn new(name: &str, spec: LineLookSpec, revision: u64) -> Result<Self, LookError> {
        let name = check_name(name)?;
        check_width(spec.width)?;
        Ok(LineLook {
            name,
            width: spec.width,
            dash: spec.dash,
            head: spec.head,
            tail: spec.tail,
            revision,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn dash(&self) -> LinePattern {
        self.dash
    }

    pub fn head(&self) -> Option<&LookId> {
        self.head.as_ref()
    }

    pub fn tail(&self) -> Option<&LookId> {
        self.tail.as_ref()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn spec(&self) -> LineLookSpec {
        LineLookSpec {
            width: self.width,
            dash: self.dash,
            head: self.head.clone(),
            tail: self.tail.clone(),
        }
    }

    /// The pictures it needs, head first.
    pub fn pictures(&self) -> impl Iterator<Item = &LookId> {
        self.head.iter().chain(self.tail.iter())
    }

    pub fn rename(&mut self, name: &str) -> Result<(), LookError> {
        self.name = check_name(name)?;
        self.revision += 1;
        Ok(())
    }

    pub fn set(&mut self, spec: LineLookSpec) -> Result<(), LookError> {
        check_width(spec.width)?;
        self.width = spec.width;
        self.dash = spec.dash;
        self.head = spec.head;
        self.tail = spec.tail;
        self.revision += 1;
        Ok(())
    }
}

fn check_width(width: f32) -> Result<(), LookError> {
    if !(MIN_WIDTH..=MAX_WIDTH).contains(&width) {
        return Err(LookError::Width);
    }
    Ok(())
}

fn check_name(name: &str) -> Result<String, LookError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(LookError::NoName);
    }
    if name.chars().count() > MAX_NAME {
        return Err(LookError::NameTooLong);
    }
    Ok(name.to_owned())
}

/// Checks `input` against the safe subset and rebuilds it; returns the SVG and its
/// size.
pub fn sanitize(input: &str) -> Result<(String, (f32, f32)), LookError> {
    if input.len() > MAX_SVG_BYTES {
        return Err(LookError::TooLarge);
    }
    // The default options refuse a DOCTYPE, so no entities can be declared.
    let doc = roxmltree::Document::parse(input).map_err(|e| LookError::Parse(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err(LookError::NotSvg);
    }
    if doc.descendants().filter(|n| n.is_element()).count() > MAX_ELEMENTS {
        return Err(LookError::TooManyElements);
    }
    let vb = view_box(root).ok_or(LookError::NoSize)?;
    let mut out = String::new();
    let how = Writing {
        prefix: "",
        placed: None,
    };
    element(&mut out, root, &how)?;
    // Cropped to what it draws: an AI-drawn picture often sits small in a large
    // viewBox, and lines should reach the drawing, not the margin around it.
    match drawn_extent(root, vb) {
        Some(tight) => {
            let placed = format!(
                r#"xmlns="{SVG_NS}" viewBox="{} {} {} {}""#,
                tight.0, tight.1, tight.2, tight.3
            );
            let mut cropped = String::new();
            element(
                &mut cropped,
                root,
                &Writing {
                    prefix: "",
                    placed: Some(&placed),
                },
            )?;
            Ok((cropped, (tight.2, tight.3)))
        }
        None => Ok((out, (vb.2, vb.3))),
    }
}

/// The smallest viewBox around what the picture draws (strokes included), when that
/// is smaller than its own viewBox `vb`; in viewBox units, rounded to hundredths.
fn drawn_extent(root: roxmltree::Node, vb: (f32, f32, f32, f32)) -> Option<(f32, f32, f32, f32)> {
    // Read with only the viewBox, so the drawing's coordinates are viewBox units
    // shifted to the origin.
    let placed = format!(
        r#"xmlns="{SVG_NS}" viewBox="{} {} {} {}""#,
        vb.0, vb.1, vb.2, vb.3
    );
    let mut text = String::new();
    element(
        &mut text,
        root,
        &Writing {
            prefix: "",
            placed: Some(&placed),
        },
    )
    .ok()?;
    let tree = resvg::usvg::Tree::from_str(&text, &crate::export::options()).ok()?;
    let b = tree.root().abs_stroke_bounding_box();
    let hundredths = |v: f32| (v * 100.0).round() / 100.0;
    let tight = (
        hundredths(b.x() + vb.0),
        hundredths(b.y() + vb.1),
        hundredths(b.width()),
        hundredths(b.height()),
    );
    let drawn = tight.2 > 0.5 && tight.3 > 0.5 && tight.2.is_finite() && tight.3.is_finite();
    let smaller = tight.2 < vb.2 - 0.5 || tight.3 < vb.3 - 0.5;
    (drawn && smaller).then_some(tight)
}

/// How [`element`] writes what it checked.
struct Writing<'a> {
    /// Put before every id and every reference to one.
    prefix: &'a str,
    /// Replaces the attributes of the outermost `<svg>` that place and size it.
    placed: Option<&'a str>,
}

/// Attributes of the outermost `<svg>` that [`Writing::placed`] replaces.
const PLACEMENT: &[&str] = &[
    "x",
    "y",
    "width",
    "height",
    "viewBox",
    "preserveAspectRatio",
    "version",
];

fn element(out: &mut String, node: roxmltree::Node, how: &Writing) -> Result<(), LookError> {
    let tag = node.tag_name();
    let name = tag.name();
    if !matches!(tag.namespace(), None | Some(SVG_NS)) || !ELEMENTS.contains(&name) {
        return Err(LookError::Element(qualified(tag.namespace(), name)));
    }
    let outermost = name == "svg" && node.parent_element().is_none();
    let _ = write!(out, "<{name}");
    if outermost {
        match how.placed {
            Some(placed) => {
                let _ = write!(out, " {placed}");
            }
            None => {
                let _ = write!(out, r#" xmlns="{SVG_NS}""#);
            }
        }
    }
    for attr in node.attributes() {
        let key = attr.name();
        if attr.namespace().is_some() || !ATTRIBUTES.contains(&key) {
            return Err(LookError::Attribute(qualified(attr.namespace(), key)));
        }
        let value = attr.value();
        check_value(key, value)?;
        if outermost && how.placed.is_some() && PLACEMENT.contains(&key) {
            continue;
        }
        let value = if key == "id" {
            format!("{}{value}", how.prefix)
        } else {
            value.replace("url(#", &format!("url(#{}", how.prefix))
        };
        let _ = write!(out, r#" {key}="{}""#, escape(&value));
    }
    out.push('>');
    let keeps_text = TEXT_ELEMENTS.contains(&name);
    for child in node.children() {
        if child.is_element() {
            element(out, child, how)?;
        } else if child.is_text() && keeps_text {
            out.push_str(&escape(child.text().unwrap_or_default()));
        }
        // Comments, processing instructions and whitespace between elements go.
    }
    let _ = write!(out, "</{name}>");
    Ok(())
}

/// The look as an `<svg>` element placed on `rect` of a bigger picture, scaled without
/// stretching. Its ids get `prefix`, so several looks in one document never share one.
pub fn embed(svg: &str, size: (f32, f32), rect: Rect, prefix: &str) -> String {
    let Ok(doc) = roxmltree::Document::parse(svg) else {
        return String::new();
    };
    let root = doc.root_element();
    let view_box = root
        .attribute("viewBox")
        .map_or_else(|| format!("0 0 {} {}", size.0, size.1), str::to_owned);
    let placed = format!(
        r#"x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" viewBox="{}" preserveAspectRatio="xMidYMid meet""#,
        rect.min.x,
        rect.min.y,
        rect.width(),
        rect.height(),
        escape(&view_box)
    );
    let how = Writing {
        prefix,
        placed: Some(&placed),
    };
    let mut out = String::new();
    match element(&mut out, root, &how) {
        Ok(()) => out,
        Err(_) => String::new(),
    }
}

/// The picture with `color` as its `currentColor`: a stencil in the line's colour.
/// Parts drawn in a colour of their own keep it.
pub fn tinted(svg: &str, color: crate::model::Rgb) -> String {
    let Ok(doc) = roxmltree::Document::parse(svg) else {
        return svg.to_owned();
    };
    let root = doc.root_element();
    let Some(vb) = view_box(root) else {
        return svg.to_owned();
    };
    let placed = format!(
        r#"xmlns="{SVG_NS}" viewBox="{} {} {} {}" color="{color}""#,
        vb.0, vb.1, vb.2, vb.3
    );
    let mut out = String::new();
    match element(
        &mut out,
        root,
        &Writing {
            prefix: "",
            placed: Some(&placed),
        },
    ) {
        Ok(()) => out,
        Err(_) => svg.to_owned(),
    }
}

/// The largest rect of the picture's proportions that fits into `area`, centred.
pub fn fit(size: (f32, f32), area: Rect) -> Rect {
    let scale = (area.width() / size.0).min(area.height() / size.1).max(0.0);
    Rect::from_center(area.center(), size.0 * scale, size.1 * scale)
}

fn qualified(namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some("http://www.w3.org/1999/xlink") => format!("xlink:{name}"),
        Some("http://www.w3.org/XML/1998/namespace") => format!("xml:{name}"),
        Some(SVG_NS) | None => name.to_owned(),
        Some(ns) => format!("{{{ns}}}{name}"),
    }
}

/// Only references to something inside the look itself: `url(#name)`.
fn check_value(key: &str, value: &str) -> Result<(), LookError> {
    let refused = || LookError::Reference(key.to_owned(), value.to_owned());
    let lower = value.to_ascii_lowercase();
    if lower.contains("javascript:") || lower.contains("data:") {
        return Err(refused());
    }
    let mut rest = lower.as_str();
    while let Some(at) = rest.find("url(") {
        let inner = &rest[at + 4..];
        let end = inner.find(')').ok_or_else(refused)?;
        let target = inner[..end].trim();
        let local = target.strip_prefix('#').is_some_and(|id| {
            !id.is_empty()
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        });
        if !local {
            return Err(refused());
        }
        rest = &inner[end..];
    }
    Ok(())
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The picture's viewBox: x, y, width and height; from its width and height when it
/// has none.
fn view_box(root: roxmltree::Node) -> Option<(f32, f32, f32, f32)> {
    let positive =
        |(_, _, w, h): (f32, f32, f32, f32)| w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite();
    if let Some(vb) = root.attribute("viewBox") {
        let n: Vec<f32> = vb
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        if let [x, y, w, h] = n[..] {
            return Some((x, y, w, h)).filter(|s| positive(*s));
        }
        return None;
    }
    let length = |a: &str| -> Option<f32> {
        root.attribute(a)?
            .trim()
            .trim_end_matches("px")
            .parse()
            .ok()
    };
    Some((0.0, 0.0, length("width")?, length("height")?)).filter(|s| positive(*s))
}

// ----- the per-user library -----

const FILE: &str = "looks.yaml";

/// A look as written into the library file and into project files.
#[derive(Serialize, Deserialize)]
pub(crate) struct LookDto {
    id: LookId,
    name: String,
    mode: LookMode,
    revision: u64,
    svg: String,
}

/// A picture as written into the library file and into project files.
#[derive(Serialize, Deserialize)]
pub(crate) struct PictureDto {
    id: LookId,
    name: String,
    revision: u64,
    svg: String,
}

/// A line look as written into the library file and into project files.
#[derive(Serialize, Deserialize)]
pub(crate) struct LineLookDto {
    id: LookId,
    name: String,
    revision: u64,
    width: f32,
    #[serde(default)]
    dash: LinePattern,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    head: Option<LookId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tail: Option<LookId>,
}

/// Looks keyed by id, as stored in the library or in a project file.
pub type Looks = BTreeMap<LookId, Look>;
/// Pictures keyed by id.
pub type Pictures = BTreeMap<LookId, Picture>;
/// Line looks keyed by id.
pub type LineLooks = BTreeMap<LookId, LineLook>;

pub(crate) fn pictures_to_dto(pictures: &Pictures) -> Vec<PictureDto> {
    pictures
        .iter()
        .map(|(id, p)| PictureDto {
            id: id.clone(),
            name: p.name.clone(),
            revision: p.revision,
            svg: p.svg.to_string(),
        })
        .collect()
}

pub(crate) fn pictures_from_dto(list: Vec<PictureDto>) -> Result<Pictures, String> {
    list.into_iter()
        .map(|d| {
            let picture = Picture::new(&d.name, &d.svg, d.revision)
                .map_err(|e| format!("picture {}: {e}", d.name))?;
            Ok((d.id, picture))
        })
        .collect()
}

pub(crate) fn line_looks_to_dto(looks: &LineLooks) -> Vec<LineLookDto> {
    looks
        .iter()
        .map(|(id, l)| LineLookDto {
            id: id.clone(),
            name: l.name.clone(),
            revision: l.revision,
            width: l.width,
            dash: l.dash,
            head: l.head.clone(),
            tail: l.tail.clone(),
        })
        .collect()
}

pub(crate) fn line_looks_from_dto(list: Vec<LineLookDto>) -> Result<LineLooks, String> {
    list.into_iter()
        .map(|d| {
            let spec = LineLookSpec {
                width: d.width,
                dash: d.dash,
                head: d.head,
                tail: d.tail,
            };
            let look = LineLook::new(&d.name, spec, d.revision)
                .map_err(|e| format!("line look {}: {e}", d.name))?;
            Ok((d.id, look))
        })
        .collect()
}

/// The library file: looks, pictures and line looks.
#[derive(Serialize, Deserialize)]
struct LibraryDto {
    #[serde(default)]
    looks: Vec<LookDto>,
    #[serde(default)]
    pictures: Vec<PictureDto>,
    #[serde(default)]
    line_looks: Vec<LineLookDto>,
}

/// The library file as written before pictures and line looks: looks only.
#[derive(Deserialize)]
#[serde(untagged)]
enum LibraryFile {
    Current(LibraryDto),
    Legacy(Vec<LookDto>),
}

pub(crate) fn to_dto(looks: &Looks) -> Vec<LookDto> {
    looks
        .iter()
        .map(|(id, l)| LookDto {
            id: id.clone(),
            name: l.name.clone(),
            mode: l.mode,
            revision: l.revision,
            svg: l.svg.to_string(),
        })
        .collect()
}

/// The library: every look, picture and line look the user has, usable in every
/// project.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Library {
    pub looks: Looks,
    pub pictures: Pictures,
    pub line_looks: LineLooks,
}

impl Library {
    /// The stored library. A broken file gives an empty library; an entry that does
    /// not pass the check is left out.
    pub fn load(dir: &Path) -> Self {
        let dto = match fs::read_to_string(dir.join(FILE))
            .ok()
            .and_then(|t| serde_yaml_ng::from_str::<LibraryFile>(&t).ok())
        {
            Some(LibraryFile::Current(dto)) => dto,
            Some(LibraryFile::Legacy(looks)) => LibraryDto {
                looks,
                pictures: Vec::new(),
                line_looks: Vec::new(),
            },
            None => LibraryDto {
                looks: Vec::new(),
                pictures: Vec::new(),
                line_looks: Vec::new(),
            },
        };
        let looks = dto
            .looks
            .into_iter()
            .filter_map(|d| Some((d.id, Look::new(&d.name, d.mode, &d.svg, d.revision).ok()?)))
            .collect();
        let pictures = dto
            .pictures
            .into_iter()
            .filter_map(|d| Some((d.id, Picture::new(&d.name, &d.svg, d.revision).ok()?)))
            .collect();
        let line_looks = dto
            .line_looks
            .into_iter()
            .filter_map(|d| {
                let spec = LineLookSpec {
                    width: d.width,
                    dash: d.dash,
                    head: d.head,
                    tail: d.tail,
                };
                Some((d.id, LineLook::new(&d.name, spec, d.revision).ok()?))
            })
            .collect();
        Library {
            looks,
            pictures,
            line_looks,
        }
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let dto = LibraryDto {
            looks: to_dto(&self.looks),
            pictures: pictures_to_dto(&self.pictures),
            line_looks: line_looks_to_dto(&self.line_looks),
        };
        let text = serde_yaml_ng::to_string(&dto).map_err(io::Error::other)?;
        fs::write(dir.join(FILE), text)
    }

    /// Adds a new look and returns its id.
    pub fn add(&mut self, look: Look) -> LookId {
        let id = LookId::fresh();
        self.looks.insert(id.clone(), look);
        id
    }

    /// Adds a new picture and returns its id.
    pub fn add_picture(&mut self, picture: Picture) -> LookId {
        let id = LookId::fresh();
        self.pictures.insert(id.clone(), picture);
        id
    }

    /// Adds a new line look and returns its id.
    pub fn add_line_look(&mut self, look: LineLook) -> LookId {
        let id = LookId::fresh();
        self.line_looks.insert(id.clone(), look);
        id
    }

    /// The look with this name, ignoring case.
    pub fn find(&self, name: &str) -> Option<&LookId> {
        self.looks
            .iter()
            .find(|(_, l)| l.name.eq_ignore_ascii_case(name.trim()))
            .map(|(id, _)| id)
    }

    /// The picture with this name, ignoring case.
    pub fn find_picture(&self, name: &str) -> Option<&LookId> {
        self.pictures
            .iter()
            .find(|(_, p)| p.name.eq_ignore_ascii_case(name.trim()))
            .map(|(id, _)| id)
    }

    /// The line look with this name, ignoring case.
    pub fn find_line_look(&self, name: &str) -> Option<&LookId> {
        self.line_looks
            .iter()
            .find(|(_, l)| l.name.eq_ignore_ascii_case(name.trim()))
            .map(|(id, _)| id)
    }
}

/// A line look with the pictures it needs, as copied into a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineLookCopy {
    pub id: LookId,
    pub look: LineLook,
    pub pictures: Vec<(LookId, Picture)>,
}

/// Parses looks as written by [`to_dto`]; every one is checked again.
pub(crate) fn from_dto(list: Vec<LookDto>) -> Result<Looks, String> {
    list.into_iter()
        .map(|d| {
            let look = Look::new(&d.name, d.mode, &d.svg, d.revision)
                .map_err(|e| format!("look {}: {e}", d.name))?;
            Ok((d.id, look))
        })
        .collect()
}

/// A project's copies of looks take the library's version when it is newer.
/// Returns whether anything changed.
pub fn sync(project: &mut Looks, library: &Library) -> bool {
    take_newer(project, &library.looks, Look::revision)
}

/// The same for a project's pictures.
pub fn sync_pictures(project: &mut Pictures, library: &Library) -> bool {
    take_newer(project, &library.pictures, Picture::revision)
}

/// The same for a project's line looks.
pub fn sync_line_looks(project: &mut LineLooks, library: &Library) -> bool {
    take_newer(project, &library.line_looks, LineLook::revision)
}

fn take_newer<T: Clone>(
    project: &mut BTreeMap<LookId, T>,
    library: &BTreeMap<LookId, T>,
    revision: fn(&T) -> u64,
) -> bool {
    let mut changed = false;
    for (id, copy) in project.iter_mut() {
        if let Some(newer) = library.get(id).filter(|l| revision(l) > revision(copy)) {
            *copy = newer.clone();
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    const KAFKA: &str = r##"<?xml version="1.0"?>
<!-- a comment -->
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20">
  <defs>
    <linearGradient id="g"><stop offset="0" stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient>
  </defs>
  <g transform="translate(1 1)"><rect width="38" height="18" rx="3" fill="url(#g)"/></g>
  <text x="20" y="12" text-anchor="middle">K &amp; Q</text>
</svg>"##;

    #[test]
    fn the_safe_subset_is_rebuilt() -> Result<(), LookError> {
        let (svg, size) = sanitize(KAFKA)?;
        // Cropped to the drawing, which sits 1 px inside the viewBox.
        assert_eq!(size, (38.0, 18.0));
        assert!(svg.starts_with(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="1 1 38 18">"#));
        assert!(svg.contains(r##"fill="url(#g)""##));
        assert!(svg.contains(">K &amp; Q</text>"));
        assert!(!svg.contains("comment") && !svg.contains("<?xml"));
        // Rebuilding the rebuilt picture changes nothing.
        assert_eq!(sanitize(&svg)?.0, svg);
        Ok(())
    }

    #[test]
    fn an_embedded_look_is_placed_and_its_ids_prefixed() -> Result<(), LookError> {
        let (svg, size) = sanitize(KAFKA)?;
        let rect = Rect::from_min_size(crate::geom::Pos::new(10.0, 20.0), 80.0, 40.0);
        let out = embed(&svg, size, rect, "b7-");
        assert!(out.starts_with(
            r#"<svg x="10.0" y="20.0" width="80.0" height="40.0" viewBox="1 1 38 18" preserveAspectRatio="xMidYMid meet">"#
        ));
        assert!(out.contains(r#"id="b7-g""#) && out.contains("url(#b7-g)"));
        assert!(!out.contains("xmlns") && !out.contains(r#"id="g""#));
        let wide = fit(
            size,
            Rect::from_min_size(crate::geom::Pos::new(0.0, 0.0), 100.0, 100.0),
        );
        let (w, h, y) = (wide.width(), wide.height(), wide.min.y);
        assert!(
            (w - 100.0).abs() < 0.01 && (h - 47.37).abs() < 0.01 && (y - 26.32).abs() < 0.01,
            "{w} {h} {y}"
        );
        Ok(())
    }

    fn refused(body: &str) -> LookError {
        let svg =
            format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">{body}</svg>"#);
        match sanitize(&svg) {
            Err(e) => e,
            Ok(_) => LookError::NoName,
        }
    }

    #[test]
    fn everything_outside_the_subset_is_refused_by_name() {
        for el in [
            "script",
            "style",
            "image",
            "use",
            "a",
            "foreignObject",
            "filter",
            "animate",
            "set",
            "pattern",
        ] {
            assert_eq!(
                refused(&format!("<{el}/>")),
                LookError::Element(el.into()),
                "{el}"
            );
        }
        assert_eq!(
            refused(r#"<rect onclick="alert(1)"/>"#),
            LookError::Attribute("onclick".into())
        );
        assert_eq!(
            refused(r#"<rect style="fill:red"/>"#),
            LookError::Attribute("style".into())
        );
        assert_eq!(
            refused(r#"<rect href="x"/>"#),
            LookError::Attribute("href".into())
        );
        assert_eq!(
            refused(r#"<rect xmlns:xlink="http://www.w3.org/1999/xlink" xlink:href="x"/>"#),
            LookError::Attribute("xlink:href".into())
        );
        assert_eq!(
            refused(r#"<rect fill="url(http://evil/x.svg#a)"/>"#),
            LookError::Reference("fill".into(), "url(http://evil/x.svg#a)".into())
        );
        assert_eq!(
            refused(r#"<rect fill="url(#a) url(file:///etc/passwd)"/>"#),
            LookError::Reference("fill".into(), "url(#a) url(file:///etc/passwd)".into())
        );
        assert_eq!(
            refused(r#"<rect fill="javascript:alert(1)"/>"#),
            LookError::Reference("fill".into(), "javascript:alert(1)".into())
        );
        assert_eq!(
            refused(r#"<svg:script xmlns:svg="http://www.w3.org/2000/svg"/>"#),
            LookError::Element("script".into())
        );
        assert_eq!(
            refused(r#"<h:p xmlns:h="http://www.w3.org/1999/xhtml"/>"#),
            LookError::Element("{http://www.w3.org/1999/xhtml}p".into())
        );
    }

    #[test]
    fn entities_size_and_root_are_checked() {
        let dtd = r#"<!DOCTYPE svg [<!ENTITY x "boom">]><svg viewBox="0 0 1 1">&x;</svg>"#;
        assert!(matches!(sanitize(dtd), Err(LookError::Parse(_))));
        assert_eq!(sanitize("<html/>"), Err(LookError::NotSvg));
        assert_eq!(sanitize("<svg><rect/></svg>"), Err(LookError::NoSize));
        assert_eq!(
            sanitize(r#"<svg viewBox="0 0 0 5"/>"#),
            Err(LookError::NoSize)
        );
        assert!(
            sanitize(r#"<svg width="30px" height="10"/>"#).is_ok_and(|(_, s)| s == (30.0, 10.0))
        );
        let big = format!(
            r#"<svg viewBox="0 0 1 1">{}</svg>"#,
            " ".repeat(MAX_SVG_BYTES)
        );
        assert_eq!(sanitize(&big), Err(LookError::TooLarge));
        let many = format!(
            r#"<svg viewBox="0 0 1 1">{}</svg>"#,
            "<g/>".repeat(MAX_ELEMENTS)
        );
        assert_eq!(sanitize(&many), Err(LookError::TooManyElements));
    }

    #[test]
    fn names_are_checked() {
        let svg = r#"<svg viewBox="0 0 1 1"/>"#;
        assert_eq!(
            Look::new("  ", LookMode::Icon, svg, 0),
            Err(LookError::NoName)
        );
        assert_eq!(
            Look::new(&"x".repeat(MAX_NAME + 1), LookMode::Icon, svg, 0),
            Err(LookError::NameTooLong)
        );
    }

    #[test]
    fn the_library_round_trips_and_projects_take_newer_versions()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        assert_eq!(Library::load(dir.path()), Library::default());
        let mut lib = Library::default();
        let id = lib.add(Look::new("Kafka", LookMode::Below, KAFKA, 0)?);
        lib.save(dir.path())?;
        let loaded = Library::load(dir.path());
        assert_eq!(loaded, lib);
        assert_eq!(loaded.find("kafka "), Some(&id));

        let mut project: Looks = lib.looks.clone();
        assert!(!sync(&mut project, &lib));
        if let Some(l) = lib.looks.get_mut(&id) {
            l.rename("Kafka topic")?;
        }
        assert!(sync(&mut project, &lib));
        assert_eq!(project.get(&id).map(Look::name), Some("Kafka topic"));
        // A library without the look leaves the project's copy alone.
        assert!(!sync(&mut project, &Library::default()));
        assert_eq!(project.len(), 1);

        // A tampered library entry is left out rather than loaded.
        let text = fs::read_to_string(dir.path().join(FILE))?.replace("<text", "<script");
        fs::write(dir.path().join(FILE), text)?;
        assert!(Library::load(dir.path()).looks.is_empty());
        Ok(())
    }

    #[test]
    fn pictures_and_line_looks_live_in_the_library_too() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut lib = Library::default();
        lib.add(Look::new("Kafka", LookMode::Below, KAFKA, 0)?);
        let plug = lib.add_picture(Picture::new(
            "Plug",
            r#"<svg viewBox="0 0 10 6"><path d="M0 0h10v6H0z" fill="currentColor"/></svg>"#,
            0,
        )?);
        let spec = LineLookSpec {
            width: 3.0,
            dash: LinePattern::Dashed,
            head: Some(plug.clone()),
            tail: None,
        };
        let async_ = lib.add_line_look(LineLook::new("async", spec.clone(), 0)?);
        lib.save(dir.path())?;
        let loaded = Library::load(dir.path());
        assert_eq!(loaded, lib);
        assert_eq!(loaded.find_picture("plug"), Some(&plug));
        assert_eq!(loaded.find_line_look("ASYNC"), Some(&async_));
        assert_eq!(
            loaded.line_looks.get(&async_).map(LineLook::spec),
            Some(spec)
        );

        // The width is checked.
        let thin = LineLookSpec {
            width: 0.1,
            ..LineLookSpec::default()
        };
        assert_eq!(LineLook::new("x", thin, 0), Err(LookError::Width));

        // A project's copies take newer versions.
        let mut pictures = lib.pictures.clone();
        let mut line_looks = lib.line_looks.clone();
        assert!(!sync_pictures(&mut pictures, &lib));
        assert!(!sync_line_looks(&mut line_looks, &lib));
        if let Some(p) = lib.pictures.get_mut(&plug) {
            p.rename("Socket")?;
        }
        if let Some(l) = lib.line_looks.get_mut(&async_) {
            l.set(LineLookSpec {
                width: 4.0,
                ..l.spec()
            })?;
        }
        assert!(sync_pictures(&mut pictures, &lib));
        assert!(sync_line_looks(&mut line_looks, &lib));
        assert_eq!(pictures.get(&plug).map(Picture::name), Some("Socket"));
        assert_eq!(line_looks.get(&async_).map(LineLook::width), Some(4.0));

        // A library file from before pictures and line looks still loads.
        let old = Library {
            looks: lib.looks.clone(),
            ..Library::default()
        };
        let legacy = serde_yaml_ng::to_string(&to_dto(&old.looks))?;
        fs::write(dir.path().join(FILE), legacy)?;
        assert_eq!(Library::load(dir.path()), old);
        Ok(())
    }
}
