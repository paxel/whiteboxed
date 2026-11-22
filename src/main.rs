// No console window next to the app on Windows.
#![windows_subsystem = "windows"]
#![forbid(unsafe_code)]

use std::path::PathBuf;

fn main() -> eframe::Result {
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    whiteboxed::ui::run(path)
}
