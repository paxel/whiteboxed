//! The AI tools for custom looks, pictures and line looks. They work in arc42 projects
//! and doodles alike: the library belongs to the user, not to the open project.

use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ApiResult, NoArgs, Output, ToolInfo, args, rejected, schema};
use crate::editor::Editor;
use crate::look::{LineLookCopy, LineLookSpec, Look, LookId, LookMode, Picture};
use crate::model::LinePattern;

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(rename_all = "snake_case")]
pub enum ApiLookMode {
    /// The picture is the box; the name is written inside it.
    Inside,
    /// The picture is the box; the name is written below it.
    Below,
    /// The box keeps its shape and shows the picture above the name.
    Icon,
}

impl From<ApiLookMode> for LookMode {
    fn from(m: ApiLookMode) -> LookMode {
        match m {
            ApiLookMode::Inside => LookMode::Inside,
            ApiLookMode::Below => LookMode::Below,
            ApiLookMode::Icon => LookMode::Icon,
        }
    }
}

fn mode_name(m: LookMode) -> &'static str {
    match m {
        LookMode::Inside => "inside",
        LookMode::Below => "below",
        LookMode::Icon => "icon",
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct AddLookArgs {
    /// Unique in the library; a look with this name is replaced.
    pub name: String,
    /// The picture: an <svg> with a viewBox, using only shapes, paths, groups,
    /// gradients and text (no style, scripts, images, links or filters).
    pub svg: String,
    pub mode: ApiLookMode,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct AddPictureArgs {
    /// Unique among the pictures; one with this name is replaced.
    pub name: String,
    /// The picture, in the same safe subset of SVG as a look. On a line it points
    /// right with its tip at the right edge of its viewBox; fill="currentColor" (or
    /// stroke) takes the line's colour, other colours stay.
    pub svg: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct AddLineLookArgs {
    /// Unique among the line looks; one with this name is replaced.
    pub name: String,
    /// Stroke width, 0.5 to 8; default 1.5.
    #[serde(default)]
    pub width: Option<f32>,
    /// solid (default), dashed or dotted.
    #[serde(default)]
    pub dash: Option<super::doodle::ApiPattern>,
    /// The picture at the ends an arrow points to, by name; omitted: the plain arrowhead.
    #[serde(default)]
    pub head: Option<String>,
    /// The picture at the other ends, by name; omitted: nothing.
    #[serde(default)]
    pub tail: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SetPortalsArgs {
    /// The picture (by name) drawn where a line goes into a mousehole; "" goes back
    /// to the arch; omitted keeps the current one.
    #[serde(default, rename = "in")]
    pub into: Option<String>,
    /// The picture drawn where the line comes out; "" and omitted as for `in`.
    #[serde(default)]
    pub out: Option<String>,
}

/// Most looks the library holds.
pub const MAX_LOOKS: usize = 200;

pub fn tools() -> Vec<ToolInfo> {
    vec![
        ToolInfo {
            name: "list_looks",
            description: "The custom looks in the user's library and in the open project: name, mode and picture size.",
            schema: schema::<NoArgs>(),
        },
        ToolInfo {
            name: "add_picture",
            description: "Add a picture to the user's library for the ends of lines and for mouseholes (portals): the same safe subset of SVG as add_look. Drawn pointing right with its tip at the right edge, scaled to the arrowhead height; currentColor takes the line's colour. Use it in add_line_look or set_portals.",
            schema: schema::<AddPictureArgs>(),
        },
        ToolInfo {
            name: "add_line_look",
            description: "Add a line look to the user's library: a kind of line with stroke width, dash and pictures at its ends (head where the arrow points, tail elsewhere), named like \"async\" or \"data flow\". A line wears it on every level (edit_relation, connect_new, connect_existing or the doodle line tools, field look); the legend shows a sample with its name, and many kinds on one diagram count against readability.",
            schema: schema::<AddLineLookArgs>(),
        },
        ToolInfo {
            name: "set_portals",
            description: "Set the pictures the project draws as mouseholes: one where a line goes in (near its tail), one where it comes out (near its head). An undo step for the user.",
            schema: schema::<SetPortalsArgs>(),
        },
        ToolInfo {
            name: "add_look",
            description: "Add a custom look to the user's library, usable in every project: an SVG picture for special nodes (Kafka, a company logo, a device). Mode inside or below: the picture is the box, scaled without stretching, with the name inside or below it; icon: the box keeps its shape and shows the picture above the name. Only a safe subset of SVG is accepted: svg, g, defs, path, rect, circle, ellipse, line, polyline, polygon, text, tspan, linearGradient, radialGradient, stop, with presentation attributes and url(#id) references; anything else is refused by name. Put the look on a box with edit_box (or a doodle shape with doodle_edit_shape).",
            schema: schema::<AddLookArgs>(),
        },
    ]
}

pub fn call(editor: &mut Editor, tool: &str, arguments: Value) -> Option<ApiResult<Output>> {
    match tool {
        "list_looks" => Some(args::<NoArgs>(arguments).map(|_| {
            let looks: Vec<Value> = editor
                .looks_offered()
                .into_iter()
                .filter_map(|(id, _)| editor.look_copy(&id))
                .map(|(_, l)| {
                    let (w, h) = l.size();
                    json!({"name": l.name(), "mode": mode_name(l.mode()), "width": w, "height": h})
                })
                .collect();
            let pictures: Vec<Value> = editor
                .pictures_offered()
                .into_iter()
                .filter_map(|(id, _)| editor.picture_copy(&id))
                .map(|(_, p)| {
                    let (w, h) = p.size();
                    json!({"name": p.name(), "width": w, "height": h})
                })
                .collect();
            let picture_name = |id: Option<&LookId>| {
                id.and_then(|id| editor.picture_copy(id))
                    .map(|(_, p)| p.name().to_owned())
            };
            let line_looks: Vec<Value> = editor
                .line_looks_offered()
                .into_iter()
                .filter_map(|(id, _)| editor.line_look_copy(&id))
                .map(|c| {
                    json!({
                        "name": c.look.name(),
                        "width": c.look.width(),
                        "dash": c.look.dash().label(),
                        "head": picture_name(c.look.head()),
                        "tail": picture_name(c.look.tail()),
                    })
                })
                .collect();
            Output::Json(json!({
                "looks": looks,
                "pictures": pictures,
                "line_looks": line_looks,
                "portals": {
                    "in": picture_name(editor.project.portal_in.as_ref()),
                    "out": picture_name(editor.project.portal_out.as_ref()),
                },
            }))
        })),
        "add_picture" => Some(args::<AddPictureArgs>(arguments).and_then(|a| {
            let replaces = editor.library.find_picture(&a.name).is_some();
            if !replaces && editor.library.pictures.len() >= MAX_LOOKS {
                return rejected(format!("the library holds at most {MAX_LOOKS} pictures"));
            }
            let id = editor.add_picture(&a.name, &a.svg).or_else(rejected)?;
            let picture = editor.picture_copy(&id).map(|(_, p)| p.name().to_owned());
            Ok(Output::Json(
                json!({"picture": picture, "replaced": replaces, "ok": true}),
            ))
        })),
        "add_line_look" => Some(args::<AddLineLookArgs>(arguments).and_then(|a| {
            let replaces = editor.library.find_line_look(&a.name).is_some();
            if !replaces && editor.library.line_looks.len() >= MAX_LOOKS {
                return rejected(format!("the library holds at most {MAX_LOOKS} line looks"));
            }
            let picture = |name: &Option<String>| -> ApiResult<Option<LookId>> {
                match name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                    Some(n) => picture_by_name(editor, n).map(|p| p.map(|(id, _)| id)),
                    None => Ok(None),
                }
            };
            let spec = LineLookSpec {
                width: a.width.unwrap_or(1.5),
                dash: a.dash.map(LinePattern::from).unwrap_or_default(),
                head: picture(&a.head)?,
                tail: picture(&a.tail)?,
            };
            let id = editor.add_line_look(&a.name, spec).or_else(rejected)?;
            let look = editor.line_look_copy(&id).map(|c| c.look.name().to_owned());
            Ok(Output::Json(
                json!({"line_look": look, "replaced": replaces, "ok": true}),
            ))
        })),
        "set_portals" => Some(args::<SetPortalsArgs>(arguments).and_then(|a| {
            let pick =
                |name: &Option<String>, current: &Option<LookId>| -> ApiResult<Option<LookId>> {
                    match name.as_deref() {
                        None => Ok(current.clone()),
                        Some(n) => picture_by_name(editor, n).map(|p| p.map(|(id, _)| id)),
                    }
                };
            let into = pick(&a.into, &editor.project.portal_in)?;
            let out = pick(&a.out, &editor.project.portal_out)?;
            let into = into.and_then(|p| editor.picture_copy(&p));
            let out = out.and_then(|p| editor.picture_copy(&p));
            editor.apply_ai(|p| p.set_portals(into, out))?;
            let picture_name = |id: Option<&LookId>| {
                id.and_then(|id| editor.picture_copy(id))
                    .map(|(_, p)| p.name().to_owned())
            };
            Ok(Output::Json(json!({
                "in": picture_name(editor.project.portal_in.as_ref()),
                "out": picture_name(editor.project.portal_out.as_ref()),
                "ok": true,
            })))
        })),
        "add_look" => Some(args::<AddLookArgs>(arguments).and_then(|a| {
            let replaces = editor.library.find(&a.name).is_some();
            if !replaces && editor.library.looks.len() >= MAX_LOOKS {
                return rejected(format!("the library holds at most {MAX_LOOKS} looks"));
            }
            let id = editor
                .add_look(&a.name, a.mode.into(), &a.svg)
                .or_else(rejected)?;
            let look = editor.look_copy(&id).map(|(_, l)| l.name().to_owned());
            Ok(Output::Json(
                json!({"look": look, "replaced": replaces, "ok": true}),
            ))
        })),
        _ => None,
    }
}

/// A picture by name: `""` is none.
pub fn picture_by_name(editor: &Editor, name: &str) -> ApiResult<Option<(LookId, Picture)>> {
    if name.trim().is_empty() {
        return Ok(None);
    }
    editor
        .pictures_offered()
        .into_iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(name.trim()))
        .and_then(|(id, _)| editor.picture_copy(&id))
        .map(Some)
        .map_or_else(
            || {
                rejected(format!(
                    "no picture named \"{name}\"; list_looks shows them"
                ))
            },
            Ok,
        )
}

/// The line look a line should wear by name: `""` takes it off.
pub fn line_look_by_name(editor: &Editor, name: &str) -> ApiResult<Option<LineLookCopy>> {
    if name.trim().is_empty() {
        return Ok(None);
    }
    editor
        .line_looks_offered()
        .into_iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(name.trim()))
        .and_then(|(id, _)| editor.line_look_copy(&id))
        .map(Some)
        .map_or_else(
            || {
                rejected(format!(
                    "no line look named \"{name}\"; list_looks shows them"
                ))
            },
            Ok,
        )
}

/// The look a box should wear by name: `""` takes it off.
pub fn by_name(editor: &Editor, name: &str) -> ApiResult<Option<(LookId, Look)>> {
    if name.trim().is_empty() {
        return Ok(None);
    }
    editor
        .looks_offered()
        .into_iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(name.trim()))
        .and_then(|(id, _)| editor.look_copy(&id))
        .map(Some)
        .map_or_else(
            || rejected(format!("no look named \"{name}\"; list_looks shows them")),
            Ok,
        )
}
