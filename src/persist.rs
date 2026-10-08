//! The project file: one YAML document holding the model and the layout slots, never
//! pixel coordinates. Lists are written in id order so diffs only show real changes.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::{
    Anchor, Block, BlockId, BlockKind, Cell, DEFAULT_LABEL_LIMIT, Direction, Endpoint, LineStyle,
    MAX_BLOCKS_PER_DIAGRAM, MAX_RELATIONS_PER_DIAGRAM, Project, Relation, RelationId, Rgb, Side,
    Tag, TagId,
};

pub const FORMAT: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    #[error("cannot read or write the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a valid project file: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),
    #[error("project file is inconsistent: {0}")]
    Invalid(String),
}

#[derive(Serialize, Deserialize)]
struct FileDto {
    format: u32,
    /// The project's name, when set by the user.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    name: String,
    /// Explanation of the context view.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    motivation: String,
    /// Written only when it differs from the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    line_style: Option<LineStyle>,
    /// Written only when it differs from the default; `never` turns shortening off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label_limit: Option<LimitDto>,
    #[serde(default)]
    tags: Vec<TagDto>,
    #[serde(default)]
    blocks: Vec<BlockDto>,
    #[serde(default)]
    relations: Vec<RelationDto>,
}

#[derive(Serialize, Deserialize)]
struct TagDto {
    id: TagId,
    name: String,
    color: Rgb,
}

#[derive(Serialize, Deserialize)]
struct BlockDto {
    id: BlockId,
    name: String,
    kind: BlockKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent: Option<BlockId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tag: Option<TagId>,
    /// Every box but a band has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cell: Option<Cell>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    band: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    responsibility: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    motivation: String,
}

#[derive(Serialize, Deserialize)]
struct AnchorDto {
    block: BlockId,
    side: Side,
}

#[derive(Serialize, Deserialize)]
struct RelationDto {
    id: RelationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner: Option<BlockId>,
    a: Vec<AnchorDto>,
    b: Vec<AnchorDto>,
    direction: Direction,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    style: Option<LineStyle>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    short: String,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(untagged)]
enum LimitDto {
    Chars(u32),
    Never(Never),
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum Never {
    Never,
}

fn anchors_dto(end: &Endpoint) -> Vec<AnchorDto> {
    end.anchors
        .iter()
        .map(|a| AnchorDto {
            block: a.block,
            side: a.side,
        })
        .collect()
}

fn endpoint(anchors: Vec<AnchorDto>) -> Endpoint {
    Endpoint {
        anchors: anchors
            .into_iter()
            .map(|a| Anchor {
                block: a.block,
                side: a.side,
            })
            .collect(),
    }
}

pub fn to_yaml(project: &Project) -> Result<String, PersistError> {
    let dto = FileDto {
        format: FORMAT,
        name: project.name.clone(),
        motivation: project.motivation.clone(),
        line_style: (project.line_style != LineStyle::default()).then_some(project.line_style),
        label_limit: (project.label_limit != DEFAULT_LABEL_LIMIT).then_some(
            match project.label_limit {
                Some(n) => LimitDto::Chars(n),
                None => LimitDto::Never(Never::Never),
            },
        ),
        tags: project
            .tags
            .iter()
            .map(|(id, t)| TagDto {
                id: *id,
                name: t.name.clone(),
                color: t.color,
            })
            .collect(),
        blocks: project
            .blocks
            .iter()
            .map(|(id, b)| BlockDto {
                id: *id,
                name: b.name.clone(),
                kind: b.kind,
                parent: b.parent,
                tag: b.tag,
                cell: (!b.band).then_some(b.cell),
                band: b.band,
                responsibility: b.responsibility.clone(),
                motivation: b.motivation.clone(),
            })
            .collect(),
        relations: project
            .relations
            .iter()
            .map(|(id, r)| RelationDto {
                id: *id,
                owner: r.owner,
                a: anchors_dto(&r.a),
                b: anchors_dto(&r.b),
                direction: r.direction,
                text: r.text.clone(),
                style: r.style,
                short: r.short.clone(),
            })
            .collect(),
    };
    Ok(serde_yaml_ng::to_string(&dto)?)
}

pub fn from_yaml(text: &str) -> Result<Project, PersistError> {
    let dto: FileDto = serde_yaml_ng::from_str(text)?;
    if dto.format > FORMAT {
        return Err(PersistError::Invalid(format!(
            "file format {} is newer than this app understands ({FORMAT})",
            dto.format
        )));
    }
    let mut project = Project::new();
    project.name = dto.name.trim().to_owned();
    project.motivation = dto.motivation;
    project.line_style = dto.line_style.unwrap_or_default();
    project.label_limit = match dto.label_limit {
        None => DEFAULT_LABEL_LIMIT,
        Some(LimitDto::Chars(n)) => Some(n),
        Some(LimitDto::Never(_)) => None,
    };
    let mut ids = BTreeSet::new();
    let mut fresh = |id: u64| {
        if ids.insert(id) {
            Ok(())
        } else {
            Err(PersistError::Invalid(format!("id {id} is used twice")))
        }
    };
    for t in dto.tags {
        fresh(t.id.0)?;
        project.tags.insert(
            t.id,
            Tag {
                name: t.name,
                color: t.color,
            },
        );
    }
    for b in dto.blocks {
        fresh(b.id.0)?;
        let cell = match (b.cell, b.band) {
            (Some(cell), false) => cell,
            (_, true) => Cell::new(0, 0),
            (None, false) => {
                return Err(PersistError::Invalid(format!("box {} has no cell", b.id.0)));
            }
        };
        project.blocks.insert(
            b.id,
            Block {
                name: b.name,
                kind: b.kind,
                tag: b.tag,
                parent: b.parent,
                cell,
                band: b.band,
                responsibility: b.responsibility,
                motivation: b.motivation,
            },
        );
    }
    for r in dto.relations {
        fresh(r.id.0)?;
        project.relations.insert(
            r.id,
            Relation {
                owner: r.owner,
                a: endpoint(r.a),
                b: endpoint(r.b),
                direction: r.direction,
                text: r.text,
                style: r.style,
                short: r.short,
            },
        );
    }
    project.next_id = ids.last().copied().unwrap_or(0);
    check(&project)?;
    Ok(project)
}

/// Every reference points to something that exists, and every chain follows the
/// nesting of the boxes.
fn check(project: &Project) -> Result<(), PersistError> {
    let invalid = |msg: String| Err(PersistError::Invalid(msg));
    for (id, b) in &project.blocks {
        if let Some(parent) = b.parent
            && !project.blocks.contains_key(&parent)
        {
            return invalid(format!("box {} has an unknown parent", id.0));
        }
        if let Some(tag) = b.tag
            && !project.tags.contains_key(&tag)
        {
            return invalid(format!("box {} has an unknown tag", id.0));
        }
        if project.path(b.parent).contains(id) {
            return invalid(format!("box {} is nested inside itself", id.0));
        }
    }
    let mut per_diagram: BTreeMap<Option<BlockId>, usize> = BTreeMap::new();
    for (id, b) in &project.blocks {
        if !b.cell.in_grid() {
            return invalid(format!("box {} lies outside the grid", id.0));
        }
        let count = per_diagram.entry(b.parent).or_default();
        *count += 1;
        if *count > MAX_BLOCKS_PER_DIAGRAM {
            return invalid(format!(
                "more than {MAX_BLOCKS_PER_DIAGRAM} boxes in one diagram"
            ));
        }
    }
    let mut cells = BTreeMap::new();
    for (id, b) in project.blocks.iter().filter(|(_, b)| !b.band) {
        if let Some(other) = cells.insert((b.parent, b.cell), *id) {
            return invalid(format!("boxes {} and {} share a cell", other.0, id.0));
        }
    }
    for (id, b) in project.blocks.iter().filter(|(_, b)| b.band) {
        if b.kind.is_neighbour() {
            return invalid(format!("box {} cannot be a band", id.0));
        }
        let touched = project.relations.values().any(|r| {
            r.a.anchors
                .iter()
                .chain(&r.b.anchors)
                .any(|a| a.block == *id)
        });
        if touched {
            return invalid(format!("band {} has lines", id.0));
        }
    }
    let mut per_owner: BTreeMap<Option<BlockId>, usize> = BTreeMap::new();
    for r in project.relations.values() {
        let count = per_owner.entry(r.owner).or_default();
        *count += 1;
        if *count > MAX_RELATIONS_PER_DIAGRAM {
            return invalid(format!(
                "more than {MAX_RELATIONS_PER_DIAGRAM} relations in one diagram"
            ));
        }
    }
    // Each end is a tree: the first box on the owner's level, every other one directly
    // inside a box named before it, none twice.
    for (id, r) in &project.relations {
        for end in [&r.a, &r.b] {
            let mut seen: Vec<BlockId> = Vec::new();
            for (k, anchor) in end.anchors.iter().enumerate() {
                let Some(block) = project.blocks.get(&anchor.block) else {
                    return invalid(format!("relation {} names an unknown box", id.0));
                };
                let fits = if k == 0 {
                    block.parent == r.owner
                } else {
                    block.parent.is_some_and(|p| seen.contains(&p))
                };
                if !fits {
                    return invalid(format!("relation {} skips a level", id.0));
                }
                if seen.contains(&anchor.block) {
                    return invalid(format!("relation {} names a box twice", id.0));
                }
                seen.push(anchor.block);
            }
        }
    }
    Ok(())
}

/// Writes the file through a temporary sibling, so a crash never leaves half a file.
pub fn save(path: &Path, project: &Project) -> Result<(), PersistError> {
    let text = to_yaml(project)?;
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut tmp = tempfile_in(dir)?;
    tmp.1.write_all(text.as_bytes())?;
    tmp.1.sync_all()?;
    drop(tmp.1);
    fs::rename(&tmp.0, path)?;
    Ok(())
}

fn tempfile_in(dir: &Path) -> std::io::Result<(std::path::PathBuf, fs::File)> {
    let pid = std::process::id();
    for n in 0..1000 {
        let candidate = dir.join(format!(".whiteboxed-{pid}-{n}.tmp"));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("no free temporary file name"))
}

pub fn load(path: &Path) -> Result<Project, PersistError> {
    from_yaml(&fs::read_to_string(path)?)
}
