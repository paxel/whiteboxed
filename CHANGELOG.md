# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0] - Unreleased

### Added

- Boxes without a relation can be added anywhere: right-click empty space > Add box here puts the box in that cell, and + Box in the menu bar puts it in the next free cell.
- Short labels for relations; long texts without one appear as [1], [2] … with the full text in a legend below the diagram (in the app, the export and as a tooltip). When texts get shortened is set in Project settings.
- Line styles for relations: square, slightly rounded, rounded or curved; set for the project under File > Project settings and per relation, with your choice as the default for new projects.

### Changed

- Lines between boxes that face each other are straight, and relations from the level above enter a whitebox opposite the box that takes them, instead of being spread evenly along the side.
- Lines on one side of a box keep 40 px apart instead of 26 px, so a box visibly grows from its second relation on a side; the minimum box size stays.
- Several new boxes on one side of a box fill a block that stays as square as possible (2×2, 3×2, 3×3 …) instead of one long row.
- Moving a box turns its lines to the sides that face their partners (or the frame side a line from outside enters at). A side can also be chosen by hand in the relation dialog and through the AI's edit_relation, until the next move.
- Lines avoid crossing each other where a short detour or a different lane order allows it; where lines still cross, the horizontal one jumps over the other in a small arc, and labels move aside so they do not hide it.
- The question about unsaved changes before quitting, starting a new project or opening another one is now a large dialog over the dimmed window, with buttons that say what they do; Enter saves, Esc keeps editing.

### Fixed

- Relation texts next to a whitebox frame were cut off at the edge of the picture; labels now stay inside the frame, wrapped if needed.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
