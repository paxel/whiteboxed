//! Personal preferences: starting values for new projects and how the app starts,
//! stored per user.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::LineStyle;

const FILE: &str = "prefs.yaml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub line_style: LineStyle,
    /// Open the last file on start when none is given on the command line.
    #[serde(default = "yes")]
    pub reopen_last: bool,
}

fn yes() -> bool {
    true
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            line_style: LineStyle::default(),
            reopen_last: true,
        }
    }
}

/// The stored preferences, or the defaults when there are none (or they are broken).
pub fn load(dir: &Path) -> Prefs {
    fs::read_to_string(dir.join(FILE))
        .ok()
        .and_then(|t| serde_yaml_ng::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, prefs: &Prefs) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let text = serde_yaml_ng::to_string(prefs).map_err(io::Error::other)?;
    fs::write(dir.join(FILE), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_round_trip_and_default() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        assert_eq!(load(dir.path()), Prefs::default());
        let p = Prefs {
            line_style: LineStyle::Curved,
            reopen_last: false,
        };
        save(dir.path(), &p)?;
        assert_eq!(load(dir.path()), p);
        fs::write(dir.path().join(FILE), "line_style: nonsense")?;
        assert_eq!(load(dir.path()), Prefs::default());
        Ok(())
    }
}
