//! Changing the nesting of boxes: move a box up out of its whitebox, move it into a
//! neighbour, group boxes into a new box, dissolve a whitebox. Relations follow on
//! their own: every end keeps landing on the same boxes, and the relation is rebuilt
//! around the new nesting.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    Anchor, BlockId, BlockSpec, Cell, DiagramId, End, Endpoint, ModelError, ModelResult, Project,
    Relation, RelationId, Side,
};

/// The deepest boxes each end of a relation reaches, with their sides, taken before
/// the nesting changes.
type Leaves = [Vec<Anchor>; 2];

impl Project {
    /// Moves `id` out of its whitebox into the diagram one level up, next to its old
    /// whitebox box on the side its lines leave towards (else to the right).
    pub fn move_up(&mut self, id: BlockId) -> ModelResult<()> {
        let block = self.block(id)?;
        let outer = block.parent.ok_or(ModelError::AtTop)?;
        let up = self.block(outer)?.parent;
        self.check_name(up, &block.name.clone(), Some(id))?;
        self.check_room(up, 1)?;
        let side = self.leaving_side(id).unwrap_or(Side::Right);
        let near = self.block(outer)?.cell.toward(side);
        let cell = if self.block(id)?.band {
            Cell::new(0, 0)
        } else {
            self.free_cell_near(up, near, side, None)
        };
        self.renest(&[id], |p| {
            if let Some(b) = p.blocks.get_mut(&id) {
                b.parent = up;
                b.cell = cell;
            }
        })?;
        self.reface(up, &[id])
    }

    /// Moves `id` into the whitebox of its neighbour `target`, next to the box inside
    /// it has the most lines with (else into the next free cell).
    pub fn move_into(&mut self, id: BlockId, target: BlockId) -> ModelResult<()> {
        if id == target {
            return Err(ModelError::SelfRelation);
        }
        let block = self.block(id)?.clone();
        let host = self.block(target)?;
        if host.parent != block.parent {
            return Err(ModelError::DifferentDiagrams);
        }
        if !host.kind.can_drill() {
            return Err(ModelError::NotDrillable(host.kind.label()));
        }
        let inside = Some(target);
        self.check_kind(inside, block.kind)?;
        self.check_name(inside, &block.name, Some(id))?;
        self.check_room(inside, 1)?;
        // Seen from the target: the side the box came from.
        let from = host.cell.side_facing(block.cell);
        let start = self.next_column(inside);
        self.renest(&[id], |p| {
            if let Some(b) = p.blocks.get_mut(&id) {
                b.parent = inside;
                b.cell = start;
            }
        })?;
        if !block.band {
            let cell = match self.busiest_partner(id) {
                Some(partner) => {
                    let near = self.block(partner)?.cell.toward(from);
                    self.free_cell_near(inside, near, from, Some(id))
                }
                None => start,
            };
            if let Some(b) = self.blocks.get_mut(&id) {
                b.cell = cell;
            }
        }
        self.reface(inside, &[id])
    }

    /// Puts `boxes` (all of one diagram) into the whitebox of a new box made from
    /// `spec`, which takes the cell of `at`. The boxes keep their arrangement inside.
    pub fn group(
        &mut self,
        boxes: &[BlockId],
        at: BlockId,
        spec: &BlockSpec,
    ) -> ModelResult<BlockId> {
        if !boxes.contains(&at) {
            return Err(ModelError::NotInGroup);
        }
        let diagram = self.block(at)?.parent;
        for b in boxes {
            let block = self.block(*b)?;
            if block.parent != diagram {
                return Err(ModelError::DifferentDiagrams);
            }
            if block.kind.is_neighbour() {
                return Err(ModelError::NeighbourBelowContext(block.kind.label()));
            }
        }
        if spec.band {
            return Err(ModelError::BandKind("group"));
        }
        if !spec.kind.can_drill() {
            return Err(ModelError::NotDrillable(spec.kind.label()));
        }
        let cell = self.block(at)?.cell;
        let group = self.insert_block(diagram, spec, cell)?;
        self.renest(boxes, |p| {
            for b in boxes {
                if let Some(block) = p.blocks.get_mut(b) {
                    block.parent = Some(group);
                }
            }
        })?;
        Ok(group)
    }

    /// Dissolves the whitebox of `id`: its boxes take its place, keeping their
    /// arrangement (the boxes around move aside), and `id` with its own texts is gone.
    pub fn dissolve(&mut self, id: BlockId) -> ModelResult<()> {
        let block = self.block(id)?.clone();
        let up = block.parent;
        let children: Vec<BlockId> = self.blocks_in(Some(id)).map(|(c, _)| c).collect();
        for c in &children {
            let child = self.block(*c)?;
            if self
                .blocks_in(up)
                .any(|(o, b)| o != id && b.name.to_lowercase() == child.name.to_lowercase())
            {
                return Err(ModelError::DuplicateName(child.name.clone()));
            }
        }
        if self.blocks_in(up).count() - 1 + children.len() > super::MAX_BLOCKS_PER_DIAGRAM {
            return Err(ModelError::DiagramFull);
        }
        // The children's block in the grid, and how far the others move aside.
        let cells: Vec<Cell> = children
            .iter()
            .filter_map(|c| self.blocks.get(c))
            .filter(|b| !b.band)
            .map(|b| b.cell)
            .collect();
        let col0 = cells.iter().map(|c| c.col).min().unwrap_or(0);
        let row0 = cells.iter().map(|c| c.row).min().unwrap_or(0);
        let wide = cells.iter().map(|c| c.col - col0 + 1).max().unwrap_or(1);
        let high = cells.iter().map(|c| c.row - row0 + 1).max().unwrap_or(1);
        let at = block.cell;
        let others: Vec<BlockId> = self
            .blocks_in(up)
            .filter(|(o, b)| *o != id && !b.band)
            .map(|(o, _)| o)
            .collect();
        let moved: Vec<BlockId> = std::iter::once(id)
            .chain(children.iter().copied())
            .collect();
        self.renest(&moved, |p| {
            for o in &others {
                if let Some(b) = p.blocks.get_mut(o) {
                    if b.cell.col > at.col {
                        b.cell.col += wide - 1;
                    }
                    if b.cell.row > at.row {
                        b.cell.row += high - 1;
                    }
                }
            }
            for c in &children {
                if let Some(b) = p.blocks.get_mut(c) {
                    b.parent = up;
                    if !b.band {
                        b.cell = Cell::new(at.col + b.cell.col - col0, at.row + b.cell.row - row0);
                    }
                }
            }
            p.blocks.remove(&id);
        })?;
        if self.blocks.values().any(|b| !b.cell.in_grid()) {
            return Err(ModelError::OutsideGrid);
        }
        self.reface(up, &children)
    }

    /// The box of the same diagram that `id` has the most relations with.
    fn busiest_partner(&self, id: BlockId) -> Option<BlockId> {
        let parent = self.block(id).ok()?.parent;
        let mut count: BTreeMap<BlockId, usize> = BTreeMap::new();
        for r in self.relations.values().filter(|r| r.owner == parent) {
            let (Some(a), Some(b)) = (r.a.anchors.first(), r.b.anchors.first()) else {
                continue;
            };
            if a.block == id {
                *count.entry(b.block).or_default() += 1;
            } else if b.block == id {
                *count.entry(a.block).or_default() += 1;
            }
        }
        count
            .into_iter()
            .max_by_key(|(b, n)| (*n, std::cmp::Reverse(*b)))
            .map(|(b, _)| b)
    }

    /// The side most of `id`'s lines to its siblings leave it on, turned around: where
    /// the box sits from its siblings' point of view.
    fn leaving_side(&self, id: BlockId) -> Option<Side> {
        let parent = self.block(id).ok()?.parent;
        let mut count: BTreeMap<Side, usize> = BTreeMap::new();
        for r in self.relations.values().filter(|r| r.owner == parent) {
            for end in [End::A, End::B] {
                if let Some(a) = r.end(end).anchors.first()
                    && a.block == id
                    && !r.end(end.other()).is_open()
                {
                    *count.entry(a.side.opposite()).or_default() += 1;
                }
            }
        }
        count
            .into_iter()
            .max_by_key(|(side, n)| (*n, std::cmp::Reverse(*side)))
            .map(|(side, _)| side)
    }

    /// Fails if `diagram` cannot take `more` boxes.
    pub(super) fn check_room(&self, diagram: DiagramId, more: usize) -> ModelResult<()> {
        if self.blocks_in(diagram).count() + more > super::MAX_BLOCKS_PER_DIAGRAM {
            return Err(ModelError::DiagramFull);
        }
        Ok(())
    }

    /// Runs `change` (which changes the nesting of the `moved` boxes) and rebuilds
    /// every relation that reaches into them.
    pub(super) fn renest(
        &mut self,
        moved: &[BlockId],
        change: impl FnOnce(&mut Project),
    ) -> ModelResult<()> {
        let touched: BTreeSet<BlockId> = moved.iter().flat_map(|m| self.subtree(*m)).collect();
        let before: BTreeMap<RelationId, Leaves> = self
            .relations
            .iter()
            .filter(|(_, r)| {
                r.a.anchors
                    .iter()
                    .chain(&r.b.anchors)
                    .any(|a| touched.contains(&a.block))
            })
            .map(|(id, r)| (*id, [self.leaves(&r.a), self.leaves(&r.b)]))
            .collect();
        change(self);
        for (rel, leaves) in before {
            self.rebuild(rel, leaves)?;
        }
        Ok(())
    }

    /// The anchors of an end that no other anchor of that end lies inside.
    fn leaves(&self, e: &Endpoint) -> Vec<Anchor> {
        e.anchors
            .iter()
            .filter(|a| {
                !e.anchors.iter().any(|b| {
                    b.block != a.block
                        && self
                            .blocks
                            .get(&b.block)
                            .is_some_and(|x| x.parent == Some(a.block))
                })
            })
            .copied()
            .collect()
    }

    /// Rebuilds a relation from the boxes its ends reach, under the current nesting:
    /// the owner is the deepest level both ends lie below; an end that reaches into
    /// several boxes of that level becomes one relation per box; an end that reaches
    /// nothing any more becomes an open end.
    pub(super) fn rebuild(&mut self, rel: RelationId, leaves: Leaves) -> ModelResult<()> {
        let Some(old) = self.relations.get(&rel).cloned() else {
            return Ok(());
        };
        let path = |p: &Project, b: BlockId| p.path(Some(b));
        let [mut a, mut b] = leaves.map(|l| {
            l.into_iter()
                .filter(|x| self.blocks.contains_key(&x.block))
                .collect::<Vec<_>>()
        });
        // An end inside the box at the other end has nothing left to point at.
        let inside = |x: &Anchor, others: &[Anchor], p: &Project| {
            others.iter().any(|o| p.within(o.block, x.block))
        };
        let a_keep: Vec<Anchor> = a.iter().copied().filter(|x| !inside(x, &b, self)).collect();
        let b_keep: Vec<Anchor> = b.iter().copied().filter(|x| !inside(x, &a, self)).collect();
        (a, b) = (a_keep, b_keep);
        self.relations.remove(&rel);
        if a.is_empty() && b.is_empty() {
            return Ok(());
        }
        let all: Vec<Vec<BlockId>> = a.iter().chain(&b).map(|x| path(self, x.block)).collect();
        // Both ends: below their deepest common box. One end: a stub from the top.
        let k = if a.is_empty() || b.is_empty() {
            0
        } else {
            let first = &all[0];
            (0..first.len())
                .take_while(|i| all.iter().all(|p| p.get(*i) == first.get(*i)))
                .count()
        };
        let owner = if k == 0 {
            None
        } else {
            all[0].get(k - 1).copied()
        };
        let sides: BTreeMap<(End, BlockId), Side> = [End::A, End::B]
            .into_iter()
            .flat_map(|e| {
                old.end(e)
                    .anchors
                    .iter()
                    .map(move |x| ((e, x.block), x.side))
            })
            .collect();
        let group = |end: End, leaves: &[Anchor], p: &Project| -> Vec<Vec<Anchor>> {
            let mut groups: Vec<(BlockId, Vec<Anchor>)> = Vec::new();
            for leaf in leaves {
                let full = path(p, leaf.block);
                let Some(root) = full.get(k).copied() else {
                    continue;
                };
                let chain: Vec<Anchor> = full[k..]
                    .iter()
                    .map(|blk| Anchor {
                        block: *blk,
                        side: if *blk == leaf.block {
                            leaf.side
                        } else {
                            sides.get(&(end, *blk)).copied().unwrap_or(leaf.side)
                        },
                    })
                    .collect();
                match groups.iter_mut().find(|(r, _)| *r == root) {
                    Some((_, g)) => {
                        for c in chain {
                            if !g.iter().any(|x| x.block == c.block) {
                                g.push(c);
                            }
                        }
                    }
                    None => groups.push((root, chain)),
                }
            }
            groups
                .into_iter()
                .map(|(_, mut g)| {
                    // Every box after the box it lies in.
                    g.sort_by_key(|x| p.path(Some(x.block)).len());
                    g
                })
                .collect()
        };
        let ga = if a.is_empty() {
            vec![Vec::new()]
        } else {
            group(End::A, &a, self)
        };
        let gb = if b.is_empty() {
            vec![Vec::new()]
        } else {
            group(End::B, &b, self)
        };
        let mut first = true;
        for ea in &ga {
            for eb in &gb {
                let relation = Relation {
                    owner,
                    a: Endpoint {
                        anchors: ea.clone(),
                    },
                    b: Endpoint {
                        anchors: eb.clone(),
                    },
                    ..old.clone()
                };
                let id = if first {
                    first = false;
                    self.relations.insert(rel, relation);
                    rel
                } else {
                    self.insert_relation(relation)
                };
                self.settle_sides(id)?;
            }
        }
        Ok(())
    }

    /// After a rebuild: both owner-level boxes face each other, and every box below
    /// takes the side of the frame its line enters through.
    fn settle_sides(&mut self, rel: RelationId) -> ModelResult<()> {
        let relation = self.relation(rel)?;
        if !relation.a.is_open() && !relation.b.is_open() {
            self.reside(rel)?;
        }
        let parents: BTreeMap<BlockId, Option<BlockId>> =
            self.blocks.iter().map(|(id, b)| (*id, b.parent)).collect();
        if let Some(r) = self.relations.get_mut(&rel) {
            for end in [&mut r.a, &mut r.b] {
                let sides: BTreeMap<BlockId, Side> =
                    end.anchors.iter().map(|x| (x.block, x.side)).collect();
                for x in end.anchors.iter_mut().skip(1) {
                    if let Some(side) = parents
                        .get(&x.block)
                        .copied()
                        .flatten()
                        .and_then(|p| sides.get(&p))
                    {
                        x.side = *side;
                    }
                }
            }
        }
        Ok(())
    }
}
