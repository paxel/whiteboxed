//! How readable a diagram is: the number of boxes (on a doodle page: shapes), the
//! lines that cross, the busiest box, the lines through mouseholes, and the line
//! looks in use, each judged against limits the file can change. The worst of the
//! five gives the diagram's colour.

use std::collections::BTreeMap;

use crate::layout::{self, Layout};
use crate::model::{BlockId, BlockKind, DiagramId, Project, ScoreLimits};
use crate::view::{self, ViewEnd};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Green,
    Yellow,
    Red,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Green => "good",
            Level::Yellow => "getting crowded",
            Level::Red => "hard to read",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Score {
    pub boxes: usize,
    pub crossings: usize,
    /// The box with the most lines, and how many.
    pub busiest: Option<(BlockId, String, usize)>,
    pub boxes_level: Level,
    pub crossings_level: Level,
    pub lines_level: Level,
    /// Lines through mouseholes: pairs of holes to match up.
    pub mouseholes: usize,
    pub mouseholes_level: Level,
    /// Line looks in use: kinds of line to tell apart.
    pub line_looks: usize,
    pub line_looks_level: Level,
    /// A doodle page: shapes instead of boxes, and help without arc42.
    pub doodle: bool,
}

impl Score {
    /// The worst of the five measures.
    pub fn level(&self) -> Level {
        self.boxes_level
            .max(self.crossings_level)
            .max(self.lines_level)
            .max(self.mouseholes_level)
            .max(self.line_looks_level)
    }

    /// What is not green, each with what helps.
    pub fn causes(&self) -> Vec<Cause> {
        let mut out = Vec::new();
        if self.boxes_level > Level::Green {
            out.push(Cause {
                level: self.boxes_level,
                what: format!("{} {}", self.boxes, self.things()),
                help: if self.doodle {
                    "Move part of the sketch to a page of its own (+ Page), or leave out \
                     shapes that add little."
                        .into()
                } else {
                    "Group boxes that belong together into a new box (Ctrl+click them, then \
                     right-click > Group into new box), or move details into the whitebox of \
                     a box. A concern that every part uses, such as logging or security, can \
                     be a cross-cutting band or a concept in arc42 section 8."
                        .into()
                },
            });
        }
        if self.crossings_level > Level::Green {
            out.push(Cause {
                level: self.crossings_level,
                what: format!(
                    "{} crossing{}",
                    self.crossings,
                    if self.crossings == 1 { "" } else { "s" }
                ),
                help: if self.doodle {
                    "Drag shapes so partners face each other. If lines still cross, the page \
                     may hold too much: split it."
                        .into()
                } else {
                    "Drag boxes so partners face each other. If lines still cross, the \
                     diagram may hold too much: group boxes."
                        .into()
                },
            });
        }
        if let (true, Some((_, name, n))) = (self.lines_level > Level::Green, &self.busiest) {
            out.push(Cause {
                level: self.lines_level,
                what: format!("{name} has {n} lines"),
                help: if self.doodle {
                    format!("{name} may do too much. Split it into several shapes.")
                } else {
                    format!(
                        "{name} may do too much. Split it into several boxes, or move partners \
                         that only it talks to into its whitebox (right-click > Move into…)."
                    )
                },
            });
        }
        if self.mouseholes_level > Level::Green {
            out.push(Cause {
                level: self.mouseholes_level,
                what: format!(
                    "{} line{} through mouseholes",
                    self.mouseholes,
                    if self.mouseholes == 1 { "" } else { "s" }
                ),
                help: format!(
                    "Finding the other hole gets hard. Move partners closer together so \
                     their lines can be drawn, or set the lines that matter to never use \
                     mouseholes (right-click the line > Edit…), and move {} of the rest \
                     elsewhere.",
                    self.things()
                ),
            });
        }
        if self.line_looks_level > Level::Green {
            out.push(Cause {
                level: self.line_looks_level,
                what: format!("{} kinds of line", self.line_looks),
                help: "Many kinds of line are hard to tell apart, legend or not. Use fewer \
                       line looks here, or say the difference in the lines' texts."
                    .into(),
            });
        }
        out
    }

    /// What the diagram holds: boxes, or on a doodle page shapes.
    pub fn things(&self) -> &'static str {
        if self.doodle { "shapes" } else { "boxes" }
    }

    /// For a tooltip where no details panel explains it (doodle pages): the summary,
    /// then what helps.
    pub fn explained(&self) -> String {
        let mut out = self.summary();
        for cause in self.causes() {
            out.push_str("\n\n");
            out.push_str(&cause.help);
        }
        out
    }

    /// One line for a tooltip: the colour and its causes.
    pub fn summary(&self) -> String {
        let causes: Vec<String> = self.causes().into_iter().map(|c| c.what).collect();
        if causes.is_empty() {
            format!(
                "Readability: {} ({} {}, no crossings)",
                self.level().label(),
                self.boxes,
                self.things()
            )
        } else {
            format!(
                "Readability: {} ({})",
                self.level().label(),
                causes.join(", ")
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cause {
    pub level: Level,
    pub what: String,
    pub help: String,
}

/// The level of `value`: yellow from `limits.0`, red from `limits.1`.
fn judge(value: usize, limits: (u32, u32)) -> Level {
    if value >= limits.1 as usize {
        Level::Red
    } else if value >= limits.0 as usize {
        Level::Yellow
    } else {
        Level::Green
    }
}

/// The score of a diagram, from its layout.
pub fn score(project: &Project, l: &Layout) -> Score {
    let limits: ScoreLimits = project.score_limits;
    // Groups only frame shapes; they are not counted themselves.
    let boxes = l
        .blocks
        .iter()
        .filter(|g| {
            project
                .blocks
                .get(&g.id)
                .is_none_or(|b| b.kind != BlockKind::Group)
        })
        .count();
    let crossings = layout::crossings(l).len();
    let mouseholes = l.lines.iter().filter(|g| g.mousehole.is_some()).count();
    let line_looks = l
        .lines
        .iter()
        .filter_map(|g| g.look.as_ref().map(|k| &k.id))
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let v = view::diagram_view(project, l.diagram);
    let mut per_box: BTreeMap<BlockId, usize> = BTreeMap::new();
    for line in &v.lines {
        for end in [&line.a, &line.b] {
            if let ViewEnd::Block { block, .. } = end {
                *per_box.entry(*block).or_default() += 1;
            }
        }
    }
    let busiest = per_box
        .iter()
        .max_by_key(|(id, n)| (**n, std::cmp::Reverse(**id)))
        .map(|(id, n)| {
            let name = project
                .blocks
                .get(id)
                .map_or_else(|| "?".to_owned(), |b| b.name.clone());
            (*id, name, *n)
        });
    Score {
        boxes,
        crossings,
        boxes_level: judge(boxes, limits.boxes),
        crossings_level: judge(crossings, limits.crossings),
        lines_level: judge(busiest.as_ref().map_or(0, |b| b.2), limits.lines),
        mouseholes,
        mouseholes_level: judge(mouseholes, limits.mouseholes),
        line_looks,
        line_looks_level: judge(line_looks, limits.line_looks),
        busiest,
        doodle: project.is_doodle(),
    }
}

/// The score of a diagram, laid out from scratch.
pub fn of(project: &Project, diagram: DiagramId) -> Score {
    score(
        project,
        &layout::layout(project, &view::diagram_view(project, diagram)),
    )
}
