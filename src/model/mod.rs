//! The architecture model: boxes ("blocks") nested into whiteboxes, relations between
//! them, and project-wide tags.
//!
//! A diagram is identified by the block whose whitebox it is; `None` is level 0, the
//! context view. A relation lives in the diagram of its `owner`. Each of its two
//! endpoints is a chain of anchors: the first anchor is a block in the owner diagram,
//! every further anchor is a block inside the whitebox of the previous one. An empty
//! chain is an open end (a stub).

mod edit;

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use edit::{BlockSpec, Placement};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlockId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RelationId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TagId(pub u64);

/// The diagram something lives in: `None` is level 0, `Some(b)` the whitebox of `b`.
pub type DiagramId = Option<BlockId>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Component,
    Database,
    Queue,
    Cache,
    FileStorage,
    Ui,
    Person,
    ExternalSystem,
}

impl BlockKind {
    pub const ALL: [BlockKind; 8] = [
        BlockKind::Component,
        BlockKind::Database,
        BlockKind::Queue,
        BlockKind::Cache,
        BlockKind::FileStorage,
        BlockKind::Ui,
        BlockKind::Person,
        BlockKind::ExternalSystem,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BlockKind::Component => "component",
            BlockKind::Database => "database",
            BlockKind::Queue => "queue/topic",
            BlockKind::Cache => "cache",
            BlockKind::FileStorage => "file storage",
            BlockKind::Ui => "UI",
            BlockKind::Person => "person",
            BlockKind::ExternalSystem => "external system",
        }
    }

    /// Neighbours live in the context view only and never open into a whitebox.
    pub fn is_neighbour(self) -> bool {
        matches!(self, BlockKind::Person | BlockKind::ExternalSystem)
    }

    pub fn can_drill(self) -> bool {
        !self.is_neighbour()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::Top, Side::Right, Side::Bottom, Side::Left];

    pub fn opposite(self) -> Side {
        match self {
            Side::Top => Side::Bottom,
            Side::Right => Side::Left,
            Side::Bottom => Side::Top,
            Side::Left => Side::Right,
        }
    }

    pub fn is_horizontal(self) -> bool {
        matches!(self, Side::Left | Side::Right)
    }

    pub fn label(self) -> &'static str {
        match self {
            Side::Top => "top",
            Side::Right => "right",
            Side::Bottom => "bottom",
            Side::Left => "left",
        }
    }
}

/// Direction of a relation, seen from its endpoint `a` (the box it was created from).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Out,
    In,
    Bi,
}

impl Direction {
    pub const ALL: [Direction; 3] = [Direction::Out, Direction::In, Direction::Bi];

    pub fn label(self) -> &'static str {
        match self {
            Direction::Out => "out",
            Direction::In => "in",
            Direction::Bi => "bi",
        }
    }

    /// Whether an arrowhead points into endpoint `a`.
    pub fn arrow_at_a(self) -> bool {
        matches!(self, Direction::In | Direction::Bi)
    }

    /// Whether an arrowhead points into endpoint `b`.
    pub fn arrow_at_b(self) -> bool {
        matches!(self, Direction::Out | Direction::Bi)
    }
}

/// A slot in the layout grid of a diagram. Only the order matters; the layout
/// normalises the coordinates, so they may be negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Cell {
    pub col: i32,
    pub row: i32,
}

impl Cell {
    pub fn new(col: i32, row: i32) -> Self {
        Cell { col, row }
    }

    pub fn toward(self, side: Side) -> Cell {
        match side {
            Side::Top => Cell::new(self.col, self.row - 1),
            Side::Right => Cell::new(self.col + 1, self.row),
            Side::Bottom => Cell::new(self.col, self.row + 1),
            Side::Left => Cell::new(self.col - 1, self.row),
        }
    }

    /// Whether `other` lies on `side` of this cell.
    pub fn sees_on(self, side: Side, other: Cell) -> bool {
        match side {
            Side::Top => other.row < self.row,
            Side::Right => other.col > self.col,
            Side::Bottom => other.row > self.row,
            Side::Left => other.col < self.col,
        }
    }

    /// The side of this cell that faces `other`, preferring the dominant axis.
    pub fn side_facing(self, other: Cell) -> Side {
        let dc = other.col - self.col;
        let dr = other.row - self.row;
        if dc.abs() >= dr.abs() && dc != 0 {
            if dc > 0 { Side::Right } else { Side::Left }
        } else if dr < 0 {
            Side::Top
        } else {
            Side::Bottom
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

impl Rgb {
    pub fn parse(text: &str) -> Option<Rgb> {
        let hex = text.strip_prefix('#')?;
        if hex.len() != 6 || !hex.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Rgb(byte(0)?, byte(2)?, byte(4)?))
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Rgb::parse(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("not a #rrggbb colour: {text}")))
    }
}

/// Tag colours, handed out in order to new tags.
pub const PALETTE: [Rgb; 10] = [
    Rgb(0xa8, 0xd5, 0xf7),
    Rgb(0xb8, 0xe6, 0xa8),
    Rgb(0xff, 0xd5, 0x8f),
    Rgb(0xf4, 0xb0, 0xc8),
    Rgb(0xd0, 0xbf, 0xf2),
    Rgb(0x9f, 0xe3, 0xd9),
    Rgb(0xf7, 0xc0, 0xa1),
    Rgb(0xe3, 0xe3, 0x8f),
    Rgb(0xc4, 0xcf, 0xdb),
    Rgb(0xe8, 0xb4, 0xf0),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub color: Rgb,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub name: String,
    pub kind: BlockKind,
    pub tag: Option<TagId>,
    pub parent: DiagramId,
    pub cell: Cell,
    /// What this building block is responsible for (arc42 blackbox description).
    pub responsibility: String,
    /// Why the whitebox of this block is decomposed the way it is.
    pub motivation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub block: BlockId,
    pub side: Side,
}

/// One end of a relation: a chain of anchors from the owner diagram inward. Empty
/// means open (a stub).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Endpoint {
    pub anchors: Vec<Anchor>,
}

impl Endpoint {
    pub fn at(block: BlockId, side: Side) -> Self {
        Endpoint {
            anchors: vec![Anchor { block, side }],
        }
    }

    pub fn open() -> Self {
        Endpoint::default()
    }

    pub fn is_open(&self) -> bool {
        self.anchors.is_empty()
    }

    pub fn position_of(&self, block: BlockId) -> Option<usize> {
        self.anchors.iter().position(|a| a.block == block)
    }
}

/// Which endpoint of a relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum End {
    A,
    B,
}

impl End {
    pub fn other(self) -> End {
        match self {
            End::A => End::B,
            End::B => End::A,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub owner: DiagramId,
    pub a: Endpoint,
    pub b: Endpoint,
    pub direction: Direction,
    pub text: String,
    /// Overrides the project's line style for this relation.
    pub style: Option<LineStyle>,
    /// What the diagram shows instead of a long text; empty uses the text.
    pub short: String,
}

/// How the bends of relation lines are drawn.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum LineStyle {
    /// Sharp right angles.
    Square,
    /// Corners rounded with a 6 px radius.
    Round6,
    /// Corners rounded with a 12 px radius.
    #[default]
    Round12,
    /// Smooth curves instead of corners.
    Curved,
}

impl LineStyle {
    pub const ALL: [LineStyle; 4] = [
        LineStyle::Square,
        LineStyle::Round6,
        LineStyle::Round12,
        LineStyle::Curved,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LineStyle::Square => "square corners",
            LineStyle::Round6 => "slightly rounded (6 px)",
            LineStyle::Round12 => "rounded (12 px)",
            LineStyle::Curved => "curved",
        }
    }
}

impl Relation {
    pub fn end(&self, end: End) -> &Endpoint {
        match end {
            End::A => &self.a,
            End::B => &self.b,
        }
    }

    pub fn end_mut(&mut self, end: End) -> &mut Endpoint {
        match end {
            End::A => &mut self.a,
            End::B => &mut self.b,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    #[error("a box needs a name")]
    EmptyName,
    #[error("\"{0}\" already exists in this diagram")]
    DuplicateName(String),
    #[error("{0} boxes only exist in the context view")]
    NeighbourBelowContext(&'static str),
    #[error("{0} boxes cannot be opened")]
    NotDrillable(&'static str),
    #[error("a box with content cannot become a {0}")]
    KindHasContent(&'static str),
    #[error("unknown box")]
    UnknownBlock,
    #[error("unknown relation")]
    UnknownRelation,
    #[error("unknown tag")]
    UnknownTag,
    #[error("both boxes must be in the same diagram")]
    DifferentDiagrams,
    #[error("a box cannot connect to itself")]
    SelfRelation,
    #[error("this end does not pass through that box")]
    NotOnChain,
    #[error("this end is not open")]
    NotOpen,
    #[error("a name can have at most {MAX_NAME} characters")]
    NameTooLong,
    #[error("a text can have at most {MAX_TEXT} characters")]
    TextTooLong,
    #[error("names and texts cannot contain control characters")]
    ControlCharacter,
    #[error("a diagram can hold at most {MAX_BLOCKS_PER_DIAGRAM} boxes")]
    DiagramFull,
    #[error("grid cells range from -{MAX_CELL} to {MAX_CELL}")]
    OutsideGrid,
    #[error("a diagram can hold at most {MAX_RELATIONS_PER_DIAGRAM} relations")]
    TooManyRelations,
}

pub type ModelResult<T> = Result<T, ModelError>;

/// Limits that keep a project drawable, also when an AI client fills it.
pub const MAX_NAME: usize = 120;
pub const MAX_TEXT: usize = 10_000;
pub const MAX_BLOCKS_PER_DIAGRAM: usize = 250;
pub const MAX_CELL: i32 = 1_000;
pub const MAX_RELATIONS_PER_DIAGRAM: usize = 1_000;

impl Cell {
    pub fn in_grid(self) -> bool {
        self.col.abs() <= MAX_CELL && self.row.abs() <= MAX_CELL
    }
}

/// A whole project: every level of every whitebox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub blocks: BTreeMap<BlockId, Block>,
    pub relations: BTreeMap<RelationId, Relation>,
    pub tags: BTreeMap<TagId, Tag>,
    /// The project's name as set by the user; empty: see [`Project::display_name`].
    pub name: String,
    /// The explanation of the context view.
    pub motivation: String,
    /// Line style for relations without their own.
    pub line_style: LineStyle,
    /// Relation texts longer than this many characters (without a short label)
    /// become numbers with a legend; 0 numbers every text, `None` never shortens.
    pub label_limit: Option<u32>,
    pub next_id: u64,
}

/// The label limit a new project starts with.
pub const DEFAULT_LABEL_LIMIT: Option<u32> = Some(24);

impl Default for Project {
    fn default() -> Self {
        Project {
            blocks: BTreeMap::new(),
            relations: BTreeMap::new(),
            tags: BTreeMap::new(),
            name: String::new(),
            motivation: String::new(),
            line_style: LineStyle::default(),
            label_limit: DEFAULT_LABEL_LIMIT,
            next_id: 0,
        }
    }
}

impl Project {
    pub fn new() -> Self {
        Project::default()
    }

    /// The project's name: the one set by the user, or else [`Project::auto_name`].
    pub fn display_name(&self) -> Option<String> {
        if self.name.is_empty() {
            self.auto_name()
        } else {
            Some(self.name.clone())
        }
    }

    /// The name of the system: the first box of the context view that is neither a
    /// person nor an external system. It follows that box's renames.
    pub fn auto_name(&self) -> Option<String> {
        self.blocks_in(None)
            .find(|(_, b)| b.kind.can_drill())
            .map(|(_, b)| b.name.clone())
    }

    pub fn block(&self, id: BlockId) -> ModelResult<&Block> {
        self.blocks.get(&id).ok_or(ModelError::UnknownBlock)
    }

    pub fn relation(&self, id: RelationId) -> ModelResult<&Relation> {
        self.relations.get(&id).ok_or(ModelError::UnknownRelation)
    }

    /// Blocks of one diagram, in id order.
    pub fn blocks_in(&self, diagram: DiagramId) -> impl Iterator<Item = (BlockId, &Block)> {
        self.blocks
            .iter()
            .filter(move |(_, b)| b.parent == diagram)
            .map(|(id, b)| (*id, b))
    }

    pub fn has_content(&self, block: BlockId) -> bool {
        self.blocks.values().any(|b| b.parent == Some(block))
    }

    /// Depth of a diagram: 0 for the context view, 1 for the whitebox of a level-0
    /// box, and so on.
    pub fn level(&self, diagram: DiagramId) -> usize {
        self.path(diagram).len()
    }

    /// The blocks from level 0 down to and including `diagram`'s owner.
    pub fn path(&self, diagram: DiagramId) -> Vec<BlockId> {
        let mut path = Vec::new();
        let mut current = diagram;
        while let Some(id) = current {
            path.push(id);
            current = self.blocks.get(&id).and_then(|b| b.parent);
            if path.len() > self.blocks.len() {
                break;
            }
        }
        path.reverse();
        path
    }

    /// `block` and every block nested inside it, at any depth.
    pub fn subtree(&self, block: BlockId) -> Vec<BlockId> {
        let mut out = vec![block];
        let mut i = 0;
        while i < out.len() {
            let current = out[i];
            out.extend(
                self.blocks
                    .iter()
                    .filter(|(_, b)| b.parent == Some(current))
                    .map(|(id, _)| *id),
            );
            i += 1;
        }
        out
    }

    /// The motivation of a diagram: the context view's or a whitebox's.
    pub fn motivation(&self, diagram: DiagramId) -> &str {
        match diagram {
            None => &self.motivation,
            Some(id) => self.blocks.get(&id).map_or("", |b| b.motivation.as_str()),
        }
    }

    pub fn tag_by_name(&self, name: &str) -> Option<TagId> {
        let wanted = name.trim().to_lowercase();
        self.tags
            .iter()
            .find(|(_, t)| t.name.to_lowercase() == wanted)
            .map(|(id, _)| *id)
    }

    fn allocate(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
}
