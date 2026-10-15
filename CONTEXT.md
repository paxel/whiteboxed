# whiteboxed

A WYSIWYG editor for arc42 building-block views: the user describes a system as boxes and relations, and whiteboxed lays them out.

## Language

### Files

**Project**:
An arc42 model of one system: its context view, its whiteboxes and their texts.

**Doodle**:
A free sketch on pages, without the arc42 rules; a sibling of a project, not a kind of one.

**File**:
A project or a doodle, when it does not matter which; each is exactly one file.
_Avoid_: Document

**Diagram**:
One drawing in a file: in a project the context view or a whitebox, in a doodle a page.

### Architecture

**Box**:
A building block of the architecture in the arc42 sense, drawn on a diagram.
_Avoid_: Block

**Box type**:
What a box is in arc42 terms (component, database, queue/topic, cache, file storage, UI, person, external system); the only thing about a box that carries rules, such as where it may sit and whether it opens into a whitebox.
_Avoid_: Kind, shape type

**Band**:
A box that every box of its diagram uses, drawn across the bottom of the diagram without lines.
_Avoid_: Cross-cutting concept (that is arc42 section 8, not a box)

**Whitebox**:
The diagram that shows the inside of one box.

**Blackbox**:
A box seen from outside, in the diagram it sits in, described by its responsibility; the counterpart of its whitebox.

**Frame**:
The border of a whitebox, labelled with its box's name; interfaces enter through it. Not used for the outline of a group.

**Context view**:
The diagram at level 0, the only one that is not a whitebox.

**Neighbour**:
A person or external system next to the system in the context view; it never opens into a whitebox. Not used for any other box of the same diagram.

**Relation**:
A connection between two boxes in the model, owned by one diagram; it also reaches into the whiteboxes of the boxes it ends at.

**Line**:
A connection drawn in a diagram. In a project it is how one relation is drawn, and a relation has a line in every diagram it reaches; in a doodle it is the connection itself, between shapes or groups, with no relation behind it.

**Partner**:
The box at the other end of a relation, seen from one box; on any level, not only a neighbour.

**Stub**:
A relation with an open end; once the open end is connected, it is a plain relation.

**Open end**:
The end of a stub that reaches no box yet.

**Interface**:
A relation from outside, as seen in the whitebox it enters through the frame; in its own diagram it is a plain relation.

**Attach**:
To land an interface on a box inside the whitebox; an interface attached to no box yet is unattached. The reverse is to detach it.
_Avoid_: Take, assign, handled by

**Level**:
How deeply a diagram is nested: the context view is level 0, and a whitebox is one level deeper than the diagram its box sits in. A box's level is the level of its diagram.
_Avoid_: Depth

### Doodles

**Page**:
A named diagram of a doodle, shown as a tab.

**Shape**:
Something drawn on a page that lines connect, without responsibility, whitebox or arc42 meaning. Not a box; the plain outlines are the rectangle and the rounded rectangle.
_Avoid_: Box, rounded box

**Group**:
A labelled outline around a rectangle of cells on a page, holding every shape inside it; groups nest, and lines connect shapes and groups. Not a shape. Grouping boxes into a new box in a project is a different thing and adds a level.

**Shape type**:
How a shape is drawn (rectangle, rounded rectangle, circle, diamond, cylinder, document, cloud, person, note); it carries no rules.
_Avoid_: Box type, kind

### Appearance

**Look**:
A named picture from the library that a box or a shape can wear; on a box it changes only the drawing, never the box type's rules. The picture is the SVG inside a look.
_Avoid_: Custom look

**Tag**:
A named category of boxes across all levels of a project, shown by its colour; a box has at most one. Doodles have no tags.

**Legend**:
The numbered list below a diagram that holds the texts of lines too long to write on them, and of every line that goes through mouseholes.

**Mousehole**:
A labelled hole at a box, a shape or a frame where a line goes under the diagram; the line comes out at a mousehole with the same label at its other end, and nothing is drawn between them. The label is the line's legend number. In projects and doodles alike.
_Avoid_: Jump (a line hopping over a crossing line)

### Readability

**Readability**:
How easy a diagram is to read, pages included: green, yellow or red, set by its worst measure. A working aid, not part of arc42.
_Avoid_: Score, traffic light

**Measure**:
One count that feeds a diagram's readability: boxes (or shapes) in the diagram, lines that cross, lines on the busiest box (or shape), or pairs of mouseholes (a bundle's pair counts once, each branch of a fan-out once). A line through mouseholes crosses nothing but still counts for its boxes.

**Limit**:
The count from which a measure turns a diagram yellow, or red; set per file.
