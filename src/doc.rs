//! arc42 text export: one AsciiDoc or Markdown file per diagram, with the image, the
//! motivation and the tables arc42 asks for, plus an index that ties them together.

use crate::model::{BlockId, DiagramId, End, Project};
use crate::view::{self, ViewEnd};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocFormat {
    AsciiDoc,
    Markdown,
}

impl DocFormat {
    pub fn extension(self) -> &'static str {
        match self {
            DocFormat::AsciiDoc => "adoc",
            DocFormat::Markdown => "md",
        }
    }
}

const NONE: &str = "\u{2013}";

/// A small writer that knows both markups.
struct Writer {
    format: DocFormat,
    out: String,
}

impl Writer {
    fn new(format: DocFormat) -> Self {
        Writer {
            format,
            out: String::new(),
        }
    }

    /// `level` 0 is the document title, 1 a chapter, 2 a diagram, 3 its parts.
    fn heading(&mut self, level: usize, text: &str) {
        let mark = match self.format {
            DocFormat::AsciiDoc => "=",
            DocFormat::Markdown => "#",
        };
        self.out
            .push_str(&format!("{} {text}\n\n", mark.repeat(level + 1)));
    }

    fn paragraph(&mut self, text: &str) {
        self.out.push_str(text.trim());
        self.out.push_str("\n\n");
    }

    fn image(&mut self, file: &str, alt: &str) {
        match self.format {
            DocFormat::AsciiDoc => self.out.push_str(&format!("image::{file}[{alt}]\n\n")),
            DocFormat::Markdown => self.out.push_str(&format!("![{alt}](<{file}>)\n\n")),
        }
    }

    fn link_item(&mut self, file: &str, text: &str) {
        self.out.push_str(&format!("- [{text}](<{file}>)\n"));
    }

    fn include(&mut self, file: &str) {
        self.out.push_str(&format!("include::{file}[]\n\n"));
    }

    fn table(&mut self, header: &[&str], rows: &[Vec<String>]) {
        let cell = |text: &str| -> String {
            let text = if text.trim().is_empty() {
                NONE
            } else {
                text.trim()
            };
            let escaped = text.replace('|', "\\|");
            match self.format {
                DocFormat::AsciiDoc => escaped.lines().collect::<Vec<_>>().join(" +\n"),
                DocFormat::Markdown => escaped.lines().collect::<Vec<_>>().join("<br>"),
            }
        };
        match self.format {
            DocFormat::AsciiDoc => {
                self.out.push_str("[options=\"header\"]\n|===\n");
                self.out.push_str(
                    &header
                        .iter()
                        .map(|h| format!("|{h}"))
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                self.out.push('\n');
                for row in rows {
                    self.out.push('\n');
                    for c in row {
                        self.out.push_str(&format!("|{}\n", cell(c)));
                    }
                }
                self.out.push_str("|===\n\n");
            }
            DocFormat::Markdown => {
                // Padded so the pipes line up in the raw file too.
                let body: Vec<Vec<String>> = rows
                    .iter()
                    .map(|r| r.iter().map(|c| cell(c)).collect())
                    .collect();
                let width = |i: usize| {
                    body.iter()
                        .filter_map(|r| r.get(i))
                        .map(|c| c.chars().count())
                        .chain(std::iter::once(header[i].chars().count()))
                        .max()
                        .unwrap_or(3)
                        .max(3)
                };
                let widths: Vec<usize> = (0..header.len()).map(width).collect();
                let line = |cells: Vec<String>| {
                    let padded: Vec<String> = cells
                        .iter()
                        .zip(&widths)
                        .map(|(c, w)| format!("{c}{}", " ".repeat(w - c.chars().count())))
                        .collect();
                    format!("| {} |\n", padded.join(" | "))
                };
                self.out
                    .push_str(&line(header.iter().map(|h| (*h).to_owned()).collect()));
                let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w + 2)).collect();
                self.out.push_str(&format!("|{}|\n", rule.join("|")));
                for row in body {
                    self.out.push_str(&line(row));
                }
                self.out.push('\n');
            }
        }
    }
}

/// Every diagram worth exporting: the context view and each whitebox with content,
/// by level and then by name.
pub fn diagrams(project: &Project) -> Vec<DiagramId> {
    let mut whiteboxes: Vec<(usize, String, BlockId)> = project
        .blocks
        .iter()
        .filter(|(id, _)| project.has_content(**id))
        .map(|(id, b)| (project.level(Some(*id)), b.name.to_lowercase(), *id))
        .collect();
    whiteboxes.sort();
    std::iter::once(None)
        .chain(whiteboxes.into_iter().map(|(_, _, id)| Some(id)))
        .collect()
}

/// The text of one diagram; `image` is the file name of its SVG.
pub fn diagram_doc(
    project: &Project,
    diagram: DiagramId,
    image: &str,
    format: DocFormat,
) -> String {
    let mut w = Writer::new(format);
    match diagram {
        None => context_doc(&mut w, project, image),
        Some(id) => whitebox_doc(&mut w, project, id, image),
    }
    w.out
}

fn name(project: &Project, id: BlockId) -> String {
    project
        .blocks
        .get(&id)
        .map_or_else(|| "?".to_owned(), |b| b.name.clone())
}

fn responsibilities(project: &Project, diagram: DiagramId, own_only: bool) -> Vec<Vec<String>> {
    let mut rows: Vec<(String, String)> = project
        .blocks_in(diagram)
        .filter(|(_, b)| !own_only || !b.kind.is_neighbour())
        .map(|(_, b)| (b.name.clone(), b.responsibility.clone()))
        .collect();
    rows.sort_by_key(|(n, _)| n.to_lowercase());
    rows.into_iter().map(|(n, r)| vec![n, r]).collect()
}

fn context_doc(w: &mut Writer, project: &Project, image: &str) {
    w.heading(2, "Context");
    w.image(image, "Context view");
    if !project.motivation.trim().is_empty() {
        w.paragraph(&project.motivation);
    }

    let own: Vec<BlockId> = project
        .blocks_in(None)
        .filter(|(_, b)| !b.kind.is_neighbour())
        .map(|(id, _)| id)
        .collect();
    let mut partners: Vec<(String, BlockId)> = project
        .blocks_in(None)
        .filter(|(_, b)| b.kind.is_neighbour())
        .map(|(id, b)| (b.name.to_lowercase(), id))
        .collect();
    partners.sort();
    let mut rows = Vec::new();
    for (_, partner) in partners {
        let mut input = Vec::new();
        let mut output = Vec::new();
        for rel in project.relations.values().filter(|r| r.owner.is_none()) {
            let (Some(a), Some(b)) = (rel.a.anchors.first(), rel.b.anchors.first()) else {
                continue;
            };
            let (partner_end, other) = if a.block == partner {
                (End::A, b.block)
            } else if b.block == partner {
                (End::B, a.block)
            } else {
                continue;
            };
            if !own.contains(&other) {
                continue;
            }
            let mut item = if rel.text.is_empty() {
                format!("({})", name(project, other))
            } else {
                rel.text.clone()
            };
            if own.len() > 1 && !rel.text.is_empty() {
                item = format!("{item} ({})", name(project, other));
            }
            let into_partner = match partner_end {
                End::A => rel.direction.arrow_at_a(),
                End::B => rel.direction.arrow_at_b(),
            };
            let into_system = match partner_end {
                End::A => rel.direction.arrow_at_b(),
                End::B => rel.direction.arrow_at_a(),
            };
            if into_system {
                input.push(item.clone());
            }
            if into_partner {
                output.push(item);
            }
        }
        let description = project
            .blocks
            .get(&partner)
            .map(|b| b.responsibility.clone())
            .unwrap_or_default();
        rows.push(vec![
            name(project, partner),
            description,
            input.join("; "),
            output.join("; "),
        ]);
    }
    if !rows.is_empty() {
        w.heading(3, "Communication partners");
        w.table(&["Partner", "Description", "Input", "Output"], &rows);
    }
    let blocks = responsibilities(project, None, true);
    if !blocks.is_empty() {
        w.heading(3, "Building blocks");
        w.table(&["Name", "Responsibility"], &blocks);
    }
}

/// "in" when the arrow points into the box, "out" when it points away.
fn direction_word(toward_first: bool, toward_second: bool) -> &'static str {
    match (toward_first, toward_second) {
        (true, true) => "bi",
        (true, false) => "in",
        (false, true) => "out",
        (false, false) => NONE,
    }
}

fn whitebox_doc(w: &mut Writer, project: &Project, owner: BlockId, image: &str) {
    let title = name(project, owner);
    w.heading(2, &format!("Whitebox {title}"));
    w.image(image, &format!("Whitebox {title}"));
    let motivation = project.motivation(Some(owner));
    if !motivation.trim().is_empty() {
        w.heading(3, "Motivation");
        w.paragraph(motivation);
    }
    w.heading(3, "Contained building blocks");
    w.table(
        &["Name", "Responsibility"],
        &responsibilities(project, Some(owner), false),
    );

    let view = view::diagram_view(project, Some(owner));
    let mut external = Vec::new();
    let mut internal = Vec::new();
    for line in &view.lines {
        let end_name = |end: &ViewEnd| match end {
            ViewEnd::Block { block, .. } => name(project, *block),
            ViewEnd::Open => "open".to_owned(),
            ViewEnd::Dangling => "not assigned".to_owned(),
            ViewEnd::Frame { partner, .. } => partner.clone(),
        };
        let frame = [End::A, End::B]
            .into_iter()
            .find(|e| matches!(line.end(*e), ViewEnd::Frame { .. }));
        match frame {
            Some(frame_end) => {
                let inner = line.end(frame_end.other());
                let (into_inner, into_frame) = match frame_end {
                    End::A => (line.direction.arrow_at_b(), line.direction.arrow_at_a()),
                    End::B => (line.direction.arrow_at_a(), line.direction.arrow_at_b()),
                };
                external.push(vec![
                    end_name(line.end(frame_end)),
                    end_name(inner),
                    direction_word(into_inner, into_frame).to_owned(),
                    line.text.clone(),
                ]);
            }
            None => internal.push(vec![
                end_name(&line.a),
                end_name(&line.b),
                line.direction.label().to_owned(),
                line.text.clone(),
            ]),
        }
    }
    if !external.is_empty() {
        w.heading(3, "External interfaces");
        w.table(
            &["Partner outside", "Handled by", "Direction", "Text"],
            &external,
        );
    }
    if !internal.is_empty() {
        w.heading(3, "Internal relations");
        w.table(&["From", "To", "Direction", "Text"], &internal);
    }
}

/// The index over all diagram files. `entries` are (diagram, file stem) in order.
pub fn index_doc(
    project: &Project,
    title: &str,
    entries: &[(DiagramId, String)],
    format: DocFormat,
) -> String {
    let mut w = Writer::new(format);
    let ext = format.extension();
    w.heading(0, title);
    w.heading(1, "Context and scope");
    let label = |d: DiagramId| match d {
        None => "Context".to_owned(),
        Some(id) => format!("Whitebox {}", name(project, id)),
    };
    let (context, whiteboxes): (Vec<_>, Vec<_>) = entries.iter().partition(|(d, _)| d.is_none());
    for (d, stem) in context {
        match format {
            DocFormat::AsciiDoc => w.include(&format!("{stem}.{ext}")),
            DocFormat::Markdown => w.link_item(&format!("{stem}.{ext}"), &label(*d)),
        }
    }
    if format == DocFormat::Markdown {
        w.out.push('\n');
    }
    if !whiteboxes.is_empty() {
        w.heading(1, "Building block view");
        for (d, stem) in whiteboxes {
            match format {
                DocFormat::AsciiDoc => w.include(&format!("{stem}.{ext}")),
                DocFormat::Markdown => w.link_item(&format!("{stem}.{ext}"), &label(*d)),
            }
        }
        if format == DocFormat::Markdown {
            w.out.push('\n');
        }
    }
    w.out
}
