//! The "unsaved changes" question before quitting, starting anew or opening another
//! project: large, centred over a dimmed window, with the panicked cat. The
//! destructive choice stands apart on the left; Enter saves, Esc keeps editing.

use egui::{Color32, Context, Id, Key, RichText, Stroke, vec2};

use super::icons;

/// What the user is about to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Occasion {
    Quit,
    New,
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Save,
    Discard,
    Keep,
}

impl Occasion {
    fn labels(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Occasion::Quit => (
                "Save and quit",
                "Quit without saving",
                "Quitting now without saving loses those changes.",
            ),
            Occasion::New => (
                "Save and start new",
                "Start new without saving",
                "Starting a new project without saving loses those changes.",
            ),
            Occasion::Open => (
                "Save and open",
                "Open without saving",
                "Opening another project without saving loses those changes.",
            ),
        }
    }
}

const SAVE_FILL: Color32 = Color32::from_rgb(0xbf, 0xe6, 0xb4);
const SAVE_TEXT: Color32 = Color32::from_rgb(0x17, 0x3d, 0x12);
const SAVE_EDGE: Color32 = Color32::from_rgb(0x5b, 0xa8, 0x4a);
const DISCARD_FILL: Color32 = Color32::from_rgb(0xf6, 0xc2, 0xbd);
const DISCARD_TEXT: Color32 = Color32::from_rgb(0x5a, 0x14, 0x10);
const DISCARD_EDGE: Color32 = Color32::from_rgb(0xe9, 0xa5, 0x9e);
const KEEP_FILL: Color32 = Color32::from_rgb(0xe4, 0xe8, 0xee);
const INK: Color32 = Color32::from_rgb(0x1d, 0x27, 0x33);

fn button(text: &str, fg: Color32, fill: Color32, edge: Stroke) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).color(fg).strong().size(15.0))
        .fill(fill)
        .stroke(edge)
        .corner_radius(8.0)
        .min_size(vec2(0.0, 36.0))
}

/// Shows the dialog; returns the user's choice in the frame it was made.
pub fn show(ctx: &Context, project: &str, occasion: Occasion) -> Option<Choice> {
    let (save, discard, consequence) = occasion.labels();
    let mut choice = None;
    let frame = egui::Frame::window(&ctx.global_style())
        .fill(Color32::WHITE)
        .inner_margin(28.0)
        .corner_radius(14.0);
    let modal = egui::Modal::new(Id::new("unsaved-changes"))
        .frame(frame)
        .backdrop_color(Color32::from_black_alpha(140))
        .show(ctx, |ui| {
            ui.set_width(560.0);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(104.0, 104.0), egui::Sense::hover());
                icons::panic_cat(ui.painter(), rect);
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(format!("Save changes to \u{201c}{project}\u{201d}?"))
                            .size(20.0)
                            .strong()
                            .color(INK),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!(
                            "You changed the project since it was last saved. {consequence}"
                        ))
                        .size(14.0)
                        .color(Color32::from_rgb(0x4a, 0x57, 0x68)),
                    );
                });
            });
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                if ui
                    .add(button(
                        discard,
                        DISCARD_TEXT,
                        DISCARD_FILL,
                        Stroke::new(1.0, DISCARD_EDGE),
                    ))
                    .clicked()
                {
                    choice = Some(Choice::Discard);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(button(
                            save,
                            SAVE_TEXT,
                            SAVE_FILL,
                            Stroke::new(2.0, SAVE_EDGE),
                        ))
                        .clicked()
                    {
                        choice = Some(Choice::Save);
                    }
                    if ui
                        .add(button(
                            "Keep editing",
                            INK,
                            KEEP_FILL,
                            Stroke::new(1.0, Color32::from_rgb(0xcf, 0xd6, 0xdf)),
                        ))
                        .clicked()
                    {
                        choice = Some(Choice::Keep);
                    }
                });
            });
            ui.add_space(10.0);
            ui.label(
                RichText::new(format!("Enter: {save}   \u{b7}   Esc: keep editing"))
                    .size(12.0)
                    .color(Color32::from_rgb(0x6b, 0x76, 0x85)),
            );
        });
    if choice.is_none() && ctx.input(|i| i.key_pressed(Key::Enter)) {
        choice = Some(Choice::Save);
    }
    // Esc or a click on the dimmed window keeps editing.
    if choice.is_none() && modal.should_close() {
        choice = Some(Choice::Keep);
    }
    choice
}
