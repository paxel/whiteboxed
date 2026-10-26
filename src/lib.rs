//! whiteboxed: a WYSIWYG editor for arc42 building-block views.

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

pub mod api;
pub mod doc;
pub mod editor;
pub mod export;
pub mod geom;
pub mod hit;
pub mod html;
pub mod launcher;
pub mod layout;
pub mod look;
pub mod mcp;
pub mod model;
pub mod persist;
pub mod prefs;
pub mod recovery;
pub mod scene;
pub mod score;
pub mod session;
pub mod text;
pub mod ui;
pub mod view;
