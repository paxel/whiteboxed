//! The details panel: the responsibility of the selected box, or the motivation of
//! the current diagram when nothing is selected. Typing stays in a draft; the project
//! changes (one undo step) when the field loses focus or the target changes.

use egui::{Id, RichText, Ui};

use crate::editor::Editor;
use crate::layout::kind_caption;
use crate::model::{BlockId, DiagramId};

const FIELD: &str = "details-text";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Block(BlockId),
    Diagram(DiagramId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    target: Target,
    text: String,
    /// The field had focus last frame.
    editing: bool,
}

/// What the panel edits right now.
pub fn target(editor: &Editor) -> Target {
    match editor.selected {
        Some(b) if editor.project.blocks.contains_key(&b) => Target::Block(b),
        _ => Target::Diagram(editor.diagram),
    }
}

fn stored(editor: &Editor, target: Target) -> String {
    match target {
        Target::Block(b) => editor
            .project
            .blocks
            .get(&b)
            .map(|x| x.responsibility.clone())
            .unwrap_or_default(),
        Target::Diagram(d) => editor.project.motivation(d).to_owned(),
    }
}

/// Writes a pending draft into the project.
pub fn commit(editor: &mut Editor, draft: &Option<Draft>) {
    let Some(d) = draft else { return };
    if d.text.trim() == stored(editor, d.target).trim() {
        return;
    }
    match d.target {
        Target::Block(b) => editor.set_responsibility(b, &d.text),
        Target::Diagram(diagram) => editor.set_motivation(diagram, &d.text),
    }
}

pub fn show(ui: &mut Ui, editor: &mut Editor, draft: &mut Option<Draft>) {
    let id = Id::new(FIELD);
    let target = target(editor);
    let focused = ui.ctx().memory(|m| m.has_focus(id));
    if draft.as_ref().map(|d| d.target) != Some(target) {
        commit(editor, draft);
        *draft = None;
    }
    // Focus can be gone before the field sees it go: keep what was typed.
    if !focused && draft.as_ref().is_some_and(|d| d.editing) {
        commit(editor, draft);
    }
    // Outside the field the project is the truth (undo, redo, a new file).
    if !focused || draft.is_none() {
        *draft = Some(Draft {
            target,
            text: stored(editor, target),
            editing: false,
        });
    }
    let Some(d) = draft.as_mut() else { return };

    let (title, caption, label, hint) = match target {
        Target::Block(b) => {
            let block = editor.project.blocks.get(&b);
            (
                block.map_or_else(String::new, |x| x.name.clone()),
                block.map_or_else(String::new, |x| kind_caption(x.kind)),
                "Responsibility",
                "What is this building block responsible for?",
            )
        }
        Target::Diagram(None) => (
            "Context".to_owned(),
            "context view".to_owned(),
            "Motivation",
            "Who uses the system, and what does it talk to?",
        ),
        Target::Diagram(Some(b)) => (
            format!(
                "Whitebox {}",
                editor
                    .project
                    .blocks
                    .get(&b)
                    .map_or("", |x| x.name.as_str())
            ),
            format!("level {}", editor.project.level(Some(b))),
            "Motivation",
            "Why is this box split into these building blocks?",
        ),
    };
    ui.heading("Details");
    ui.label(RichText::new(title).strong());
    ui.label(RichText::new(caption).weak());
    ui.add_space(6.0);
    ui.label(label);
    let field = ui.add(
        egui::TextEdit::multiline(&mut d.text)
            .id(id)
            .hint_text(hint)
            .desired_rows(10)
            .desired_width(f32::INFINITY),
    );
    if field.lost_focus() {
        commit(editor, draft);
    } else if let Some(d) = draft.as_mut() {
        d.editing = field.has_focus();
    }
    ui.add_space(4.0);
    ui.label(
        RichText::new(match target {
            Target::Block(_) => "Select nothing to describe the diagram itself.",
            Target::Diagram(_) => "Select a box to describe its responsibility.",
        })
        .weak()
        .small(),
    );
}
