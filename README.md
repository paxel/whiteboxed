# whiteboxed

A WYSIWYG editor for [arc42](https://arc42.org) building-block views — draw.io for
architects who don't want to push pixels.

> Status: early development. Nothing usable yet.

## Idea

You model a system as boxes and relations; the app does the layout. Every box is a
blackbox on its own level and can be opened into its whitebox one level deeper.

## Design

### Model

- One model with drill-down: double-click a box to open its whitebox one level
  deeper. No depth limit; names stay consistent across levels.
- Level 0 is the context view: your system plus external neighbours. Only the system
  box can be drilled into; external neighbours stay blackboxes.
- Fixed box types: component, database, queue/topic, cache, file storage, UI, person,
  external system. All internal types can be drilled into.
- A box has a name (unique within its diagram), a type and at most one tag.
- Tags are project-wide. Every tag has a colour (auto-assigned from a palette,
  changeable) that fills all boxes carrying it.
- Relations have a direction (in, out, bi) and a text. Two boxes can share several
  relations.

### Levels

- A relation that enters a box from the parent level appears inside its whitebox as an
  open end. It may stay unattached, marked with a warning.
- A stub created inside a whitebox bubbles up to the parent box's border.

### Interaction

- Click a side of a box border to open a popup: connect new, connect existing or
  stub; direction (default out); optional text. Enter confirms.
- Connect existing: pick the target from a filterable list or click it in the diagram.
- The clicked side is binding: an existing partner moves to that side. On a conflict
  the app asks whether to move the partner (default) or keep it and route around.
- Double-click a relation to edit its text.
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

## Platforms

Windows, Linux and macOS. Built with Rust and egui/eframe.

## Building

```sh
cargo build
cargo test
```

## License

[MIT](LICENSE)
