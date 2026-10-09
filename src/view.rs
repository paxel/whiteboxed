//! What one diagram shows: its boxes and every line that touches them, including
//! relations that enter the whitebox from a parent level.

use crate::model::{BlockId, DiagramId, Direction, End, LineStyle, Project, RelationId, Side};

/// One end of a line as drawn in a diagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewEnd {
    /// A box of this diagram, on one of its sides.
    Block { block: BlockId, side: Side },
    /// The whitebox frame: the relation continues outside, at `partner`.
    Frame { side: Side, partner: String },
    /// An open stub end on this level.
    Open,
    /// An inherited end not yet attached to a box of this whitebox.
    Dangling,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewLine {
    pub relation: RelationId,
    /// Ends in the relation's own order, so the direction applies unchanged.
    pub a: ViewEnd,
    pub b: ViewEnd,
    pub direction: Direction,
    pub text: String,
    /// The relation's own line style, or the project's.
    pub style: LineStyle,
    /// The relation's short label (empty: none).
    pub short: String,
    /// Further relations drawn as this one line (see [`bundle`]).
    pub bundle: Vec<RelationId>,
}

impl ViewLine {
    pub fn end(&self, end: End) -> &ViewEnd {
        match end {
            End::A => &self.a,
            End::B => &self.b,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramView {
    pub diagram: DiagramId,
    pub blocks: Vec<BlockId>,
    pub lines: Vec<ViewLine>,
}

/// Label for the far side of a relation that leaves a whitebox.
pub const OPEN_PARTNER: &str = "open";

pub fn diagram_view(project: &Project, diagram: DiagramId) -> DiagramView {
    let blocks = project.blocks_in(diagram).map(|(id, _)| id).collect();
    let mut lines = Vec::new();
    for (id, rel) in &project.relations {
        let end_here = |end: End| -> Option<ViewEnd> {
            let anchor = rel.end(end).anchors.first()?;
            Some(ViewEnd::Block {
                block: anchor.block,
                side: anchor.side,
            })
        };
        if rel.owner == diagram {
            let a = end_here(End::A).unwrap_or(ViewEnd::Open);
            let b = end_here(End::B).unwrap_or(ViewEnd::Open);
            lines.push(ViewLine {
                relation: *id,
                a,
                b,
                direction: rel.direction,
                text: rel.text.clone(),
                style: rel.style.unwrap_or(project.line_style),
                short: rel.short.clone(),
                bundle: Vec::new(),
            });
            continue;
        }
        let Some(owner) = diagram else { continue };
        for end in [End::A, End::B] {
            let anchors = &rel.end(end).anchors;
            let Some(side) = anchors.iter().find(|a| a.block == owner).map(|a| a.side) else {
                continue;
            };
            // One line per box it lands on inside, or one dangling line.
            let mut inners: Vec<ViewEnd> = project
                .landings(rel.end(end), owner)
                .into_iter()
                .map(|a| ViewEnd::Block {
                    block: a.block,
                    side: a.side,
                })
                .collect();
            if inners.is_empty() {
                inners.push(ViewEnd::Dangling);
            }
            let partner = partner_name(project, rel.end(end.other()), diagram);
            for inner in inners {
                let frame = ViewEnd::Frame {
                    side,
                    partner: partner.clone(),
                };
                let (a, b) = match end {
                    End::A => (inner, frame),
                    End::B => (frame, inner),
                };
                lines.push(ViewLine {
                    relation: *id,
                    a,
                    b,
                    direction: rel.direction,
                    text: rel.text.clone(),
                    style: rel.style.unwrap_or(project.line_style),
                    short: rel.short.clone(),
                    bundle: Vec::new(),
                });
            }
        }
    }
    DiagramView {
        diagram,
        blocks,
        lines,
    }
}

/// Joins lines that run between the same two ends (the same boxes, or the same frame
/// end and box) into one line: the texts are listed, the arrows show every direction
/// that occurs, and the other relations are kept in `bundle`.
pub fn bundle(view: &DiagramView) -> DiagramView {
    let key = |e: &ViewEnd| match e {
        ViewEnd::Block { block, .. } => Some(format!("b{}", block.0)),
        ViewEnd::Frame { side, partner } => Some(format!("f{side:?}{partner}")),
        ViewEnd::Open | ViewEnd::Dangling => None,
    };
    let mut lines: Vec<ViewLine> = Vec::new();
    // Arrows at the first line's end a and end b, per merged line.
    let mut arrows: Vec<(bool, bool)> = Vec::new();
    for line in &view.lines {
        let (ka, kb) = (key(&line.a), key(&line.b));
        let found = match (&ka, &kb) {
            (Some(ka), Some(kb)) => lines.iter().position(|l| {
                let (la, lb) = (key(&l.a), key(&l.b));
                (la.as_ref() == Some(ka) && lb.as_ref() == Some(kb))
                    || (la.as_ref() == Some(kb) && lb.as_ref() == Some(ka))
            }),
            _ => None,
        };
        let (at_a, at_b) = (line.direction.arrow_at_a(), line.direction.arrow_at_b());
        match found {
            Some(i) => {
                let same_way = key(&lines[i].a) == ka;
                let (to_a, to_b) = if same_way { (at_a, at_b) } else { (at_b, at_a) };
                arrows[i].0 |= to_a;
                arrows[i].1 |= to_b;
                let merged = &mut lines[i];
                merged.bundle.push(line.relation);
                if !line.text.trim().is_empty() {
                    merged.text = if merged.text.trim().is_empty() {
                        line.text.clone()
                    } else {
                        format!("{}, {}", merged.text, line.text)
                    };
                }
                merged.short = if merged.short.is_empty() || line.short.is_empty() {
                    String::new()
                } else {
                    format!("{}, {}", merged.short, line.short)
                };
            }
            None => {
                lines.push(line.clone());
                arrows.push((at_a, at_b));
            }
        }
    }
    for (line, (a, b)) in lines.iter_mut().zip(arrows) {
        if !line.bundle.is_empty() {
            line.direction = match (a, b) {
                (true, true) => Direction::Bi,
                (true, false) => Direction::In,
                (false, true) => Direction::Out,
                (false, false) => Direction::Undirected,
            };
        }
    }
    DiagramView {
        diagram: view.diagram,
        blocks: view.blocks.clone(),
        lines,
    }
}

/// The name of the far end of a line that leaves `diagram`: its box, followed by the
/// boxes it lands on inside, down to the depth of the boxes in `diagram`
/// (`B › b1, b2`).
fn partner_name(project: &Project, far: &crate::model::Endpoint, diagram: DiagramId) -> String {
    let depth = |b: BlockId| project.path(Some(b)).len();
    let Some(root) = far.anchors.first() else {
        return OPEN_PARTNER.to_owned();
    };
    let name = |b: BlockId| {
        project
            .blocks
            .get(&b)
            .map_or_else(|| "?".to_owned(), |x| x.name.clone())
    };
    // Boxes in `diagram` lie one level below it.
    let limit = project.path(diagram).len() + 1;
    let mut parts = vec![name(root.block)];
    for d in depth(root.block) + 1..=limit {
        let names: Vec<String> = far
            .anchors
            .iter()
            .filter(|a| depth(a.block) == d)
            .map(|a| name(a.block))
            .collect();
        if names.is_empty() {
            break;
        }
        parts.push(names.join(", "));
    }
    parts.join(" \u{203a} ")
}

/// Number of inherited ends in a diagram that are not attached to a box yet.
pub fn dangling_count(view: &DiagramView) -> usize {
    view.lines
        .iter()
        .filter(|l| l.a == ViewEnd::Dangling || l.b == ViewEnd::Dangling)
        .count()
}
