//! The egui front end: menu, breadcrumb, structure tree, canvas and popups.

pub mod ai;
pub mod canvas;
pub mod details;
pub mod export;
pub mod icons;
pub mod leave;
pub mod popups;
pub mod settings;

use std::path::{Path, PathBuf};
use std::time::Instant;

use egui::{
    Button, Color32, CursorIcon, Key, KeyboardShortcut, Modifiers, PointerButton, Pos2, RichText,
    Sense, Stroke, Ui, ViewportCommand, vec2,
};

use crate::editor::{Editor, Popup, Target};
use crate::geom;
use crate::hit::{self, Hit};
use crate::layout::Layout;
use crate::model::{BlockId, Cell, DiagramId, PALETTE, Side};
use crate::recovery;
use crate::scene;
use canvas::{ACCENT, View};
use popups::Action;

pub const APP_ID: &str = "io.github.paxel.whiteboxed";

/// Opens the window; `path` is a project file to open.
pub fn run(path: Option<PathBuf>) -> eframe::Result {
    let dir = recovery::default_dir();
    let mut message = None;
    let editor = match path {
        Some(p) => Editor::open(&p, dir.clone()).unwrap_or_else(|e| {
            message = Some(format!("Cannot open {}: {e}", p.display()));
            Editor::new(dir.clone())
        }),
        None => Editor::new(dir.clone()),
    };
    let mut app = App::new(editor, dir)
        .with_ai_dir(crate::mcp::settings::default_dir())
        .with_prefs_dir(crate::mcp::settings::default_dir());
    app.editor.message = message;
    if cfg!(target_os = "linux") {
        std::thread::spawn(crate::launcher::register);
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("whiteboxed")
        .with_app_id(APP_ID)
        .with_inner_size([1280.0, 800.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(crate::launcher::ICON) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "whiteboxed",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_theme(egui::Theme::Light);
            Ok(Box::new(app))
        }),
    )
}

/// Something that needs the current project out of the way first.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Leave {
    Quit,
    New,
    Open(PathBuf),
}

pub struct App {
    pub editor: Editor,
    recovery_dir: Option<PathBuf>,
    view: Option<Shown>,
    /// A zoom step asked for by a key, the menu or the zoom buttons.
    zoom_request: Option<ZoomStep>,
    canvas_rect: egui::Rect,
    anchor: Pos2,
    focus: bool,
    context: Option<Hit>,
    /// The grid cell under the last right-click.
    context_cell: Option<Cell>,
    drag: Option<BlockId>,
    picking: bool,
    /// A box waiting for the user to click the box it should move into.
    moving_into: Option<BlockId>,
    leave: Option<Leave>,
    allow_close: bool,
    shown_title: String,
    /// Popup kind and anchor shown last frame, to place a newly opened popup.
    shown_popup: Option<(std::mem::Discriminant<Popup>, Pos2)>,
    popup_serial: u64,
    draft: Option<details::Draft>,
    pub ai: ai::Ai,
    pub settings: settings::Settings,
    pub export: export::ExportDialog,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

impl App {
    pub fn new(editor: Editor, recovery_dir: Option<PathBuf>) -> Self {
        App {
            editor,
            recovery_dir,
            view: None,
            zoom_request: None,
            canvas_rect: egui::Rect::NOTHING,
            anchor: Pos2::new(200.0, 150.0),
            focus: true,
            context: None,
            context_cell: None,
            drag: None,
            picking: false,
            moving_into: None,
            leave: None,
            allow_close: false,
            shown_title: String::new(),
            shown_popup: None,
            popup_serial: 0,
            draft: None,
            ai: ai::Ai::new(None),
            settings: settings::Settings::new(None),
            export: export::ExportDialog::default(),
        }
    }

    /// Where personal preferences live; a fresh project takes its starting values.
    pub fn with_prefs_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.settings = settings::Settings::new(dir);
        self.editor.apply_prefs(&self.settings.prefs);
        self
    }

    /// Where the AI access settings (port, token) are stored; `None` disables AI access.
    pub fn with_ai_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.ai = ai::Ai::new(dir);
        self
    }

    /// The current diagram-to-screen transform, once the canvas was drawn.
    pub fn view(&self) -> Option<View> {
        self.view.as_ref().map(|s| s.view)
    }

    /// Where the canvas was drawn in the last frame.
    pub fn canvas_rect(&self) -> egui::Rect {
        self.canvas_rect
    }

    /// Whether the window is about to close (the user confirmed quitting).
    pub fn is_closing(&self) -> bool {
        self.allow_close
    }

    pub fn is_picking(&self) -> bool {
        self.picking
    }

    /// Waiting for a click on a box: to connect to, or to move into.
    fn picks(&self) -> bool {
        self.picking || self.moving_into.is_some()
    }

    /// Draws the whole window into `ui`.
    pub fn show(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        // Ctrl+= and Ctrl+− zoom the diagram, not the whole window.
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        self.handle_close(&ctx);
        self.shortcuts(&ctx);
        if let Some(action) = self.ai.pump(&mut self.editor)
            && self.ai.follow
        {
            self.follow(&action);
        }
        if let Some(wait) = self.editor.tick(Instant::now()) {
            ctx.request_repaint_after(wait);
        }
        let title = self.editor.title();
        if title != self.shown_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.shown_title = title;
        }

        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        egui::Panel::top("breadcrumb").show(ui, |ui| self.breadcrumb(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status(ui));
        egui::Panel::left("structure")
            .resizable(true)
            .default_size(230.0)
            .show(ui, |ui| self.sidebar(ui));
        egui::Panel::right("details")
            .resizable(true)
            .default_size(260.0)
            .show(ui, |ui| {
                details::show(ui, &mut self.editor, &mut self.draft)
            });
        egui::CentralPanel::default().show(ui, |ui| self.canvas(ui));
        self.popup(&ctx);
        self.ai.dialog(&ctx, &mut self.editor);
        self.settings.show(&ctx, &mut self.editor);
        self.export.show(&ctx, &mut self.editor);
        self.ai.export_prompt(&ctx, &mut self.editor);
        self.leave_dialog(&ctx);
    }

    // ----- window and keyboard -----

    fn handle_close(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        self.commit_draft();
        if self.editor.dirty && !self.allow_close {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.leave = Some(Leave::Quit);
            // Draw the question right away, even if nothing else moves.
            ctx.request_repaint();
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let pressed = |m: Modifiers, k: Key| {
            ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)))
        };
        let shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let no_popup = self.editor.popup.is_none() && self.leave.is_none();
        // Inside a text field, Ctrl+Z belongs to the field.
        if no_popup && !ctx.text_edit_focused() {
            if pressed(shift, Key::Z) || pressed(Modifiers::COMMAND, Key::Y) {
                self.redo();
            } else if pressed(Modifiers::COMMAND, Key::Z) {
                self.undo();
            }
        }
        if pressed(shift, Key::S) {
            self.save_as();
        } else if pressed(Modifiers::COMMAND, Key::S) {
            self.save();
        }
        if pressed(Modifiers::COMMAND, Key::O) {
            self.open();
        }
        if pressed(Modifiers::COMMAND, Key::N) {
            self.new_project();
        }
        if !ctx.text_edit_focused() {
            if pressed(Modifiers::COMMAND, Key::Equals) || pressed(Modifiers::COMMAND, Key::Plus) {
                self.zoom_request = Some(ZoomStep::In);
            } else if pressed(Modifiers::COMMAND, Key::Minus) {
                self.zoom_request = Some(ZoomStep::Out);
            } else if pressed(Modifiers::COMMAND, Key::Num0) {
                self.zoom_request = Some(ZoomStep::Actual);
            }
        }
        if self.picks() {
            if pressed(Modifiers::NONE, Key::Escape) {
                self.picking = false;
                self.moving_into = None;
            }
            return;
        }
        if !no_popup || ctx.text_edit_focused() {
            return;
        }
        if pressed(Modifiers::NONE, Key::Backspace) {
            self.editor.go_up();
        }
        if pressed(Modifiers::NONE, Key::Escape) {
            self.editor.selected = None;
            self.editor.also_selected.clear();
        }
        let Some(selected) = self.editor.selected else {
            return;
        };
        if pressed(Modifiers::NONE, Key::Delete) {
            self.editor.request_delete(selected);
        }
        if pressed(Modifiers::NONE, Key::F2) {
            self.focus = true;
            self.editor.start_edit_block(selected);
        }
        if pressed(Modifiers::NONE, Key::Enter) {
            self.editor.open_diagram(Some(selected));
        }
        for (key, side) in [
            (Key::ArrowUp, Side::Top),
            (Key::ArrowRight, Side::Right),
            (Key::ArrowDown, Side::Bottom),
            (Key::ArrowLeft, Side::Left),
        ] {
            if pressed(Modifiers::NONE, key) {
                self.editor.nudge(side);
            }
        }
    }

    // ----- panels -----

    fn menu(&mut self, ui: &mut Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    self.new_project();
                }
                if ui.button("Open\u{2026}").clicked() {
                    self.open();
                }
                if ui.button("Save").clicked() {
                    self.save();
                }
                if ui.button("Save as\u{2026}").clicked() {
                    self.save_as();
                }
                ui.separator();
                if ui.button("Project settings\u{2026}").clicked() {
                    self.settings.open = true;
                }
                ui.separator();
                if ui.button("Export\u{2026}").clicked() {
                    self.commit_draft();
                    self.export.start(&self.editor);
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui
                    .add_enabled(self.editor.can_undo(), Button::new("Undo"))
                    .clicked()
                {
                    self.undo();
                }
                if ui
                    .add_enabled(self.editor.can_redo(), Button::new("Redo"))
                    .clicked()
                {
                    self.redo();
                }
            });
            ui.menu_button("View", |ui| {
                if ui.button("Fit to window").clicked() {
                    self.view = None;
                }
                if ui
                    .add(Button::new("Zoom in").shortcut_text("Ctrl+="))
                    .clicked()
                {
                    self.zoom_request = Some(ZoomStep::In);
                }
                if ui
                    .add(Button::new("Zoom out").shortcut_text("Ctrl+\u{2212}"))
                    .clicked()
                {
                    self.zoom_request = Some(ZoomStep::Out);
                }
                if ui
                    .add(Button::new("Actual size (100 %)").shortcut_text("Ctrl+0"))
                    .clicked()
                {
                    self.zoom_request = Some(ZoomStep::Actual);
                }
                if ui
                    .add_enabled(self.editor.diagram.is_some(), Button::new("Up one level"))
                    .clicked()
                {
                    self.editor.go_up();
                }
            });
            ui.menu_button("AI", |ui| {
                let mut on = self.ai.is_on();
                if ui.checkbox(&mut on, "Allow AI access").clicked() {
                    if on {
                        self.ai.start(ui.ctx());
                    } else {
                        self.ai.turn_off();
                    }
                }
                ui.checkbox(&mut self.ai.follow, "Follow AI");
                if ui.button("Connection\u{2026}").clicked() {
                    self.ai.dialog = true;
                }
            });
            ui.add_space(12.0);
            let can_undo = self.editor.can_undo();
            if icons::history_button(ui, icons::History::Undo, can_undo).clicked() {
                self.undo();
            }
            let can_redo = self.editor.can_redo();
            if icons::history_button(ui, icons::History::Redo, can_redo).clicked() {
                self.redo();
            }
            ui.add_space(12.0);
            let add = ui
                .button("+ Box")
                .on_hover_text("Add a box to this diagram, in the next free cell");
            if add.clicked() {
                self.anchor = add.rect.left_bottom() + vec2(0.0, 8.0);
                self.focus = true;
                self.editor.start_add_block();
            }
        });
    }

    /// Writes the text being typed in the details panel into the project.
    fn commit_draft(&mut self) {
        details::commit(&mut self.editor, &self.draft);
        self.draft = None;
    }

    fn undo(&mut self) {
        self.commit_draft();
        self.editor.undo();
    }

    fn redo(&mut self) {
        self.commit_draft();
        self.editor.redo();
    }

    fn breadcrumb(&mut self, ui: &mut Ui) {
        let crumbs = self.editor.breadcrumb();
        let last = crumbs.len() - 1;
        let mut go = None;
        ui.horizontal(|ui| {
            for (i, (diagram, name)) in crumbs.into_iter().enumerate() {
                if i > 0 {
                    ui.label("\u{203a}");
                }
                let text = if i == last {
                    RichText::new(name).strong()
                } else {
                    RichText::new(name)
                };
                if ui.add(Button::new(text).frame(false)).clicked() {
                    go = Some(diagram);
                }
            }
            if self.editor.diagram.is_some() {
                ui.label(
                    RichText::new(format!(
                        "level {}",
                        self.editor.project.level(self.editor.diagram)
                    ))
                    .weak(),
                );
            }
            let diagram = self.editor.diagram;
            if let Some(s) = self.editor.scores().get(&diagram).cloned() {
                score_dot(ui, &s);
            }
        });
        if let Some(diagram) = go {
            self.editor.open_diagram(diagram);
        }
    }

    fn status(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            match &self.editor.message {
                Some(m) if self.editor.popup.is_none() => {
                    ui.label(RichText::new(m).color(Color32::from_rgb(0xc6, 0x28, 0x28)));
                }
                _ if self.picking => {
                    ui.label("Click the box to connect to. Esc cancels.");
                }
                _ if self.moving_into.is_some() => {
                    ui.label("Click the box to move it into. Esc cancels.");
                }
                _ => {
                    ui.label(
                        RichText::new(
                            "Click a box border to add a relation \u{b7} double-click a box to open it \u{b7} right-click for more",
                        )
                        .weak(),
                    );
                }
            }
            let dangling = self.editor.dangling_count();
            let mut go = None;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if dangling > 0 {
                    ui.label(
                        RichText::new(format!(
                            "{dangling} interface{} not assigned to a box",
                            if dangling == 1 { "" } else { "s" }
                        ))
                        .color(Color32::from_rgb(0xb0, 0x6a, 0x00)),
                    );
                }
                if self.ai.is_on() {
                    if let Some(last) = &self.editor.last_ai {
                        let link = Button::new(RichText::new(format!("AI: {}", last.summary)))
                            .frame(false);
                        if ui.add(link).on_hover_text("Show this change").clicked() {
                            go = Some(last.clone());
                        }
                    }
                    if ui
                        .add(Button::new(RichText::new("AI access on").color(ACCENT)).frame(false))
                        .clicked()
                    {
                        self.ai.dialog = true;
                    }
                }
            });
            if let Some(action) = go {
                self.follow(&action);
            }
        });
    }

    /// Shows the diagram and box an AI change was about.
    fn follow(&mut self, action: &crate::editor::AiAction) {
        let exists = |d: DiagramId| d.is_none_or(|id| self.editor.project.blocks.contains_key(&id));
        if exists(action.diagram) {
            self.commit_draft();
            self.editor.diagram = action.diagram;
            self.editor.selected = action
                .block
                .filter(|b| self.editor.project.blocks.contains_key(b));
        }
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        enum TreeAct {
            Open(DiagramId),
            Reveal(BlockId),
            Color(crate::model::TagId, crate::model::Rgb),
        }
        let mut act = None;
        let scores = self.editor.scores().clone();
        ui.heading("Structure");
        egui::ScrollArea::vertical()
            .id_salt("tree")
            .max_height(ui.available_height() * 0.65)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let root = ui.selectable_label(self.editor.diagram.is_none(), "Context");
                    if root.clicked() {
                        act = Some(TreeAct::Open(None));
                    }
                    if let Some(s) = scores.get(&None) {
                        score_dot(ui, s);
                    }
                });
                tree(ui, &self.editor, &scores, None, 1, &mut act_tree(&mut act));
            });
        ui.separator();
        ui.heading("Tags");
        if self.editor.project.tags.is_empty() {
            ui.label(RichText::new("Type a tag name when adding or editing a box.").weak());
        }
        for (id, tag) in &self.editor.project.tags {
            ui.horizontal(|ui| {
                let swatch = Button::new("      ").fill(canvas::color(tag.color));
                egui::containers::menu::MenuButton::from_button(swatch).ui(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for c in PALETTE {
                            if ui.add(Button::new("    ").fill(canvas::color(c))).clicked() {
                                act = Some(TreeAct::Color(*id, c));
                            }
                        }
                    });
                });
                ui.label(&tag.name);
            });
        }

        fn act_tree(act: &mut Option<TreeAct>) -> impl FnMut(Option<BlockId>, bool) + '_ {
            move |block, open| {
                *act = Some(match (block, open) {
                    (Some(b), false) => TreeAct::Reveal(b),
                    (b, _) => TreeAct::Open(b),
                });
            }
        }

        fn tree(
            ui: &mut Ui,
            editor: &Editor,
            scores: &std::collections::BTreeMap<DiagramId, crate::score::Score>,
            diagram: DiagramId,
            depth: usize,
            on: &mut impl FnMut(Option<BlockId>, bool),
        ) {
            let mut blocks: Vec<_> = editor.project.blocks_in(diagram).collect();
            blocks.sort_by_key(|(_, b)| b.name.to_lowercase());
            for (id, b) in blocks {
                ui.horizontal(|ui| {
                    ui.add_space(depth as f32 * 14.0);
                    let current = editor.diagram == Some(id) || editor.selected == Some(id);
                    let r = ui.selectable_label(current, &b.name);
                    if r.double_clicked() && b.kind.can_drill() {
                        on(Some(id), true);
                    } else if r.clicked() {
                        on(Some(id), false);
                    }
                    if let Some(s) = scores.get(&Some(id)) {
                        score_dot(ui, s);
                    }
                });
                if editor.project.has_content(id) {
                    tree(ui, editor, scores, Some(id), depth + 1, on);
                }
            }
        }

        match act {
            Some(TreeAct::Open(d)) => self.editor.open_diagram(d),
            Some(TreeAct::Reveal(b)) => self.editor.reveal(b),
            Some(TreeAct::Color(t, c)) => self.editor.set_tag_color(t, c),
            None => {}
        }
    }

    // ----- canvas -----

    fn canvas(&mut self, ui: &mut Ui) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::WHITE);
        let layout = self.editor.layout().clone();
        let diagram = self.editor.diagram;

        self.canvas_rect = rect;
        // The whole diagram is fitted only when it opens and on Fit; after edits the
        // view stays, and the picture stays put where the grid shifted under it.
        let mut view = match &self.view {
            Some(shown) if shown.diagram == diagram => {
                let mut v = shown.view;
                let d = drift(&shown.blocks, &layout);
                v.origin -= vec2(d.x, d.y) * v.zoom;
                for b in &layout.blocks {
                    if !shown.blocks.iter().any(|(id, _)| *id == b.id) {
                        v.origin += into_view(rect, v.rect(b.rect));
                    }
                }
                v
            }
            _ => fit(rect, &layout),
        };
        let wanted = match self.zoom_request.take() {
            Some(ZoomStep::In) => Some(view.zoom * ZOOM_STEP),
            Some(ZoomStep::Out) => Some(view.zoom / ZOOM_STEP),
            Some(ZoomStep::Actual) => Some(1.0),
            None => None,
        };
        if let Some(zoom) = wanted {
            view = zoomed(view, rect.center(), zoom);
        }
        if resp.hovered() {
            let (zoom, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
            if zoom != 1.0
                && let Some(p) = resp.hover_pos()
            {
                view = zoomed(view, p, view.zoom * zoom);
            }
            view.origin += scroll;
        }

        let project = &self.editor.project;
        let hit_with = |v: View, p: Pos2| hit::hit(project, &layout, v.diagram(p), v.zoom);
        if resp.drag_started_by(PointerButton::Primary) {
            let start = ui.input(|i| i.pointer.press_origin());
            self.drag = match start.map(|p| hit_with(view, p)) {
                // Bands stay at the bottom; they cannot be dragged.
                Some(Hit::Block(b))
                    if !self.picks() && project.blocks.get(&b).is_some_and(|x| !x.band) =>
                {
                    Some(b)
                }
                _ => None,
            };
        }
        if self.drag.is_none() && resp.dragged() {
            view.origin += resp.drag_delta();
        }
        let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
        let mut moved = None;
        if resp.drag_stopped()
            && let Some(b) = self.drag.take()
            && let Some(p) = pointer
            && let Some(cell) = hit::cell_at(&layout, view.diagram(p))
            && project.blocks.get(&b).is_some_and(|x| x.cell != cell)
        {
            moved = Some((b, cell));
        }
        self.view = Some(Shown {
            diagram,
            view,
            blocks: layout.blocks.iter().map(|b| (b.id, b.rect)).collect(),
        });
        let hit_at = |p: Pos2| hit_with(view, p);

        canvas::paint(&painter, view, &scene::scene(&layout));
        self.decorate(&painter, &layout, view, &resp, &hit_at);

        let clicked = resp
            .clicked()
            .then(|| resp.interact_pointer_pos())
            .flatten()
            .map(|p| (p, hit_at(p)));
        let double = resp
            .double_clicked()
            .then(|| resp.interact_pointer_pos())
            .flatten()
            .map(hit_at);
        let secondary = resp
            .secondary_clicked()
            .then(|| resp.interact_pointer_pos())
            .flatten()
            .map(|p| (p, hit_at(p)));

        // The full text of a shortened relation, at the pointer.
        let full = resp.hover_pos().and_then(|p| match hit_at(p) {
            Hit::Line(rel, _) => layout
                .lines
                .iter()
                .find(|l| l.relation == rel && l.text != l.full_text)
                .map(|l| l.full_text.clone()),
            _ => None,
        });
        if let Some((b, cell)) = moved {
            self.editor.move_block(b, cell);
        }
        if let Some((p, h)) = clicked {
            // The release that made the click carries its own modifiers.
            let ctrl = ui.input(|i| {
                i.modifiers.command
                    || i.events.iter().any(|e| {
                        matches!(e, egui::Event::PointerButton { pressed: false, modifiers, .. }
                            if modifiers.command)
                    })
            });
            self.click(p, h, ctrl);
        }
        if let Some(h) = double {
            self.double_click(h);
        }
        if let Some((p, h)) = secondary {
            self.anchor = p;
            self.context = Some(h);
            self.context_cell = hit::cell_at(&layout, view.diagram(p));
        }
        let resp = match full {
            Some(text) => resp.on_hover_text_at_pointer(text),
            None => resp,
        };
        resp.context_menu(|ui| self.context_menu(ui));
        self.zoom_buttons(ui, rect, view.zoom);

        if layout.blocks.is_empty() {
            let button = egui::Rect::from_center_size(rect.center(), vec2(150.0, 40.0));
            let add = ui.put(button, Button::new(RichText::new("+ add box").size(16.0)));
            if add.clicked() {
                self.anchor = rect.center() + vec2(-120.0, 40.0);
                self.focus = true;
                self.editor.start_add_block();
            }
        }
    }

    /// −, the zoom level (click: 100 %), + and Fit in the lower right corner.
    fn zoom_buttons(&mut self, ui: &mut Ui, canvas: egui::Rect, zoom: f32) {
        let size = vec2(196.0, 30.0);
        let at = egui::Rect::from_min_size(canvas.right_bottom() - size - vec2(12.0, 12.0), size);
        let mut step = None;
        let mut fit = false;
        ui.scope_builder(egui::UiBuilder::new().max_rect(at), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button("\u{2212}")
                        .on_hover_text("Zoom out (Ctrl+\u{2212}, or Ctrl + mouse wheel)")
                        .clicked()
                    {
                        step = Some(ZoomStep::Out);
                    }
                    let level =
                        Button::new(format!("{:.0} %", zoom * 100.0)).min_size(vec2(52.0, 0.0));
                    if ui
                        .add(level)
                        .on_hover_text("Actual size, 100 % (Ctrl+0)")
                        .clicked()
                    {
                        step = Some(ZoomStep::Actual);
                    }
                    if ui
                        .button("+")
                        .on_hover_text("Zoom in (Ctrl+=, or Ctrl + mouse wheel)")
                        .clicked()
                    {
                        step = Some(ZoomStep::In);
                    }
                    if ui
                        .button("Fit")
                        .on_hover_text("Show the whole diagram (View > Fit to window)")
                        .clicked()
                    {
                        fit = true;
                    }
                });
            });
        });
        if step.is_some() {
            self.zoom_request = step;
            ui.ctx().request_repaint();
        }
        if fit {
            self.view = None;
            ui.ctx().request_repaint();
        }
    }

    /// Hover feedback, selection, picking and drag preview.
    fn decorate(
        &self,
        painter: &egui::Painter,
        layout: &Layout,
        view: View,
        resp: &egui::Response,
        hit_at: &impl Fn(Pos2) -> Hit,
    ) {
        let block_rect = |b: BlockId| layout.block(b).map(|g| view.rect(g.rect));
        for r in self
            .editor
            .also_selected
            .iter()
            .filter_map(|b| block_rect(*b))
        {
            painter.rect_stroke(
                r.expand(4.0),
                4.0,
                Stroke::new(2.0, ACCENT),
                egui::StrokeKind::Outside,
            );
        }
        if let Some(r) = self.editor.selected.and_then(block_rect) {
            painter.rect_stroke(
                r.expand(4.0),
                4.0,
                Stroke::new(2.0, ACCENT),
                egui::StrokeKind::Outside,
            );
        }
        if let (Some(b), Some(p)) = (self.drag, resp.interact_pointer_pos())
            && let Some(r) = block_rect(b)
        {
            let ghost = egui::Rect::from_center_size(p, r.size());
            painter.rect_stroke(
                ghost,
                4.0,
                Stroke::new(2.0, ACCENT),
                egui::StrokeKind::Middle,
            );
            resp.ctx.set_cursor_icon(CursorIcon::Grabbing);
            return;
        }
        let Some(p) = resp.hover_pos() else {
            return;
        };
        match hit_at(p) {
            Hit::Side(b, _) | Hit::Block(b) if self.picks() => {
                if let Some(r) = block_rect(b) {
                    painter.rect_stroke(
                        r.expand(3.0),
                        4.0,
                        Stroke::new(3.0, ACCENT),
                        egui::StrokeKind::Outside,
                    );
                }
                resp.ctx.set_cursor_icon(CursorIcon::Crosshair);
            }
            _ if self.picks() => resp.ctx.set_cursor_icon(CursorIcon::Crosshair),
            Hit::Side(b, side) => {
                if let Some(r) = block_rect(b) {
                    let [a, c] = canvas::side_edge(r, side);
                    painter.line_segment([a, c], Stroke::new(5.0, ACCENT));
                    painter.circle_filled(a.lerp(c, 0.5), 9.0, ACCENT);
                    painter.text(
                        a.lerp(c, 0.5),
                        egui::Align2::CENTER_CENTER,
                        "+",
                        egui::FontId::proportional(16.0),
                        Color32::WHITE,
                    );
                }
                resp.ctx.set_cursor_icon(CursorIcon::PointingHand);
            }
            Hit::Block(b) if layout.block(b).is_some_and(|g| !g.band) => {
                resp.ctx.set_cursor_icon(CursorIcon::Grab);
            }
            Hit::Block(_) => {}
            Hit::Line(rel, landing) => {
                let hovered = layout
                    .lines
                    .iter()
                    .find(|l| l.relation == rel && l.landing == landing);
                if let Some(line) = hovered {
                    let pts: Vec<Pos2> = line.points.iter().map(|q| view.screen(*q)).collect();
                    painter.line(pts, Stroke::new(4.0, ACCENT.gamma_multiply(0.5)));
                }
            }
            Hit::OpenEnd(_) => {
                painter.circle_stroke(p, 10.0, Stroke::new(2.0, ACCENT));
                resp.ctx.set_cursor_icon(CursorIcon::PointingHand);
            }
            Hit::Empty => {}
        }
    }

    fn click(&mut self, p: Pos2, h: Hit, ctrl: bool) {
        self.anchor = p + vec2(16.0, 16.0);
        if ctrl
            && !self.picks()
            && let Hit::Block(b) | Hit::Side(b, _) = h
        {
            self.editor.toggle_selected(b);
            self.editor.close_popup();
            return;
        }
        if let Some(moving) = self.moving_into {
            if let Hit::Block(b) | Hit::Side(b, _) = h {
                self.moving_into = None;
                self.editor.move_into(moving, b);
            }
            return;
        }
        if self.picking {
            if let Hit::Block(b) | Hit::Side(b, _) = h {
                self.picking = false;
                if let Some(Popup::Connect(form)) = &mut self.editor.popup {
                    form.target = Some(Target::Block(b));
                }
                self.editor.confirm();
            }
            return;
        }
        self.focus = true;
        match h {
            Hit::Side(b, side) => self.editor.start_connect(b, side),
            Hit::Block(b) => {
                self.editor.selected = Some(b);
                self.editor.also_selected.clear();
                self.editor.close_popup();
            }
            Hit::OpenEnd(end) => self.editor.start_open_end(end),
            Hit::Line(..) | Hit::Empty => {
                self.editor.selected = None;
                self.editor.also_selected.clear();
                self.editor.close_popup();
            }
        }
    }

    fn double_click(&mut self, h: Hit) {
        if self.picks() {
            return;
        }
        match h {
            Hit::Block(b) => {
                let drillable = self
                    .editor
                    .project
                    .blocks
                    .get(&b)
                    .is_some_and(|x| x.kind.can_drill());
                if drillable {
                    self.editor.open_diagram(Some(b));
                } else {
                    self.focus = true;
                    self.editor.start_edit_block(b);
                }
            }
            Hit::Line(rel, _) => {
                self.focus = true;
                self.editor.start_edit_relation(rel);
            }
            _ => {}
        }
    }

    fn context_menu(&mut self, ui: &mut Ui) {
        match self.context {
            Some(Hit::Block(b)) | Some(Hit::Side(b, _)) => {
                if ui.button("Edit\u{2026}").clicked() {
                    self.focus = true;
                    self.editor.start_edit_block(b);
                }
                let drillable = self
                    .editor
                    .project
                    .blocks
                    .get(&b)
                    .is_some_and(|x| x.kind.can_drill());
                if drillable && ui.button("Open whitebox").clicked() {
                    self.editor.open_diagram(Some(b));
                }
                let n = {
                    let sel = self.editor.selection();
                    if sel.contains(&b) { sel.len() } else { 1 }
                };
                let label = if n > 1 {
                    format!("Group {n} boxes into new box\u{2026}")
                } else {
                    "Group into new box\u{2026}".to_owned()
                };
                if ui
                    .button(label)
                    .on_hover_text("Ctrl+click boxes to group several")
                    .clicked()
                {
                    self.focus = true;
                    self.editor.start_group(b);
                }
                if ui
                    .button("Move into\u{2026}")
                    .on_hover_text("Then click the box it should go into")
                    .clicked()
                {
                    self.moving_into = Some(b);
                }
                if self.editor.project.has_content(b)
                    && ui
                        .button("Dissolve whitebox\u{2026}")
                        .on_hover_text("Its boxes take its place in this diagram")
                        .clicked()
                {
                    self.focus = true;
                    self.editor.start_dissolve(b);
                }
                let nested = self.editor.diagram.is_some();
                if ui
                    .add_enabled(nested, Button::new("Move up a level"))
                    .on_hover_text("Out of this whitebox, into the diagram one level up")
                    .clicked()
                {
                    self.editor.move_up(b);
                }
                if ui.button("Delete").clicked() {
                    self.editor.request_delete(b);
                }
            }
            Some(Hit::Line(rel, landing)) => {
                if ui.button("Edit\u{2026}").clicked() {
                    self.focus = true;
                    self.editor.start_edit_relation(rel);
                }
                if ui.button("Delete").clicked() {
                    self.editor.remove_line_at(rel, landing);
                }
            }
            Some(Hit::OpenEnd(end)) => {
                if ui.button("Connect\u{2026}").clicked() {
                    self.focus = true;
                    self.editor.start_open_end(end);
                }
            }
            _ => {
                if ui.button("Add box here").clicked() {
                    self.focus = true;
                    match self.context_cell {
                        Some(cell) => self.editor.start_add_block_at(cell),
                        None => self.editor.start_add_block(),
                    }
                }
                if ui.button("Fit to window").clicked() {
                    self.view = None;
                }
                if self.editor.diagram.is_some() && ui.button("Up one level").clicked() {
                    self.editor.go_up();
                }
            }
        }
    }

    // ----- popups and dialogs -----

    fn popup(&mut self, ctx: &egui::Context) {
        if self.picking {
            return;
        }
        let Some(mut popup) = self.editor.popup.take() else {
            self.shown_popup = None;
            return;
        };
        let shown = (std::mem::discriminant(&popup), self.anchor);
        if self.shown_popup != Some(shown) {
            self.popup_serial += 1;
        }
        self.shown_popup = Some(shown);
        let action = popups::show(
            ctx,
            &self.editor,
            &mut popup,
            self.anchor,
            self.popup_serial,
            &mut self.focus,
        );
        self.editor.popup = Some(popup);
        match action {
            Some(Action::Confirm) => self.editor.confirm(),
            Some(Action::Cancel) => {
                self.editor.close_popup();
                self.editor.message = None;
            }
            Some(Action::Keep) => self.editor.keep_in_place(),
            Some(Action::Pick) => self.picking = true,
            Some(Action::Discard) => self.editor.discard_recovery(),
            None => {}
        }
    }

    fn leave_dialog(&mut self, ctx: &egui::Context) {
        let Some(leave) = self.leave.clone() else {
            return;
        };
        let occasion = match leave {
            Leave::Quit => leave::Occasion::Quit,
            Leave::New => leave::Occasion::New,
            Leave::Open(_) => leave::Occasion::Open,
        };
        match leave::show(ctx, &self.editor.display_name(), occasion) {
            Some(leave::Choice::Save) => {
                if self.save() {
                    self.leave = None;
                    self.proceed(ctx, leave);
                }
            }
            Some(leave::Choice::Discard) => {
                self.leave = None;
                self.editor.forget_unsaved();
                self.proceed(ctx, leave);
            }
            Some(leave::Choice::Keep) => self.leave = None,
            None => {}
        }
    }

    fn proceed(&mut self, ctx: &egui::Context, leave: Leave) {
        match leave {
            Leave::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            Leave::New => self.reset(Editor::new(self.recovery_dir.clone())),
            Leave::Open(path) => self.load(&path),
        }
    }

    fn reset(&mut self, editor: Editor) {
        // Folders allowed for AI exports belong to the session, not the project.
        let roots = std::mem::take(&mut self.editor.ai_export_roots);
        self.editor = editor;
        self.editor.ai_export_roots = roots;
        self.editor.apply_prefs(&self.settings.prefs);
        self.draft = None;
        self.view = None;
        self.drag = None;
        self.picking = false;
        self.moving_into = None;
    }

    fn load(&mut self, path: &Path) {
        match Editor::open(path, self.recovery_dir.clone()) {
            Ok(editor) => self.reset(editor),
            Err(e) => self.editor.message = Some(format!("Cannot open {}: {e}", path.display())),
        }
    }

    // ----- files -----

    fn new_project(&mut self) {
        if self.editor.dirty {
            self.leave = Some(Leave::New);
        } else {
            self.reset(Editor::new(self.recovery_dir.clone()));
        }
    }

    fn open(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("whiteboxed project", &["yaml", "yml"])
            .pick_file()
        else {
            return;
        };
        if self.editor.dirty {
            self.leave = Some(Leave::Open(path));
        } else {
            self.load(&path);
        }
    }

    /// Saves; asks for a file name the first time. Returns whether it saved.
    fn save(&mut self) -> bool {
        self.commit_draft();
        match self.editor.save() {
            Ok(true) => true,
            Ok(false) => self.save_as(),
            Err(e) => {
                self.editor.message = Some(format!("Cannot save: {e}"));
                false
            }
        }
    }

    fn save_as(&mut self) -> bool {
        self.commit_draft();
        let Some(mut path) = rfd::FileDialog::new()
            .add_filter("whiteboxed project", &["yaml", "yml"])
            .set_file_name(self.editor.suggested_file_name())
            .save_file()
        else {
            return false;
        };
        if path.extension().is_none() {
            path.set_extension("yaml");
        }
        match self.editor.save_as(&path) {
            Ok(()) => true,
            Err(e) => {
                self.editor.message = Some(format!("Cannot save: {e}"));
                false
            }
        }
    }
}

/// The colour of a readability level.
pub fn level_color(level: crate::score::Level) -> Color32 {
    match level {
        crate::score::Level::Green => Color32::from_rgb(0x3a, 0xa6, 0x55),
        crate::score::Level::Yellow => Color32::from_rgb(0xe0, 0xb4, 0x32),
        crate::score::Level::Red => Color32::from_rgb(0xd0, 0x4a, 0x3a),
    }
}

/// A small dot in the colour of a diagram's readability, explained on hover.
fn score_dot(ui: &mut Ui, score: &crate::score::Score) {
    let (rect, resp) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 4.5, level_color(score.level()));
    resp.on_hover_text(score.summary());
}

/// What the canvas showed in the last frame.
struct Shown {
    diagram: DiagramId,
    view: View,
    /// Where each box was, to keep the picture still when the layout shifts.
    blocks: Vec<(BlockId, geom::Rect)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZoomStep {
    In,
    Out,
    Actual,
}

const ZOOM_STEP: f32 = 1.25;
const MIN_ZOOM: f32 = 0.15;
const MAX_ZOOM: f32 = 4.0;

/// `view` zoomed to `zoom`, keeping the diagram point under `center` where it is.
fn zoomed(view: View, center: Pos2, zoom: f32) -> View {
    let d = view.diagram(center);
    let zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    View {
        origin: center - vec2(d.x, d.y) * zoom,
        zoom,
    }
}

/// How far more than half of the boxes that `old` and `layout` share have moved: when
/// a column or row is added in front, everything shifts by the same amount. Zero when
/// there is no such majority (e.g. two boxes swapped places).
fn drift(old: &[(BlockId, geom::Rect)], layout: &Layout) -> geom::Pos {
    let mut votes: Vec<((i32, i32), usize)> = Vec::new();
    for b in &layout.blocks {
        if let Some((_, r)) = old.iter().find(|(id, _)| *id == b.id) {
            let key = (
                (b.rect.min.x - r.min.x).round() as i32,
                (b.rect.min.y - r.min.y).round() as i32,
            );
            match votes.iter_mut().find(|(k, _)| *k == key) {
                Some((_, n)) => *n += 1,
                None => votes.push((key, 1)),
            }
        }
    }
    let shared: usize = votes.iter().map(|(_, n)| n).sum();
    votes
        .into_iter()
        .find(|(_, n)| 2 * n > shared)
        .map_or(geom::Pos::new(0.0, 0.0), |((x, y), _)| {
            geom::Pos::new(x as f32, y as f32)
        })
}

/// The shift that brings `r` inside `canvas` (with a margin), or zero.
fn into_view(canvas: egui::Rect, r: egui::Rect) -> egui::Vec2 {
    let inner = canvas.shrink(24.0);
    let along = |lo: f32, hi: f32, min: f32, max: f32| {
        if lo < min {
            min - lo
        } else if hi > max {
            (max - hi).max(min - lo)
        } else {
            0.0
        }
    };
    vec2(
        along(r.min.x, r.max.x, inner.min.x, inner.max.x),
        along(r.min.y, r.max.y, inner.min.y, inner.max.y),
    )
}

/// A view that shows the whole diagram, never larger than 1:1.
fn fit(rect: egui::Rect, layout: &Layout) -> View {
    let b = layout.bounds;
    let zoom = (rect.width() / b.width())
        .min(rect.height() / b.height())
        .clamp(MIN_ZOOM, 1.0);
    let c = b.center();
    View {
        origin: rect.center() - vec2(c.x, c.y) * zoom,
        zoom,
    }
}
