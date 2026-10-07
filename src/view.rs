//! What one diagram shows: its boxes and every line that touches them, including
//! relations that enter the whitebox from a parent level.

use crate::model::{BlockId, DiagramId, Direction, End, Project, RelationId, Side};

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
            });
            continue;
        }
        let Some(owner) = diagram else { continue };
        for end in [End::A, End::B] {
            let anchors = &rel.end(end).anchors;
            let Some(i) = anchors.iter().position(|a| a.block == owner) else {
                continue;
            };
            let side = anchors[i].side;
            let inner = match anchors.get(i + 1) {
                Some(next) => ViewEnd::Block {
                    block: next.block,
                    side: next.side,
                },
                None => ViewEnd::Dangling,
            };
            let partner = rel
                .end(end.other())
                .anchors
                .first()
                .and_then(|a| project.blocks.get(&a.block))
                .map_or_else(|| OPEN_PARTNER.to_owned(), |b| b.name.clone());
            let frame = ViewEnd::Frame { side, partner };
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
            });
        }
    }
    DiagramView {
        diagram,
        blocks,
        lines,
    }
}

/// Number of inherited ends in a diagram that are not attached to a box yet.
pub fn dangling_count(view: &DiagramView) -> usize {
    view.lines
        .iter()
        .filter(|l| l.a == ViewEnd::Dangling || l.b == ViewEnd::Dangling)
        .count()
}
