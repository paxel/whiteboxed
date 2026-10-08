//! The small dialogs: connect, add/edit box, edit relation, conflicts, deletes,
//! open ends and crash recovery. Each one only edits its form; what happens on
//! confirm is up to the editor.

use egui::{Color32, Context, Id, Key, Pos2, RichText, Ui};

use crate::editor::{BlockForm, ConnectMode, Editor, OpenEnd, Popup, Target};
use crate::model::{BlockId, BlockKind, Direction, LineStyle, Side};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Confirm,
    Cancel,
    Keep,
    Pick,
    Discard,
}

const ERROR: Color32 = Color32::from_rgb(0xc6, 0x28, 0x28);

/// Shows the popup and returns what the user chose this frame.
pub fn show(
    ctx: &Context,
    editor: &Editor,
    popup: &mut Popup,
    anchor: Pos2,
    serial: u64,
    focus: &mut bool,
) -> Option<Action> {
    let (title, salt) = match popup {
        Popup::AddBlock(..) => ("Add box", "add"),
        Popup::Connect(_) => ("Connect", "connect"),
        Popup::EditBlock(..) => ("Edit box", "edit-box"),
        Popup::EditRelation { .. } => ("Edit relation", "edit-rel"),
        Popup::Conflict { .. } => ("Box is in the way", "conflict"),
        Popup::ConfirmDelete { .. } => ("Delete box", "delete"),
        Popup::OpenEnd { .. } => ("Connect open end", "open-end"),
        Popup::Restore(_) => ("Unsaved changes found", "restore"),
    };
    let mut action = None;
    // Every opening gets its own window id: egui 0.35 keeps a window where it was
    // shown before and ignores `current_pos` for title-bar windows.
    egui::Window::new(title)
        .id(Id::new(("popup", salt, serial)))
        .collapsible(false)
        .resizable(false)
        .default_width(300.0)
        .default_pos(anchor)
        .show(ctx, |ui| {
            action = body(ui, editor, popup, focus);
            if let Some(message) = &editor.message {
                ui.label(RichText::new(message).color(ERROR));
            }
        });
    let keys = ctx.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
    match keys {
        _ if action.is_some() => action,
        (true, _) => Some(Action::Confirm),
        (_, true) => Some(Action::Cancel),
        _ => None,
    }
}

fn body(ui: &mut Ui, editor: &Editor, popup: &mut Popup, focus: &mut bool) -> Option<Action> {
    match popup {
        Popup::AddBlock(form, _) => {
            block_form(ui, editor, form, focus);
            ok_cancel(ui, "Add")
        }
        Popup::EditBlock(_, form) => {
            block_form(ui, editor, form, focus);
            ok_cancel(ui, "Save")
        }
        Popup::Connect(form) => {
            ui.horizontal(|ui| {
                for (mode, label) in [
                    (ConnectMode::New, "New box"),
                    (ConnectMode::Existing, "Existing"),
                    (ConnectMode::Stub, "Stub"),
                ] {
                    if ui.selectable_value(&mut form.mode, mode, label).clicked() {
                        *focus = true;
                    }
                }
            });
            ui.separator();
            let mut pick = false;
            match form.mode {
                ConnectMode::New => block_form(ui, editor, &mut form.block, focus),
                ConnectMode::Existing => {
                    let candidates = editor.connect_candidates(form.from, &form.filter);
                    pick = target_list(
                        ui,
                        &mut form.filter,
                        &mut form.target,
                        &candidates,
                        focus,
                        true,
                    );
                }
                ConnectMode::Stub => {
                    ui.label("The relation leaves this level. Every enclosing box shows it as an open end.");
                }
            }
            if form.mode != ConnectMode::Existing
                || !matches!(form.target, Some(Target::Dangling(..)))
            {
                direction_row(ui, &mut form.direction);
                text_row(ui, &mut form.text, false);
                short_row(ui, &mut form.short);
            }
            if pick {
                return Some(Action::Pick);
            }
            ok_cancel(ui, "Connect")
        }
        Popup::EditRelation {
            direction,
            text,
            style,
            short,
            sides,
            ..
        } => {
            direction_row(ui, direction);
            text_row(ui, text, std::mem::take(focus));
            short_row(ui, short);
            style_row(ui, style, editor.project.line_style);
            for (block, side) in sides.iter_mut() {
                let name = editor
                    .project
                    .blocks
                    .get(block)
                    .map_or("the box".to_owned(), |b| b.name.clone());
                side_row(ui, &name, *block, side);
            }
            if !sides.is_empty() {
                ui.weak("Moving a box sets its sides back to the ones facing its partners.");
            }
            ok_cancel(ui, "Save")
        }
        Popup::Conflict { pending, broken } => {
            let name = |id| {
                editor
                    .project
                    .blocks
                    .get(&id)
                    .map_or("the box".to_owned(), |b| b.name.clone())
            };
            let target = match pending {
                crate::editor::Pending::Existing { target, .. } => name(*target),
                crate::editor::Pending::Stub { target, .. } => name(*target),
            };
            let n = broken.len();
            ui.label(format!(
                "Moving {target} to that side breaks the side of {n} other relation{}.",
                if n == 1 { "" } else { "s" }
            ));
            let mut action = None;
            ui.horizontal(|ui| {
                if ui.button(format!("Move {target}")).clicked() {
                    action = Some(Action::Confirm);
                }
                if ui.button(format!("Keep {target}, route around")).clicked() {
                    action = Some(Action::Keep);
                }
                if ui.button("Cancel").clicked() {
                    action = Some(Action::Cancel);
                }
            });
            action
        }
        Popup::ConfirmDelete { block, count } => {
            let name = editor
                .project
                .blocks
                .get(block)
                .map_or("this box".to_owned(), |b| b.name.clone());
            ui.label(format!(
                "{name} contains {count} box{}. Delete everything?",
                if *count == 1 { "" } else { "es" }
            ));
            ok_cancel(ui, "Delete")
        }
        Popup::OpenEnd {
            end,
            target,
            filter,
        } => {
            match end {
                OpenEnd::Dangling(..) => {
                    ui.label("Attach this interface to a box of this whitebox:")
                }
                OpenEnd::Stub(_) => ui.label("Connect this open end to:"),
            };
            let candidates: Vec<(Target, String)> = editor
                .open_end_candidates(*end, filter)
                .into_iter()
                .map(|(id, name)| (Target::Block(id), name))
                .collect();
            let mut chosen = target.map(Target::Block);
            target_list(ui, filter, &mut chosen, &candidates, focus, false);
            *target = match chosen {
                Some(Target::Block(id)) => Some(id),
                _ => None,
            };
            ok_cancel(ui, "Connect")
        }
        Popup::Restore(_) => {
            ui.label("whiteboxed found changes from the last session that were never saved.");
            let mut action = None;
            ui.horizontal(|ui| {
                if ui.button("Restore them").clicked() {
                    action = Some(Action::Confirm);
                }
                if ui.button("Discard them").clicked() {
                    action = Some(Action::Discard);
                }
            });
            action
        }
    }
}

fn block_form(ui: &mut Ui, editor: &Editor, form: &mut BlockForm, focus: &mut bool) {
    egui::Grid::new("block-form").num_columns(2).show(ui, |ui| {
        ui.label("Name");
        let name = ui.text_edit_singleline(&mut form.name);
        if std::mem::take(focus) {
            name.request_focus();
        }
        ui.end_row();

        ui.label("Type");
        egui::ComboBox::from_id_salt("block-kind")
            .selected_text(form.kind.label())
            .show_ui(ui, |ui| {
                for kind in editor.kinds() {
                    ui.selectable_value(&mut form.kind, kind, kind.label());
                }
            });
        ui.end_row();

        ui.label("Tag");
        ui.add(egui::TextEdit::singleline(&mut form.tag).hint_text("none"));
        ui.end_row();
    });
    let suggestions = editor.tag_suggestions(&form.tag);
    if !suggestions.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for name in suggestions.into_iter().take(8) {
                let fill = editor
                    .project
                    .tag_by_name(&name)
                    .and_then(|t| editor.project.tags.get(&t))
                    .map(|t| super::canvas::color(t.color));
                let button = egui::Button::new(RichText::new(&name).color(Color32::BLACK));
                let button = match fill {
                    Some(f) => button.fill(f),
                    None => button,
                };
                if ui.add(button).clicked() {
                    form.tag = name;
                }
            }
        });
    }
    if !editor.kinds().contains(&BlockKind::Person) && form.kind.is_neighbour() {
        form.kind = BlockKind::Component;
    }
}

/// A filter field and a list of targets. Returns whether "pick in diagram" was hit.
fn target_list(
    ui: &mut Ui,
    filter: &mut String,
    target: &mut Option<Target>,
    candidates: &[(Target, String)],
    focus: &mut bool,
    allow_pick: bool,
) -> bool {
    let field = ui.add(egui::TextEdit::singleline(filter).hint_text("filter by name"));
    if std::mem::take(focus) {
        field.request_focus();
    }
    if !candidates.iter().any(|(t, _)| Some(*t) == *target) {
        *target = candidates.first().map(|(t, _)| *t);
    }
    egui::ScrollArea::vertical()
        .max_height(180.0)
        .show(ui, |ui| {
            if candidates.is_empty() {
                ui.label("Nothing to connect to.");
            }
            for (t, label) in candidates {
                ui.selectable_value(target, Some(*t), label);
            }
        });
    allow_pick && ui.button("Pick in diagram").clicked()
}

fn short_row(ui: &mut Ui, short: &mut String) {
    ui.horizontal(|ui| {
        ui.label("Short label");
        ui.add(
            egui::TextEdit::singleline(short)
                .hint_text("optional, shown on the line instead of the text")
                .desired_width(220.0),
        );
    });
}

fn style_row(ui: &mut Ui, style: &mut Option<LineStyle>, project: LineStyle) {
    ui.horizontal(|ui| {
        ui.label("Line");
        let shown = match style {
            None => format!("project default ({})", project.label()),
            Some(s) => s.label().to_owned(),
        };
        egui::ComboBox::from_id_salt("relation-style")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    style,
                    None,
                    format!("project default ({})", project.label()),
                );
                for s in LineStyle::ALL {
                    ui.selectable_value(style, Some(s), s.label());
                }
            });
    });
}

fn side_row(ui: &mut Ui, name: &str, block: BlockId, side: &mut Side) {
    ui.horizontal(|ui| {
        ui.label(format!("Side at {name}"));
        egui::ComboBox::from_id_salt(("relation-side", block))
            .selected_text(side.label())
            .show_ui(ui, |ui| {
                for s in Side::ALL {
                    ui.selectable_value(side, s, s.label());
                }
            });
    });
}

fn direction_row(ui: &mut Ui, direction: &mut Direction) {
    ui.horizontal(|ui| {
        ui.label("Direction");
        for d in Direction::ALL {
            ui.selectable_value(direction, d, d.label());
        }
    });
}

fn text_row(ui: &mut Ui, text: &mut String, focus: bool) {
    ui.horizontal(|ui| {
        ui.label("Text");
        let field = ui.add(egui::TextEdit::singleline(text).hint_text("optional"));
        if focus {
            field.request_focus();
        }
    });
}

fn ok_cancel(ui: &mut Ui, ok: &str) -> Option<Action> {
    let mut action = None;
    ui.horizontal(|ui| {
        if ui.button(ok).clicked() {
            action = Some(Action::Confirm);
        }
        if ui.button("Cancel").clicked() {
            action = Some(Action::Cancel);
        }
    });
    action
}
