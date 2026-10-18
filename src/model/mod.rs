//! The architecture model: boxes ("blocks") nested into whiteboxes, relations between
//! them, and project-wide tags.
//!
//! A diagram is identified by the block whose whitebox it is; `None` is level 0, the
//! context view. A relation lives in the diagram of its `owner`. Each of its two
//! endpoints is a chain of anchors: the first anchor is a block in the owner diagram,
//! every further anchor is a block inside the whitebox of the previous one. An empty
//! chain is an open end (a stub).

mod doodle;
pub use doodle::inside as inside_cells;
mod edit;
mod restructure;

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::look::{LineLooks, LookId, Looks, Pictures};

pub use edit::{BlockSpec, Placement};
pub use restructure::SplitPart;

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
    /// A page of a doodle: a top-level box that is never drawn; its whitebox is the
    /// page.
    Page,
    /// Doodle shapes: a process step.
    Rounded,
    /// Start or end.
    Circle,
    /// A decision.
    Diamond,
    Document,
    /// A network, the internet.
    Cloud,
    /// Text without a border.
    Note,
    /// A labelled frame around a rectangle of cells on a doodle page.
    Group,
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
            BlockKind::Page => "page",
            BlockKind::Rounded => "rounded rectangle",
            BlockKind::Circle => "circle",
            BlockKind::Diamond => "diamond",
            BlockKind::Document => "document",
            BlockKind::Cloud => "cloud",
            BlockKind::Note => "note",
            BlockKind::Group => "group",
        }
    }

    /// The shapes of a doodle, in the order the dialogs offer them.
    pub const DOODLE: [BlockKind; 9] = [
        BlockKind::Component,
        BlockKind::Rounded,
        BlockKind::Circle,
        BlockKind::Diamond,
        BlockKind::Database,
        BlockKind::Document,
        BlockKind::Cloud,
        BlockKind::Person,
        BlockKind::Note,
    ];

    /// The name of a kind as a doodle shape.
    pub fn doodle_label(self) -> &'static str {
        match self {
            BlockKind::Component => "rectangle",
            BlockKind::Database => "cylinder",
            other => other.label(),
        }
    }

    /// Shapes that exist only in doodles.
    pub fn doodle_only(self) -> bool {
        matches!(
            self,
            BlockKind::Rounded
                | BlockKind::Circle
                | BlockKind::Diamond
                | BlockKind::Document
                | BlockKind::Cloud
                | BlockKind::Note
                | BlockKind::Group
        )
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
    /// A plain line without arrowheads.
    #[serde(rename = "none")]
    Undirected,
}

impl Direction {
    pub const ALL: [Direction; 4] = [
        Direction::Out,
        Direction::In,
        Direction::Bi,
        Direction::Undirected,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Direction::Out => "out",
            Direction::In => "in",
            Direction::Bi => "bi",
            Direction::Undirected => "none",
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
    /// The side of `to` an L from here arrives at, leaving on `side` and bending once
    /// in `to`'s column (sideways legs) or row, when `to` lies ahead on that side and
    /// every cell both legs run through is free. `None`: no such L.
    pub fn l_side(self, side: Side, to: Cell, free: impl Fn(Cell) -> bool) -> Option<Side> {
        if !self.sees_on(side, to) || (self.col == to.col || self.row == to.row) {
            return None;
        }
        let between = |a: i32, b: i32| {
            let (lo, hi) = (a.min(b), a.max(b));
            (lo + 1)..hi
        };
        let (corner, legs_free, arrives) = if side.is_horizontal() {
            let corner = Cell::new(to.col, self.row);
            let along = between(self.col, to.col).all(|c| free(Cell::new(c, self.row)));
            let down = between(self.row, to.row).all(|r| free(Cell::new(to.col, r)));
            let arrives = if to.row > self.row {
                Side::Top
            } else {
                Side::Bottom
            };
            (corner, along && down, arrives)
        } else {
            let corner = Cell::new(self.col, to.row);
            let down = between(self.row, to.row).all(|r| free(Cell::new(self.col, r)));
            let along = between(self.col, to.col).all(|c| free(Cell::new(c, to.row)));
            let arrives = if to.col > self.col {
                Side::Left
            } else {
                Side::Right
            };
            (corner, down && along, arrives)
        };
        (legs_free && free(corner)).then_some(arrives)
    }

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
    /// In a doodle, any text, over several lines.
    pub name: String,
    pub kind: BlockKind,
    pub tag: Option<TagId>,
    pub parent: DiagramId,
    /// Unused for a band.
    pub cell: Cell,
    /// A cross-cutting band across the bottom of the diagram, without lines.
    pub band: bool,
    /// A doodle shape's own colour; `None`: the tag's or the default.
    pub color: Option<Rgb>,
    /// A group's far corner; `cell` is the near one. Every shape in the rectangle
    /// belongs to the group.
    pub span: Option<Cell>,
    /// A custom look from the project's [`Project::looks`].
    pub look: Option<LookId>,
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

/// One end of a relation: anchors from the owner diagram inward. The first is the box
/// on the owner's level; every other anchor lies directly inside the box of an earlier
/// one, so a line that fans out to several boxes of a whitebox forms a tree. Empty
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
    /// Solid, dashed or dotted.
    pub pattern: LinePattern,
    /// The line's own colour on every level; `None`: the default.
    pub color: Option<Rgb>,
    /// Whether the line goes through mouseholes, per diagram it shows in (its own,
    /// or the box whose whitebox it enters); absent: automatic.
    pub mouseholes: BTreeMap<DiagramId, MouseholeMode>,
    /// Frame sides chosen by hand, per whitebox (the box whose whitebox it enters);
    /// elsewhere the side is chosen automatically.
    pub frame_sides: BTreeMap<BlockId, Side>,
    /// The line look it wears on every level, from [`Project::line_looks`].
    pub look: Option<LookId>,
}

/// Whether a line goes through mouseholes in a diagram: into a numbered hole near one
/// end and out of the hole with the same number near the other, nothing drawn between.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum MouseholeMode {
    /// When a straight line from end to end would cross more than two boxes, or
    /// the routed line would cross more than two other lines.
    #[default]
    Automatic,
    Always,
    Never,
}

impl MouseholeMode {
    pub const ALL: [MouseholeMode; 3] = [
        MouseholeMode::Automatic,
        MouseholeMode::Always,
        MouseholeMode::Never,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MouseholeMode::Automatic => "automatic",
            MouseholeMode::Always => "always",
            MouseholeMode::Never => "never",
        }
    }
}

/// How a line is drawn along its length.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum LinePattern {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

impl LinePattern {
    pub const ALL: [LinePattern; 3] =
        [LinePattern::Solid, LinePattern::Dashed, LinePattern::Dotted];

    pub fn label(self) -> &'static str {
        match self {
            LinePattern::Solid => "solid",
            LinePattern::Dashed => "dashed",
            LinePattern::Dotted => "dotted",
        }
    }
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
    /// Whether the line goes through mouseholes in `diagram`.
    pub fn mousehole_in(&self, diagram: DiagramId) -> MouseholeMode {
        self.mouseholes.get(&diagram).copied().unwrap_or_default()
    }

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
    #[error("{0} boxes cannot be cross-cutting bands")]
    BandKind(&'static str),
    #[error("cross-cutting bands have no lines")]
    BandHasNoLines,
    #[error("delete the lines of this box before making it a cross-cutting band")]
    LinesOnBand,
    #[error("cross-cutting bands stay at the bottom of the diagram")]
    BandStays,
    #[error("this box is already in the context view")]
    AtTop,
    #[error("the box to group at must be one of the grouped boxes")]
    NotInGroup,
    #[error("pages, groups and bands cannot wear a look")]
    NoLook,
    #[error("a doodle keeps at least one page")]
    LastPage,
    #[error("lines in a doodle connect two shapes; there are no open ends")]
    NoOpenEnds,
    #[error("{0} shapes exist only in doodles")]
    DoodleOnly(&'static str),
    #[error("a group can lie inside another one, but not partly over it")]
    GroupsOverlap,
    #[error("a group needs at least one shape")]
    EmptyGroup,
    #[error("a group and a shape inside it cannot be connected")]
    GroupMember,
    #[error("pages only exist in doodles, as the top level")]
    PageOutOfPlace,
    #[error("a relation with an open end cannot be split")]
    SplitOpen,
    #[error("say at least one pair of boxes to split into")]
    SplitEmpty,
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
    /// An arc42 model or a doodle.
    pub kind: ProjectKind,
    /// The project's name as set by the user; empty: see [`Project::display_name`].
    pub name: String,
    /// The explanation of the context view.
    pub motivation: String,
    /// Line style for relations without their own.
    pub line_style: LineStyle,
    /// Relation texts longer than this many characters (without a short label)
    /// become numbers with a legend; 0 numbers every text, `None` never shortens.
    pub label_limit: Option<u32>,
    /// The last choices made in File > Export…
    pub export: Option<ExportChoice>,
    /// From when a diagram counts as crowded or hard to read.
    pub score_limits: ScoreLimits,
    /// Whether relations between the same two boxes are drawn as one line.
    pub bundle: bool,
    /// Copies of the custom looks the boxes wear.
    pub looks: Looks,
    /// Copies of the pictures the line looks and portals use.
    pub pictures: Pictures,
    /// Copies of the line looks the relations wear.
    pub line_looks: LineLooks,
    /// The pictures drawn as mouseholes: where a line goes in, and where it comes out.
    /// Without one the hole is an arch.
    pub portal_in: Option<LookId>,
    pub portal_out: Option<LookId>,
    pub next_id: u64,
}

/// What a project file holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectKind {
    /// An arc42 building-block view: levels, whiteboxes, arc42 texts.
    #[default]
    Arc42,
    /// A free sketch on named pages, without the arc42 rules.
    Doodle,
}

/// Readability limits per diagram, each as (yellow from, red from).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreLimits {
    pub boxes: (u32, u32),
    pub crossings: (u32, u32),
    /// Lines on the busiest box.
    pub lines: (u32, u32),
    /// Pairs of mouseholes.
    #[serde(default = "mousehole_limits")]
    pub mouseholes: (u32, u32),
    /// Line looks in use.
    #[serde(default = "line_look_limits")]
    pub line_looks: (u32, u32),
}

fn mousehole_limits() -> (u32, u32) {
    (3, 5)
}

fn line_look_limits() -> (u32, u32) {
    (3, 5)
}

impl Default for ScoreLimits {
    fn default() -> Self {
        ScoreLimits {
            boxes: (8, 10),
            crossings: (1, 3),
            lines: (6, 9),
            mouseholes: mousehole_limits(),
            line_looks: line_look_limits(),
        }
    }
}

/// What File > Export… writes, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportChoice {
    /// Every diagram, or only the one shown.
    pub all: bool,
    pub svg: bool,
    pub png: bool,
    /// Pixels per diagram unit for the PNG: 1, 2 or 3.
    pub png_scale: u8,
    /// A text file per diagram (and an index when exporting all), or none.
    pub text: Option<crate::doc::DocFormat>,
    /// One HTML file with every diagram, its texts and tables.
    pub html: bool,
    /// The target folder: relative to the project file when it lies in the same
    /// repository, otherwise absolute. Empty until one is chosen.
    pub folder: String,
}

impl Default for ExportChoice {
    fn default() -> Self {
        ExportChoice {
            all: true,
            svg: true,
            png: true,
            png_scale: 2,
            text: Some(crate::doc::DocFormat::AsciiDoc),
            html: false,
            folder: String::new(),
        }
    }
}

/// The label limit a new project starts with.
pub const DEFAULT_LABEL_LIMIT: Option<u32> = Some(24);

impl Default for Project {
    fn default() -> Self {
        Project {
            blocks: BTreeMap::new(),
            relations: BTreeMap::new(),
            tags: BTreeMap::new(),
            kind: ProjectKind::Arc42,
            name: String::new(),
            motivation: String::new(),
            line_style: LineStyle::default(),
            label_limit: DEFAULT_LABEL_LIMIT,
            export: None,
            score_limits: ScoreLimits::default(),
            bundle: true,
            looks: Looks::new(),
            pictures: Pictures::new(),
            line_looks: LineLooks::new(),
            portal_in: None,
            portal_out: None,
            next_id: 0,
        }
    }
}

impl Block {
    /// Whether the box takes no grid cell of its own: a band, a doodle page, or a
    /// group (which spans cells).
    pub fn gridless(&self) -> bool {
        self.band || matches!(self.kind, BlockKind::Page | BlockKind::Group)
    }

    /// A group's rectangle of cells: near and far corner.
    pub fn group_cells(&self) -> Option<(Cell, Cell)> {
        (self.kind == BlockKind::Group).then(|| (self.cell, self.span.unwrap_or(self.cell)))
    }
}

impl Project {
    pub fn new() -> Self {
        Project::default()
    }

    /// A new doodle with one empty page.
    pub fn new_doodle() -> Self {
        let mut p = Project {
            kind: ProjectKind::Doodle,
            ..Project::default()
        };
        let _ = p.add_page("Page 1");
        p
    }

    pub fn is_doodle(&self) -> bool {
        self.kind == ProjectKind::Doodle
    }

    /// The pages of a doodle, in the order they were made.
    pub fn pages(&self) -> Vec<BlockId> {
        self.blocks_in(None)
            .filter(|(_, b)| b.kind == BlockKind::Page)
            .map(|(id, _)| id)
            .collect()
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
        if self.is_doodle() {
            return None;
        }
        self.blocks_in(None)
            .find(|(_, b)| b.kind.can_drill() && !b.band)
            .map(|(_, b)| b.name.clone())
    }

    pub fn block(&self, id: BlockId) -> ModelResult<&Block> {
        self.blocks.get(&id).ok_or(ModelError::UnknownBlock)
    }

    pub fn relation(&self, id: RelationId) -> ModelResult<&Relation> {
        self.relations.get(&id).ok_or(ModelError::UnknownRelation)
    }

    /// Blocks of one diagram, in id order.
    /// Whether no box of `diagram` sits in `cell` (groups and bands take no cell).
    pub fn cell_free(&self, diagram: DiagramId, cell: Cell) -> bool {
        !self
            .blocks_in(diagram)
            .any(|(_, b)| !b.gridless() && b.cell == cell)
    }

    /// The sides two boxes of one diagram leave on toward each other: facing sides
    /// when they share a row or column; around one corner through free cells when
    /// they lie diagonally, the upper box leaving sideways (else the lower one);
    /// otherwise the sides facing each other, with a step in between.
    pub fn sides_between(&self, a: BlockId, b: BlockId) -> ModelResult<(Side, Side)> {
        let (ca, cb) = (self.block(a)?.cell, self.block(b)?.cell);
        let diagram = self.block(a)?.parent;
        let facing = ca.side_facing(cb);
        if ca.col == cb.col || ca.row == cb.row {
            return Ok((facing, facing.opposite()));
        }
        let free = |cell| self.cell_free(diagram, cell);
        let (upper, lower, a_is_upper) = if ca.row < cb.row {
            (ca, cb, true)
        } else {
            (cb, ca, false)
        };
        let sideways = if lower.col > upper.col {
            Side::Right
        } else {
            Side::Left
        };
        let l = [sideways, Side::Bottom].into_iter().find_map(|leave| {
            upper
                .l_side(leave, lower, free)
                .map(|arrive| (leave, arrive))
        });
        Ok(match (l, a_is_upper) {
            (Some((leave, arrive)), true) => (leave, arrive),
            (Some((leave, arrive)), false) => (arrive, leave),
            (None, _) => (facing, facing.opposite()),
        })
    }

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

    /// Whether `block` is `ancestor` or lies inside it, at any depth.
    pub fn within(&self, block: BlockId, ancestor: BlockId) -> bool {
        self.path(Some(block)).contains(&ancestor)
    }

    /// Where an endpoint lands inside the whitebox of `outer`: its anchors on boxes
    /// directly inside `outer`. One incoming line can land on several boxes.
    pub fn landings(&self, endpoint: &Endpoint, outer: BlockId) -> Vec<Anchor> {
        endpoint
            .anchors
            .iter()
            .filter(|a| {
                self.blocks
                    .get(&a.block)
                    .is_some_and(|b| b.parent == Some(outer))
            })
            .copied()
            .collect()
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
