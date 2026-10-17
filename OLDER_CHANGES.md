# Older changes

Releases before the current one. The current version is in [CHANGELOG.md](CHANGELOG.md).

## [0.5.0] - 2026-10-16

### Added

- Doodle pages get a readability colour, shown as a dot on the page tab that explains on hover what helps; the limits are in File > Project settings… and the AI sees it in doodle_get.
- Lines can have a colour of their own, picked in the line's Edit… dialog or set by the AI; a relation keeps it on every level.
- Mouseholes: a line that would run straight over more than two boxes goes into a numbered hole near each end instead, with its text in the legend; hovering a hole shows both holes and the route. Mouseholes in the line's Edit… dialog (and the AI) set it per line and diagram to automatic, always or never, and lines through mouseholes are a readability measure with limits of their own.

### Changed

- Lines leave a box in the middle of its side, join a diagonal partner with one bend through free cells, enter a box in the order they arrive so none crosses another on its way in, and keep 40 px apart in gaps that widen for them; a crossing is worth a detour of about one cell, no more.
- A relation from outside enters a whitebox in line with the box that takes it; the partner names along the frame's top and bottom go to a further row instead of pushing the ends apart.
- An interface no box handles yet is "not attached" in the status bar and the export, whose column is now "Attached to".
- The plain doodle shapes are called rectangle and rounded rectangle.

### Fixed

- Lines reach the drawing of a look, not the empty margin of its viewBox, which AI-drawn pictures often have. With the name below, the picture is the box: the name sits in a frame of its own under it, a line from below lands on that frame, and the picture grows as a whole when a side needs room for more lines.

## [0.4.0] - 2026-10-13

### Added

- A file opens on the diagram or page and at the zoom it was left at; this is remembered per user, not in the project.
- The window opens where it was left, at the same size or maximized, and the side panels keep their widths. A window left on a monitor that is gone opens centred on the main one.
- File > Open recent with the last 10 files; missing ones are greyed out and can be removed.
- The last file is reopened on start unless a file is given on the command line; File > Preferences… turns this off.
- Open and Save as start in the folder used last.
- Custom looks: your own SVG pictures for boxes and doodle shapes, shown as the shape with the name inside or below, or as an icon above the name, always without stretching. They live in a per-user library (File > Looks…, Import .svg…), and a project keeps copies of the looks it uses and takes newer library versions when opened. Only a safe subset of SVG is accepted; anything else is refused by name, also in project files.
- AI tools list_looks and add_look, and a look for edit_box, doodle_add_shape and doodle_edit_shape.
- Where a relation enters a whitebox can be chosen by hand: drag its end to another side of the frame, or pick Enters at in the line's dialog (also for the AI, in edit_relation).

### Changed

- A relation from outside enters a whitebox on the frame side nearest the box that takes it, instead of always on the side it has one level up; the box faces that side.

### Fixed

- Partner names at the top and bottom of a whitebox frame overlapped; the ends there now keep at least the width of their names apart.

## [0.3.0] - 2026-10-12

### Added

- Doodles: File > New doodle starts a free sketch on named pages (tabs), without levels, whiteboxes or arc42 texts. Shape texts may span several lines and repeat.
- Doodle shapes: box, rounded box, circle, diamond, cylinder, document, cloud, person and note, each with its own colour; lines reach the outline of round and pointed shapes.
- Doodle lines can be solid, dashed or dotted.
- Doodle groups: labelled frames around a rectangle of cells, nestable, with their own colour; a line can end at a group.
- Doodle export: SVG and PNG per page, and the HTML document with all pages as tabs.
- AI tools for doodles (doodle_get, doodle_add_shape, doodle_add_line, doodle_group …), addressed by id; with a doodle open, the arc42 tools refuse.

## [0.2.0] - 2026-10-09

### Added

- Split relation: right-click a line, tick in a grid which boxes at both ends talk to each other, and every pair becomes its own relation with its own text and direction. Also an AI tool, split_relation.
- A frame end names its partner down to the depth of the boxes around it (B › b1, b2), so you see what talks to what inside other whiteboxes; long names go into the legend.
- Relations between the same two boxes are drawn as one bundled line with all their texts and directions; right-click lists each of them to edit or delete. A project setting turns it off.
- Dissolve whitebox: right-click a box with content to let its boxes take its place one level up, in their arrangement; relations follow, a line to several boxes becomes one relation per box, an unassigned one becomes an open end. Also an AI tool, dissolve.
- Group into new box: Ctrl+click boxes to select several, then right-click > Group into new box… puts them, in their arrangement, into the whitebox of a new box; relations follow. Also an AI tool, group.
- Move into: right-click a box > Move into…, then click a neighbour, to put the box with its content into that neighbour's whitebox; its relations follow. Also an AI tool, move_into.
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

## [0.1.0] - 2026-10-08

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
- Limits on name and text length, boxes and relations per diagram, grid size and exported image size, so a project (or an AI client) cannot make the app unresponsive.
- An app icon; on Linux the app adds itself to the application menu.
- Installation with Homebrew (Linux, macOS) and Scoop (Windows).
