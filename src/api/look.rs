//! The AI tools for custom looks. They work in arc42 projects and doodles alike: the
//! library belongs to the user, not to the open project.

use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ApiResult, NoArgs, Output, ToolInfo, args, rejected, schema};
use crate::editor::Editor;
use crate::look::{Look, LookId, LookMode};

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
            Output::Json(json!({ "looks": looks }))
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
