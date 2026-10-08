//! The tool layer behind the AI interface: every tool an AI client can call, its
//! input schema, and how it maps onto the editor. Each call is one undo step, the
//! same as a click; errors come back as readable text for the model to correct.

use std::path::Path;

use base64::Engine;
use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::doc::DocFormat;
use crate::editor::{AiAction, Editor};
use crate::export;
use crate::layout;
use crate::model::{
    BlockId, BlockKind, BlockSpec, Cell, DiagramId, Direction, End, Endpoint, ModelError,
    Placement, Project, RelationId, Side,
};
use crate::scene;
use crate::view::{self, ViewEnd};

/// Longest side of a rendered diagram, in pixels.
pub const RENDER_LIMIT: f32 = 1500.0;

/// What a tool returns.
#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    Json(Value),
    Png(Vec<u8>),
}

impl Output {
    pub fn png_base64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// No such tool.
    UnknownTool(String),
    /// The arguments did not match the tool's schema.
    BadArguments(String),
    /// The model refused the change, or a reference did not resolve.
    Rejected(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::UnknownTool(t) => write!(f, "unknown tool {t}"),
            ApiError::BadArguments(m) => write!(f, "invalid arguments: {m}"),
            ApiError::Rejected(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<ModelError> for ApiError {
    fn from(e: ModelError) -> Self {
        ApiError::Rejected(e.to_string())
    }
}

type ApiResult<T> = Result<T, ApiError>;

fn rejected<T>(msg: impl Into<String>) -> ApiResult<T> {
    Err(ApiError::Rejected(msg.into()))
}

// ----- parameter types -----

/// A box: its id, its path of names from the context view (`["Web Shop", "Orders"]`),
/// or that path as one string separated by `/` (`"Web Shop/Orders"`).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(untagged)]
pub enum BoxRef {
    Id(u64),
    Path(Vec<String>),
    Text(String),
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(rename_all = "snake_case")]
pub enum ApiSide {
    Top,
    Right,
    Bottom,
    Left,
}

impl From<ApiSide> for Side {
    fn from(s: ApiSide) -> Side {
        match s {
            ApiSide::Top => Side::Top,
            ApiSide::Right => Side::Right,
            ApiSide::Bottom => Side::Bottom,
            ApiSide::Left => Side::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(rename_all = "snake_case")]
pub enum ApiKind {
    Component,
    Database,
    Queue,
    Cache,
    FileStorage,
    Ui,
    Person,
    ExternalSystem,
}

impl From<ApiKind> for BlockKind {
    fn from(k: ApiKind) -> BlockKind {
        match k {
            ApiKind::Component => BlockKind::Component,
            ApiKind::Database => BlockKind::Database,
            ApiKind::Queue => BlockKind::Queue,
            ApiKind::Cache => BlockKind::Cache,
            ApiKind::FileStorage => BlockKind::FileStorage,
            ApiKind::Ui => BlockKind::Ui,
            ApiKind::Person => BlockKind::Person,
            ApiKind::ExternalSystem => BlockKind::ExternalSystem,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(rename_all = "snake_case")]
pub enum ApiDirection {
    Out,
    In,
    Bi,
}

impl From<ApiDirection> for Direction {
    fn from(d: ApiDirection) -> Direction {
        match d {
            ApiDirection::Out => Direction::Out,
            ApiDirection::In => Direction::In,
            ApiDirection::Bi => Direction::Bi,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(rename_all = "snake_case")]
pub enum ApiPlacement {
    /// Move the existing box to the requested side (default).
    Move,
    /// Leave it where it is and route the line around.
    Keep,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(rename_all = "snake_case")]
pub enum ApiFormat {
    Asciidoc,
    Markdown,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct NoArgs {}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DiagramArgs {
    /// The box whose whitebox to use; omit (or null) for the context view.
    #[serde(default)]
    pub diagram: Option<BoxRef>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct AddBoxArgs {
    /// The box whose whitebox gets the new box; omit for the context view.
    #[serde(default)]
    pub diagram: Option<BoxRef>,
    /// Unique within its diagram.
    pub name: String,
    /// person and external_system exist only in the context view.
    pub kind: ApiKind,
    /// A project-wide tag; boxes with the same tag share a colour.
    #[serde(default)]
    pub tag: Option<String>,
    /// What this building block is responsible for.
    #[serde(default)]
    pub responsibility: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ConnectNewArgs {
    /// The existing box to start from.
    pub from: BoxRef,
    /// The side of `from` where the new box goes.
    pub side: ApiSide,
    pub name: String,
    pub kind: ApiKind,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub responsibility: Option<String>,
    /// Seen from `from`: out (arrow to the new box, default), in, or bi.
    #[serde(default)]
    pub direction: Option<ApiDirection>,
    /// The interface, e.g. "REST", "orders via browser".
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ConnectExistingArgs {
    pub from: BoxRef,
    /// The side of `from` where `to` should be.
    pub side: ApiSide,
    /// Another box of the same diagram.
    pub to: BoxRef,
    #[serde(default)]
    pub direction: Option<ApiDirection>,
    #[serde(default)]
    pub text: Option<String>,
    /// What to do when moving `to` breaks the side of its other relations.
    #[serde(default)]
    pub placement: Option<ApiPlacement>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct AddStubArgs {
    pub from: BoxRef,
    pub side: ApiSide,
    #[serde(default)]
    pub direction: Option<ApiDirection>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ConnectOpenEndArgs {
    /// The relation whose open end to connect (from get_diagram or get_model).
    pub relation: u64,
    /// The box to connect it to. The relation then belongs to that box's diagram.
    pub to: BoxRef,
    /// The side of `to` the line arrives at; by default the one facing the partner.
    #[serde(default)]
    pub side: Option<ApiSide>,
    #[serde(default)]
    pub placement: Option<ApiPlacement>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct AttachArgs {
    /// A relation that enters the whitebox of `whitebox` from the level above.
    pub relation: u64,
    /// The box whose whitebox the relation enters.
    pub whitebox: BoxRef,
    /// The box inside that whitebox which handles the relation.
    pub to: BoxRef,
    /// The side of `to`; by default the side the relation enters the frame on.
    #[serde(default)]
    pub side: Option<ApiSide>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EditBoxArgs {
    #[serde(rename = "box")]
    pub target: BoxRef,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub kind: Option<ApiKind>,
    /// A tag name; an empty string removes the tag.
    #[serde(default)]
    pub tag: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EditRelationArgs {
    pub relation: u64,
    #[serde(default)]
    pub direction: Option<ApiDirection>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SetResponsibilityArgs {
    #[serde(rename = "box")]
    pub target: BoxRef,
    pub text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SetMotivationArgs {
    /// The box whose whitebox it describes; omit for the context view.
    #[serde(default)]
    pub diagram: Option<BoxRef>,
    pub text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct MoveBoxArgs {
    #[serde(rename = "box")]
    pub target: BoxRef,
    /// Grid column; a box already in the cell swaps places.
    pub col: i32,
    pub row: i32,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DeleteBoxArgs {
    #[serde(rename = "box")]
    pub target: BoxRef,
    /// Must be true to delete a box whose whitebox has content.
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DeleteRelationArgs {
    pub relation: u64,
    /// Omit to delete the relation. Give the whitebox to only detach it from the box
    /// inside that whitebox; the relation stays on its own level.
    #[serde(default)]
    pub detach_in: Option<BoxRef>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ExportArgs {
    /// Absolute path of the folder; created if missing.
    pub folder: String,
    pub format: ApiFormat,
}

// ----- tool catalogue -----

pub struct ToolInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Map<String, Value>,
}

fn schema<T: JsonSchema>() -> Map<String, Value> {
    match serde_json::to_value(schemars::schema_for!(T)) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

/// Guidance for the AI client on how to use the tools.
pub const INSTRUCTIONS: &str = "\
whiteboxed holds an arc42 building-block model that the user watches live while you edit it. \
Work top-down: \
1. Context view first (no diagram argument): add the system as a component, then every \
person and external_system it talks to with connect_new, using direction and a short \
interface text. \
2. Then each whitebox level by level: add_box with diagram set to the box, connect the \
building blocks, and attach every relation that enters from the level above (get_diagram \
lists them as dangling) to the box that handles it. \
3. Give every box a one or two sentence responsibility and every diagram a motivation \
(why it is split this way). \
4. Check the result with render_diagram and tidy it with move_box when lines cross. \
5. Finish with export_docs into the repository's documentation folder; the user is \
asked to allow that folder, so tell them and call it again after they agreed. \
Boxes are addressed by id or by their path of names from the context view. The side you \
connect on decides where the partner sits; layout is automatic. Every call is one undo \
step for the user; saving the project is up to the user.";

pub fn tools() -> Vec<ToolInfo> {
    vec![
        ToolInfo {
            name: "get_model",
            description: "The whole project: every box with id, path, type, tag and texts, and every relation with both ends.",
            schema: schema::<NoArgs>(),
        },
        ToolInfo {
            name: "get_diagram",
            description: "One diagram as drawn: its boxes with grid cells, its lines, open ends (stubs) and relations from the level above that no box handles yet (dangling).",
            schema: schema::<DiagramArgs>(),
        },
        ToolInfo {
            name: "render_diagram",
            description: "A PNG image of one diagram, exactly as the user sees it.",
            schema: schema::<DiagramArgs>(),
        },
        ToolInfo {
            name: "add_box",
            description: "Add a box without a relation, e.g. the first box of an empty diagram.",
            schema: schema::<AddBoxArgs>(),
        },
        ToolInfo {
            name: "connect_new",
            description: "Create a new box on one side of an existing box and connect them.",
            schema: schema::<ConnectNewArgs>(),
        },
        ToolInfo {
            name: "connect_existing",
            description: "Connect two existing boxes of the same diagram; `to` moves to `side` of `from` unless placement is keep.",
            schema: schema::<ConnectExistingArgs>(),
        },
        ToolInfo {
            name: "add_stub",
            description: "A relation whose other end is not known yet. It leaves the level: every enclosing box shows it as an open end up to the context view.",
            schema: schema::<AddStubArgs>(),
        },
        ToolInfo {
            name: "connect_open_end",
            description: "Connect the open end of a stub to a box, on the level of that box.",
            schema: schema::<ConnectOpenEndArgs>(),
        },
        ToolInfo {
            name: "attach",
            description: "Attach a relation that enters a whitebox from the level above to the box inside that handles it.",
            schema: schema::<AttachArgs>(),
        },
        ToolInfo {
            name: "edit_box",
            description: "Change name, type or tag of a box; omitted fields stay.",
            schema: schema::<EditBoxArgs>(),
        },
        ToolInfo {
            name: "edit_relation",
            description: "Change direction or text of a relation; omitted fields stay.",
            schema: schema::<EditRelationArgs>(),
        },
        ToolInfo {
            name: "set_responsibility",
            description: "Set what a box is responsible for (arc42 blackbox description).",
            schema: schema::<SetResponsibilityArgs>(),
        },
        ToolInfo {
            name: "set_motivation",
            description: "Set the motivation of a diagram: why the whitebox is decomposed this way, or for the context view who uses the system.",
            schema: schema::<SetMotivationArgs>(),
        },
        ToolInfo {
            name: "move_box",
            description: "Move a box to another grid cell of its diagram.",
            schema: schema::<MoveBoxArgs>(),
        },
        ToolInfo {
            name: "delete_box",
            description: "Delete a box and its relations. A box with whitebox content needs recursive: true.",
            schema: schema::<DeleteBoxArgs>(),
        },
        ToolInfo {
            name: "delete_relation",
            description: "Delete a relation, or with detach_in only detach it inside that whitebox.",
            schema: schema::<DeleteRelationArgs>(),
        },
        ToolInfo {
            name: "export_docs",
            description: "Write images, one arc42 text file per diagram and an index into a folder. The user must allow the folder: the first call for a new folder asks them and fails; call again after they agreed.",
            schema: schema::<ExportArgs>(),
        },
    ]
}

// ----- dispatch -----

fn args<T: for<'de> Deserialize<'de>>(value: Value) -> ApiResult<T> {
    let value = if value.is_null() {
        Value::Object(Map::new())
    } else {
        value
    };
    serde_json::from_value(value).map_err(|e| ApiError::BadArguments(e.to_string()))
}

/// Runs one tool against the editor.
pub fn call(editor: &mut Editor, tool: &str, arguments: Value) -> ApiResult<Output> {
    match tool {
        "get_model" => {
            let _: NoArgs = args(arguments)?;
            Ok(Output::Json(model_json(&editor.project)))
        }
        "get_diagram" => {
            let a: DiagramArgs = args(arguments)?;
            let d = diagram_of(&editor.project, a.diagram.as_ref())?;
            Ok(Output::Json(diagram_json(&editor.project, d)))
        }
        "render_diagram" => {
            let a: DiagramArgs = args(arguments)?;
            let d = diagram_of(&editor.project, a.diagram.as_ref())?;
            let l = layout::layout(&editor.project, &view::diagram_view(&editor.project, d));
            let longest = l.bounds.width().max(l.bounds.height()).max(1.0);
            let scale = (RENDER_LIMIT / longest).min(1.0);
            let png = export::to_png(&scene::scene(&l), scale)
                .map_err(|e| ApiError::Rejected(e.to_string()))?;
            Ok(Output::Png(png))
        }
        "add_box" => add_box(editor, args(arguments)?),
        "connect_new" => connect_new(editor, args(arguments)?),
        "connect_existing" => connect_existing(editor, args(arguments)?),
        "add_stub" => add_stub(editor, args(arguments)?),
        "connect_open_end" => connect_open_end(editor, args(arguments)?),
        "attach" => attach(editor, args(arguments)?),
        "edit_box" => edit_box(editor, args(arguments)?),
        "edit_relation" => edit_relation(editor, args(arguments)?),
        "set_responsibility" => {
            let a: SetResponsibilityArgs = args(arguments)?;
            let id = resolve(&editor.project, &a.target)?;
            editor.apply_ai(|p| p.set_responsibility(id, &a.text))?;
            done(editor, "described", Some(id))
        }
        "set_motivation" => {
            let a: SetMotivationArgs = args(arguments)?;
            let d = diagram_of(&editor.project, a.diagram.as_ref())?;
            editor.apply_ai(|p| p.set_motivation(d, &a.text))?;
            note(
                editor,
                format!(
                    "wrote the motivation of {}",
                    diagram_label(&editor.project, d)
                ),
                d,
                None,
            );
            Ok(Output::Json(json!({"ok": true})))
        }
        "move_box" => {
            let a: MoveBoxArgs = args(arguments)?;
            let id = resolve(&editor.project, &a.target)?;
            editor.apply_ai(|p| p.move_block(id, Cell::new(a.col, a.row)))?;
            done(editor, "moved", Some(id))
        }
        "delete_box" => delete_box(editor, args(arguments)?),
        "delete_relation" => delete_relation(editor, args(arguments)?),
        "export_docs" => export_docs(editor, args(arguments)?),
        other => Err(ApiError::UnknownTool(other.to_owned())),
    }
}

fn spec(name: &str, kind: ApiKind, tag: &Option<String>) -> BlockSpec {
    BlockSpec {
        name: name.to_owned(),
        kind: kind.into(),
        tag: tag.clone(),
    }
}

fn direction(d: Option<ApiDirection>) -> Direction {
    d.map_or(Direction::Out, Direction::from)
}

fn placement(p: Option<ApiPlacement>) -> Placement {
    match p {
        Some(ApiPlacement::Keep) => Placement::Keep,
        _ => Placement::Move,
    }
}

fn add_box(editor: &mut Editor, a: AddBoxArgs) -> ApiResult<Output> {
    let d = diagram_of(&editor.project, a.diagram.as_ref())?;
    let s = spec(&a.name, a.kind, &a.tag);
    let id = editor.apply_ai(|p| {
        let id = p.add_block(d, &s)?;
        if let Some(text) = &a.responsibility {
            p.set_responsibility(id, text)?;
        }
        Ok(id)
    })?;
    done(editor, "added", Some(id))
}

fn connect_new(editor: &mut Editor, a: ConnectNewArgs) -> ApiResult<Output> {
    let from = resolve(&editor.project, &a.from)?;
    let s = spec(&a.name, a.kind, &a.tag);
    let text = a.text.unwrap_or_default();
    let (id, rel) = editor.apply_ai(|p| {
        let (id, rel) = p.connect_new(from, a.side.into(), &s, direction(a.direction), &text)?;
        if let Some(r) = &a.responsibility {
            p.set_responsibility(id, r)?;
        }
        Ok((id, rel))
    })?;
    let mut out = box_json(&editor.project, id);
    out["relation"] = json!(rel.0);
    let summary = format!(
        "added {} next to {}",
        name(&editor.project, id),
        name(&editor.project, from)
    );
    note(editor, summary, parent(&editor.project, id), Some(id));
    Ok(Output::Json(out))
}

fn connect_existing(editor: &mut Editor, a: ConnectExistingArgs) -> ApiResult<Output> {
    let from = resolve(&editor.project, &a.from)?;
    let to = resolve(&editor.project, &a.to)?;
    let text = a.text.unwrap_or_default();
    let rel = editor.apply_ai(|p| {
        p.connect_existing(
            from,
            a.side.into(),
            to,
            direction(a.direction),
            &text,
            placement(a.placement),
        )
    })?;
    let summary = format!(
        "connected {} and {}",
        name(&editor.project, from),
        name(&editor.project, to)
    );
    note(editor, summary, parent(&editor.project, from), Some(to));
    Ok(Output::Json(relation_json(&editor.project, rel)))
}

fn add_stub(editor: &mut Editor, a: AddStubArgs) -> ApiResult<Output> {
    let from = resolve(&editor.project, &a.from)?;
    let text = a.text.unwrap_or_default();
    let rel =
        editor.apply_ai(|p| p.add_stub(from, a.side.into(), direction(a.direction), &text))?;
    let summary = format!("left an open end on {}", name(&editor.project, from));
    note(editor, summary, parent(&editor.project, from), Some(from));
    Ok(Output::Json(relation_json(&editor.project, rel)))
}

fn connect_open_end(editor: &mut Editor, a: ConnectOpenEndArgs) -> ApiResult<Output> {
    let rel = RelationId(a.relation);
    editor.project.relation(rel)?;
    let to = resolve(&editor.project, &a.to)?;
    let d = parent(&editor.project, to);
    let side = a.side.map(Side::from);
    editor.apply_ai(|p| p.connect_stub(rel, d, to, side, placement(a.placement)))?;
    let summary = format!("connected an open end to {}", name(&editor.project, to));
    note(editor, summary, d, Some(to));
    Ok(Output::Json(relation_json(&editor.project, rel)))
}

fn attach(editor: &mut Editor, a: AttachArgs) -> ApiResult<Output> {
    let rel = RelationId(a.relation);
    let outer = resolve(&editor.project, &a.whitebox)?;
    let inner = resolve(&editor.project, &a.to)?;
    let r = editor.project.relation(rel)?;
    let end = [End::A, End::B]
        .into_iter()
        .find(|e| r.end(*e).position_of(outer).is_some())
        .ok_or_else(|| {
            ApiError::Rejected(format!(
                "relation {} does not enter the whitebox of {}",
                rel.0,
                name(&editor.project, outer)
            ))
        })?;
    let frame_side = r
        .end(end)
        .anchors
        .iter()
        .find(|x| x.block == outer)
        .map(|x| x.side);
    let side = a.side.map(Side::from).or(frame_side).unwrap_or(Side::Left);
    editor.apply_ai(|p| p.attach(rel, end, outer, inner, side))?;
    let summary = format!("attached a relation to {}", name(&editor.project, inner));
    note(editor, summary, Some(outer), Some(inner));
    Ok(Output::Json(relation_json(&editor.project, rel)))
}

fn edit_box(editor: &mut Editor, a: EditBoxArgs) -> ApiResult<Output> {
    let id = resolve(&editor.project, &a.target)?;
    let b = editor.project.block(id)?;
    let current_tag = b
        .tag
        .and_then(|t| editor.project.tags.get(&t))
        .map(|t| t.name.clone());
    let s = BlockSpec {
        name: a.name.unwrap_or_else(|| b.name.clone()),
        kind: a.kind.map_or(b.kind, BlockKind::from),
        tag: a.tag.or(current_tag),
    };
    editor.apply_ai(|p| p.edit_block(id, &s))?;
    done(editor, "edited", Some(id))
}

fn edit_relation(editor: &mut Editor, a: EditRelationArgs) -> ApiResult<Output> {
    let rel = RelationId(a.relation);
    let r = editor.project.relation(rel)?;
    let d = a.direction.map_or(r.direction, Direction::from);
    let text = a.text.unwrap_or_else(|| r.text.clone());
    let owner = r.owner;
    editor.apply_ai(|p| p.edit_relation(rel, d, &text))?;
    note(editor, "edited a relation".to_owned(), owner, None);
    Ok(Output::Json(relation_json(&editor.project, rel)))
}

fn delete_box(editor: &mut Editor, a: DeleteBoxArgs) -> ApiResult<Output> {
    let id = resolve(&editor.project, &a.target)?;
    let inside = editor.project.subtree(id).len() - 1;
    if inside > 0 && !a.recursive {
        return rejected(format!(
            "{} contains {inside} boxes; pass recursive: true to delete them too",
            name(&editor.project, id)
        ));
    }
    let label = name(&editor.project, id);
    let d = parent(&editor.project, id);
    editor.apply_ai(|p| p.delete_block(id))?;
    note(editor, format!("deleted {label}"), d, None);
    Ok(Output::Json(
        json!({"deleted": label, "boxes_inside": inside}),
    ))
}

fn delete_relation(editor: &mut Editor, a: DeleteRelationArgs) -> ApiResult<Output> {
    let rel = RelationId(a.relation);
    let owner = editor.project.relation(rel)?.owner;
    let d = match &a.detach_in {
        Some(b) => Some(resolve(&editor.project, b)?),
        None => owner,
    };
    editor.apply_ai(|p| p.remove_line(rel, d))?;
    let what = if d == owner { "deleted" } else { "detached" };
    note(editor, format!("{what} a relation"), d, None);
    Ok(Output::Json(json!({ what: rel.0 })))
}

fn export_docs(editor: &mut Editor, a: ExportArgs) -> ApiResult<Output> {
    let target = crate::editor::resolve_folder(Path::new(&a.folder))
        .map_err(|e| ApiError::Rejected(e.to_string()))?;
    if !editor.ai_export_allowed(&target) {
        let msg = format!(
            "the user has not allowed exports into {} yet; whiteboxed is asking them now. \
             Call export_docs again once they agreed.",
            target.display()
        );
        editor.ai_export_request = Some(target);
        return rejected(msg);
    }
    let dir = target.as_path();
    std::fs::create_dir_all(dir).map_err(|e| ApiError::Rejected(e.to_string()))?;
    let format = match a.format {
        ApiFormat::Asciidoc => DocFormat::AsciiDoc,
        ApiFormat::Markdown => DocFormat::Markdown,
    };
    let n = editor
        .export_all(dir, format)
        .map_err(|e| ApiError::Rejected(e.to_string()))?;
    let summary = format!("exported {n} diagrams to {}", dir.display());
    editor.last_ai = Some(AiAction {
        summary: summary.clone(),
        diagram: editor.diagram,
        block: None,
    });
    Ok(Output::Json(
        json!({"diagrams": n, "folder": dir.display().to_string(), "index": format!("index.{}", format.extension())}),
    ))
}

fn done(editor: &mut Editor, verb: &str, id: Option<BlockId>) -> ApiResult<Output> {
    let Some(id) = id else {
        return Ok(Output::Json(json!({"ok": true})));
    };
    let d = parent(&editor.project, id);
    let summary = format!(
        "{verb} {} in {}",
        name(&editor.project, id),
        diagram_label(&editor.project, d)
    );
    note(editor, summary, d, Some(id));
    Ok(Output::Json(box_json(&editor.project, id)))
}

fn note(editor: &mut Editor, summary: String, diagram: DiagramId, block: Option<BlockId>) {
    editor.last_ai = Some(AiAction {
        summary,
        diagram,
        block,
    });
}

// ----- references -----

/// Resolves a box reference. Names compare case-insensitively; a `/`-separated
/// string may also match names that contain `/` themselves.
pub fn resolve(project: &Project, r: &BoxRef) -> ApiResult<BlockId> {
    match r {
        BoxRef::Id(n) => {
            let id = BlockId(*n);
            if project.blocks.contains_key(&id) {
                Ok(id)
            } else {
                rejected(format!("no box with id {n}"))
            }
        }
        BoxRef::Path(names) => {
            let mut diagram = None;
            let mut found = None;
            for n in names {
                let wanted = n.trim().to_lowercase();
                let hit = project
                    .blocks_in(diagram)
                    .find(|(_, b)| b.name.to_lowercase() == wanted)
                    .map(|(id, _)| id);
                match hit {
                    Some(id) => {
                        found = Some(id);
                        diagram = Some(id);
                    }
                    None => {
                        return rejected(format!(
                            "no box \"{n}\" in {}",
                            diagram_label(project, diagram)
                        ));
                    }
                }
            }
            found.map_or_else(|| rejected("empty path"), Ok)
        }
        BoxRef::Text(text) => {
            let mut hits = Vec::new();
            match_text(project, None, text.trim(), &mut hits);
            match hits.as_slice() {
                [one] => Ok(*one),
                [] => rejected(format!("no box \"{text}\"")),
                _ => rejected(format!(
                    "\"{text}\" is ambiguous; use the id or a list of names"
                )),
            }
        }
    }
}

fn match_text(project: &Project, diagram: DiagramId, rest: &str, hits: &mut Vec<BlockId>) {
    // Split only at '/' in the original text: lowercasing can change byte lengths
    // ('İ' becomes two characters), so positions in a lowercased copy are wrong.
    let splits: Vec<usize> = rest.match_indices('/').map(|(i, _)| i).collect();
    for (id, b) in project.blocks_in(diagram) {
        let name = b.name.to_lowercase();
        if rest.to_lowercase() == name {
            hits.push(id);
        }
        for &i in &splits {
            if let (Some(head), Some(tail)) = (rest.get(..i), rest.get(i + 1..))
                && head.to_lowercase() == name
            {
                match_text(project, Some(id), tail, hits);
            }
        }
    }
}

/// The diagram a reference points at: `None` (or an empty path) is the context view.
pub fn diagram_of(project: &Project, r: Option<&BoxRef>) -> ApiResult<DiagramId> {
    match r {
        None => Ok(None),
        Some(BoxRef::Path(p)) if p.is_empty() => Ok(None),
        Some(BoxRef::Text(t)) if t.trim().is_empty() => Ok(None),
        Some(r) => {
            let id = resolve(project, r)?;
            let b = project.block(id)?;
            if !b.kind.can_drill() {
                return rejected(format!("{} boxes have no whitebox", b.kind.label()));
            }
            Ok(Some(id))
        }
    }
}

// ----- JSON views -----

fn name(project: &Project, id: BlockId) -> String {
    project
        .blocks
        .get(&id)
        .map_or_else(|| format!("#{}", id.0), |b| b.name.clone())
}

fn parent(project: &Project, id: BlockId) -> DiagramId {
    project.blocks.get(&id).and_then(|b| b.parent)
}

fn diagram_label(project: &Project, d: DiagramId) -> String {
    match d {
        None => "the context view".to_owned(),
        Some(id) => format!("the whitebox of {}", name(project, id)),
    }
}

fn path(project: &Project, id: BlockId) -> Vec<String> {
    let mut out: Vec<String> = project
        .path(parent(project, id))
        .into_iter()
        .map(|b| name(project, b))
        .collect();
    out.push(name(project, id));
    out
}

fn side_name(s: Side) -> &'static str {
    match s {
        Side::Top => "top",
        Side::Right => "right",
        Side::Bottom => "bottom",
        Side::Left => "left",
    }
}

fn kind_name(k: BlockKind) -> &'static str {
    match k {
        BlockKind::Component => "component",
        BlockKind::Database => "database",
        BlockKind::Queue => "queue",
        BlockKind::Cache => "cache",
        BlockKind::FileStorage => "file_storage",
        BlockKind::Ui => "ui",
        BlockKind::Person => "person",
        BlockKind::ExternalSystem => "external_system",
    }
}

fn box_json(project: &Project, id: BlockId) -> Value {
    let Some(b) = project.blocks.get(&id) else {
        return json!({"id": id.0});
    };
    let mut v = json!({
        "id": id.0,
        "path": path(project, id),
        "name": b.name,
        "kind": kind_name(b.kind),
        "level": project.level(b.parent),
        "cell": {"col": b.cell.col, "row": b.cell.row},
        "has_whitebox_content": project.has_content(id),
    });
    if let Some(tag) = b.tag.and_then(|t| project.tags.get(&t)) {
        v["tag"] = json!(tag.name);
    }
    if !b.responsibility.is_empty() {
        v["responsibility"] = json!(b.responsibility);
    }
    if !b.motivation.is_empty() {
        v["motivation"] = json!(b.motivation);
    }
    v
}

fn endpoint_json(project: &Project, e: &Endpoint) -> Value {
    if e.is_open() {
        return json!("open");
    }
    Value::Array(
        e.anchors
            .iter()
            .map(|a| json!({"id": a.block.0, "name": name(project, a.block), "side": side_name(a.side)}))
            .collect(),
    )
}

fn relation_json(project: &Project, rel: RelationId) -> Value {
    let Some(r) = project.relations.get(&rel) else {
        return json!({"id": rel.0});
    };
    json!({
        "id": rel.0,
        "diagram": r.owner.map(|o| path(project, o)),
        "a": endpoint_json(project, &r.a),
        "b": endpoint_json(project, &r.b),
        "direction": r.direction.label(),
        "text": r.text,
    })
}

pub fn model_json(project: &Project) -> Value {
    json!({
        "context_motivation": project.motivation,
        "tags": project.tags.values().map(|t| json!({"name": t.name, "color": t.color.to_string()})).collect::<Vec<_>>(),
        "boxes": project.blocks.keys().map(|id| box_json(project, *id)).collect::<Vec<_>>(),
        "relations": project.relations.keys().map(|r| relation_json(project, *r)).collect::<Vec<_>>(),
    })
}

fn view_end_json(project: &Project, e: &ViewEnd) -> Value {
    match e {
        ViewEnd::Block { block, side } => {
            json!({"box": block.0, "name": name(project, *block), "side": side_name(*side)})
        }
        ViewEnd::Frame { side, partner } => {
            json!({"frame": side_name(*side), "partner_outside": partner})
        }
        ViewEnd::Open => json!("open"),
        ViewEnd::Dangling => json!("dangling"),
    }
}

pub fn diagram_json(project: &Project, d: DiagramId) -> Value {
    let v = view::diagram_view(project, d);
    json!({
        "diagram": d.map(|id| path(project, id)),
        "level": project.level(d),
        "motivation": project.motivation(d),
        "boxes": v.blocks.iter().map(|id| box_json(project, *id)).collect::<Vec<_>>(),
        "lines": v.lines.iter().map(|l| json!({
            "relation": l.relation.0,
            "a": view_end_json(project, &l.a),
            "b": view_end_json(project, &l.b),
            "direction": l.direction.label(),
            "text": l.text,
        })).collect::<Vec<_>>(),
        "dangling": view::dangling_count(&v),
    })
}
