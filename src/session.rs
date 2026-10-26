//! What the app remembers between runs, per user: recently used files, the folder
//! last used in Open and Save, the window and panels, and per file the diagram and
//! zoom last shown. Never stored in a project.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::BlockId;

const FILE: &str = "state.yaml";
/// Most files in File > Open recent.
pub const MAX_RECENT: usize = 10;
/// Most files whose view is remembered.
pub const MAX_VIEWS: usize = 100;

/// Where the window was. Some systems (Wayland) do not tell the position.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowPlace {
    #[serde(default)]
    pub x: Option<f32>,
    #[serde(default)]
    pub y: Option<f32>,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub maximized: bool,
}

/// The diagram (or doodle page) and zoom last shown of a file.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FileView {
    #[serde(default)]
    pub diagram: Option<BlockId>,
    pub zoom: f32,
    /// When it was last shown, to forget the oldest first.
    #[serde(default)]
    pub seen: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// Most recent first.
    #[serde(default)]
    pub recent: Vec<PathBuf>,
    /// The folder of the file last opened or saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowPlace>,
    /// Widths of the side panels, by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub panels: BTreeMap<String, f32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub views: BTreeMap<PathBuf, FileView>,
}

/// The absolute form of a path, so the same file is remembered once.
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

impl Session {
    /// The stored session, or an empty one when there is none (or it is broken).
    pub fn load(dir: &Path) -> Self {
        fs::read_to_string(dir.join(FILE))
            .ok()
            .and_then(|t| serde_yaml_ng::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let text = serde_yaml_ng::to_string(self).map_err(io::Error::other)?;
        fs::write(dir.join(FILE), text)
    }

    /// Puts a file opened or saved at the top of the recent files, and remembers its
    /// folder.
    pub fn used(&mut self, path: &Path) {
        let path = absolute(path);
        self.recent.retain(|p| *p != path);
        self.recent.insert(0, path.clone());
        self.recent.truncate(MAX_RECENT);
        self.folder = path.parent().map(Path::to_path_buf);
    }

    pub fn forget(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
    }

    /// The file to reopen on start: the most recent one that still exists.
    pub fn last_file(&self) -> Option<&Path> {
        self.recent
            .first()
            .filter(|p| p.is_file())
            .map(PathBuf::as_path)
    }

    pub fn view_of(&self, path: &Path) -> Option<FileView> {
        self.views.get(&absolute(path)).copied()
    }

    /// Remembers what a file showed; the oldest are forgotten beyond [`MAX_VIEWS`].
    pub fn remember_view(&mut self, path: &Path, diagram: Option<BlockId>, zoom: f32) {
        let seen = self
            .views
            .values()
            .map(|v| v.seen)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.views.insert(
            absolute(path),
            FileView {
                diagram,
                zoom,
                seen,
            },
        );
        while self.views.len() > MAX_VIEWS {
            let oldest = self
                .views
                .iter()
                .min_by_key(|(_, v)| v.seen)
                .map(|(p, _)| p.clone());
            match oldest {
                Some(p) => self.views.remove(&p),
                None => break,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_files_are_unique_newest_first_and_capped() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut s = Session::default();
        let file = |n: usize| dir.path().join(format!("p{n}.yaml"));
        for n in 0..12 {
            fs::write(file(n), "format: 1")?;
            s.used(&file(n));
        }
        s.used(&file(5));
        assert_eq!(s.recent.len(), MAX_RECENT);
        assert_eq!(s.recent[0], file(5));
        assert_eq!(s.recent.iter().filter(|p| **p == file(5)).count(), 1);
        assert_eq!(s.folder.as_deref(), Some(dir.path()));
        assert_eq!(s.last_file(), Some(file(5).as_path()));
        fs::remove_file(file(5))?;
        assert_eq!(s.last_file(), None);
        s.forget(&file(5));
        assert_eq!(s.recent[0], file(11));

        s.save(dir.path())?;
        assert_eq!(Session::load(dir.path()), s);
        fs::write(dir.path().join(FILE), "recent: 7")?;
        assert_eq!(Session::load(dir.path()), Session::default());
        Ok(())
    }

    #[test]
    fn views_are_remembered_per_file_and_the_oldest_forgotten() {
        let mut s = Session::default();
        for n in 0..=MAX_VIEWS {
            s.remember_view(Path::new(&format!("/p/{n}.yaml")), None, 1.0);
        }
        assert_eq!(s.views.len(), MAX_VIEWS);
        assert_eq!(s.view_of(Path::new("/p/0.yaml")), None);
        s.remember_view(Path::new("/p/1.yaml"), Some(BlockId(3)), 2.0);
        let v = s.view_of(Path::new("/p/1.yaml"));
        assert_eq!(
            v.map(|v| (v.diagram, v.zoom)),
            Some((Some(BlockId(3)), 2.0))
        );
    }
}
