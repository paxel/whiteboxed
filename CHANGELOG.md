# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.0] - Unreleased

### Added

- Editor window with the context view, drill-down into whiteboxes, a breadcrumb and a structure tree.
- Eight box types: component, database, queue/topic, cache, file storage, UI, person and external system.
- Relations created by clicking a box border: to a new box, to an existing box, or as a stub that leaves the level.
- Relations with a direction (in, out, bi) and a text.
- Relations from a parent level appear inside a whitebox and can be attached to its boxes; unattached ones are marked.
- Automatic grid layout with orthogonal lines; boxes grow with their relations and can be moved by dragging or with the arrow keys.
- A prompt when connecting a box would break the side of another relation: move the box or route around it.
- Project-wide tags that colour every box carrying them.
- A details panel for the responsibility of each box and the motivation of each diagram.
- Unlimited undo and redo, from the arrows in the toolbar or the keyboard.
- Projects saved as readable YAML files, with automatic recovery of unsaved changes after a crash.
- Export of the current diagram as SVG and PNG, and of all diagrams as images plus arc42 text files (AsciiDoc or Markdown) with an index.
- AI access: an MCP endpoint on localhost, off by default and guarded by a token, through which an AI client can build and change the model while you watch.
- An app icon; on Linux the app adds itself to the application menu.
- Installation with Homebrew (Linux, macOS) and Scoop (Windows).
