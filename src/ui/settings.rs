//! File > Project settings…: options stored in the project, plus taking them over as
//! your own starting values for new projects.

use std::path::PathBuf;

use egui::{Context, RichText};

use crate::editor::Editor;
use crate::model::LineStyle;
use crate::prefs::{self, Prefs};

pub struct Settings {
    pub open: bool,
    dir: Option<PathBuf>,
    pub prefs: Prefs,
    note: Option<String>,
}

impl Settings {
    /// `dir` holds the user's preferences; `None` keeps them in memory only.
    pub fn new(dir: Option<PathBuf>) -> Self {
        let prefs = dir.as_deref().map(prefs::load).unwrap_or_default();
        Settings {
            open: false,
            dir,
            prefs,
            note: None,
        }
    }

    pub fn show(&mut self, ctx: &Context, editor: &mut Editor) {
        if !self.open {
            return;
        }
        let mut open = true;
        let mut style = editor.project.line_style;
        let mut limit = editor.project.label_limit;
        let mut make_default = false;
        egui::Window::new("Project settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.label(RichText::new("Line style").strong());
                ui.label(
                    RichText::new("How relation lines bend. A relation can override it.").weak(),
                );
                for s in LineStyle::ALL {
                    ui.radio_value(&mut style, s, s.label());
                }
                ui.add_space(10.0);
                ui.label(RichText::new("Long relation texts").strong());
                ui.label(
                    RichText::new(
                        "Without a short label, longer texts become [1], [2] … with the full \
                         text in a legend below the diagram.",
                    )
                    .weak(),
                );
                let mut shorten = limit.is_some();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut shorten, "Shorten texts longer than");
                    let mut n = limit.unwrap_or(24);
                    ui.add_enabled(shorten, egui::DragValue::new(&mut n).range(0..=200));
                    ui.label("characters (0: number every text)");
                    limit = shorten.then_some(n);
                });
                ui.add_space(10.0);
                if ui.button("Use as my default for new projects").clicked() {
                    make_default = true;
                }
                if let Some(note) = &self.note {
                    ui.label(RichText::new(note).weak());
                }
            });
        if style != editor.project.line_style {
            editor.set_line_style(style);
        }
        if limit != editor.project.label_limit {
            editor.set_label_limit(limit);
        }
        if make_default {
            self.prefs.line_style = editor.project.line_style;
            self.note = Some(match &self.dir {
                Some(dir) => match prefs::save(dir, &self.prefs) {
                    Ok(()) => "New projects start with these settings.".into(),
                    Err(e) => format!("Cannot store your preferences: {e}"),
                },
                None => "New projects in this session start with these settings.".into(),
            });
        }
        if !open {
            self.open = false;
            self.note = None;
        }
    }
}
