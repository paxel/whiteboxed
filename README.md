# whiteboxed

A WYSIWYG editor for [arc42](https://arc42.org) building-block views — draw.io for
architects who don't want to push pixels.

> Status: early development, not released yet.

## Idea

You model a system as boxes and relations; the app does the layout. Every box is a
blackbox on its own level and can be opened into its whitebox one level deeper.

## Design

### Model

- One model with drill-down: double-click a box to open its whitebox one level
  deeper. No depth limit; names stay consistent across levels.
- Level 0 is the context view: your system plus external neighbours.
- Fixed box types: component, database, queue/topic, cache, file storage, UI, person,
  external system. Persons and external systems exist only in the context view and
  never open; every other type can be drilled into.
- A box has a name (unique within its diagram), a type and at most one tag.
- Tags are project-wide. Every tag has a colour (auto-assigned from a palette,
  changeable) that fills all boxes carrying it.
- Relations have a direction (in, out, bi) and a text. Two boxes can share several
  relations.

### Levels

- A relation that enters a box from the parent level appears inside its whitebox as an
  open end. It may stay unattached, marked with a warning.
- A stub created inside a whitebox bubbles up to every enclosing box's border, up to
  the context view. It can be connected on any of those levels.
- Deleting a line inside a whitebox only detaches it there; the relation stays on its
  own level.

### Interaction

- An empty diagram shows "+ add box".
- Click a side of a box border to open a popup: connect new, connect existing or
  stub; direction (default out); optional text. Enter confirms.
- Connect existing: pick the target from a filterable list or click it in the diagram.
- The clicked side is binding: an existing partner moves to that side. On a conflict
  the app asks whether to move the partner (default) or keep it and route around.
- Right-click a box (edit, open whitebox, delete) or a line (edit, delete).
- Double-click a relation to edit its text.
- Click an open end (stub circle or warning marker) to connect it to a box.
- Deleting a box deletes its relations. Deleting a box with whitebox content asks
  first.
- Unlimited undo/redo within a session.
- Navigation via breadcrumb and a tree sidebar.

### Layout

- Grid based and fully automatic. A box occupies grid cells; each side grows with the
  number of relations attached to it. Lines are routed orthogonally in the gaps.
- If the target cell is taken, the new box goes to a free cell next to it in the same
  column.
- Boxes can be moved to another cell by dragging (snaps) or with the arrow keys
  (swaps with an occupant).
- The layout is deterministic: the same model always gives the same picture.

### Files

- One YAML file per project, with a stable key order. It stores the model and the
  layout hints only, no coordinates.
- Autosave goes to a recovery file; Ctrl+S writes the project file.
- Export to SVG and PNG, one image per diagram.

## Using it

Start `whiteboxed`, or `whiteboxed path/to/project.yaml` to open a project.

| Action                         | How                                       |
|--------------------------------|-------------------------------------------|
| Add the first box              | "+ add box" in an empty diagram           |
| Add a relation                 | Click a box border on the side you want   |
| Open a whitebox                | Double-click the box, or Enter            |
| Go up one level                | Backspace, or click the breadcrumb        |
| Edit a box                     | Right-click > Edit, or F2                 |
| Edit a relation                | Double-click the line                     |
| Move a box                     | Drag it to another cell, or arrow keys    |
| Delete the selected box        | Delete                                    |
| Undo / redo                    | Ctrl+Z / Ctrl+Shift+Z (or Ctrl+Y)         |
| Save / save as                 | Ctrl+S / Ctrl+Shift+S                     |
| Pan / zoom                     | Drag empty space or scroll / Ctrl+scroll  |
| Change a tag colour            | Click its swatch in the sidebar           |

## Platforms

Windows, Linux and macOS. Built with Rust and egui/eframe.

## Building

```sh
cargo build
cargo test
```

## License

[MIT](LICENSE)
