// No console window next to the app on Windows.
#![windows_subsystem = "windows"]
#![forbid(unsafe_code)]
// Shipped code may not index, slice or do arithmetic that can panic either; the
// unwrap/expect/panic part of the policy lives in Cargo.toml. Unit tests are
// exempt via `cfg(test)`; integration tests are separate crates.
#![cfg_attr(
    not(test),
    deny(
        clippy::unreachable,
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::arithmetic_side_effects
    )
)]

use std::path::PathBuf;

fn main() -> eframe::Result {
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    whiteboxed::ui::run(path)
}
