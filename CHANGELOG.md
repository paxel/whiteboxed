# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.6.1]

### Added

- get_diagram lists labels drawn over a box (labels_over_boxes), labels another line runs through (labels_over_lines) and lines running along each other (overlaps), and the score counts the overlaps.

### Changed

- Mousehole arches always point up, whichever way the line runs, so they read as holes everywhere.
- In a whitebox with cross-cutting bands, a relation entering it no longer takes the bottom frame side on its own, so its line does not run through the bands; a side set to bottom by hand goes under them through a pair of mouseholes at the band edges.
- The MCP server instructions tell the AI to leave mouseholes on automatic and what the automatic rule does, so it stops forcing lines under.
- Routes, lanes, the order of the lines on each side, mouseholes and the frame points of fanned-out relations are chosen together by one measure over the whole picture, so a fan-out over a whole row no longer turns into a tangle. A line on automatic mouseholes goes under only where that saves about three crossings; a branch of a fanned-out relation gets a frame point of its own, in line with its box, before that.
- A line's label sits where it has room: on a stretch of the line clear of boxes, of the title and the bands, of other lines, of the pictures at line ends, of the labels at mouseholes and of other labels, on either side of the line; with no such place it shows its legend number and the text goes to the legend.

### Fixed

- Turning AI access on no longer opens the AI access dialog every time; it opens only when the endpoint cannot start. The command to connect stays under AI > Connection….
- get_diagram no longer lists the partner names at frame ends as cut-off labels or as labels another line runs through; only the lines' own labels count.
- Two lines of different relations no longer run along each other between a box and the lanes; they cross cleanly instead, and a stretch two lines still share counts for readability and is named in the status bar.
- A relation that fans out inside a whitebox names its partner once at the frame instead of once per branch.
- The branches of a fanned-out relation that go through mouseholes get a frame point, a pair of holes and a number each, listed in the legend with the box they reach.
- A relation entering a whitebox gets its frame point next to the box that takes it; it is no longer dragged next to the other frame points on that side.
- The straight branch of a fanned-out relation no longer jogs sideways by a few pixels just inside the frame.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
