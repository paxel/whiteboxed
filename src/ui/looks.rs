//! File > Looks…: the library of custom looks, shared by every project. Shows each
//! look, renames and deletes them, and imports `.svg` files.

use std::collections::BTreeMap;

use egui::{Context, RichText};

use super::canvas::LookTextures;
use crate::editor::Editor;
use crate::look::{LookId, LookMode};

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
}

impl LooksDialog {
    pub fn start(&mut self) {
        self.names.clear();
        self.error = None;
        self.open = true;
    }

    pub fn show(&mut self, ctx: &Context, editor: &mut Editor) {
        if !self.open {
            return;
        }
        let mut open = true;
        let mut rename: Option<(LookId, String)> = None;
        let mut delete: Option<LookId> = None;
        let mut import = false;
        egui::Window::new("Looks")
            .open(&mut open)
            .collapsible(false)
            .default_width(460.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Custom looks for boxes, in every project. A project keeps \
                         copies of the looks it uses.",
                    )
                    .weak(),
                );
                ui.add_space(6.0);
                if editor.library.looks.is_empty() {
                    ui.label("No looks yet: import an .svg file, or ask the AI for one.");
                }
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        egui::Grid::new("looks").num_columns(4).show(ui, |ui| {
                            for (id, look) in &editor.library.looks {
                                let (w, h) = look.size();
                                let size = egui::vec2(THUMB_H * w / h, THUMB_H)
                                    .min(egui::vec2(2.0 * THUMB_H, THUMB_H));
                                let pixels = size.x * ctx.pixels_per_point();
                                match self.thumbs.get(ctx, look.svg(), pixels) {
                                    Some(texture) => {
                                        ui.image((texture, size));
                                    }
                                    None => {
                                        ui.label("?");
                                    }
                                }
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
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Import .svg\u{2026}").clicked() {
                        import = true;
                    }
                    ui.label("shown as");
                    egui::ComboBox::from_id_salt("import-mode")
                        .selected_text(self.import_mode.label())
                        .show_ui(ui, |ui| {
                            for mode in LookMode::ALL {
                                ui.selectable_value(&mut self.import_mode, mode, mode.label());
                            }
                        });
                });
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
        self.open = open;
    }
}
