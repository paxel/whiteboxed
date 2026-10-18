# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.6.1]

### Added

- get_diagram lists labels drawn over a box (labels_over_boxes), labels another line runs through (labels_over_lines) and lines running along each other (overlaps), and the score counts the overlaps.

### Changed

- A line's label sits where it has room: on a stretch of the line clear of boxes and of other lines, on either side of the line; with no such place it shows its legend number and the text goes to the legend.

### Fixed

- Two lines of different relations no longer run along each other between a box and the lanes; they cross cleanly instead, and a stretch two lines still share counts for readability and is named in the status bar.
- A relation that fans out inside a whitebox names its partner once at the frame instead of once per branch.
- The branches of a fanned-out relation that go through mouseholes get a number each, listed in the legend with the box they reach; where they go under together at the frame, one hole carries all the numbers.
- A relation entering a whitebox gets its frame point next to the box that takes it; it is no longer dragged next to the other frame points on that side.
- The straight branch of a fanned-out relation no longer jogs sideways by a few pixels just inside the frame.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
