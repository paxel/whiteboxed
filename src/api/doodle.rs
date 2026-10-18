//! The AI tools for doodles. Shapes, groups and lines are addressed by id: texts may
//! repeat and span several lines.

use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ApiError, ApiResult, Output, ToolInfo, args, note, rejected, schema};
use crate::editor::Editor;
use crate::model::{
    BlockId, BlockKind, BlockSpec, Cell, Direction, LinePattern, Placement, Project, RelationId,
    Rgb,
};
use crate::{export, layout, scene, view};

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(rename_all = "snake_case")]
pub enum ApiShape {
    Box,
    /// A process step.
    Rounded,
    /// Start or end.
    Circle,
    /// A decision.
    Diamond,
    /// Data.
    Cylinder,
    Document,
    /// A network, the internet.
    Cloud,
    Person,
    /// Text without a border; with a colour a sticky note.
    Note,
}

impl From<ApiShape> for BlockKind {
    fn from(s: ApiShape) -> BlockKind {
        match s {
            ApiShape::Box => BlockKind::Component,
            ApiShape::Rounded => BlockKind::Rounded,
            ApiShape::Circle => BlockKind::Circle,
            ApiShape::Diamond => BlockKind::Diamond,
            ApiShape::Cylinder => BlockKind::Database,
            ApiShape::Document => BlockKind::Document,
            ApiShape::Cloud => BlockKind::Cloud,
            ApiShape::Person => BlockKind::Person,
            ApiShape::Note => BlockKind::Note,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(rename_all = "snake_case")]
pub enum ApiPattern {
    Solid,
    Dashed,
    Dotted,
}

impl From<ApiPattern> for LinePattern {
    fn from(p: ApiPattern) -> LinePattern {
        match p {
            ApiPattern::Solid => LinePattern::Solid,
            ApiPattern::Dashed => LinePattern::Dashed,
            ApiPattern::Dotted => LinePattern::Dotted,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct PageArgs {
    pub page: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct NameArgs {
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct RenamePageArgs {
    pub page: u64,
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct AddShapeArgs {
    pub page: u64,
    /// Any text; \n starts a new line.
    pub text: String,
    #[serde(default)]
    pub shape: Option<ApiShape>,
    /// "#rrggbb"; omitted: white.
    #[serde(default)]
    pub color: Option<String>,
    /// A custom look by name (see list_looks), instead of a shape.
    #[serde(default)]
    pub look: Option<String>,
    /// Grid cell; omitted: the next free column.
    #[serde(default)]
    pub col: Option<i32>,
    #[serde(default)]
    pub row: Option<i32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EditShapeArgs {
    /// A shape or group id.
    pub shape: u64,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default, rename = "as")]
    pub kind: Option<ApiShape>,
    /// "#rrggbb", or "" for no colour.
    #[serde(default)]
    pub color: Option<String>,
    /// A custom look by name (see list_looks); an empty string removes it.
    #[serde(default)]
    pub look: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct MoveShapeArgs {
    pub shape: u64,
    pub col: i32,
    pub row: i32,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ShapeArgs {
    /// A shape or group id.
    pub shape: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct AddLineArgs {
    /// A shape or group id.
    pub from: u64,
    pub to: u64,
    /// Direction seen from `from`: out (default), in, bi, none.
    #[serde(default)]
    pub direction: Option<super::ApiDirection>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub pattern: Option<ApiPattern>,
    /// The line's colour as #rrggbb; left out: the default.
    #[serde(default)]
    pub color: Option<String>,
    /// A line look by name (see list_looks).
    #[serde(default)]
    pub look: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EditLineArgs {
    pub line: u64,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub direction: Option<super::ApiDirection>,
    #[serde(default)]
    pub pattern: Option<ApiPattern>,
    /// The line's colour as #rrggbb; "" goes back to the default.
    #[serde(default)]
    pub color: Option<String>,
    /// Whether the line goes through mouseholes: automatic (over more than two
    /// shapes, or where going under saves about three crossings), always, or never.
    #[serde(default)]
    pub mousehole: Option<super::ApiMousehole>,
    /// A line look by name (see list_looks); "" takes it off.
    #[serde(default)]
    pub look: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct LineArgs {
    pub line: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct GroupArgs {
    /// Shapes (or groups) of one page; the frame is the rectangle of cells around them.
    pub shapes: Vec<u64>,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct WidenGroupArgs {
    pub group: u64,
    pub shapes: Vec<u64>,
}

pub fn tools() -> Vec<ToolInfo> {
    let tool = |name, description, schema| ToolInfo {
        name,
        description,
        schema,
    };
    vec![
        tool(
            "doodle_get",
            "The open doodle: every page with its shapes (id, text, shape, colour, cell), groups (id, name, cells, members), lines (id, from, to, direction, text, pattern) and readability (level, counts, causes with what helps).",
            schema::<super::NoArgs>(),
        ),
        tool(
            "doodle_render",
            "One doodle page as PNG, as the user sees it.",
            schema::<PageArgs>(),
        ),
        tool(
            "doodle_add_page",
            "Add a page (a tab) to the doodle.",
            schema::<NameArgs>(),
        ),
        tool(
            "doodle_rename_page",
            "Rename a page.",
            schema::<RenamePageArgs>(),
        ),
        tool(
            "doodle_delete_page",
            "Delete a page with everything on it; the last page stays.",
            schema::<PageArgs>(),
        ),
        tool(
            "doodle_add_shape",
            "Put a shape on a page: box, rounded (a step), circle (start, end), diamond (a decision), cylinder (data), document, cloud (a network), person, note. Text may repeat and span lines.",
            schema::<AddShapeArgs>(),
        ),
        tool(
            "doodle_edit_shape",
            "Change a shape's text, shape or colour, or a group's name or colour; omitted fields stay.",
            schema::<EditShapeArgs>(),
        ),
        tool(
            "doodle_move_shape",
            "Move a shape to another grid cell; a shape there swaps places.",
            schema::<MoveShapeArgs>(),
        ),
        tool(
            "doodle_delete_shape",
            "Delete a shape or group with its lines (a group's shapes stay).",
            schema::<ShapeArgs>(),
        ),
        tool(
            "doodle_add_line",
            "Draw a line between two shapes or groups of a page, leaving each on the side that faces the other.",
            schema::<AddLineArgs>(),
        ),
        tool(
            "doodle_edit_line",
            "Change a line's text, direction, pattern (solid, dashed, dotted), colour or mousehole setting.",
            schema::<EditLineArgs>(),
        ),
        tool("doodle_delete_line", "Delete a line.", schema::<LineArgs>()),
        tool(
            "doodle_group",
            "Frame shapes as a labelled group: every shape in the rectangle of cells around them belongs to it. Groups may nest, not partly overlap.",
            schema::<GroupArgs>(),
        ),
        tool(
            "doodle_widen_group",
            "Grow a group's rectangle around further shapes.",
            schema::<WidenGroupArgs>(),
        ),
    ]
}

/// Runs a doodle tool, or `None` if `tool` is no doodle tool.
pub fn call(editor: &mut Editor, tool: &str, arguments: Value) -> Option<ApiResult<Output>> {
    if !tool.starts_with("doodle_") {
        return None;
    }
    if !editor.project.is_doodle() {
        return Some(rejected(
            "no doodle is open; the doodle_ tools work on doodles (File > New doodle)",
        ));
    }
    Some(run(editor, tool, arguments))
}

fn run(editor: &mut Editor, tool: &str, arguments: Value) -> ApiResult<Output> {
    match tool {
        "doodle_get" => {
            let _: super::NoArgs = args(arguments)?;
            Ok(Output::Json(doodle_json(&editor.project)))
        }
        "doodle_render" => {
            let a: PageArgs = args(arguments)?;
            let page = page(&editor.project, a.page)?;
            let l = layout::layout(
                &editor.project,
                &view::diagram_view(&editor.project, Some(page)),
            );
            let longest = l.bounds.width().max(l.bounds.height()).max(1.0);
            let scale = (super::RENDER_LIMIT / longest).min(1.0);
            let png = export::to_png(&scene::scene(&l), scale)
                .map_err(|e| ApiError::Rejected(e.to_string()))?;
            Ok(Output::Png(png))
        }
        "doodle_add_page" => {
            let a: NameArgs = args(arguments)?;
            let id = editor.apply_ai(|p| p.add_page(&a.name))?;
            note(editor, format!("added page {}", a.name), Some(id), None);
            Ok(Output::Json(json!({"page": id.0, "name": a.name})))
        }
        "doodle_rename_page" => {
            let a: RenamePageArgs = args(arguments)?;
            let page = page(&editor.project, a.page)?;
            editor.apply_ai(|p| p.edit_block(page, &BlockSpec::new(&a.name, BlockKind::Page)))?;
            note(
                editor,
                format!("renamed a page to {}", a.name),
                Some(page),
                None,
            );
            Ok(Output::Json(json!({"page": page.0, "name": a.name})))
        }
        "doodle_delete_page" => {
            let a: PageArgs = args(arguments)?;
            let page = page(&editor.project, a.page)?;
            editor.apply_ai(|p| p.delete_block(page))?;
            Ok(Output::Json(json!({"deleted": page.0})))
        }
        "doodle_add_shape" => {
            let a: AddShapeArgs = args(arguments)?;
            let page = page(&editor.project, a.page)?;
            let spec = BlockSpec {
                color: color(a.color.as_deref())?,
                ..BlockSpec::new(
                    &a.text,
                    a.shape.map_or(BlockKind::Component, BlockKind::from),
                )
            };
            let look = a
                .look
                .as_deref()
                .map(|name| super::look::by_name(editor, name))
                .transpose()?
                .flatten();
            let id = editor.apply_ai(|p| {
                let id = match (a.col, a.row) {
                    (Some(col), Some(row)) => {
                        p.add_block_at(Some(page), &spec, Cell::new(col, row))
                    }
                    _ => p.add_block(Some(page), &spec),
                }?;
                if look.is_some() {
                    p.wear_look(id, look)?;
                }
                Ok(id)
            })?;
            shape_done(editor, "added", id)
        }
        "doodle_edit_shape" => {
            let a: EditShapeArgs = args(arguments)?;
            let id = shape(&editor.project, a.shape)?;
            let b = editor.project.block(id)?;
            let kind = match (a.kind, b.kind) {
                (Some(_), BlockKind::Group) => return rejected("a group has no shape"),
                (Some(k), _) => k.into(),
                (None, k) => k,
            };
            let spec = BlockSpec {
                color: match a.color.as_deref() {
                    Some(c) => color(Some(c))?,
                    None => b.color,
                },
                ..BlockSpec::new(a.text.as_deref().unwrap_or(&b.name), kind)
            };
            let look = a
                .look
                .as_deref()
                .map(|name| super::look::by_name(editor, name))
                .transpose()?;
            editor.apply_ai(|p| {
                p.edit_block(id, &spec)?;
                match look {
                    Some(look) => p.wear_look(id, look),
                    None => Ok(()),
                }
            })?;
            shape_done(editor, "edited", id)
        }
        "doodle_move_shape" => {
            let a: MoveShapeArgs = args(arguments)?;
            let id = shape(&editor.project, a.shape)?;
            editor.apply_ai(|p| p.move_block(id, Cell::new(a.col, a.row)))?;
            shape_done(editor, "moved", id)
        }
        "doodle_delete_shape" => {
            let a: ShapeArgs = args(arguments)?;
            let id = shape(&editor.project, a.shape)?;
            let page = editor.project.block(id)?.parent;
            editor.apply_ai(|p| p.delete_block(id))?;
            note(editor, "deleted a shape".into(), page, None);
            Ok(Output::Json(json!({"deleted": id.0})))
        }
        "doodle_add_line" => {
            let a: AddLineArgs = args(arguments)?;
            let from = shape(&editor.project, a.from)?;
            let to = shape(&editor.project, a.to)?;
            let (side, _) = editor.project.sides_between(from, to)?;
            let text = a.text.unwrap_or_default();
            let direction = super::direction(a.direction);
            let pattern = a.pattern.map(LinePattern::from).unwrap_or_default();
            let line_color = color(a.color.as_deref())?;
            let look = match a.look.as_deref() {
                Some(n) => super::look::line_look_by_name(editor, n)?,
                None => None,
            };
            let rel = editor.apply_ai(|p| {
                let rel = p.connect_existing(from, side, to, direction, &text, Placement::Keep)?;
                p.set_relation_pattern(rel, pattern)?;
                p.set_relation_color(rel, line_color)?;
                if look.is_some() {
                    p.wear_line_look(rel, look)?;
                }
                Ok(rel)
            })?;
            let page = editor.project.block(from)?.parent;
            note(editor, "drew a line".into(), page, None);
            Ok(Output::Json(line_json(&editor.project, rel)))
        }
        "doodle_edit_line" => {
            let a: EditLineArgs = args(arguments)?;
            let rel = RelationId(a.line);
            let r = editor.project.relation(rel)?;
            let direction = a.direction.map_or(r.direction, Direction::from);
            let text = a.text.unwrap_or_else(|| r.text.clone());
            let pattern = a.pattern.map_or(r.pattern, LinePattern::from);
            let line_color = match a.color.as_deref() {
                Some(text) => color(Some(text))?,
                None => r.color,
            };
            let page = r.owner;
            let mousehole = a.mousehole.map(crate::model::MouseholeMode::from);
            let look = match a.look.as_deref() {
                Some(n) => Some(super::look::line_look_by_name(editor, n)?),
                None => None,
            };
            editor.apply_ai(|p| {
                p.edit_relation(rel, direction, &text)?;
                p.set_relation_pattern(rel, pattern)?;
                p.set_relation_color(rel, line_color)?;
                if let Some(look) = look {
                    p.wear_line_look(rel, look)?;
                }
                match mousehole {
                    Some(mode) => p.set_mousehole(rel, page, mode),
                    None => Ok(()),
                }
            })?;
            note(editor, "edited a line".into(), page, None);
            Ok(Output::Json(line_json(&editor.project, rel)))
        }
        "doodle_delete_line" => {
            let a: LineArgs = args(arguments)?;
            let rel = RelationId(a.line);
            let page = editor.project.relation(rel)?.owner;
            editor.apply_ai(|p| p.remove_line(rel, page))?;
            note(editor, "deleted a line".into(), page, None);
            Ok(Output::Json(json!({"deleted": rel.0})))
        }
        "doodle_group" => {
            let a: GroupArgs = args(arguments)?;
            let shapes = a
                .shapes
                .iter()
                .map(|s| shape(&editor.project, *s))
                .collect::<ApiResult<Vec<_>>>()?;
            let c = color(a.color.as_deref())?;
            let id = editor.apply_ai(|p| p.add_group(&shapes, &a.name, c))?;
            shape_done(editor, "framed", id)
        }
        "doodle_widen_group" => {
            let a: WidenGroupArgs = args(arguments)?;
            let group = shape(&editor.project, a.group)?;
            let shapes = a
                .shapes
                .iter()
                .map(|s| shape(&editor.project, *s))
                .collect::<ApiResult<Vec<_>>>()?;
            editor.apply_ai(|p| p.widen_group(group, &shapes))?;
            shape_done(editor, "widened", group)
        }
        other => Err(ApiError::UnknownTool(other.to_owned())),
    }
}

fn page(project: &Project, id: u64) -> ApiResult<BlockId> {
    let id = BlockId(id);
    match project.blocks.get(&id) {
        Some(b) if b.kind == BlockKind::Page => Ok(id),
        _ => rejected(format!("{} is no page; doodle_get lists them", id.0)),
    }
}

fn shape(project: &Project, id: u64) -> ApiResult<BlockId> {
    let id = BlockId(id);
    match project.blocks.get(&id) {
        Some(b) if b.kind != BlockKind::Page => Ok(id),
        _ => rejected(format!(
            "{} is no shape or group; doodle_get lists them",
            id.0
        )),
    }
}

fn color(text: Option<&str>) -> ApiResult<Option<Rgb>> {
    match text.map(str::trim) {
        None | Some("") => Ok(None),
        Some(t) => Rgb::parse(t)
            .map(Some)
            .ok_or_else(|| ApiError::Rejected(format!("{t} is no colour; use #rrggbb"))),
    }
}

fn shape_done(editor: &mut Editor, verb: &str, id: BlockId) -> ApiResult<Output> {
    let page = editor.project.block(id)?.parent;
    let text = editor.project.block(id)?.name.replace('\n', " ");
    note(editor, format!("{verb} {text}"), page, Some(id));
    Ok(Output::Json(shape_json(&editor.project, id)))
}

fn shape_json(project: &Project, id: BlockId) -> Value {
    let Some(b) = project.blocks.get(&id) else {
        return json!({"id": id.0});
    };
    let color = b.color.map(|c| c.to_string());
    match b.group_cells() {
        Some((near, far)) => json!({
            "id": id.0,
            "group": b.name,
            "color": color,
            "from": {"col": near.col, "row": near.row},
            "to": {"col": far.col, "row": far.row},
            "members": project.members(id).iter().map(|m| m.0).collect::<Vec<_>>(),
        }),
        None => json!({
            "id": id.0,
            "text": b.name,
            "shape": shape_name(b.kind),
            "look": b.look.as_ref().and_then(|l| project.looks.get(l)).map(|l| l.name()),
            "color": color,
            "cell": {"col": b.cell.col, "row": b.cell.row},
        }),
    }
}

fn shape_name(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::Component => "box",
        BlockKind::Database => "cylinder",
        BlockKind::Rounded => "rounded",
        BlockKind::Circle => "circle",
        BlockKind::Diamond => "diamond",
        BlockKind::Document => "document",
        BlockKind::Cloud => "cloud",
        BlockKind::Person => "person",
        BlockKind::Note => "note",
        other => other.label(),
    }
}

fn line_json(project: &Project, rel: RelationId) -> Value {
    let Some(r) = project.relations.get(&rel) else {
        return json!({"id": rel.0});
    };
    let end = |e: &crate::model::Endpoint| e.anchors.first().map(|a| a.block.0);
    json!({
        "id": rel.0,
        "from": end(&r.a),
        "to": end(&r.b),
        "direction": r.direction.label(),
        "text": r.text,
        "pattern": r.pattern.label(),
        "color": r.color.map(|c| c.to_string()),
        "mousehole": r.mousehole_in(r.owner).label(),
        "look": r.look.as_ref().and_then(|l| project.line_looks.get(l)).map(|l| l.name()),
    })
}

fn doodle_json(project: &Project) -> Value {
    let pages: Vec<Value> = project
        .pages()
        .into_iter()
        .map(|page| {
            let name = project.blocks.get(&page).map(|b| b.name.clone());
            let all: Vec<BlockId> = project.blocks_in(Some(page)).map(|(id, _)| id).collect();
            let (groups, shapes): (Vec<BlockId>, Vec<BlockId>) = all.into_iter().partition(|id| {
                project
                    .blocks
                    .get(id)
                    .is_some_and(|b| b.kind == BlockKind::Group)
            });
            let lines: Vec<Value> = project
                .relations
                .iter()
                .filter(|(_, r)| r.owner == Some(page))
                .map(|(id, _)| line_json(project, *id))
                .collect();
            json!({
                "id": page.0,
                "name": name,
                "shapes": shapes.iter().map(|s| shape_json(project, *s)).collect::<Vec<_>>(),
                "groups": groups.iter().map(|g| shape_json(project, *g)).collect::<Vec<_>>(),
                "lines": lines,
                "readability": super::score_json(&crate::score::of(project, Some(page))),
            })
        })
        .collect();
    json!({ "pages": pages })
}
