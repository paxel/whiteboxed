# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.6.0]

### Added

- Line looks: a named kind of line with its own width, dash and pictures at the ends (the head where the arrow points, the tail elsewhere), kept in the library next to the looks and worn by a line on every level. Pick one under Look in the line's Edit… or Connect dialog; the legend shows a sample of every line look on the diagram with its name, and the kinds of line in use are a readability measure with limits of their own.
- Pictures: SVGs in the library for the ends of lines and for mouseholes, imported in File > Looks… or drawn by the AI (add_picture); their currentColor parts take the line's colour.
- Portals: two pictures that replace the mousehole arches, one where a line goes in and one where it comes out, chosen under Mouseholes in File > Project settings… or with set_portals.
- Inside a whitebox, the frame is the box's own look stretched as a nine-slice; with the name below, the title sits in a frame under it, and an icon look sits next to the title.
- The AI adds line looks (add_line_look), puts them on lines (connect_new, connect_existing, edit_relation and the doodle line tools) and sees them in list_looks.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
