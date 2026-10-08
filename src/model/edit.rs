//! Every change the editor can make to a [`Project`]. Each operation validates
//! first and only then mutates, so a failed operation leaves the project unchanged.

use super::{
    Anchor, Block, BlockId, BlockKind, Cell, DiagramId, Direction, End, Endpoint,
    MAX_BLOCKS_PER_DIAGRAM, MAX_NAME, MAX_TEXT, ModelError, ModelResult, PALETTE, Project,
    Relation, RelationId, Rgb, Side, Tag, TagId,
};

/// What the user enters for a box: name, type and an optional tag name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSpec {
    pub name: String,
    pub kind: BlockKind,
    pub tag: Option<String>,
}

impl BlockSpec {
    pub fn new(name: &str, kind: BlockKind) -> Self {
        BlockSpec {
            name: name.to_owned(),
            kind,
            tag: None,
        }
    }

    pub fn tagged(mut self, tag: &str) -> Self {
        self.tag = Some(tag.to_owned());
        self
    }
}

/// What happens to an existing box that is connected on a side it is not on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Move the box to the requested side; relations that no longer fit lose their side.
    Move,
    /// Leave the box where it is and route the line around.
    Keep,
}

impl Project {
    // ----- boxes -----

    /// Adds a box without a relation (the "+ add box" of an empty diagram).
    pub fn add_block(&mut self, diagram: DiagramId, spec: &BlockSpec) -> ModelResult<BlockId> {
        let cell = match self.blocks_in(diagram).map(|(_, b)| b.cell.col).max() {
            Some(max_col) => Cell::new(max_col + 1, 0),
            None => Cell::new(0, 0),
        };
        self.insert_block(diagram, spec, cell)
    }

    /// Changes name, type and tag of a box.
    pub fn edit_block(&mut self, id: BlockId, spec: &BlockSpec) -> ModelResult<()> {
        let parent = self.block(id)?.parent;
        let name = self.check_name(parent, &spec.name, Some(id))?;
        self.check_kind(parent, spec.kind)?;
        if !spec.kind.can_drill() && self.has_content(id) {
            return Err(ModelError::KindHasContent(spec.kind.label()));
        }
        if let Some(tag) = &spec.tag {
            check_name_text(tag)?;
        }
        let tag = self.resolve_tag(spec.tag.as_deref());
        let block = self.blocks.get_mut(&id).ok_or(ModelError::UnknownBlock)?;
        block.name = name;
        block.kind = spec.kind;
        block.tag = tag;
        Ok(())
    }

    /// Moves a box to another grid cell; a box already there takes the old cell.
    pub fn move_block(&mut self, id: BlockId, cell: Cell) -> ModelResult<()> {
        if !cell.in_grid() {
            return Err(ModelError::OutsideGrid);
        }
        let block = self.block(id)?;
        let (parent, old) = (block.parent, block.cell);
        let occupant = self
            .blocks_in(parent)
            .find(|(other, b)| *other != id && b.cell == cell)
            .map(|(other, _)| other);
        if let Some(other) = occupant
            && let Some(b) = self.blocks.get_mut(&other)
        {
            b.cell = old;
        }
        if let Some(b) = self.blocks.get_mut(&id) {
            b.cell = cell;
        }
        Ok(())
    }

    /// Deletes a box, everything inside it and every relation of it. Relations that
    /// only pass through it from a parent level are detached instead.
    pub fn delete_block(&mut self, id: BlockId) -> ModelResult<()> {
        self.block(id)?;
        let gone = self.subtree(id);
        self.relations.retain(|_, rel| {
            if rel.owner.is_some_and(|o| gone.contains(&o)) {
                return false;
            }
            for end in [&rel.a, &rel.b] {
                if end.anchors.first().is_some_and(|a| gone.contains(&a.block)) {
                    return false;
                }
            }
            true
        });
        for rel in self.relations.values_mut() {
            for end in [&mut rel.a, &mut rel.b] {
                if let Some(k) = end.anchors.iter().position(|a| gone.contains(&a.block)) {
                    end.anchors.truncate(k);
                }
            }
        }
        for block in gone {
            self.blocks.remove(&block);
        }
        Ok(())
    }

    /// Sets the responsibility text of a box.
    pub fn set_responsibility(&mut self, id: BlockId, text: &str) -> ModelResult<()> {
        let text = check_text(text)?;
        let block = self.blocks.get_mut(&id).ok_or(ModelError::UnknownBlock)?;
        block.responsibility = text;
        Ok(())
    }

    /// Sets the motivation of the context view (`None`) or of a whitebox.
    pub fn set_motivation(&mut self, diagram: DiagramId, text: &str) -> ModelResult<()> {
        let text = check_text(text)?;
        match diagram {
            None => self.motivation = text,
            Some(id) => {
                let block = self.blocks.get_mut(&id).ok_or(ModelError::UnknownBlock)?;
                if !block.kind.can_drill() {
                    return Err(ModelError::NotDrillable(block.kind.label()));
                }
                block.motivation = text;
            }
        }
        Ok(())
    }

    // ----- relations -----

    /// Creates a new box on `side` of `from` and connects the two.
    pub fn connect_new(
        &mut self,
        from: BlockId,
        side: Side,
        spec: &BlockSpec,
        direction: Direction,
        text: &str,
    ) -> ModelResult<(BlockId, RelationId)> {
        let origin = self.block(from)?;
        let diagram = origin.parent;
        let text = check_text(text)?;
        let cell = self.free_cell_near(diagram, origin.cell.toward(side), side, None);
        let new = self.insert_block(diagram, spec, cell)?;
        let rel = self.insert_relation(Relation {
            owner: diagram,
            a: Endpoint::at(from, side),
            b: Endpoint::at(new, side.opposite()),
            direction,
            text,
        });
        Ok((new, rel))
    }

    /// Relations that would lose their side if `target` moved to `side` of `from`.
    pub fn conflicts(
        &self,
        from: BlockId,
        side: Side,
        target: BlockId,
    ) -> ModelResult<Vec<RelationId>> {
        let Some(new_cell) = self.partner_cell(from, side, target)? else {
            return Ok(Vec::new());
        };
        let diagram = self.block(target)?.parent;
        let mut out = Vec::new();
        for (id, rel) in &self.relations {
            if rel.owner != diagram {
                continue;
            }
            let (Some(a), Some(b)) = (rel.a.anchors.first(), rel.b.anchors.first()) else {
                continue;
            };
            if a.block != target && b.block != target {
                continue;
            }
            let ca = self.block(a.block)?.cell;
            let cb = self.block(b.block)?.cell;
            let before = ca.sees_on(a.side, cb);
            let (na, nb) = if a.block == target {
                (new_cell, cb)
            } else {
                (ca, new_cell)
            };
            if before && !na.sees_on(a.side, nb) {
                out.push(*id);
            }
        }
        Ok(out)
    }

    /// Connects `from` with an existing box of the same diagram on `side`.
    pub fn connect_existing(
        &mut self,
        from: BlockId,
        side: Side,
        target: BlockId,
        direction: Direction,
        text: &str,
        placement: Placement,
    ) -> ModelResult<RelationId> {
        let diagram = self.check_pair(from, target)?;
        let text = check_text(text)?;
        self.place_partner(from, side, target, placement)?;
        let target_side = self.partner_side(from, side, target)?;
        Ok(self.insert_relation(Relation {
            owner: diagram,
            a: Endpoint::at(from, side),
            b: Endpoint::at(target, target_side),
            direction,
            text,
        }))
    }

    /// Creates a relation from `from` that leaves its level. It bubbles up: every
    /// enclosing box shows it as a stub on the same side, up to the context view.
    pub fn add_stub(
        &mut self,
        from: BlockId,
        side: Side,
        direction: Direction,
        text: &str,
    ) -> ModelResult<RelationId> {
        let parent = self.block(from)?.parent;
        let text = check_text(text)?;
        let mut anchors: Vec<Anchor> = self
            .path(parent)
            .into_iter()
            .map(|block| Anchor { block, side })
            .collect();
        anchors.push(Anchor { block: from, side });
        Ok(self.insert_relation(Relation {
            owner: None,
            a: Endpoint { anchors },
            b: Endpoint::open(),
            direction,
            text,
        }))
    }

    /// The box that carries the near end of a stub in `diagram`, with its side.
    pub fn stub_anchor_in(&self, rel: RelationId, diagram: DiagramId) -> ModelResult<Anchor> {
        let relation = self.relation(rel)?;
        let (near, _) = open_ends(relation)?;
        near.anchors
            .iter()
            .find(|a| {
                self.blocks
                    .get(&a.block)
                    .is_some_and(|b| b.parent == diagram)
            })
            .copied()
            .ok_or(ModelError::NotOnChain)
    }

    /// Relations that would lose their side if the stub were connected to `target`.
    pub fn stub_conflicts(
        &self,
        rel: RelationId,
        diagram: DiagramId,
        target: BlockId,
    ) -> ModelResult<Vec<RelationId>> {
        let near = self.stub_anchor_in(rel, diagram)?;
        self.conflicts(near.block, near.side, target)
    }

    /// Connects the open end of a stub to `target` in `diagram`. The relation then
    /// belongs to `diagram`; it no longer leaves that level. `target_side` pins the
    /// side of `target` the line arrives at (when the user clicked one).
    pub fn connect_stub(
        &mut self,
        rel: RelationId,
        diagram: DiagramId,
        target: BlockId,
        target_side: Option<Side>,
        placement: Placement,
    ) -> ModelResult<()> {
        let near = self.stub_anchor_in(rel, diagram)?;
        self.check_pair(near.block, target)?;
        self.place_partner(near.block, near.side, target, placement)?;
        let target_side = match target_side {
            Some(side) => side,
            None => self.partner_side(near.block, near.side, target)?,
        };
        let relation = self
            .relations
            .get_mut(&rel)
            .ok_or(ModelError::UnknownRelation)?;
        let open_is_b = relation.b.is_open();
        let near_end = if open_is_b {
            &mut relation.a
        } else {
            &mut relation.b
        };
        let k = near_end
            .position_of(near.block)
            .ok_or(ModelError::NotOnChain)?;
        near_end.anchors.drain(..k);
        let far = Endpoint::at(target, target_side);
        if open_is_b {
            relation.b = far;
        } else {
            relation.a = far;
        }
        relation.owner = diagram;
        Ok(())
    }

    /// Attaches an end that enters the whitebox of `outer` to the inner box `inner`.
    pub fn attach(
        &mut self,
        rel: RelationId,
        end: End,
        outer: BlockId,
        inner: BlockId,
        side: Side,
    ) -> ModelResult<()> {
        if self.block(inner)?.parent != Some(outer) {
            return Err(ModelError::DifferentDiagrams);
        }
        let relation = self
            .relations
            .get_mut(&rel)
            .ok_or(ModelError::UnknownRelation)?;
        let endpoint = relation.end_mut(end);
        let i = endpoint.position_of(outer).ok_or(ModelError::NotOnChain)?;
        endpoint.anchors.truncate(i + 1);
        endpoint.anchors.push(Anchor { block: inner, side });
        Ok(())
    }

    /// Removes a line as seen in `diagram`: a relation of this diagram is deleted, a
    /// relation from a parent level is only detached from the inner box here.
    pub fn remove_line(&mut self, rel: RelationId, diagram: DiagramId) -> ModelResult<()> {
        let relation = self.relation(rel)?;
        if relation.owner == diagram {
            self.relations.remove(&rel);
            return Ok(());
        }
        let blocks = &self.blocks;
        let in_diagram = |a: &Anchor| blocks.get(&a.block).is_some_and(|b| b.parent == diagram);
        let hit = [End::A, End::B].into_iter().find_map(|end| {
            relation
                .end(end)
                .anchors
                .iter()
                .position(in_diagram)
                .map(|k| (end, k))
        });
        let (end, k) = hit.ok_or(ModelError::NotOnChain)?;
        if let Some(relation) = self.relations.get_mut(&rel) {
            relation.end_mut(end).anchors.truncate(k);
        }
        Ok(())
    }

    pub fn edit_relation(
        &mut self,
        rel: RelationId,
        direction: Direction,
        text: &str,
    ) -> ModelResult<()> {
        let text = check_text(text)?;
        let relation = self
            .relations
            .get_mut(&rel)
            .ok_or(ModelError::UnknownRelation)?;
        relation.direction = direction;
        relation.text = text;
        Ok(())
    }

    // ----- tags -----

    /// The tag with this name; a new one gets the next palette colour. Blank names
    /// mean "no tag".
    pub fn ensure_tag(&mut self, name: &str) -> Option<TagId> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        if let Some(id) = self.tag_by_name(name) {
            return Some(id);
        }
        let color = PALETTE[self.tags.len() % PALETTE.len()];
        let id = TagId(self.allocate());
        self.tags.insert(
            id,
            Tag {
                name: name.to_owned(),
                color,
            },
        );
        Some(id)
    }

    pub fn set_tag_color(&mut self, tag: TagId, color: Rgb) -> ModelResult<()> {
        let tag = self.tags.get_mut(&tag).ok_or(ModelError::UnknownTag)?;
        tag.color = color;
        Ok(())
    }

    // ----- helpers -----

    fn insert_block(
        &mut self,
        diagram: DiagramId,
        spec: &BlockSpec,
        cell: Cell,
    ) -> ModelResult<BlockId> {
        self.check_diagram(diagram)?;
        let name = self.check_name(diagram, &spec.name, None)?;
        self.check_kind(diagram, spec.kind)?;
        if let Some(tag) = &spec.tag {
            check_name_text(tag)?;
        }
        if !cell.in_grid() {
            return Err(ModelError::OutsideGrid);
        }
        if self.blocks_in(diagram).count() >= MAX_BLOCKS_PER_DIAGRAM {
            return Err(ModelError::DiagramFull);
        }
        let tag = self.resolve_tag(spec.tag.as_deref());
        let id = BlockId(self.allocate());
        self.blocks.insert(
            id,
            Block {
                name,
                kind: spec.kind,
                tag,
                parent: diagram,
                cell,
                responsibility: String::new(),
                motivation: String::new(),
            },
        );
        Ok(id)
    }

    fn insert_relation(&mut self, relation: Relation) -> RelationId {
        let id = RelationId(self.allocate());
        self.relations.insert(id, relation);
        id
    }

    fn resolve_tag(&mut self, tag: Option<&str>) -> Option<TagId> {
        tag.and_then(|name| self.ensure_tag(name))
    }

    fn check_diagram(&self, diagram: DiagramId) -> ModelResult<()> {
        if let Some(owner) = diagram {
            let kind = self.block(owner)?.kind;
            if !kind.can_drill() {
                return Err(ModelError::NotDrillable(kind.label()));
            }
        }
        Ok(())
    }

    fn check_name(
        &self,
        diagram: DiagramId,
        name: &str,
        except: Option<BlockId>,
    ) -> ModelResult<String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ModelError::EmptyName);
        }
        check_name_text(name)?;
        let lower = name.to_lowercase();
        let taken = self
            .blocks_in(diagram)
            .any(|(id, b)| Some(id) != except && b.name.to_lowercase() == lower);
        if taken {
            return Err(ModelError::DuplicateName(name.to_owned()));
        }
        Ok(name.to_owned())
    }

    fn check_kind(&self, diagram: DiagramId, kind: BlockKind) -> ModelResult<()> {
        if kind.is_neighbour() && diagram.is_some() {
            return Err(ModelError::NeighbourBelowContext(kind.label()));
        }
        Ok(())
    }

    fn check_pair(&self, from: BlockId, target: BlockId) -> ModelResult<DiagramId> {
        if from == target {
            return Err(ModelError::SelfRelation);
        }
        let diagram = self.block(from)?.parent;
        if self.block(target)?.parent != diagram {
            return Err(ModelError::DifferentDiagrams);
        }
        Ok(diagram)
    }

    fn occupied(&self, diagram: DiagramId, cell: Cell, except: Option<BlockId>) -> bool {
        self.blocks_in(diagram)
            .any(|(id, b)| Some(id) != except && b.cell == cell)
    }

    /// The first free cell at `target`, else next to it across `side`'s axis: a box
    /// requested to the right goes above or below a taken cell, in the same column.
    fn free_cell_near(
        &self,
        diagram: DiagramId,
        target: Cell,
        side: Side,
        except: Option<BlockId>,
    ) -> Cell {
        let step = |k: i32| {
            if side.is_horizontal() {
                Cell::new(target.col, target.row + k)
            } else {
                Cell::new(target.col + k, target.row)
            }
        };
        let mut k = 0;
        loop {
            for candidate in [step(k), step(-k)] {
                if !self.occupied(diagram, candidate, except) {
                    return candidate;
                }
            }
            k += 1;
        }
    }

    /// Where `target` has to go to sit on `side` of `from`; `None` if it already does.
    fn partner_cell(
        &self,
        from: BlockId,
        side: Side,
        target: BlockId,
    ) -> ModelResult<Option<Cell>> {
        let origin = self.block(from)?;
        let partner = self.block(target)?;
        if origin.cell.sees_on(side, partner.cell) {
            return Ok(None);
        }
        Ok(Some(self.free_cell_near(
            origin.parent,
            origin.cell.toward(side),
            side,
            Some(target),
        )))
    }

    fn place_partner(
        &mut self,
        from: BlockId,
        side: Side,
        target: BlockId,
        placement: Placement,
    ) -> ModelResult<()> {
        if placement == Placement::Keep {
            return Ok(());
        }
        let broken = self.conflicts(from, side, target)?;
        if let Some(cell) = self.partner_cell(from, side, target)? {
            if !cell.in_grid() {
                return Err(ModelError::OutsideGrid);
            }
            if let Some(block) = self.blocks.get_mut(&target) {
                block.cell = cell;
            }
        }
        for rel in broken {
            self.reside(rel)?;
        }
        Ok(())
    }

    /// The side of `target` a new relation from `side` of `from` arrives at.
    fn partner_side(&self, from: BlockId, side: Side, target: BlockId) -> ModelResult<Side> {
        let origin = self.block(from)?.cell;
        let partner = self.block(target)?.cell;
        Ok(if origin.sees_on(side, partner) {
            side.opposite()
        } else {
            partner.side_facing(origin)
        })
    }

    /// Gives a relation the sides that face each other in the current layout.
    fn reside(&mut self, rel: RelationId) -> ModelResult<()> {
        let relation = self.relation(rel)?;
        let (Some(a), Some(b)) = (relation.a.anchors.first(), relation.b.anchors.first()) else {
            return Ok(());
        };
        let ca = self.block(a.block)?.cell;
        let cb = self.block(b.block)?.cell;
        let side = ca.side_facing(cb);
        if let Some(relation) = self.relations.get_mut(&rel) {
            if let Some(a) = relation.a.anchors.first_mut() {
                a.side = side;
            }
            if let Some(b) = relation.b.anchors.first_mut() {
                b.side = side.opposite();
            }
        }
        Ok(())
    }
}

/// The near (connected) and the far (open) endpoint of a stub.
fn open_ends(relation: &Relation) -> ModelResult<(&Endpoint, &Endpoint)> {
    if relation.b.is_open() && !relation.a.is_open() {
        Ok((&relation.a, &relation.b))
    } else if relation.a.is_open() && !relation.b.is_open() {
        Ok((&relation.b, &relation.a))
    } else {
        Err(ModelError::NotOpen)
    }
}

/// A name or tag: one line, no control characters, at most `MAX_NAME` characters.
fn check_name_text(name: &str) -> ModelResult<()> {
    if name.chars().any(char::is_control) {
        return Err(ModelError::ControlCharacter);
    }
    if name.trim().chars().count() > MAX_NAME {
        return Err(ModelError::NameTooLong);
    }
    Ok(())
}

/// A free text: line breaks and tabs allowed, other control characters not.
fn check_text(text: &str) -> ModelResult<String> {
    let text = text.trim().replace("\r\n", "\n");
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(ModelError::ControlCharacter);
    }
    if text.chars().count() > MAX_TEXT {
        return Err(ModelError::TextTooLong);
    }
    Ok(text)
}
