//! arc42 text export: one AsciiDoc or Markdown file per diagram, with the image, the
//! motivation and the tables arc42 asks for, plus an index that ties them together.

use crate::model::{BlockId, DiagramId, Direction, End, Project, RelationId};
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

/// Makes text typed by a person (or an AI) inert: no includes, passthroughs or
/// attribute references in AsciiDoc, no raw HTML or links in Markdown.
fn inert(format: DocFormat, text: &str) -> String {
    match format {
        // Control characters never reach the model, so U+0001 is a safe placeholder.
        DocFormat::AsciiDoc => text
            .replace('\\', "\u{1}")
            .replace('{', "\\{")
            .replace('+', "{plus}")
            .replace("pass:", "\\pass:")
            .replace('\u{1}', "{backslash}")
            .lines()
            .map(|line| {
                let start = line.trim_start();
                let directive = ["include::", "ifdef::", "ifndef::", "ifeval::", "endif::"]
                    .iter()
                    .any(|d| start.starts_with(d));
                if directive {
                    format!("\\{start}")
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        DocFormat::Markdown => text
            .replace('\\', "\\\\")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('[', "\\[")
            .replace(']', "\\]"),
    }
}

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
        let text = inert(self.format, text);
        let mark = match self.format {
            DocFormat::AsciiDoc => "=",
            DocFormat::Markdown => "#",
        };
        self.out
            .push_str(&format!("{} {text}\n\n", mark.repeat(level + 1)));
    }

    fn paragraph(&mut self, text: &str) {
        let text = inert(self.format, text.trim());
        self.out.push_str(&text);
        self.out.push_str("\n\n");
    }

    /// An image; nothing when there is no file (the export wrote no picture).
    fn image(&mut self, file: &str, alt: &str) {
        if file.is_empty() {
            return;
        }
        let alt = inert(self.format, alt).replace(']', "\\]");
        match self.format {
            DocFormat::AsciiDoc => self.out.push_str(&format!("image::{file}[{alt}]\n\n")),
            DocFormat::Markdown => self.out.push_str(&format!("![{alt}](<{file}>)\n\n")),
        }
    }

    fn link_item(&mut self, file: &str, text: &str) {
        let text = inert(self.format, text);
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
            let escaped = inert(self.format, text).replace('|', "\\|");
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
    let text = diagram_text(project, diagram);
    let mut w = Writer::new(format);
    w.heading(2, &text.title);
    let alt = if diagram.is_none() {
        "Context view"
    } else {
        text.title.as_str()
    };
    w.image(image, alt);
    if !text.motivation.is_empty() {
        if text.motivation_heading {
            w.heading(3, "Motivation");
        }
        w.paragraph(&text.motivation);
    }
    for table in &text.tables {
        w.heading(3, table.title);
        let rows: Vec<Vec<String>> = table.rows.iter().map(|r| r.cells.clone()).collect();
        w.table(table.header, &rows);
    }
    w.out
}

fn name(project: &Project, id: BlockId) -> String {
    project
        .blocks
        .get(&id)
        .map_or_else(|| "?".to_owned(), |b| b.name.clone())
}

/// What a table row is about, so an HTML export can link it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKey {
    Box(BlockId),
    Relation(RelationId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub key: RowKey,
    pub cells: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub title: &'static str,
    pub header: &'static [&'static str],
    pub rows: Vec<Row>,
}

/// The texts of one diagram, before they become AsciiDoc, Markdown or HTML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramText {
    /// "Context" or "Whitebox <name>".
    pub title: String,
    pub motivation: String,
    /// Whether the motivation gets its own heading (whiteboxes) or stands as the
    /// explanation under the image (context).
    pub motivation_heading: bool,
    pub tables: Vec<Table>,
}

fn responsibilities(project: &Project, diagram: DiagramId, own_only: bool) -> Vec<Row> {
    let mut rows: Vec<(String, BlockId, String)> = project
        .blocks_in(diagram)
        .filter(|(_, b)| !own_only || !b.kind.is_neighbour())
        .map(|(id, b)| (b.name.clone(), id, b.responsibility.clone()))
        .collect();
    rows.sort_by_key(|(n, _, _)| n.to_lowercase());
    rows.into_iter()
        .map(|(n, id, r)| Row {
            key: RowKey::Box(id),
            cells: vec![n, r],
        })
        .collect()
}

/// The texts of a diagram: title, motivation and the arc42 tables.
pub fn diagram_text(project: &Project, diagram: DiagramId) -> DiagramText {
    match diagram {
        None => context_text(project),
        Some(id) => whitebox_text(project, id),
    }
}

fn context_text(project: &Project) -> DiagramText {
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
            // A line without arrows goes both ways as far as the table can tell.
            let plain = rel.direction == Direction::Undirected;
            if into_system || plain {
                input.push(item.clone());
            }
            if into_partner || plain {
                output.push(item);
            }
        }
        let description = project
            .blocks
            .get(&partner)
            .map(|b| b.responsibility.clone())
            .unwrap_or_default();
        rows.push(Row {
            key: RowKey::Box(partner),
            cells: vec![
                name(project, partner),
                description,
                input.join("; "),
                output.join("; "),
            ],
        });
    }
    let mut tables = Vec::new();
    if !rows.is_empty() {
        tables.push(Table {
            title: "Communication partners",
            header: &["Partner", "Description", "Input", "Output"],
            rows,
        });
    }
    let blocks = responsibilities(project, None, true);
    if !blocks.is_empty() {
        tables.push(Table {
            title: "Building blocks",
            header: &["Name", "Responsibility"],
            rows: blocks,
        });
    }
    DiagramText {
        title: "Context".into(),
        motivation: project.motivation.trim().to_owned(),
        motivation_heading: false,
        tables,
    }
}

/// "in" when the arrow points into the box, "out" when it points away, "none" for a
/// line without arrows.
fn direction_word(toward_first: bool, toward_second: bool) -> &'static str {
    match (toward_first, toward_second) {
        (true, true) => "bi",
        (true, false) => "in",
        (false, true) => "out",
        (false, false) => "none",
    }
}

fn whitebox_text(project: &Project, owner: BlockId) -> DiagramText {
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
        let key = RowKey::Relation(line.relation);
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
                external.push(Row {
                    key,
                    cells: vec![
                        end_name(line.end(frame_end)),
                        end_name(inner),
                        direction_word(into_inner, into_frame).to_owned(),
                        line.text.clone(),
                    ],
                });
            }
            None => internal.push(Row {
                key,
                cells: vec![
                    end_name(&line.a),
                    end_name(&line.b),
                    line.direction.label().to_owned(),
                    line.text.clone(),
                ],
            }),
        }
    }
    let mut tables = vec![Table {
        title: "Contained building blocks",
        header: &["Name", "Responsibility"],
        rows: responsibilities(project, Some(owner), false),
    }];
    if !external.is_empty() {
        tables.push(Table {
            title: "External interfaces",
            header: &["Partner outside", "Handled by", "Direction", "Text"],
            rows: external,
        });
    }
    if !internal.is_empty() {
        tables.push(Table {
            title: "Internal relations",
            header: &["From", "To", "Direction", "Text"],
            rows: internal,
        });
    }
    DiagramText {
        title: format!("Whitebox {}", name(project, owner)),
        motivation: project.motivation(Some(owner)).trim().to_owned(),
        motivation_heading: true,
        tables,
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
