//! Crash recovery: unsaved work is written to a recovery file next to nothing the
//! user owns, so the project file only changes on an explicit save.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::model::Project;
use crate::persist::{self, PersistError};

/// The per-user directory recovery files live in.
pub fn default_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("io.github", "paxel", "whiteboxed")
        .map(|d| d.data_dir().join("recovery"))
}

/// The recovery file for a project file, or for an untitled project.
pub fn path_for(dir: &Path, project: Option<&Path>) -> PathBuf {
    match project {
        None => dir.join("untitled.yaml"),
        Some(p) => {
            let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
            let stem = abs
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "project".into());
            dir.join(format!("{stem}-{:016x}.yaml", fnv(&abs.to_string_lossy())))
        }
    }
}

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

pub fn write(file: &Path, project: &Project) -> Result<(), PersistError> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    persist::save(file, project)
}

pub fn remove(file: &Path) {
    // A missing recovery file is the normal case.
    let _ = fs::remove_file(file);
}

/// A recovered project if its file exists and is not older than the project file.
pub fn find(file: &Path, project: Option<&Path>) -> Option<Project> {
    let recovered = fs::metadata(file).and_then(|m| m.modified()).ok()?;
    if let Some(p) = project {
        let saved = fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        // Saving removes the recovery file, so one that is left is unsaved work;
        // only a project file changed later (e.g. by a git pull) wins over it.
        if saved > recovered {
            return None;
        }
    }
    persist::load(file).ok()
}
