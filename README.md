# whiteboxed

A WYSIWYG editor for [arc42](https://arc42.org) building-block views — draw.io for
architects who don't want to push pixels.

> Status: early development, not released yet.

You describe your system as boxes and relations; whiteboxed does the layout. The
context view is level 0, and every box opens into its whitebox one level deeper, as
deep as you like. Relations that reach a box show up inside its whitebox, so the
levels cannot drift apart.

![The context view of a web shop](docs/screenshots/context.png)

## What it does

- **Click, don't draw.** Click the side of a box where its partner should be: connect a
  new box, an existing one, or leave the relation open as a stub. Boxes grow with
  their relations; lines route themselves around boxes.
- **arc42 levels.** Context view with persons and external systems, then whiteboxes
  without a depth limit, a breadcrumb and a structure tree.
- **Consistent across levels.** Interfaces from the parent level enter a whitebox
  through its frame and are marked until a box inside takes them. Stubs bubble up to
  the context view.
- **Architecture box types.** Component, database, queue/topic, cache, file storage,
  UI, person, external system.
- **Tags** colour boxes project-wide.
- **Git-friendly files.** One YAML file per project with the model and grid cells, no
  pixel coordinates. Unsaved work is recovered after a crash.
- **arc42 texts.** A responsibility per box and a motivation per diagram, typed in a
  details panel; undo and redo from the toolbar arrows or the keyboard.
- **Export** every diagram as SVG and PNG, together with arc42 text files
  (AsciiDoc or Markdown) holding the partner, building-block and interface tables,
  and an index that ties them together.

![The whitebox of the web shop, with two interfaces not assigned yet](docs/screenshots/whitebox.png)

## Documentation

- [Workflows](docs/workflows.md): step-by-step recipes, from the first box to the
  exported arc42 images.
- [Guide](docs/guide.md): levels, box types, relations, layout rules, keys and the
  file format.

## Install

Once the first version is released:

| OS      | Channel                                                                                        |
|---------|------------------------------------------------------------------------------------------------|
| Linux   | `brew install paxel/tap/whiteboxed`, or the plain tarball (x86_64)                             |
| macOS   | `brew install paxel/tap/whiteboxed`                                                            |
| Windows | `scoop bucket add paxel https://github.com/paxel/scoop-bucket` then `scoop install whiteboxed` |
| Any     | `cargo install whiteboxed` (builds from source)                                                |

On Linux, `brew install` puts whiteboxed into the application menu, and the app keeps
its menu entry and icon up to date on every start, however it was installed. Run
`whiteboxed-install-icon` if the entry is missing, and `whiteboxed-install-icon
--uninstall` before `brew uninstall` to remove it. The macOS build is **unsigned**;
start it from a terminal with `whiteboxed`.

## Running

```sh
cargo run --release                      # empty project
cargo run --release -- architecture.yaml # open a project
```

Windows, Linux and macOS. Built with Rust and egui/eframe.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test                               # model, layout, editor and headless UI tests
cargo test --features render-tests       # also render the window (needs a GPU or lavapipe)
```

The render tests draw the real window through wgpu and check pixels. CI runs them on
Linux with software Vulkan (`mesa-vulkan-drivers`). The ignored `doc_screenshot_*`
tests regenerate the images in `docs/screenshots`:

```sh
cargo test --features render-tests --test render doc_screenshot -- --ignored
```

## License

[MIT](LICENSE)
