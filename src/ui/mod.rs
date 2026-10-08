//! The egui front end: menu, breadcrumb, structure tree, canvas and popups.

pub mod canvas;
pub mod details;
pub mod icons;
pub mod popups;

use std::path::{Path, PathBuf};
use std::time::Instant;

use egui::{
    Button, Color32, CursorIcon, Key, KeyboardShortcut, Modifiers, PointerButton, Pos2, RichText,
    Sense, Stroke, Ui, ViewportCommand, vec2,
};

use crate::doc::DocFormat;
use crate::editor::{Editor, Popup, Target};
use crate::geom;
use crate::hit::{self, Hit};
use crate::layout::Layout;
use crate::model::{BlockId, DiagramId, PALETTE, Side};
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
    let mut app = App::new(editor, dir);
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
    view: Option<(DiagramId, geom::Rect, View)>,
    anchor: Pos2,
    focus: bool,
    context: Option<Hit>,
    drag: Option<BlockId>,
    picking: bool,
    leave: Option<Leave>,
    allow_close: bool,
    shown_title: String,
    /// Popup kind and anchor shown last frame, to place a newly opened popup.
    shown_popup: Option<(std::mem::Discriminant<Popup>, Pos2)>,
    popup_serial: u64,
    draft: Option<details::Draft>,
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
            anchor: Pos2::new(200.0, 150.0),
            focus: true,
            context: None,
            drag: None,
            picking: false,
            leave: None,
            allow_close: false,
            shown_title: String::new(),
            shown_popup: None,
            popup_serial: 0,
            draft: None,
        }
    }

    /// The current diagram-to-screen transform, once the canvas was drawn.
    pub fn view(&self) -> Option<View> {
        self.view.map(|(_, _, v)| v)
    }

    pub fn is_picking(&self) -> bool {
        self.picking
    }

    /// Draws the whole window into `ui`.
    pub fn show(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.handle_close(&ctx);
        self.shortcuts(&ctx);
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
        if self.picking {
            if pressed(Modifiers::NONE, Key::Escape) {
                self.picking = false;
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
                if ui.button("Export diagram as SVG\u{2026}").clicked() {
                    self.export("svg");
                }
                if ui.button("Export diagram as PNG\u{2026}").clicked() {
                    self.export("png");
                }
                if ui.button("Export all as AsciiDoc\u{2026}").clicked() {
                    self.export_all(DocFormat::AsciiDoc);
                }
                if ui.button("Export all as Markdown\u{2026}").clicked() {
                    self.export_all(DocFormat::Markdown);
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
                    .add_enabled(self.editor.diagram.is_some(), Button::new("Up one level"))
                    .clicked()
                {
                    self.editor.go_up();
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
            if dangling > 0 {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{dangling} interface{} not assigned to a box",
                            if dangling == 1 { "" } else { "s" }
                        ))
                        .color(Color32::from_rgb(0xb0, 0x6a, 0x00)),
                    );
                });
            }
        });
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        enum TreeAct {
            Open(DiagramId),
            Reveal(BlockId),
            Color(crate::model::TagId, crate::model::Rgb),
        }
        let mut act = None;
        ui.heading("Structure");
        egui::ScrollArea::vertical()
            .id_salt("tree")
            .max_height(ui.available_height() * 0.65)
            .show(ui, |ui| {
                let root = ui.selectable_label(self.editor.diagram.is_none(), "Context");
                if root.clicked() {
                    act = Some(TreeAct::Open(None));
                }
                tree(ui, &self.editor, None, 1, &mut act_tree(&mut act));
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
                });
                if editor.project.has_content(id) {
                    tree(ui, editor, Some(id), depth + 1, on);
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

        let mut view = match self.view {
            Some((d, bounds, v)) if d == diagram && bounds == layout.bounds => v,
            _ => fit(rect, &layout),
        };
        if resp.hovered() {
            let (zoom, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
            if zoom != 1.0
                && let Some(p) = resp.hover_pos()
            {
                let d = view.diagram(p);
                view.zoom = (view.zoom * zoom).clamp(0.15, 4.0);
                view.origin = p - vec2(d.x, d.y) * view.zoom;
            }
            view.origin += scroll;
        }

        let project = &self.editor.project;
        let hit_with = |v: View, p: Pos2| hit::hit(project, &layout, v.diagram(p), v.zoom);
        if resp.drag_started_by(PointerButton::Primary) {
            let start = ui.input(|i| i.pointer.press_origin());
            self.drag = match start.map(|p| hit_with(view, p)) {
                Some(Hit::Block(b)) if !self.picking => Some(b),
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
        self.view = Some((diagram, layout.bounds, view));
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

        if let Some((b, cell)) = moved {
            self.editor.move_block(b, cell);
        }
        if let Some((p, h)) = clicked {
            self.click(p, h);
        }
        if let Some(h) = double {
            self.double_click(h);
        }
        if let Some((p, h)) = secondary {
            self.anchor = p;
            self.context = Some(h);
        }
        resp.context_menu(|ui| self.context_menu(ui));

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
            Hit::Side(b, _) | Hit::Block(b) if self.picking => {
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
            _ if self.picking => resp.ctx.set_cursor_icon(CursorIcon::Crosshair),
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
            Hit::Block(_) => resp.ctx.set_cursor_icon(CursorIcon::Grab),
            Hit::Line(rel) => {
                if let Some(line) = layout.lines.iter().find(|l| l.relation == rel) {
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

    fn click(&mut self, p: Pos2, h: Hit) {
        self.anchor = p + vec2(16.0, 16.0);
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
                self.editor.close_popup();
            }
            Hit::OpenEnd(end) => self.editor.start_open_end(end),
            Hit::Line(_) | Hit::Empty => {
                self.editor.selected = None;
                self.editor.close_popup();
            }
        }
    }

    fn double_click(&mut self, h: Hit) {
        if self.picking {
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
            Hit::Line(rel) => {
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
                if ui.button("Delete").clicked() {
                    self.editor.request_delete(b);
                }
            }
            Some(Hit::Line(rel)) => {
                if ui.button("Edit\u{2026}").clicked() {
                    self.focus = true;
                    self.editor.start_edit_relation(rel);
                }
                if ui.button("Delete").clicked() {
                    self.editor.remove_line(rel);
                }
            }
            Some(Hit::OpenEnd(end)) => {
                if ui.button("Connect\u{2026}").clicked() {
                    self.focus = true;
                    self.editor.start_open_end(end);
                }
            }
            _ => {
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
        let mut choice = None;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("Save the changes to this project first?");
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        choice = Some(0);
                    }
                    if ui.button("Don't save").clicked() {
                        choice = Some(1);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(2);
                    }
                });
            });
        match choice {
            Some(0) => {
                if self.save() {
                    self.leave = None;
                    self.proceed(ctx, leave);
                }
            }
            Some(1) => {
                self.leave = None;
                self.editor.forget_unsaved();
                self.proceed(ctx, leave);
            }
            Some(_) => self.leave = None,
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
        self.editor = editor;
        self.draft = None;
        self.view = None;
        self.drag = None;
        self.picking = false;
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
            .set_file_name("architecture.yaml")
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

    fn export(&mut self, ext: &str) {
        self.commit_draft();
        let name = format!("{}.{ext}", self.editor.file_name_for(self.editor.diagram));
        let Some(path) = rfd::FileDialog::new()
            .add_filter(ext.to_uppercase(), &[ext])
            .set_file_name(name)
            .save_file()
        else {
            return;
        };
        let result = if ext == "svg" {
            self.editor.export_svg(&path)
        } else {
            self.editor.export_png(&path)
        };
        self.editor.message = Some(match result {
            Ok(()) => format!("Exported {}", path.display()),
            Err(e) => format!("Export failed: {e}"),
        });
    }

    fn export_all(&mut self, format: DocFormat) {
        self.commit_draft();
        let Some(dir) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        self.editor.message = Some(match self.editor.export_all(&dir, format) {
            Ok(n) => format!("Exported {n} diagrams to {}", dir.display()),
            Err(e) => format!("Export failed: {e}"),
        });
    }
}

/// A view that shows the whole diagram, never larger than 1:1.
fn fit(rect: egui::Rect, layout: &Layout) -> View {
    let b = layout.bounds;
    let zoom = (rect.width() / b.width())
        .min(rect.height() / b.height())
        .clamp(0.15, 1.0);
    let c = b.center();
    View {
        origin: rect.center() - vec2(c.x, c.y) * zoom,
        zoom,
    }
}
