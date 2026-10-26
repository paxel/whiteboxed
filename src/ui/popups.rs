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
    /// Edit this one of the relations a line stands for.
    Edit(crate::model::RelationId),
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
        Popup::Group { .. } => ("Group into new box", "group"),
        Popup::EditRelation { .. } => ("Edit relation", "edit-rel"),
        Popup::Conflict { .. } => ("Box is in the way", "conflict"),
        Popup::ConfirmDelete { .. } => ("Delete box", "delete"),
        Popup::ConfirmDissolve { .. } => ("Dissolve whitebox", "dissolve"),
        Popup::ChooseRelation(_) => ("Which relation?", "choose"),
        Popup::Split(_) => ("Split relation", "split"),
        Popup::Frame { .. } => ("Frame as group", "frame"),
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
    // A doodle shape's text takes Enter for a new line; Ctrl+Enter confirms there.
    let lines = editor.project.is_doodle() && shape_text(popup);
    let keys = ctx.input(|i| {
        let enter = i.key_pressed(Key::Enter) && (!lines || i.modifiers.command);
        (enter, i.key_pressed(Key::Escape))
    });
    match keys {
        _ if action.is_some() => action,
        (true, _) => Some(Action::Confirm),
        (_, true) => Some(Action::Cancel),
        _ => None,
    }
}

/// Whether the popup edits the text of a shape (not a page).
fn shape_text(popup: &Popup) -> bool {
    match popup {
        Popup::AddBlock(form, _) | Popup::EditBlock(_, form) => {
            !matches!(form.kind, BlockKind::Page | BlockKind::Group)
        }
        Popup::Connect(form) => form.mode == ConnectMode::New,
        _ => false,
    }
}

fn body(ui: &mut Ui, editor: &Editor, popup: &mut Popup, focus: &mut bool) -> Option<Action> {
    match popup {
        Popup::AddBlock(form, _) => {
            block_form(ui, editor, form, focus);
            if !editor.project.is_doodle() {
                band_row(ui, &mut form.band);
            }
            ok_cancel(ui, "Add")
        }
        Popup::Group { form, boxes, .. } => {
            let n = boxes.len();
            ui.label(format!(
                "The new box takes {n} box{} into its whitebox.",
                if n == 1 { "" } else { "es" }
            ));
            block_form(ui, editor, form, focus);
            ok_cancel(ui, "Group")
        }
        Popup::EditBlock(_, form) => {
            block_form(ui, editor, form, focus);
            if !editor.project.is_doodle() {
                band_row(ui, &mut form.band);
            }
            ok_cancel(ui, "Save")
        }
        Popup::Connect(form) => {
            ui.horizontal(|ui| {
                let modes: &[(ConnectMode, &str)] = if editor.project.is_doodle() {
                    &[
                        (ConnectMode::New, "New shape"),
                        (ConnectMode::Existing, "Existing"),
                    ]
                } else {
                    &[
                        (ConnectMode::New, "New box"),
                        (ConnectMode::Existing, "Existing"),
                        (ConnectMode::Stub, "Stub"),
                    ]
                };
                for &(mode, label) in modes {
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
                line_look_row(ui, editor, &mut form.look);
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
            pattern,
            color,
            mousehole,
            look,
            frame,
            sides,
            ..
        } => {
            direction_row(ui, direction);
            text_row(ui, text, std::mem::take(focus));
            short_row(ui, short);
            line_look_row(ui, editor, look);
            style_row(ui, style, editor.project.line_style);
            if let Some((_, side)) = frame {
                ui.horizontal(|ui| {
                    ui.label("Enters at");
                    ui.selectable_value(side, None, "automatic");
                    for s in Side::ALL {
                        ui.selectable_value(side, Some(s), s.label());
                    }
                });
            }
            line_color_row(ui, color);
            ui.horizontal(|ui| {
                ui.label("Mouseholes").on_hover_text(
                    "Into a numbered hole near one end, out of its twin near the other",
                );
                for m in crate::model::MouseholeMode::ALL {
                    ui.selectable_value(mousehole, m, m.label());
                }
            });
            if editor.project.is_doodle() {
                ui.horizontal(|ui| {
                    ui.label("Pattern");
                    for p in crate::model::LinePattern::ALL {
                        ui.selectable_value(pattern, p, p.label());
                    }
                });
            }
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
        Popup::Frame { form, shapes } => {
            let n = shapes.len();
            ui.label(format!(
                "A frame around {n} shape{} and everything between them.",
                if n == 1 { "" } else { "s" }
            ));
            doodle_form(ui, editor, form, focus);
            ok_cancel(ui, "Frame")
        }
        Popup::Split(form) => {
            ui.label("Tick who talks to whom. Every ticked pair becomes its own relation.");
            let n = form.cols.len();
            egui::Grid::new("split-grid").striped(true).show(ui, |ui| {
                ui.label("");
                for (_, name) in &form.cols {
                    ui.label(RichText::new(name).strong());
                }
                ui.end_row();
                for (r, (_, name)) in form.rows.iter().enumerate() {
                    ui.label(RichText::new(name).strong());
                    for c in 0..n {
                        let k = r.saturating_mul(n).saturating_add(c);
                        if let Some(pair) = form.pairs.get_mut(k) {
                            ui.checkbox(&mut pair.on, "");
                        }
                    }
                    ui.end_row();
                }
            });
            for (i, pair) in form.pairs.iter_mut().enumerate().filter(|(_, p)| p.on) {
                let from = i
                    .checked_div(n.max(1))
                    .and_then(|k| form.rows.get(k))
                    .map_or("", |r| r.1.as_str());
                let to = i
                    .checked_rem(n.max(1))
                    .and_then(|k| form.cols.get(k))
                    .map_or("", |c| c.1.as_str());
                ui.separator();
                ui.label(RichText::new(format!("{from} \u{2013} {to}")).strong());
                ui.push_id(i, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Text");
                        ui.text_edit_singleline(&mut pair.text);
                    });
                    direction_row(ui, &mut pair.direction);
                });
            }
            ui.add_space(6.0);
            if form.pairs.iter().any(|p| p.on) {
                ok_cancel(ui, "Split")
            } else {
                ui.label(RichText::new("Tick at least one pair.").weak());
                if ui.button("Cancel").clicked() {
                    Some(Action::Cancel)
                } else {
                    None
                }
            }
        }
        Popup::ChooseRelation(rels) => {
            ui.label("This line stands for several relations. Edit:");
            let mut action = None;
            for rel in rels.iter() {
                if ui.button(editor.relation_label(*rel)).clicked() {
                    action = Some(Action::Edit(*rel));
                }
            }
            if ui.button("Cancel").clicked() {
                action = Some(Action::Cancel);
            }
            action
        }
        Popup::ConfirmDissolve { block } => {
            let b = editor.project.blocks.get(block);
            let name = b.map_or("this box".to_owned(), |b| b.name.clone());
            let count = editor.project.blocks_in(Some(*block)).count();
            ui.label(format!(
                "The {count} box{} inside {name} take its place one level up.",
                if count == 1 { "" } else { "es" }
            ));
            let mut lost = Vec::new();
            if b.is_some_and(|b| !b.responsibility.trim().is_empty()) {
                lost.push("responsibility");
            }
            if b.is_some_and(|b| !b.motivation.trim().is_empty()) {
                lost.push("motivation");
            }
            if lost.is_empty() {
                ui.label(format!("{name} itself is removed."));
            } else {
                ui.label(format!(
                    "{name} is removed with its {}. Undo brings it back.",
                    lost.join(" and ")
                ));
            }
            ok_cancel(ui, "Dissolve")
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

fn band_row(ui: &mut Ui, band: &mut bool) {
    ui.checkbox(band, "Cross-cutting band").on_hover_text(
        "A concern every part uses, such as logging or security: drawn as a band \
             across the bottom of the diagram, without lines",
    );
}

fn block_form(ui: &mut Ui, editor: &Editor, form: &mut BlockForm, focus: &mut bool) {
    // A doodle page has only a name.
    if form.kind == BlockKind::Page {
        ui.horizontal(|ui| {
            ui.label("Name");
            let name = ui.text_edit_singleline(&mut form.name);
            if std::mem::take(focus) {
                name.request_focus();
            }
        });
        return;
    }
    if editor.project.is_doodle() {
        doodle_form(ui, editor, form, focus);
        return;
    }
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

        // A look goes on top of the type: the type and its rules stay.
        let looks = editor.looks_offered();
        if !looks.is_empty() || form.look.is_some() {
            ui.label("Look");
            egui::ComboBox::from_id_salt("block-look")
                .selected_text(look_name(&looks, form.look.as_ref()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut form.look, None, "none");
                    for (id, name) in &looks {
                        ui.selectable_value(&mut form.look, Some(id.clone()), name);
                    }
                });
            ui.end_row();
        }
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

fn look_name(
    looks: &[(crate::look::LookId, String)],
    look: Option<&crate::look::LookId>,
) -> String {
    look.and_then(|l| looks.iter().find(|(id, _)| id == l))
        .map_or_else(|| "none".to_owned(), |(_, name)| name.clone())
}

/// A doodle shape: text over several lines, a shape, and a colour.
fn doodle_form(ui: &mut Ui, editor: &Editor, form: &mut BlockForm, focus: &mut bool) {
    let group = form.kind == BlockKind::Group;
    ui.label(if group { "Name" } else { "Text" });
    let field = if group {
        egui::TextEdit::singleline(&mut form.name)
    } else {
        egui::TextEdit::multiline(&mut form.name).desired_rows(3)
    };
    let text = ui.add(field.desired_width(260.0));
    if std::mem::take(focus) {
        text.request_focus();
    }
    if !group {
        ui.label(
            RichText::new("Enter starts a new line; Ctrl+Enter confirms.")
                .weak()
                .small(),
        );
    }
    // A group has a name and a colour, no shape.
    ui.horizontal(|ui| {
        if group {
            return;
        }
        ui.label("Shape");
        // The library's looks are shapes too.
        let looks = editor.looks_offered();
        let shown = match &form.look {
            Some(_) => look_name(&looks, form.look.as_ref()),
            None => form.kind.doodle_label().to_owned(),
        };
        egui::ComboBox::from_id_salt("shape-kind")
            .selected_text(shown)
            // Every shape and look at once, without scrolling.
            .height(600.0)
            .show_ui(ui, |ui| {
                for kind in editor.kinds() {
                    let on = form.look.is_none() && form.kind == kind;
                    if ui.selectable_label(on, kind.doodle_label()).clicked() {
                        form.kind = kind;
                        form.look = None;
                    }
                }
                if !looks.is_empty() {
                    ui.separator();
                }
                for (id, name) in &looks {
                    let on = form.look.as_ref() == Some(id);
                    if ui.selectable_label(on, name).clicked() {
                        form.kind = BlockKind::Component;
                        form.look = Some(id.clone());
                    }
                }
            });
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Colour");
        let none = egui::Button::new("none").selected(form.color.is_none());
        if ui.add(none).clicked() {
            form.color = None;
        }
        for c in crate::model::PALETTE {
            let mut swatch = egui::Button::new("    ").fill(super::canvas::color(c));
            if form.color == Some(c) {
                swatch = swatch.stroke(egui::Stroke::new(2.0, Color32::BLACK));
            }
            if ui.add(swatch).clicked() {
                form.color = Some(c);
            }
        }
    });
}

/// The colour of a line: the default, or any colour from the picker.
fn line_color_row(ui: &mut Ui, color: &mut Option<crate::model::Rgb>) {
    ui.horizontal(|ui| {
        ui.label("Colour");
        if ui
            .add(egui::Button::new("default").selected(color.is_none()))
            .clicked()
        {
            *color = None;
        }
        let crate::model::Rgb(r, g, b) = color.unwrap_or(crate::scene::LINE);
        let mut rgb = [r, g, b];
        if ui.color_edit_button_srgb(&mut rgb).changed() {
            *color = Some(crate::model::Rgb(rgb[0], rgb[1], rgb[2]));
        }
    });
}

/// The line look a line wears: plain, or one of the library's and the project's.
/// Shown only when there is one to choose.
fn line_look_row(ui: &mut Ui, editor: &Editor, look: &mut Option<crate::look::LookId>) {
    let looks = editor.line_looks_offered();
    if looks.is_empty() && look.is_none() {
        return;
    }
    ui.horizontal(|ui| {
        ui.label("Look");
        let shown = look
            .as_ref()
            .and_then(|l| looks.iter().find(|(id, _)| id == l))
            .map_or_else(|| "plain".to_owned(), |(_, name)| name.clone());
        egui::ComboBox::from_id_salt("line-look")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                ui.selectable_value(look, None, "plain");
                for (id, name) in &looks {
                    ui.selectable_value(look, Some(id.clone()), name);
                }
            });
    });
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
