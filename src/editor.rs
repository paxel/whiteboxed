//! Editor state and every user action, independent of the GUI toolkit so it can be
//! tested directly. The egui front end only maps input to these methods.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::doc::{self, DocFormat};
use crate::export::{self, ExportError};
use crate::layout::{self, Layout};
use crate::model::{
    BlockId, BlockKind, BlockSpec, Cell, DiagramId, Direction, End, ModelError, Placement, Project,
    RelationId, Rgb, Side, TagId,
};
use crate::persist::{self, PersistError};
use crate::recovery;
use crate::scene;
use crate::view::{self, ViewEnd};

/// How long after the last change the recovery file is written.
pub const AUTOSAVE_DELAY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockForm {
    pub name: String,
    pub kind: BlockKind,
    pub tag: String,
}

impl Default for BlockForm {
    fn default() -> Self {
        BlockForm {
            name: String::new(),
            kind: BlockKind::Component,
            tag: String::new(),
        }
    }
}

impl BlockForm {
    pub fn spec(&self) -> BlockSpec {
        BlockSpec {
            name: self.name.clone(),
            kind: self.kind,
            tag: Some(self.tag.clone()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectMode {
    New,
    Existing,
    Stub,
}

/// Something a relation can be connected to in the current diagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Block(BlockId),
    /// An inherited end that is not attached to a box yet.
    Dangling(RelationId, End),
    /// The open end of a stub.
    Stub(RelationId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectForm {
    pub from: BlockId,
    pub side: Side,
    pub mode: ConnectMode,
    pub block: BlockForm,
    pub target: Option<Target>,
    pub filter: String,
    pub direction: Direction,
    pub text: String,
}

/// An open end that was clicked and is waiting for a box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenEnd {
    Dangling(RelationId, End),
    Stub(RelationId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    Existing {
        from: BlockId,
        side: Side,
        target: BlockId,
        direction: Direction,
        text: String,
    },
    Stub {
        rel: RelationId,
        target: BlockId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Popup {
    AddBlock(BlockForm),
    Connect(ConnectForm),
    EditBlock(BlockId, BlockForm),
    EditRelation {
        rel: RelationId,
        direction: Direction,
        text: String,
    },
    Conflict {
        pending: Pending,
        broken: Vec<RelationId>,
    },
    ConfirmDelete {
        block: BlockId,
        count: usize,
    },
    OpenEnd {
        end: OpenEnd,
        target: Option<BlockId>,
        filter: String,
    },
    Restore(Box<Project>),
}

pub struct Editor {
    pub project: Project,
    pub diagram: DiagramId,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub selected: Option<BlockId>,
    pub popup: Option<Popup>,
    pub message: Option<String>,
    undo: Vec<Project>,
    redo: Vec<Project>,
    recovery_dir: Option<PathBuf>,
    autosave_at: Option<Instant>,
    layout_cache: Option<(Project, DiagramId, Layout)>,
}

impl Editor {
    /// A new, empty project. Offers to restore an unsaved untitled project.
    pub fn new(recovery_dir: Option<PathBuf>) -> Self {
        let mut editor = Editor {
            project: Project::new(),
            diagram: None,
            path: None,
            dirty: false,
            selected: None,
            popup: None,
            message: None,
            undo: Vec::new(),
            redo: Vec::new(),
            recovery_dir,
            autosave_at: None,
            layout_cache: None,
        };
        editor.offer_recovery();
        editor
    }

    /// Opens a project file; offers to restore newer unsaved changes of it.
    pub fn open(path: &Path, recovery_dir: Option<PathBuf>) -> Result<Self, PersistError> {
        let project = persist::load(path)?;
        let mut editor = Editor::new_without_recovery(recovery_dir);
        editor.project = project;
        editor.path = Some(path.to_path_buf());
        editor.offer_recovery();
        Ok(editor)
    }

    fn new_without_recovery(recovery_dir: Option<PathBuf>) -> Self {
        Editor {
            project: Project::new(),
            diagram: None,
            path: None,
            dirty: false,
            selected: None,
            popup: None,
            message: None,
            undo: Vec::new(),
            redo: Vec::new(),
            recovery_dir,
            autosave_at: None,
            layout_cache: None,
        }
    }

    fn recovery_file(&self) -> Option<PathBuf> {
        self.recovery_dir
            .as_ref()
            .map(|d| recovery::path_for(d, self.path.as_deref()))
    }

    fn offer_recovery(&mut self) {
        if let Some(file) = self.recovery_file()
            && let Some(found) = recovery::find(&file, self.path.as_deref())
            && found != self.project
        {
            self.popup = Some(Popup::Restore(Box::new(found)));
        }
    }

    pub fn restore(&mut self) {
        if let Some(Popup::Restore(found)) = self.popup.take() {
            self.change_to(*found);
        }
    }

    pub fn discard_recovery(&mut self) {
        self.popup = None;
        if let Some(file) = self.recovery_file() {
            recovery::remove(&file);
        }
    }

    // ----- view -----

    pub fn layout(&mut self) -> &Layout {
        let fresh = matches!(&self.layout_cache,
            Some((p, d, _)) if *p == self.project && *d == self.diagram);
        if !fresh {
            self.layout_cache = None;
        }
        let (project, diagram) = (&self.project, self.diagram);
        &self
            .layout_cache
            .get_or_insert_with(|| {
                let l = layout::layout(project, &view::diagram_view(project, diagram));
                (project.clone(), diagram, l)
            })
            .2
    }

    pub fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "untitled".into());
        let star = if self.dirty { " *" } else { "" };
        format!("whiteboxed \u{2013} {name}{star}")
    }

    /// Names from the context view down to the current diagram.
    pub fn breadcrumb(&self) -> Vec<(DiagramId, String)> {
        let mut out = vec![(None, "Context".to_owned())];
        for id in self.project.path(self.diagram) {
            if let Some(b) = self.project.blocks.get(&id) {
                out.push((Some(id), b.name.clone()));
            }
        }
        out
    }

    pub fn dangling_count(&self) -> usize {
        view::dangling_count(&view::diagram_view(&self.project, self.diagram))
    }

    // ----- navigation -----

    pub fn open_diagram(&mut self, diagram: DiagramId) {
        if let Some(id) = diagram {
            match self.project.blocks.get(&id) {
                Some(b) if b.kind.can_drill() => {}
                Some(b) => {
                    self.message = Some(ModelError::NotDrillable(b.kind.label()).to_string());
                    return;
                }
                None => return,
            }
        }
        self.diagram = diagram;
        self.selected = None;
        self.popup = None;
    }

    pub fn go_up(&mut self) {
        if let Some(id) = self.diagram {
            let parent = self.project.blocks.get(&id).and_then(|b| b.parent);
            self.open_diagram(parent);
            self.selected = Some(id);
        }
    }

    /// Shows a box in the diagram it lives in.
    pub fn reveal(&mut self, block: BlockId) {
        if let Some(b) = self.project.blocks.get(&block) {
            self.diagram = b.parent;
            self.selected = Some(block);
            self.popup = None;
        }
    }

    // ----- changes -----

    /// Runs one change on a copy of the project and keeps it only if it succeeds.
    fn apply<T>(
        &mut self,
        change: impl FnOnce(&mut Project) -> Result<T, ModelError>,
    ) -> Option<T> {
        let mut next = self.project.clone();
        match change(&mut next) {
            Ok(value) => {
                self.change_to(next);
                Some(value)
            }
            Err(e) => {
                self.message = Some(e.to_string());
                None
            }
        }
    }

    fn change_to(&mut self, next: Project) {
        if next == self.project {
            return;
        }
        let previous = std::mem::replace(&mut self.project, next);
        self.undo.push(previous);
        self.redo.clear();
        self.touched();
    }

    fn touched(&mut self) {
        self.dirty = true;
        self.message = None;
        self.autosave_at = Some(Instant::now() + AUTOSAVE_DELAY);
        self.fix_view();
    }

    /// Keeps diagram and selection pointing at things that still exist.
    fn fix_view(&mut self) {
        while let Some(id) = self.diagram {
            if self.project.blocks.contains_key(&id) {
                break;
            }
            self.diagram = None;
        }
        if self
            .selected
            .is_some_and(|b| !self.project.blocks.contains_key(&b))
        {
            self.selected = None;
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(previous) = self.undo.pop() {
            let current = std::mem::replace(&mut self.project, previous);
            self.redo.push(current);
            self.popup = None;
            self.touched();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            let current = std::mem::replace(&mut self.project, next);
            self.undo.push(current);
            self.popup = None;
            self.touched();
        }
    }

    // ----- popups -----

    pub fn close_popup(&mut self) {
        self.popup = None;
    }

    pub fn start_add_block(&mut self) {
        self.popup = Some(Popup::AddBlock(BlockForm::default()));
    }

    pub fn start_connect(&mut self, from: BlockId, side: Side) {
        self.selected = Some(from);
        self.popup = Some(Popup::Connect(ConnectForm {
            from,
            side,
            mode: ConnectMode::New,
            block: BlockForm::default(),
            target: None,
            filter: String::new(),
            direction: Direction::Out,
            text: String::new(),
        }));
    }

    pub fn start_edit_block(&mut self, block: BlockId) {
        if let Some(b) = self.project.blocks.get(&block) {
            let tag = b
                .tag
                .and_then(|t| self.project.tags.get(&t))
                .map(|t| t.name.clone())
                .unwrap_or_default();
            self.selected = Some(block);
            self.popup = Some(Popup::EditBlock(
                block,
                BlockForm {
                    name: b.name.clone(),
                    kind: b.kind,
                    tag,
                },
            ));
        }
    }

    pub fn start_edit_relation(&mut self, rel: RelationId) {
        if let Some(r) = self.project.relations.get(&rel) {
            self.popup = Some(Popup::EditRelation {
                rel,
                direction: r.direction,
                text: r.text.clone(),
            });
        }
    }

    pub fn start_open_end(&mut self, end: OpenEnd) {
        self.popup = Some(Popup::OpenEnd {
            end,
            target: None,
            filter: String::new(),
        });
    }

    /// Kinds offered in the current diagram: neighbours only in the context view.
    pub fn kinds(&self) -> Vec<BlockKind> {
        BlockKind::ALL
            .into_iter()
            .filter(|k| self.diagram.is_none() || !k.is_neighbour())
            .collect()
    }

    /// Tag names that start with what was typed so far.
    pub fn tag_suggestions(&self, typed: &str) -> Vec<String> {
        let typed = typed.trim().to_lowercase();
        self.project
            .tags
            .values()
            .map(|t| t.name.clone())
            .filter(|n| n.to_lowercase().starts_with(&typed) && n.to_lowercase() != typed)
            .collect()
    }

    /// What "connect existing" from `from` offers, filtered by name.
    pub fn connect_candidates(&self, from: BlockId, filter: &str) -> Vec<(Target, String)> {
        let filter = filter.trim().to_lowercase();
        let mut out = Vec::new();
        for (id, b) in self.project.blocks_in(self.diagram) {
            if id != from {
                out.push((Target::Block(id), b.name.clone()));
            }
        }
        let view = view::diagram_view(&self.project, self.diagram);
        for line in &view.lines {
            for end in [End::A, End::B] {
                if *line.end(end) != ViewEnd::Dangling {
                    continue;
                }
                if let ViewEnd::Frame { partner, .. } = line.end(end.other()) {
                    out.push((
                        Target::Dangling(line.relation, end),
                        with_text(&format!("{partner} (outside)"), &line.text),
                    ));
                }
            }
        }
        for (rel, r) in &self.project.relations {
            if !(r.a.is_open() ^ r.b.is_open()) {
                continue;
            }
            let Ok(near) = self.project.stub_anchor_in(*rel, self.diagram) else {
                continue;
            };
            if near.block == from {
                continue;
            }
            let name = self
                .project
                .blocks
                .get(&near.block)
                .map_or("?", |b| b.name.as_str());
            out.push((
                Target::Stub(*rel),
                with_text(&format!("{name} (open end)"), &r.text),
            ));
        }
        out.retain(|(_, label)| label.to_lowercase().contains(&filter));
        out
    }

    /// Boxes an open end can be connected to.
    pub fn open_end_candidates(&self, end: OpenEnd, filter: &str) -> Vec<(BlockId, String)> {
        let filter = filter.trim().to_lowercase();
        let near = match end {
            OpenEnd::Stub(rel) => self
                .project
                .stub_anchor_in(rel, self.diagram)
                .ok()
                .map(|a| a.block),
            OpenEnd::Dangling(..) => None,
        };
        self.project
            .blocks_in(self.diagram)
            .filter(|(id, b)| Some(*id) != near && b.name.to_lowercase().contains(&filter))
            .map(|(id, b)| (id, b.name.clone()))
            .collect()
    }

    /// Confirms whatever popup is open.
    pub fn confirm(&mut self) {
        let Some(popup) = self.popup.clone() else {
            return;
        };
        match popup {
            Popup::AddBlock(form) => {
                let diagram = self.diagram;
                if let Some(id) = self.apply(|p| p.add_block(diagram, &form.spec())) {
                    self.selected = Some(id);
                    self.popup = None;
                }
            }
            Popup::Connect(form) => self.confirm_connect(form),
            Popup::EditBlock(block, form) => {
                if self.apply(|p| p.edit_block(block, &form.spec())).is_some() {
                    self.popup = None;
                }
            }
            Popup::EditRelation {
                rel,
                direction,
                text,
            } => {
                if self
                    .apply(|p| p.edit_relation(rel, direction, &text))
                    .is_some()
                {
                    self.popup = None;
                }
            }
            Popup::Conflict { pending, .. } => self.run_pending(pending, Placement::Move),
            Popup::ConfirmDelete { block, .. } => {
                if self.apply(|p| p.delete_block(block)).is_some() {
                    self.popup = None;
                }
            }
            Popup::OpenEnd { end, target, .. } => self.confirm_open_end(end, target),
            Popup::Restore(_) => self.restore(),
        }
    }

    /// The second choice of the conflict popup: keep the box where it is.
    pub fn keep_in_place(&mut self) {
        if let Some(Popup::Conflict { pending, .. }) = self.popup.clone() {
            self.run_pending(pending, Placement::Keep);
        }
    }

    fn confirm_connect(&mut self, form: ConnectForm) {
        let ConnectForm {
            from,
            side,
            direction,
            ref text,
            ..
        } = form;
        match form.mode {
            ConnectMode::New => {
                let spec = form.block.spec();
                if let Some((id, _)) =
                    self.apply(|p| p.connect_new(from, side, &spec, direction, text))
                {
                    self.selected = Some(id);
                    self.popup = None;
                }
            }
            ConnectMode::Stub => {
                if self
                    .apply(|p| p.add_stub(from, side, direction, text))
                    .is_some()
                {
                    self.popup = None;
                }
            }
            ConnectMode::Existing => match form.target {
                None => self.message = Some("Pick a box to connect to.".into()),
                Some(Target::Block(target)) => self.request(Pending::Existing {
                    from,
                    side,
                    target,
                    direction,
                    text: text.clone(),
                }),
                Some(Target::Dangling(rel, end)) => {
                    let Some(owner) = self.diagram else { return };
                    if self
                        .apply(|p| p.attach(rel, end, owner, from, side))
                        .is_some()
                    {
                        self.popup = None;
                    }
                }
                Some(Target::Stub(rel)) => {
                    let diagram = self.diagram;
                    if self
                        .apply(|p| p.connect_stub(rel, diagram, from, Some(side), Placement::Keep))
                        .is_some()
                    {
                        self.popup = None;
                    }
                }
            },
        }
    }

    fn confirm_open_end(&mut self, end: OpenEnd, target: Option<BlockId>) {
        let Some(target) = target else {
            self.message = Some("Pick a box to connect to.".into());
            return;
        };
        match end {
            OpenEnd::Dangling(rel, which) => {
                let Some(owner) = self.diagram else { return };
                let side = self
                    .project
                    .relations
                    .get(&rel)
                    .and_then(|r| {
                        r.end(which)
                            .anchors
                            .iter()
                            .find(|a| a.block == owner)
                            .map(|a| a.side)
                    })
                    .unwrap_or(Side::Left);
                if self
                    .apply(|p| p.attach(rel, which, owner, target, side))
                    .is_some()
                {
                    self.popup = None;
                }
            }
            OpenEnd::Stub(rel) => self.request(Pending::Stub { rel, target }),
        }
    }

    /// Runs a connection, or asks first when it would move a box out of place.
    fn request(&mut self, pending: Pending) {
        let broken = match &pending {
            Pending::Existing {
                from, side, target, ..
            } => self.project.conflicts(*from, *side, *target),
            Pending::Stub { rel, target } => {
                self.project.stub_conflicts(*rel, self.diagram, *target)
            }
        };
        match broken {
            Ok(broken) if broken.is_empty() => self.run_pending(pending, Placement::Move),
            Ok(broken) => self.popup = Some(Popup::Conflict { pending, broken }),
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    fn run_pending(&mut self, pending: Pending, placement: Placement) {
        let diagram = self.diagram;
        let done = match pending {
            Pending::Existing {
                from,
                side,
                target,
                direction,
                text,
            } => self
                .apply(|p| p.connect_existing(from, side, target, direction, &text, placement))
                .is_some(),
            Pending::Stub { rel, target } => self
                .apply(|p| p.connect_stub(rel, diagram, target, None, placement))
                .is_some(),
        };
        if done {
            self.popup = None;
        }
    }

    pub fn request_delete(&mut self, block: BlockId) {
        let count = self.project.subtree(block).len().saturating_sub(1);
        if count > 0 {
            self.popup = Some(Popup::ConfirmDelete { block, count });
        } else if self.apply(|p| p.delete_block(block)).is_some() {
            self.popup = None;
        }
    }

    pub fn remove_line(&mut self, rel: RelationId) {
        let diagram = self.diagram;
        self.apply(|p| p.remove_line(rel, diagram));
    }

    pub fn move_block(&mut self, block: BlockId, cell: Cell) {
        self.apply(|p| p.move_block(block, cell));
    }

    /// Moves the selected box one cell; a box there swaps places.
    pub fn nudge(&mut self, side: Side) {
        if let Some(block) = self.selected
            && let Some(b) = self.project.blocks.get(&block)
        {
            let cell = b.cell.toward(side);
            self.move_block(block, cell);
        }
    }

    pub fn set_tag_color(&mut self, tag: TagId, color: Rgb) {
        self.apply(|p| p.set_tag_color(tag, color));
    }

    // ----- files -----

    pub fn save(&mut self) -> Result<bool, PersistError> {
        match self.path.clone() {
            Some(path) => {
                self.save_as(&path)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn save_as(&mut self, path: &Path) -> Result<(), PersistError> {
        let old_recovery = self.recovery_file();
        persist::save(path, &self.project)?;
        if let Some(file) = old_recovery {
            recovery::remove(&file);
        }
        self.path = Some(path.to_path_buf());
        self.dirty = false;
        self.autosave_at = None;
        if let Some(file) = self.recovery_file() {
            recovery::remove(&file);
        }
        Ok(())
    }

    /// Writes the recovery file once the autosave delay has passed. Returns when it
    /// should be called again, if at all.
    pub fn tick(&mut self, now: Instant) -> Option<Duration> {
        let due = self.autosave_at?;
        if now < due {
            return Some(due - now);
        }
        self.autosave_at = None;
        if self.dirty
            && let Some(file) = self.recovery_file()
            && let Err(e) = recovery::write(&file, &self.project)
        {
            self.message = Some(format!("Autosave failed: {e}"));
        }
        None
    }

    /// Drops the recovery file when the user leaves without wanting the changes.
    pub fn forget_unsaved(&mut self) {
        self.autosave_at = None;
        if let Some(file) = self.recovery_file() {
            recovery::remove(&file);
        }
    }

    // ----- export -----

    pub fn export_svg(&mut self, path: &Path) -> Result<(), ExportError> {
        let svg = export::to_svg(&scene::scene(self.layout()));
        std::fs::write(path, svg)?;
        Ok(())
    }

    pub fn export_png(&mut self, path: &Path) -> Result<(), ExportError> {
        let png = export::to_png(&scene::scene(self.layout()), 2.0)?;
        std::fs::write(path, png)?;
        Ok(())
    }

    /// Exports the context view and every whitebox with content into `dir`: an SVG
    /// and a PNG each, a text file each in `format`, and an index over the texts.
    /// Returns the number of diagrams written.
    pub fn export_all(&self, dir: &Path, format: DocFormat) -> Result<usize, ExportError> {
        let diagrams = doc::diagrams(&self.project);
        let ext = format.extension();
        let mut entries = Vec::new();
        for diagram in &diagrams {
            let l = layout::layout(&self.project, &view::diagram_view(&self.project, *diagram));
            let s = scene::scene(&l);
            let name = self.file_name_for(*diagram);
            std::fs::write(dir.join(format!("{name}.svg")), export::to_svg(&s))?;
            std::fs::write(dir.join(format!("{name}.png")), export::to_png(&s, 2.0)?)?;
            let text = doc::diagram_doc(&self.project, *diagram, &format!("{name}.svg"), format);
            std::fs::write(dir.join(format!("{name}.{ext}")), text)?;
            entries.push((*diagram, name));
        }
        let title = self.path.as_ref().and_then(|p| p.file_stem()).map_or_else(
            || "Architecture".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        let index = doc::index_doc(&self.project, &title, &entries, format);
        std::fs::write(dir.join(format!("index.{ext}")), index)?;
        Ok(diagrams.len())
    }

    /// File name for a diagram: its breadcrumb, e.g. `context - Shop - Orders`.
    pub fn file_name_for(&self, diagram: DiagramId) -> String {
        let mut parts = vec!["context".to_owned()];
        for id in self.project.path(diagram) {
            if let Some(b) = self.project.blocks.get(&id) {
                parts.push(b.name.clone());
            }
        }
        parts
            .join(" - ")
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || " -_.".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }
}

fn with_text(label: &str, text: &str) -> String {
    if text.is_empty() {
        label.to_owned()
    } else {
        format!("{label}: {text}")
    }
}
