//! Every change the editor can make to a [`Project`]. Each operation validates
//! first and only then mutates, so a failed operation leaves the project unchanged.

use super::{
    Anchor, Block, BlockId, BlockKind, Cell, DiagramId, Direction, End, Endpoint, LineStyle,
    MAX_BLOCKS_PER_DIAGRAM, MAX_NAME, MAX_RELATIONS_PER_DIAGRAM, MAX_TEXT, ModelError, ModelResult,
    PALETTE, Project, Relation, RelationId, Rgb, Side, Tag, TagId,
};

/// What the user enters for a box: name, type and an optional tag name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSpec {
    pub name: String,
    pub kind: BlockKind,
    pub tag: Option<String>,
    /// A cross-cutting band across the bottom of the diagram instead of a grid box.
    pub band: bool,
}

impl BlockSpec {
    pub fn new(name: &str, kind: BlockKind) -> Self {
        BlockSpec {
            name: name.to_owned(),
            kind,
            tag: None,
            band: false,
        }
    }

    pub fn tagged(mut self, tag: &str) -> Self {
        self.tag = Some(tag.to_owned());
        self
    }

    pub fn as_band(mut self) -> Self {
        self.band = true;
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
        let cell = self.next_column(diagram);
        self.insert_block(diagram, spec, cell)
    }

    /// The first cell right of every box of the diagram.
    pub(super) fn next_column(&self, diagram: DiagramId) -> Cell {
        match self
            .blocks_in(diagram)
            .filter(|(_, b)| !b.band)
            .map(|(_, b)| b.cell.col)
            .max()
        {
            Some(max_col) => Cell::new(max_col + 1, 0),
            None => Cell::new(0, 0),
        }
    }

    /// Adds a box without a relation in `cell`, or in the nearest free cell next to it
    /// if that one is taken.
    pub fn add_block_at(
        &mut self,
        diagram: DiagramId,
        spec: &BlockSpec,
        cell: Cell,
    ) -> ModelResult<BlockId> {
        if !cell.in_grid() {
            return Err(ModelError::OutsideGrid);
        }
        let cell = self.free_cell_near(diagram, cell, Side::Right, None);
        if !cell.in_grid() {
            return Err(ModelError::OutsideGrid);
        }
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
        let was_band = self.block(id)?.band;
        if spec.band {
            check_band_kind(spec.kind)?;
        }
        if spec.band && !was_band {
            let touched = self.relations.values().any(|r| {
                r.a.anchors
                    .iter()
                    .chain(&r.b.anchors)
                    .any(|a| a.block == id)
            });
            if touched {
                return Err(ModelError::LinesOnBand);
            }
        }
        let cell = (was_band && !spec.band).then(|| self.next_column(parent));
        let tag = self.resolve_tag(spec.tag.as_deref());
        let block = self.blocks.get_mut(&id).ok_or(ModelError::UnknownBlock)?;
        block.name = name;
        block.kind = spec.kind;
        block.tag = tag;
        block.band = spec.band;
        if let Some(cell) = cell {
            block.cell = cell;
        }
        Ok(())
    }

    /// Moves a box to another grid cell; a box already there takes the old cell.
    /// Lines of the moved boxes then leave them on the sides that face their partners,
    /// replacing sides set by hand.
    pub fn move_block(&mut self, id: BlockId, cell: Cell) -> ModelResult<()> {
        if !cell.in_grid() {
            return Err(ModelError::OutsideGrid);
        }
        let block = self.block(id)?;
        if block.band {
            return Err(ModelError::BandStays);
        }
        let (parent, old) = (block.parent, block.cell);
        let occupant = self
            .blocks_in(parent)
            .find(|(other, b)| *other != id && !b.band && b.cell == cell)
            .map(|(other, _)| other);
        if let Some(other) = occupant
            && let Some(b) = self.blocks.get_mut(&other)
        {
            b.cell = old;
        }
        if let Some(b) = self.blocks.get_mut(&id) {
            b.cell = cell;
        }
        let moved: Vec<BlockId> = std::iter::once(id).chain(occupant).collect();
        self.reface(parent, &moved)
    }

    /// Gives every line at the `moved` boxes of `diagram` the sides that face its
    /// partner: the partner box, or the frame side a line from outside enters at.
    pub(super) fn reface(&mut self, diagram: DiagramId, moved: &[BlockId]) -> ModelResult<()> {
        let at_moved = |e: &Endpoint| e.anchors.first().is_some_and(|a| moved.contains(&a.block));
        let own: Vec<RelationId> = self
            .relations
            .iter()
            .filter(|(_, r)| r.owner == diagram && (at_moved(&r.a) || at_moved(&r.b)))
            .map(|(id, _)| *id)
            .collect();
        for rel in own {
            self.reside(rel)?;
        }
        let Some(outer) = diagram else {
            return Ok(());
        };
        for rel in self.relations.values_mut() {
            for end in [End::A, End::B] {
                let anchors = &mut rel.end_mut(end).anchors;
                let Some(side) = anchors.iter().find(|a| a.block == outer).map(|a| a.side) else {
                    continue;
                };
                // Moved boxes of this diagram are directly inside `outer`.
                for inner in anchors.iter_mut().filter(|a| moved.contains(&a.block)) {
                    inner.side = side;
                }
            }
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
        // `gone` holds whole subtrees, so landings below a deleted box go with it.
        for rel in self.relations.values_mut() {
            for end in [&mut rel.a, &mut rel.b] {
                end.anchors.retain(|a| !gone.contains(&a.block));
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
        if origin.band {
            return Err(ModelError::BandHasNoLines);
        }
        let diagram = origin.parent;
        let text = check_text(text)?;
        self.check_relation_room(diagram)?;
        let cell = self.free_cell_near(diagram, origin.cell.toward(side), side, None);
        let new = self.insert_block(diagram, spec, cell)?;
        let rel = self.insert_relation(Relation {
            owner: diagram,
            a: Endpoint::at(from, side),
            b: Endpoint::at(new, side.opposite()),
            direction,
            text,
            style: None,
            short: String::new(),
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
        self.check_relation_room(diagram)?;
        self.place_partner(from, side, target, placement)?;
        let target_side = self.partner_side(from, side, target)?;
        Ok(self.insert_relation(Relation {
            owner: diagram,
            a: Endpoint::at(from, side),
            b: Endpoint::at(target, target_side),
            direction,
            text,
            style: None,
            short: String::new(),
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
        self.check_lines(from)?;
        let text = check_text(text)?;
        self.check_relation_room(None)?;
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
            style: None,
            short: String::new(),
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
        // The relation now starts at `near`: what lies above it and beside it goes.
        let relation = self.relation(rel)?;
        let open_is_b = relation.b.is_open();
        let near_end = if open_is_b { &relation.a } else { &relation.b };
        if near_end.position_of(near.block).is_none() {
            return Err(ModelError::NotOnChain);
        }
        let keep: Vec<BlockId> = near_end
            .anchors
            .iter()
            .map(|a| a.block)
            .filter(|b| self.within(*b, near.block))
            .collect();
        let relation = self
            .relations
            .get_mut(&rel)
            .ok_or(ModelError::UnknownRelation)?;
        let near_end = if open_is_b {
            &mut relation.a
        } else {
            &mut relation.b
        };
        near_end.anchors.retain(|a| keep.contains(&a.block));
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
        self.check_lines(inner)?;
        if self.block(inner)?.parent != Some(outer) {
            return Err(ModelError::DifferentDiagrams);
        }
        let relation = self
            .relations
            .get_mut(&rel)
            .ok_or(ModelError::UnknownRelation)?;
        let endpoint = relation.end_mut(end);
        if endpoint.position_of(outer).is_none() {
            return Err(ModelError::NotOnChain);
        }
        // Another landing: the line fans out to this box as well. Landing on a box it
        // already reaches only changes the side.
        match endpoint.anchors.iter_mut().find(|a| a.block == inner) {
            Some(anchor) => anchor.side = side,
            None => endpoint.anchors.push(Anchor { block: inner, side }),
        }
        Ok(())
    }

    /// Removes a line as seen in `diagram`: a relation of this diagram is deleted, a
    /// relation from a parent level is only detached from the inner boxes here.
    pub fn remove_line(&mut self, rel: RelationId, diagram: DiagramId) -> ModelResult<()> {
        if self.relation(rel)?.owner == diagram {
            self.relations.remove(&rel);
            return Ok(());
        }
        self.detach(rel, diagram, None)
    }

    /// Detaches a relation from a parent level from the boxes of `diagram` it lands on
    /// (only from `landing`, if given), together with everything it reaches inside
    /// them.
    pub fn detach(
        &mut self,
        rel: RelationId,
        diagram: DiagramId,
        landing: Option<BlockId>,
    ) -> ModelResult<()> {
        let relation = self.relation(rel)?;
        if relation.owner == diagram {
            return Err(ModelError::NotOnChain);
        }
        let landed = |a: &Anchor| {
            self.blocks
                .get(&a.block)
                .is_some_and(|b| b.parent == diagram)
                && landing.is_none_or(|l| l == a.block)
        };
        let mut gone = Vec::new();
        for end in [End::A, End::B] {
            for a in relation.end(end).anchors.iter().filter(|a| landed(a)) {
                gone.push((end, a.block));
            }
        }
        if gone.is_empty() {
            return Err(ModelError::NotOnChain);
        }
        let below: Vec<(End, BlockId)> = [End::A, End::B]
            .into_iter()
            .flat_map(|end| {
                relation
                    .end(end)
                    .anchors
                    .iter()
                    .map(move |a| (end, a.block))
            })
            .filter(|(end, b)| {
                gone.iter()
                    .any(|(g_end, g)| g_end == end && self.within(*b, *g))
            })
            .collect();
        if let Some(relation) = self.relations.get_mut(&rel) {
            for end in [End::A, End::B] {
                relation
                    .end_mut(end)
                    .anchors
                    .retain(|a| !below.contains(&(end, a.block)));
            }
        }
        Ok(())
    }

    /// Sets the side of `block` a relation's line leaves it on. The next move of a box
    /// of that line replaces it with the facing side.
    pub fn set_side_at(&mut self, rel: RelationId, block: BlockId, side: Side) -> ModelResult<()> {
        let relation = self
            .relations
            .get_mut(&rel)
            .ok_or(ModelError::UnknownRelation)?;
        for end in [End::A, End::B] {
            if let Some(anchor) = relation
                .end_mut(end)
                .anchors
                .iter_mut()
                .find(|a| a.block == block)
            {
                anchor.side = side;
                return Ok(());
            }
        }
        Err(ModelError::NotOnChain)
    }

    /// Sets the line style of one relation; `None` follows the project.
    pub fn set_relation_style(
        &mut self,
        rel: RelationId,
        style: Option<LineStyle>,
    ) -> ModelResult<()> {
        let relation = self
            .relations
            .get_mut(&rel)
            .ok_or(ModelError::UnknownRelation)?;
        relation.style = style;
        Ok(())
    }

    /// Sets the short label a relation shows instead of its text (empty: none).
    pub fn set_relation_short(&mut self, rel: RelationId, short: &str) -> ModelResult<()> {
        check_name_text(short)?;
        let relation = self
            .relations
            .get_mut(&rel)
            .ok_or(ModelError::UnknownRelation)?;
        relation.short = short.trim().to_owned();
        Ok(())
    }

    /// Sets when relation texts become numbers (see [`Project::label_limit`]).
    pub fn set_label_limit(&mut self, limit: Option<u32>) {
        self.label_limit = limit;
    }

    /// Sets the project's name; empty takes it from the context view again.
    pub fn set_name(&mut self, name: &str) -> ModelResult<()> {
        check_name_text(name)?;
        self.name = name.trim().to_owned();
        Ok(())
    }

    /// Sets whether relations between the same two boxes are drawn as one line.
    pub fn set_bundle(&mut self, on: bool) {
        self.bundle = on;
    }

    /// Sets the readability limits of the project.
    pub fn set_score_limits(&mut self, limits: super::ScoreLimits) {
        self.score_limits = limits;
    }

    /// Sets the project's line style.
    pub fn set_line_style(&mut self, style: LineStyle) {
        self.line_style = style;
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

    pub(super) fn insert_block(
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
        if spec.band {
            check_band_kind(spec.kind)?;
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
                cell: if spec.band { Cell::new(0, 0) } else { cell },
                band: spec.band,
                responsibility: String::new(),
                motivation: String::new(),
            },
        );
        Ok(id)
    }

    pub(super) fn check_relation_room(&self, diagram: DiagramId) -> ModelResult<()> {
        let count = self
            .relations
            .values()
            .filter(|r| r.owner == diagram)
            .count();
        if count >= MAX_RELATIONS_PER_DIAGRAM {
            return Err(ModelError::TooManyRelations);
        }
        Ok(())
    }

    pub(super) fn insert_relation(&mut self, relation: Relation) -> RelationId {
        let id = RelationId(self.allocate());
        self.relations.insert(id, relation);
        id
    }

    fn resolve_tag(&mut self, tag: Option<&str>) -> Option<TagId> {
        tag.and_then(|name| self.ensure_tag(name))
    }

    pub(super) fn check_diagram(&self, diagram: DiagramId) -> ModelResult<()> {
        if let Some(owner) = diagram {
            let kind = self.block(owner)?.kind;
            if !kind.can_drill() {
                return Err(ModelError::NotDrillable(kind.label()));
            }
        }
        Ok(())
    }

    pub(super) fn check_name(
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

    pub(super) fn check_kind(&self, diagram: DiagramId, kind: BlockKind) -> ModelResult<()> {
        if kind.is_neighbour() && diagram.is_some() {
            return Err(ModelError::NeighbourBelowContext(kind.label()));
        }
        Ok(())
    }

    /// Bands have no lines.
    pub(super) fn check_lines(&self, id: BlockId) -> ModelResult<()> {
        if self.block(id)?.band {
            return Err(ModelError::BandHasNoLines);
        }
        Ok(())
    }

    fn check_pair(&self, from: BlockId, target: BlockId) -> ModelResult<DiagramId> {
        if from == target {
            return Err(ModelError::SelfRelation);
        }
        self.check_lines(from)?;
        self.check_lines(target)?;
        let diagram = self.block(from)?.parent;
        if self.block(target)?.parent != diagram {
            return Err(ModelError::DifferentDiagrams);
        }
        Ok(diagram)
    }

    pub(super) fn occupied(&self, diagram: DiagramId, cell: Cell, except: Option<BlockId>) -> bool {
        self.blocks_in(diagram)
            .any(|(id, b)| Some(id) != except && !b.band && b.cell == cell)
    }

    /// The first free cell at `target`, else the nearest free cell of a block that
    /// grows from `target` away from the origin on `side` as squarely as possible:
    /// 1, 2 side by side, 2x2, 3x2, 3x3 … boxes.
    pub(super) fn free_cell_near(
        &self,
        diagram: DiagramId,
        target: Cell,
        side: Side,
        except: Option<BlockId>,
    ) -> Cell {
        // `across` runs along the side (0, 1, -1, 2, -2 …), `depth` away from it.
        let at = |across: i32, depth: i32| {
            let (dc, dr) = match side {
                Side::Right => (depth, across),
                Side::Left => (-depth, across),
                Side::Bottom => (across, depth),
                Side::Top => (across, -depth),
            };
            Cell::new(target.col + dc, target.row + dr)
        };
        let across = |k: i32| if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) };
        let (mut wide, mut deep) = (1, 1);
        loop {
            for depth in 0..deep {
                for k in 0..wide {
                    let cell = at(across(k), depth);
                    if !self.occupied(diagram, cell, except) {
                        return cell;
                    }
                }
            }
            if wide > deep {
                deep += 1;
            } else {
                wide += 1;
            }
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
        let Some(cell) = self.partner_cell(from, side, target)? else {
            return Ok(());
        };
        if !cell.in_grid() {
            return Err(ModelError::OutsideGrid);
        }
        let diagram = self.block(target)?.parent;
        if let Some(block) = self.blocks.get_mut(&target) {
            block.cell = cell;
        }
        // Like any move: the moved box's lines face their partners.
        self.reface(diagram, &[target])
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
    pub(super) fn reside(&mut self, rel: RelationId) -> ModelResult<()> {
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

/// People and external systems are neighbours, never cross-cutting bands.
fn check_band_kind(kind: BlockKind) -> ModelResult<()> {
    if kind.is_neighbour() {
        return Err(ModelError::BandKind(kind.label()));
    }
    Ok(())
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
