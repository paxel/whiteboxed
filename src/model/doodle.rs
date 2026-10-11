//! Groups on doodle pages: labelled rectangles of cells. Whatever lies inside a
//! rectangle belongs to it; rectangles may nest but never partly overlap.

use super::{Block, BlockId, BlockKind, BlockSpec, Cell, ModelError, ModelResult, Project};

impl Project {
    /// The groups on a page.
    pub fn groups_on(&self, page: BlockId) -> Vec<BlockId> {
        self.blocks_in(Some(page))
            .filter(|(_, b)| b.kind == BlockKind::Group)
            .map(|(id, _)| id)
            .collect()
    }

    /// The shapes inside a group's rectangle.
    pub fn members(&self, group: BlockId) -> Vec<BlockId> {
        let Some(g) = self.blocks.get(&group) else {
            return Vec::new();
        };
        let Some(rect) = g.group_cells() else {
            return Vec::new();
        };
        self.blocks_in(g.parent)
            .filter(|(id, b)| *id != group && !b.gridless() && inside(b.cell, rect))
            .map(|(id, _)| id)
            .collect()
    }

    /// Frames `shapes` (all on one doodle page) as a new group named `name`: the
    /// smallest rectangle of cells around them.
    pub fn add_group(
        &mut self,
        shapes: &[BlockId],
        name: &str,
        color: Option<super::Rgb>,
    ) -> ModelResult<BlockId> {
        let (page, rect) = self.around(shapes)?;
        self.check_nesting(page, rect, None)?;
        let spec = BlockSpec {
            color,
            ..BlockSpec::new(name, BlockKind::Group)
        };
        let id = self.insert_block(Some(page), &spec, rect.0)?;
        if let Some(b) = self.blocks.get_mut(&id) {
            b.span = Some(rect.1);
        }
        Ok(id)
    }

    /// Widens a group so it also takes in `shapes`.
    pub fn widen_group(&mut self, group: BlockId, shapes: &[BlockId]) -> ModelResult<()> {
        let g = self.block(group)?;
        let (near, far) = g.group_cells().ok_or(ModelError::EmptyGroup)?;
        let page = g.parent.ok_or(ModelError::PageOutOfPlace)?;
        let (_, add) = self.around(shapes)?;
        let rect = (
            Cell::new(near.col.min(add.0.col), near.row.min(add.0.row)),
            Cell::new(far.col.max(add.1.col), far.row.max(add.1.row)),
        );
        self.check_nesting(page, rect, Some(group))?;
        if let Some(b) = self.blocks.get_mut(&group) {
            b.cell = rect.0;
            b.span = Some(rect.1);
        }
        Ok(())
    }

    /// The page of `shapes` and the smallest rectangle of cells around them.
    fn around(&self, shapes: &[BlockId]) -> ModelResult<(BlockId, (Cell, Cell))> {
        let first = shapes.first().ok_or(ModelError::EmptyGroup)?;
        let page = self
            .block(*first)?
            .parent
            .ok_or(ModelError::PageOutOfPlace)?;
        let mut cells = Vec::new();
        for s in shapes {
            let b = self.block(*s)?;
            if b.parent != Some(page) {
                return Err(ModelError::DifferentDiagrams);
            }
            match b.group_cells() {
                Some((near, far)) => cells.extend([near, far]),
                None => cells.push(b.cell),
            }
        }
        if !self.is_doodle() || self.block(page)?.kind != BlockKind::Page {
            return Err(ModelError::PageOutOfPlace);
        }
        let min = |f: fn(&Cell) -> i32| cells.iter().map(f).min().unwrap_or(0);
        let max = |f: fn(&Cell) -> i32| cells.iter().map(f).max().unwrap_or(0);
        Ok((
            page,
            (
                Cell::new(min(|c| c.col), min(|c| c.row)),
                Cell::new(max(|c| c.col), max(|c| c.row)),
            ),
        ))
    }

    /// Other groups on the page lie fully inside `rect`, fully around it, or apart.
    fn check_nesting(
        &self,
        page: BlockId,
        rect: (Cell, Cell),
        except: Option<BlockId>,
    ) -> ModelResult<()> {
        for other in self.groups_on(page) {
            if Some(other) == except {
                continue;
            }
            let Some(o) = self.blocks.get(&other).and_then(Block::group_cells) else {
                continue;
            };
            let apart = o.1.col < rect.0.col
                || rect.1.col < o.0.col
                || o.1.row < rect.0.row
                || rect.1.row < o.0.row;
            let holds = |a: (Cell, Cell), b: (Cell, Cell)| inside(b.0, a) && inside(b.1, a);
            if !(apart || holds(rect, o) || holds(o, rect)) {
                return Err(ModelError::GroupsOverlap);
            }
        }
        Ok(())
    }

    /// Whether every group has a proper rectangle and no two partly overlap.
    pub fn groups_valid(&self) -> bool {
        self.blocks.iter().all(|(id, b)| {
            let Some((near, far)) = b.group_cells() else {
                return true;
            };
            b.span.is_some()
                && near.col <= far.col
                && near.row <= far.row
                && b.parent
                    .is_some_and(|page| self.check_nesting(page, (near, far), Some(*id)).is_ok())
        })
    }

    /// A group and a shape (or group) inside it cannot be connected.
    pub(super) fn check_group_pair(&self, a: BlockId, b: BlockId) -> ModelResult<()> {
        let within = |g: BlockId, x: BlockId| -> bool {
            let (Some(gb), Some(xb)) = (self.blocks.get(&g), self.blocks.get(&x)) else {
                return false;
            };
            let Some(rect) = gb.group_cells() else {
                return false;
            };
            match xb.group_cells() {
                Some((near, far)) => inside(near, rect) && inside(far, rect),
                None => inside(xb.cell, rect),
            }
        };
        if within(a, b) || within(b, a) {
            return Err(ModelError::GroupMember);
        }
        Ok(())
    }
}

/// Whether `cell` lies in the rectangle from `near` to `far`.
pub fn inside(cell: Cell, (near, far): (Cell, Cell)) -> bool {
    (near.col..=far.col).contains(&cell.col) && (near.row..=far.row).contains(&cell.row)
}
