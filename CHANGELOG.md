# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.5.0]

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

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
