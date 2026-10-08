//! File > Export…: one dialog for pictures, texts and the target folder. The last
//! choices are kept with the project.

use std::path::PathBuf;

use egui::{Color32, Context, RichText};

use crate::doc::{self, DocFormat};
use crate::editor::Editor;
use crate::model::ExportChoice;

#[derive(Default)]
pub struct ExportDialog {
    pub open: bool,
    choice: ExportChoice,
    /// The target folder as an absolute path.
    folder: Option<PathBuf>,
    error: Option<String>,
}

impl ExportDialog {
    /// Opens the dialog with the project's last choices.
    pub fn start(&mut self, editor: &Editor) {
        self.choice = editor.project.export.clone().unwrap_or_default();
        self.folder = editor.export_folder(&self.choice);
        self.error = None;
        self.open = true;
    }

    pub fn set_folder(&mut self, folder: PathBuf) {
        self.folder = Some(folder);
    }

    pub fn show(&mut self, ctx: &Context, editor: &mut Editor) {
        if !self.open {
            return;
        }
        let mut open = true;
        let mut export = false;
        let mut cancel = false;
        let current = editor
            .breadcrumb()
            .last()
            .map_or_else(|| "Context".to_owned(), |(_, name)| name.clone());
        let count = doc::diagrams(&editor.project).len();
        let c = &mut self.choice;
        egui::Window::new("Export")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.label(RichText::new("What").strong());
                ui.radio_value(&mut c.all, false, format!("This diagram ({current})"));
                ui.radio_value(&mut c.all, true, format!("All diagrams ({count})"));
                ui.add_space(8.0);

                ui.label(RichText::new("Pictures").strong());
                ui.checkbox(&mut c.svg, "SVG");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut c.png, "PNG");
                    ui.add_enabled_ui(c.png, |ui| {
                        for scale in 1..=3u8 {
                            ui.radio_value(&mut c.png_scale, scale, format!("{scale}\u{d7}"));
                        }
                    });
                });
                ui.add_space(8.0);

                ui.label(RichText::new("Text").strong());
                ui.horizontal(|ui| {
                    ui.radio_value(&mut c.text, None, "none");
                    ui.radio_value(&mut c.text, Some(DocFormat::AsciiDoc), "AsciiDoc");
                    ui.radio_value(&mut c.text, Some(DocFormat::Markdown), "Markdown");
                });
                ui.label(
                    RichText::new(
                        "One file per diagram with the arc42 tables; with all diagrams also \
                         an index.",
                    )
                    .weak(),
                );
                ui.add_space(8.0);

                ui.label(RichText::new("Folder").strong());
                ui.horizontal(|ui| {
                    let shown = self
                        .folder
                        .as_ref()
                        .map_or_else(|| "none chosen".to_owned(), |f| f.display().to_string());
                    ui.label(shown);
                    if ui.button("Choose\u{2026}").clicked() {
                        let mut dialog = rfd::FileDialog::new();
                        if let Some(dir) = &self.folder {
                            dialog = dialog.set_directory(dir);
                        }
                        if let Some(dir) = dialog.pick_folder() {
                            self.folder = Some(dir);
                        }
                    }
                });
                if let Some(folder) = &self.folder {
                    let note = if editor.path.is_none() {
                        Some("The project is not saved yet, so the folder is kept as an absolute path.")
                    } else if editor.folder_to_store(folder).1 {
                        Some("This folder lies outside the project's repository; it is kept as an absolute path.")
                    } else {
                        None
                    };
                    if let Some(note) = note {
                        ui.label(RichText::new(note).color(Color32::from_rgb(0xb0, 0x60, 0x00)));
                    }
                }
                if let Some(error) = &self.error {
                    ui.label(RichText::new(error).color(Color32::from_rgb(0xc0, 0x30, 0x30)));
                }
                ui.add_space(10.0);
                let something = c.svg || c.png || c.text.is_some();
                ui.horizontal(|ui| {
                    let ready = something && self.folder.is_some();
                    if ui.add_enabled(ready, egui::Button::new("Export")).clicked() {
                        export = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if export && let Some(folder) = self.folder.clone() {
            self.choice.folder = editor.folder_to_store(&folder).0;
            editor.remember_export(self.choice.clone());
            let result = std::fs::create_dir_all(&folder)
                .map_err(crate::export::ExportError::from)
                .and_then(|()| editor.export(&self.choice, &folder));
            match result {
                Ok(n) => {
                    editor.message = Some(format!("Exported {n} files to {}", folder.display()));
                    open = false;
                }
                Err(e) => self.error = Some(format!("Export failed: {e}")),
            }
        }
        if !open || cancel {
            self.open = false;
        }
    }
}
