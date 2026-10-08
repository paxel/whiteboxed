//! Editor state and every user action, independent of the GUI toolkit so it can be
//! tested directly. The egui front end only maps input to these methods.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::doc::{self, DocFormat};
use crate::export::{self, ExportError};
use crate::layout::{self, Layout};
use crate::model::{
    BlockId, BlockKind, BlockSpec, Cell, DiagramId, Direction, End, LineStyle, ModelError,
    Placement, Project, RelationId, Rgb, Side, TagId,
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
    /// Optional short label shown on the line instead of `text`.
    pub short: String,
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
        short: String,
    },
    Stub {
        rel: RelationId,
        target: BlockId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Popup {
    /// A new box, in the given cell or else in the next free one.
    AddBlock(BlockForm, Option<Cell>),
    Connect(ConnectForm),
    EditBlock(BlockId, BlockForm),
    EditRelation {
        rel: RelationId,
        direction: Direction,
        text: String,
        /// `None` follows the project's line style.
        style: Option<LineStyle>,
        short: String,
        /// The boxes of this diagram the line touches, with the side it leaves them on.
        sides: Vec<(BlockId, Side)>,
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
    /// What an AI client changed last, for the status bar and "Follow AI".
    pub last_ai: Option<AiAction>,
    /// Folders the user allowed AI exports into, this session.
    pub ai_export_roots: Vec<PathBuf>,
    /// A folder an AI client wants to export into, waiting for the user's answer.
    pub ai_export_request: Option<PathBuf>,
}

/// One change made through the AI interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAction {
    pub summary: String,
    /// The diagram the change happened in.
    pub diagram: DiagramId,
    /// The box it was about, if any.
    pub block: Option<BlockId>,
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
            last_ai: None,
            ai_export_roots: Vec::new(),
            ai_export_request: None,
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
            last_ai: None,
            ai_export_roots: Vec::new(),
            ai_export_request: None,
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

    /// The project's name for people: its own name (see [`Project::display_name`]),
    /// else the file name without extension, or "untitled" before the first save.
    pub fn display_name(&self) -> String {
        self.project
            .display_name()
            .or_else(|| self.file_stem())
            .unwrap_or_else(|| "untitled".into())
    }

    fn file_stem(&self) -> Option<String> {
        self.path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|n| n.to_string_lossy().into_owned())
    }

    pub fn title(&self) -> String {
        let star = if self.dirty { " *" } else { "" };
        format!("whiteboxed \u{2013} {}{star}", self.display_name())
    }

    /// The file name offered on the first save: the project's name, if it has one.
    pub fn suggested_file_name(&self) -> String {
        match self.project.display_name() {
            Some(name) => format!("{}.yaml", safe_file_name(&name)),
            None => "architecture.yaml".into(),
        }
    }

    pub fn set_project_name(&mut self, name: &str) {
        self.apply(|p| p.set_name(name));
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

    /// Like a user action, one undo step, but the error goes back to the caller (an
    /// AI client) instead of the status bar.
    pub fn apply_ai<T>(
        &mut self,
        change: impl FnOnce(&mut Project) -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        let mut next = self.project.clone();
        let value = change(&mut next)?;
        self.change_to(next);
        Ok(value)
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
        self.popup = Some(Popup::AddBlock(BlockForm::default(), None));
    }

    /// Asks for a new box in `cell` (the empty cell the user right-clicked).
    pub fn start_add_block_at(&mut self, cell: Cell) {
        self.popup = Some(Popup::AddBlock(BlockForm::default(), Some(cell)));
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
            short: String::new(),
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
            let here = |b: &BlockId| {
                self.project
                    .blocks
                    .get(b)
                    .is_some_and(|b| b.parent == self.diagram)
            };
            let sides = [&r.a, &r.b]
                .into_iter()
                .flat_map(|e| e.anchors.iter())
                .filter(|a| here(&a.block))
                .map(|a| (a.block, a.side))
                .collect();
            self.popup = Some(Popup::EditRelation {
                rel,
                direction: r.direction,
                text: r.text.clone(),
                style: r.style,
                short: r.short.clone(),
                sides,
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
            Popup::AddBlock(form, cell) => {
                let diagram = self.diagram;
                let added = self.apply(|p| match cell {
                    Some(cell) => p.add_block_at(diagram, &form.spec(), cell),
                    None => p.add_block(diagram, &form.spec()),
                });
                if let Some(id) = added {
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
                style,
                short,
                sides,
            } => {
                if self
                    .apply(|p| {
                        p.edit_relation(rel, direction, &text)?;
                        p.set_relation_short(rel, &short)?;
                        for (block, side) in &sides {
                            p.set_side_at(rel, *block, *side)?;
                        }
                        p.set_relation_style(rel, style)
                    })
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
            ref short,
            ..
        } = form;
        match form.mode {
            ConnectMode::New => {
                let spec = form.block.spec();
                if let Some((id, _)) = self.apply(|p| {
                    let (id, rel) = p.connect_new(from, side, &spec, direction, text)?;
                    p.set_relation_short(rel, short)?;
                    Ok((id, rel))
                }) {
                    self.selected = Some(id);
                    self.popup = None;
                }
            }
            ConnectMode::Stub => {
                if self
                    .apply(|p| {
                        let rel = p.add_stub(from, side, direction, text)?;
                        p.set_relation_short(rel, short)
                    })
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
                    short: short.clone(),
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
                short,
            } => self
                .apply(|p| {
                    let rel =
                        p.connect_existing(from, side, target, direction, &text, placement)?;
                    p.set_relation_short(rel, &short)
                })
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

    pub fn set_responsibility(&mut self, block: BlockId, text: &str) {
        self.apply(|p| p.set_responsibility(block, text));
    }

    pub fn set_motivation(&mut self, diagram: DiagramId, text: &str) {
        self.apply(|p| p.set_motivation(diagram, text));
    }

    /// Changes when relation texts become numbers (one undo step).
    pub fn set_label_limit(&mut self, limit: Option<u32>) {
        self.apply(|p| {
            p.set_label_limit(limit);
            Ok(())
        });
    }

    /// Changes the project's line style (one undo step).
    pub fn set_line_style(&mut self, style: LineStyle) {
        self.apply(|p| {
            p.set_line_style(style);
            Ok(())
        });
    }

    /// Gives a fresh, untouched project the user's preferred starting values.
    pub fn apply_prefs(&mut self, prefs: &crate::prefs::Prefs) {
        if self.path.is_none() && !self.dirty && self.project.blocks.is_empty() {
            self.project.line_style = prefs.line_style;
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
        write_file(path, svg)?;
        Ok(())
    }

    pub fn export_png(&mut self, path: &Path) -> Result<(), ExportError> {
        let png = export::to_png(&scene::scene(self.layout()), 2.0)?;
        write_file(path, png)?;
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
            write_file(&dir.join(format!("{name}.svg")), export::to_svg(&s))?;
            write_file(&dir.join(format!("{name}.png")), export::to_png(&s, 2.0)?)?;
            let text = doc::diagram_doc(&self.project, *diagram, &format!("{name}.svg"), format);
            write_file(&dir.join(format!("{name}.{ext}")), text)?;
            entries.push((*diagram, name));
        }
        let title = self
            .project
            .display_name()
            .or_else(|| self.file_stem())
            .unwrap_or_else(|| "Architecture".to_owned());
        let index = doc::index_doc(&self.project, &title, &entries, format);
        write_file(&dir.join(format!("index.{ext}")), index)?;
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
        safe_file_name(&parts.join(" - "))
    }
}

/// `name` with everything but letters, digits, spaces and `-_.` replaced by `_`.
fn safe_file_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Writes a file, but never through a symlink: an export must not land elsewhere.
fn write_file(path: &Path, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(std::io::Error::other(format!(
            "{} is a symbolic link; whiteboxed does not write through links",
            path.display()
        )));
    }
    std::fs::write(path, bytes)
}

/// An absolute folder with every existing part resolved (symlinks included), so
/// it can be compared with allowed folders. `..` is refused outright.
pub fn resolve_folder(dir: &Path) -> std::io::Result<PathBuf> {
    use std::path::Component;
    if !dir.is_absolute() {
        return Err(std::io::Error::other("the folder must be an absolute path"));
    }
    if dir.components().any(|c| c == Component::ParentDir) {
        return Err(std::io::Error::other("the folder must not contain .."));
    }
    let mut existing = dir.to_path_buf();
    let mut rest = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name().map(|n| n.to_owned()) else {
            break;
        };
        rest.push(name);
        if !existing.pop() {
            break;
        }
    }
    let mut out = std::fs::canonicalize(&existing)?;
    for name in rest.into_iter().rev() {
        out.push(name);
    }
    Ok(out)
}

impl Editor {
    /// Lets AI clients export into `dir` and below for the rest of the session.
    pub fn allow_ai_export(&mut self, dir: &Path) -> std::io::Result<()> {
        let root = resolve_folder(dir)?;
        if !self.ai_export_roots.contains(&root) {
            self.ai_export_roots.push(root);
        }
        Ok(())
    }

    /// Whether an AI export into the resolved folder `dir` is allowed.
    pub fn ai_export_allowed(&self, dir: &Path) -> bool {
        self.ai_export_roots
            .iter()
            .any(|root| dir.starts_with(root))
    }
}

fn with_text(label: &str, text: &str) -> String {
    if text.is_empty() {
        label.to_owned()
    } else {
        format!("{label}: {text}")
    }
}
