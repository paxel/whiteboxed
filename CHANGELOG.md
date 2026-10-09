# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0] - Unreleased

### Added

- Move up a level: right-click a box inside a whitebox to take it, with its content, into the diagram one level up; its relations follow. Also an AI tool, move_up.
- A readability score per diagram (green, yellow, red) from the number of boxes, crossing lines and the busiest box. A dot in the structure tree and next to the breadcrumb shows it, the details panel says what helps, the limits are project settings, and the AI gets it in get_diagram together with modelling advice.
- HTML document export: one self-contained file with every diagram, text and table. Boxes, lines and frame ends are clickable, a tree and breadcrumbs lead through the levels, and it follows light or dark mode with a switch in the top right corner. The AI's export_docs writes it with html: true.
- AI access can be opened to Docker containers (listening on the Docker bridge, reachable as host.docker.internal) or to an address you type, with a warning; this computer only stays the default and the token is always needed.
- The AI's get_diagram lists problems worth tidying: crossing lines, overlapping labels and labels that do not fit.
- An AI batch tool: several editing calls run as one undo step, all or nothing. A failing step changes nothing and is named in the error; later steps can address boxes that earlier steps created.
- A relation that enters a whitebox can land on several boxes inside: connecting it again from another box (or attaching it again through the AI) fans the line out from one frame point. Deleting a branch, or the AI's delete_relation with detach_in and landing, removes only that landing.
- Cross-cutting bands: building blocks such as logging or security drawn as bands across the bottom of a diagram, without lines. They can be opened like boxes, have a responsibility and appear in the building-block table; the AI can create them with add_box.
- A fourth direction, none: a plain line without arrowheads, in the dialogs and for the AI. In the communication partner table it appears under both Input and Output.
- A project name in Project settings, taken from the system box of the context view until you set one. It titles the window, the unsaved-changes question and the exported documentation, and is offered as the file name on the first save.
- Zoom buttons in the lower right corner of the canvas (−, the zoom level with a click for 100 %, +, Fit), Ctrl+= / Ctrl+− / Ctrl+0, and zoom entries in the View menu. The view no longer jumps back to fit the whole diagram after every change; new boxes outside it are scrolled into view.
- Boxes without a relation can be added anywhere: right-click empty space > Add box here puts the box in that cell, and + Box in the menu bar puts it in the next free cell.
- Short labels for relations; long texts without one appear as [1], [2] … with the full text in a legend below the diagram (in the app, the export and as a tooltip). When texts get shortened is set in Project settings.
- Line styles for relations: square, slightly rounded, rounded or curved; set for the project under File > Project settings and per relation, with your choice as the default for new projects.

### Changed

- The AI dialog explains connecting Claude Code in three steps: a terminal in the project folder (not inside Claude Code), the copied command, then claude and /mcp.
- One File > Export… dialog replaces the four export entries: this diagram or all, SVG, PNG at 1×/2×/3×, AsciiDoc, Markdown or no text, and the target folder. The choices are kept in the project file, the folder relative to the project file inside its repository.
- Lines between boxes that face each other are straight, also across empty cells, and relations from the level above enter a whitebox opposite the box that takes them, instead of being spread evenly along the side.
- Lines on one side of a box keep 40 px apart instead of 26 px, so a box visibly grows from its second relation on a side; the minimum box size stays.
- Several new boxes on one side of a box fill a block that stays as square as possible (2×2, 3×2, 3×3 …) instead of one long row.
- Moving a box, also when connecting moves it, turns its lines to the sides that face their partners (or the frame side a line from outside enters at). A side can also be chosen by hand in the relation dialog and through the AI's edit_relation, until the next move.
- Lines avoid crossing each other where a short detour or a different lane order allows it; where lines still cross, the horizontal one jumps over the other in a small arc, and labels move aside so they do not hide it.
- The question about unsaved changes before quitting, starting a new project or opening another one is now a large dialog over the dimmed window, with buttons that say what they do; Enter saves, Esc keeps editing.

### Fixed

- Relation texts next to a whitebox frame were cut off at the edge of the picture; labels now stay inside the frame, wrapped if needed.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
