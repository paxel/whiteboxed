# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.5.0]

### Added

- Doodle pages get a readability colour, shown as a dot on the page tab that explains on hover what helps; the limits are in File > Project settings… and the AI sees it in doodle_get.
- Lines can have a colour of their own, picked in the line's Edit… dialog or set by the AI; a relation keeps it on every level.

### Changed

- Lines leave a box in the middle of its side, join a diagonal partner with one bend through free cells, enter a box in the order they arrive so none crosses another on its way in, and keep 40 px apart in gaps that widen for them; a crossing is worth a detour of about one cell, no more.
- A relation from outside enters a whitebox in line with the box that takes it; the partner names along the frame's top and bottom go to a further row instead of pushing the ends apart.
- An interface no box handles yet is "not attached" in the status bar and the export, whose column is now "Attached to".
- The plain doodle shapes are called rectangle and rounded rectangle.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
