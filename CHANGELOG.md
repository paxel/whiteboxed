# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.4.0] - Unreleased

### Added

- The window opens where it was left, at the same size or maximized, and the side panels keep their widths.
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

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
