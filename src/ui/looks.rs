//! File > Looks…: the library of custom looks, pictures and line looks, shared by
//! every project. Shows each entry, renames, edits and deletes them, and imports
//! `.svg` files.

use std::collections::BTreeMap;

use egui::{Context, RichText, Ui};

use super::canvas::LookTextures;
use crate::editor::Editor;
use crate::look::{self, LineLookSpec, LookId, LookMode};
use crate::model::LinePattern;

/// Height of a look's picture in the list.
const THUMB_H: f32 = 40.0;

#[derive(Default)]
pub struct LooksDialog {
    pub open: bool,
    /// Names being edited, by look.
    names: BTreeMap<LookId, String>,
    /// How an imported look is shown.
    import_mode: LookMode,
    error: Option<String>,
    thumbs: LookTextures,
    /// Line look settings being edited, by line look, with the library revision they
    /// were taken from; applied when they differ from the library's and no drag or
    /// typing is under way. A newer revision (another edit, the AI) starts them over.
    specs: BTreeMap<LookId, (u64, LineLookSpec)>,
}

impl LooksDialog {
    pub fn start(&mut self) {
        self.names.clear();
        self.specs.clear();
        self.error = None;
        self.open = true;
    }

    /// The thumbnail of a picture, `h` high.
    fn thumb(&mut self, ui: &mut Ui, svg: &std::sync::Arc<str>, (w, h): (f32, f32), height: f32) {
        let size = egui::vec2(height * w / h, height).min(egui::vec2(2.0 * height, height));
        let pixels = size.x * ui.ctx().pixels_per_point();
        match self.thumbs.get(ui.ctx(), svg, pixels) {
            Some(texture) => {
                ui.image((texture, size));
            }
            None => {
                ui.label("?");
            }
        }
    }

    pub fn show(&mut self, ctx: &Context, editor: &mut Editor) {
        if !self.open {
            return;
        }
        let mut open = true;
        let mut rename: Option<(LookId, String)> = None;
        let mut delete: Option<LookId> = None;
        let mut import = false;
        let mut rename_picture: Option<(LookId, String)> = None;
        let mut delete_picture: Option<LookId> = None;
        let mut import_picture = false;
        let mut rename_line: Option<(LookId, String)> = None;
        let mut delete_line: Option<LookId> = None;
        let mut edit_line: Option<(LookId, LineLookSpec)> = None;
        let mut add_line = false;
        let pictures = editor.pictures_offered();
        egui::Window::new("Looks")
            .open(&mut open)
            .collapsible(false)
            .default_width(460.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Custom looks for boxes, pictures for line ends and mouseholes, and \
                         line looks, in every project. A project keeps copies of what it \
                         uses.",
                    )
                    .weak(),
                );
                egui::ScrollArea::vertical()
                    .max_height(480.0)
                    .show(ui, |ui| {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Box looks").strong());
                        if editor.library.looks.is_empty() {
                            ui.label("No looks yet: import an .svg file, or ask the AI for one.");
                        }
                        egui::Grid::new("looks").num_columns(4).show(ui, |ui| {
                            for (id, look) in &editor.library.looks {
                                self.thumb(ui, look.svg(), look.size(), THUMB_H);
                                let name = self
                                    .names
                                    .entry(id.clone())
                                    .or_insert_with(|| look.name().to_owned());
                                let field = ui.add_sized(
                                    [180.0, ui.spacing().interact_size.y],
                                    egui::TextEdit::singleline(name).id_salt(("look-name", id)),
                                );
                                if field.lost_focus() && name.trim() != look.name() {
                                    rename = Some((id.clone(), name.clone()));
                                }
                                ui.label(RichText::new(look.mode().label()).weak());
                                if ui.button("Delete").clicked() {
                                    delete = Some(id.clone());
                                }
                                ui.end_row();
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Import .svg\u{2026}").clicked() {
                                import = true;
                            }
                            ui.label("shown as");
                            egui::ComboBox::from_id_salt("import-mode")
                                .selected_text(self.import_mode.label())
                                .show_ui(ui, |ui| {
                                    for mode in LookMode::ALL {
                                        ui.selectable_value(
                                            &mut self.import_mode,
                                            mode,
                                            mode.label(),
                                        );
                                    }
                                });
                        });

                        ui.add_space(10.0);
                        ui.label(RichText::new("Pictures").strong());
                        ui.label(
                            RichText::new(
                                "For the ends of lines and for mouseholes. On a line a picture \
                                 points right, its tip at the right edge; currentColor takes \
                                 the line's colour.",
                            )
                            .weak(),
                        );
                        egui::Grid::new("pictures").num_columns(3).show(ui, |ui| {
                            for (id, picture) in &editor.library.pictures {
                                self.thumb(ui, picture.svg(), picture.size(), THUMB_H / 2.0);
                                let name = self
                                    .names
                                    .entry(id.clone())
                                    .or_insert_with(|| picture.name().to_owned());
                                let field = ui.add_sized(
                                    [180.0, ui.spacing().interact_size.y],
                                    egui::TextEdit::singleline(name).id_salt(("picture-name", id)),
                                );
                                if field.lost_focus() && name.trim() != picture.name() {
                                    rename_picture = Some((id.clone(), name.clone()));
                                }
                                if ui.button("Delete").clicked() {
                                    delete_picture = Some(id.clone());
                                }
                                ui.end_row();
                            }
                        });
                        if ui.button("Import picture .svg\u{2026}").clicked() {
                            import_picture = true;
                        }

                        ui.add_space(10.0);
                        ui.label(RichText::new("Line looks").strong());
                        ui.label(
                            RichText::new(
                                "Width, dash and the pictures at the ends: the head where the \
                                 arrow points, the tail at the other ends.",
                            )
                            .weak(),
                        );
                        egui::Grid::new("line-looks").num_columns(6).show(ui, |ui| {
                            let mut dragging = false;
                            for (id, line) in &editor.library.line_looks {
                                let name = self
                                    .names
                                    .entry(id.clone())
                                    .or_insert_with(|| line.name().to_owned());
                                let field = ui.add_sized(
                                    [120.0, ui.spacing().interact_size.y],
                                    egui::TextEdit::singleline(name)
                                        .id_salt(("line-look-name", id)),
                                );
                                if field.lost_focus() && name.trim() != line.name() {
                                    rename_line = Some((id.clone(), name.clone()));
                                }
                                let entry = self
                                    .specs
                                    .entry(id.clone())
                                    .or_insert_with(|| (line.revision(), line.spec()));
                                if entry.0 != line.revision() {
                                    *entry = (line.revision(), line.spec());
                                }
                                let spec = &mut entry.1;
                                ui.horizontal(|ui| {
                                    ui.label("width");
                                    let drag = ui.add(
                                        egui::DragValue::new(&mut spec.width)
                                            .range(look::MIN_WIDTH..=look::MAX_WIDTH)
                                            .speed(0.1),
                                    );
                                    dragging |= drag.dragged() || drag.has_focus();
                                });
                                egui::ComboBox::from_id_salt(("dash", id))
                                    .selected_text(spec.dash.label())
                                    .show_ui(ui, |ui| {
                                        for d in LinePattern::ALL {
                                            ui.selectable_value(&mut spec.dash, d, d.label());
                                        }
                                    });
                                for (label, end, salt) in [
                                    ("head", &mut spec.head, "head"),
                                    ("tail", &mut spec.tail, "tail"),
                                ] {
                                    ui.horizontal(|ui| {
                                        ui.label(label);
                                        let shown = end
                                            .as_ref()
                                            .and_then(|p| pictures.iter().find(|(i, _)| i == p))
                                            .map_or_else(
                                                || "none".to_owned(),
                                                |(_, name)| name.clone(),
                                            );
                                        egui::ComboBox::from_id_salt((salt, id))
                                            .selected_text(shown)
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(end, None, "none");
                                                for (pid, pname) in &pictures {
                                                    ui.selectable_value(
                                                        end,
                                                        Some(pid.clone()),
                                                        pname,
                                                    );
                                                }
                                            });
                                    });
                                }
                                if ui.button("Delete").clicked() {
                                    delete_line = Some(id.clone());
                                }
                                if !dragging && *spec != line.spec() {
                                    edit_line = Some((id.clone(), spec.clone()));
                                }
                                ui.end_row();
                            }
                        });
                        if ui.button("Add line look").clicked() {
                            add_line = true;
                        }
                    });
                ui.separator();
                if let Some(e) = &self.error {
                    ui.colored_label(egui::Color32::from_rgb(0xc0, 0x30, 0x30), e);
                }
            });
        self.thumbs.sweep();
        if let Some((id, name)) = rename {
            self.error = editor.rename_look(&id, &name).err();
            if self.error.is_some() {
                self.names.remove(&id);
            }
        }
        if let Some(id) = delete {
            self.error = editor.delete_look(&id).err();
            self.names.remove(&id);
        }
        if import
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("SVG picture", &["svg"])
                .pick_file()
        {
            self.error = editor
                .import_look(&path, self.import_mode)
                .err()
                .map(|e| format!("Cannot import {}: {e}", path.display()));
        }
        if let Some((id, name)) = rename_picture {
            self.error = editor.rename_picture(&id, &name).err();
            if self.error.is_some() {
                self.names.remove(&id);
            }
        }
        if let Some(id) = delete_picture {
            self.error = editor.delete_picture(&id).err();
            self.names.remove(&id);
        }
        if import_picture
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("SVG picture", &["svg"])
                .pick_file()
        {
            self.error = editor
                .import_picture(&path)
                .err()
                .map(|e| format!("Cannot import {}: {e}", path.display()));
        }
        if let Some((id, name)) = rename_line {
            self.error = editor.rename_line_look(&id, &name).err();
            if self.error.is_some() {
                self.names.remove(&id);
            }
        }
        if let Some((id, spec)) = edit_line {
            self.error = editor.edit_line_look(&id, spec).err();
            if self.error.is_some() {
                self.specs.remove(&id);
            }
        }
        if let Some(id) = delete_line {
            self.error = editor.delete_line_look(&id).err();
            self.names.remove(&id);
            self.specs.remove(&id);
        }
        if add_line {
            let n = editor.library.line_looks.len().saturating_add(1);
            let mut name = format!("line look {n}");
            let mut k = n;
            while editor.library.find_line_look(&name).is_some() {
                k = k.saturating_add(1);
                name = format!("line look {k}");
            }
            self.error = editor.add_line_look(&name, LineLookSpec::default()).err();
        }
        self.open = open;
    }
}
