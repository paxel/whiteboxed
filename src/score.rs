//! How readable a diagram is: the number of boxes, the lines that cross, and the
//! busiest box, each judged against limits the project can change. The worst of the
//! three gives the diagram's colour.

use std::collections::BTreeMap;

use crate::layout::{self, Layout};
use crate::model::{BlockId, DiagramId, Project, ScoreLimits};
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
}

impl Score {
    /// The worst of the three measures.
    pub fn level(&self) -> Level {
        self.boxes_level
            .max(self.crossings_level)
            .max(self.lines_level)
    }

    /// What is not green, each with what helps.
    pub fn causes(&self) -> Vec<Cause> {
        let mut out = Vec::new();
        if self.boxes_level > Level::Green {
            out.push(Cause {
                level: self.boxes_level,
                what: format!("{} boxes", self.boxes),
                help: "Group boxes that belong together into a new box (Ctrl+click them, then \
                       right-click > Group into new box), or move details into the whitebox of \
                       a box. A concern that every part uses, such as logging or security, can \
                       be a cross-cutting band or a concept in arc42 section 8."
                    .into(),
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
                help: "Drag boxes so partners face each other. If lines still cross, the \
                       diagram may hold too much: group boxes."
                    .into(),
            });
        }
        if let (true, Some((_, name, n))) = (self.lines_level > Level::Green, &self.busiest) {
            out.push(Cause {
                level: self.lines_level,
                what: format!("{name} has {n} lines"),
                help: format!(
                    "{name} may do too much. Split it into several boxes, or move partners \
                     that only it talks to into its whitebox (right-click > Move into…)."
                ),
            });
        }
        out
    }

    /// One line for a tooltip: the colour and its causes.
    pub fn summary(&self) -> String {
        let causes: Vec<String> = self.causes().into_iter().map(|c| c.what).collect();
        if causes.is_empty() {
            format!(
                "Readability: {} ({} boxes, no crossings)",
                self.level().label(),
                self.boxes
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
    let boxes = l.blocks.len();
    let crossings = layout::crossings(l).len();
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
        busiest,
    }
}

/// The score of a diagram, laid out from scratch.
pub fn of(project: &Project, diagram: DiagramId) -> Score {
    score(
        project,
        &layout::layout(project, &view::diagram_view(project, diagram)),
    )
}
