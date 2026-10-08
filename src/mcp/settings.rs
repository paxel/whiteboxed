//! Port and access token of the AI interface. Stored per user, never in a project,
//! so the token stays out of git.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_PORT: u16 = 7342;
const FILE: &str = "ai.yaml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiSettings {
    pub port: u16,
    pub token: String,
    /// Where the endpoint listens; this computer only unless the user chose more.
    #[serde(default)]
    pub listen: super::listen::Listen,
}

/// The per-user directory the settings live in.
pub fn default_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("io.github", "paxel", "whiteboxed")
        .map(|d| d.data_dir().to_path_buf())
}

/// A random token of 244 bits.
pub fn new_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// The stored settings, or new ones (default port, fresh token) written on first use.
pub fn load_or_create(dir: &Path) -> io::Result<AiSettings> {
    let file = dir.join(FILE);
    if let Ok(text) = fs::read_to_string(&file)
        && let Ok(settings) = serde_yaml_ng::from_str::<AiSettings>(&text)
        && !settings.token.is_empty()
    {
        return Ok(settings);
    }
    let settings = AiSettings {
        port: DEFAULT_PORT,
        token: new_token(),
        listen: super::listen::Listen::Local,
    };
    save(dir, &settings)?;
    Ok(settings)
}

/// Writes the settings; on Unix only the user can read the file.
pub fn save(dir: &Path, settings: &AiSettings) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let text = serde_yaml_ng::to_string(settings).map_err(io::Error::other)?;
    let file = dir.join(FILE);
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut f = options.open(&file)?;
    f.write_all(text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn settings_are_created_once_and_kept() -> TestResult {
        let dir = tempfile::tempdir()?;
        let first = load_or_create(dir.path())?;
        assert_eq!(first.port, DEFAULT_PORT);
        assert_eq!(first.token.len(), 64);
        assert_eq!(load_or_create(dir.path())?, first);
        let changed = AiSettings {
            port: 9000,
            token: new_token(),
            listen: super::super::listen::Listen::Custom("10.0.0.5".into()),
        };
        save(dir.path(), &changed)?;
        assert_eq!(load_or_create(dir.path())?, changed);
        assert_ne!(changed.token, first.token);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.path().join(FILE))?.permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        Ok(())
    }
}
