//! Registers the app in the desktop menu on Linux.
//!
//! A brew or tarball install is just a binary unless something writes the `.desktop`
//! entry and the icon, and on Wayland the window shows a generic icon until an entry
//! named after its app id exists. The app closes that gap itself: on every start it
//! writes or refreshes the icon and the launcher in the user's XDG data directory, the
//! same files `packaging/linux/install-icon.sh` installs.
//!
//! Only a launcher carrying our marker key is ever overwritten (a hand-written or
//! distribution launcher is left alone), nothing is written when the content is
//! current, and failures are ignored: a missing menu entry is no reason to fail.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::ui::APP_ID;

/// The app icon, byte-identical to `assets/icon_256.png`.
pub const ICON: &[u8] = include_bytes!("../assets/icon_256.png");

/// Marker key claiming a desktop file as ours to update.
const MARKER: &str = "X-Whiteboxed-Managed=true";

/// Registers icon and launcher for the running binary. Call from a background thread.
pub fn register() {
    let (Some(data_home), Some(exec)) = (data_home(), exec_path()) else {
        return;
    };
    if let Ok(true) = register_at(&data_home, &exec) {
        refresh_caches(&data_home);
    }
}

/// `$XDG_DATA_HOME`, else `$HOME/.local/share`.
fn data_home() -> Option<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_DATA_HOME")
        && !x.is_empty()
    {
        return Some(PathBuf::from(x));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
}

/// The running executable, via Homebrew's stable `bin` link when it runs from a
/// Cellar: the versioned Cellar path disappears with the next `brew upgrade`.
fn exec_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(stable_brew_path(&exe).unwrap_or(exe))
}

/// `<prefix>/bin/<name>` for a binary inside `<prefix>/Cellar/…`, if that link
/// really points at it.
fn stable_brew_path(exe: &Path) -> Option<PathBuf> {
    let prefix = exe
        .ancestors()
        .find(|a| a.file_name() == Some("Cellar".as_ref()))?
        .parent()?;
    let candidate = prefix.join("bin").join(exe.file_name()?);
    (fs::canonicalize(&candidate).ok()? == fs::canonicalize(exe).ok()?).then_some(candidate)
}

/// Writes icon and launcher under `data_home`. Returns whether anything changed.
fn register_at(data_home: &Path, exec: &Path) -> io::Result<bool> {
    let mut wrote = false;
    let icon_path = data_home.join("icons/hicolor/256x256/apps/whiteboxed.png");
    if fs::read(&icon_path).ok().as_deref() != Some(ICON) {
        write_atomic(&icon_path, ICON)?;
        wrote = true;
    }
    let desktop_path = data_home.join(format!("applications/{APP_ID}.desktop"));
    let desired = desktop_entry(exec);
    match fs::read_to_string(&desktop_path) {
        Ok(current) if !current.contains(MARKER) => {}
        Ok(current) if current == desired => {}
        _ => {
            write_atomic(&desktop_path, desired.as_bytes())?;
            wrote = true;
        }
    }
    Ok(wrote)
}

/// `packaging/linux/io.github.paxel.whiteboxed.desktop` with Exec pointing at `exec`.
fn desktop_entry(exec: &Path) -> String {
    let exec_str = exec.to_string_lossy();
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=whiteboxed\n\
         GenericName=Architecture Diagram Editor\n\
         Comment=Draw arc42 building-block views\n\
         TryExec={exec_str}\n\
         Exec={} %f\n\
         Icon=whiteboxed\n\
         Terminal=false\n\
         Categories=Development;Graphics;\n\
         StartupWMClass={APP_ID}\n\
         {MARKER}\n",
        quote_exec(&exec_str),
    )
}

/// Quotes a path for `Exec=` as the desktop entry spec asks; plain paths stay bare.
fn quote_exec(path: &str) -> String {
    let plain = path
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+' | ':' | '@'));
    if plain {
        return path.to_owned();
    }
    let mut out = String::with_capacity(path.len() + 2);
    out.push('"');
    for c in path.chars() {
        if matches!(c, '"' | '\\' | '$' | '`') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Writes through a temporary sibling and a rename, so no half-written file remains.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("path has no parent"))?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".whiteboxed-{}.tmp", std::process::id()));
    let mut file = fs::File::create(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&tmp, path)
}

/// Pokes the desktop caches so the entry shows up without a new login. Every tool
/// is optional.
fn refresh_caches(data_home: &Path) {
    let run = |cmd: &str, args: &[&std::ffi::OsStr]| {
        let _ = std::process::Command::new(cmd)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    };
    let icons = data_home.join("icons/hicolor");
    let apps = data_home.join("applications");
    run(
        "gtk-update-icon-cache",
        &["-f".as_ref(), "-t".as_ref(), icons.as_os_str()],
    );
    run("update-desktop-database", &[apps.as_os_str()]);
    run("kbuildsycoca6", &[]);
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn desktop(dir: &Path) -> io::Result<String> {
        fs::read_to_string(dir.join(format!("applications/{APP_ID}.desktop")))
    }

    #[test]
    fn writes_icon_and_marked_launcher_once() -> TestResult {
        let dir = tempfile::tempdir()?;
        let exec = Path::new("/opt/brew/bin/whiteboxed");
        assert!(register_at(dir.path(), exec)?);
        let icon = fs::read(dir.path().join("icons/hicolor/256x256/apps/whiteboxed.png"))?;
        assert_eq!(icon, ICON);
        let entry = desktop(dir.path())?;
        assert!(entry.contains("Exec=/opt/brew/bin/whiteboxed %f"));
        assert!(entry.contains("StartupWMClass=io.github.paxel.whiteboxed"));
        assert!(entry.contains(MARKER));
        assert!(
            !register_at(dir.path(), exec)?,
            "unchanged content is not rewritten"
        );
        Ok(())
    }

    #[test]
    fn follows_a_moved_binary_but_leaves_foreign_launchers() -> TestResult {
        let dir = tempfile::tempdir()?;
        register_at(dir.path(), Path::new("/old/whiteboxed"))?;
        assert!(register_at(dir.path(), Path::new("/new/whiteboxed"))?);
        assert!(desktop(dir.path())?.contains("Exec=/new/whiteboxed %f"));

        let foreign = "[Desktop Entry]\nName=mine\nExec=/home/me/wb\n";
        fs::write(
            dir.path().join(format!("applications/{APP_ID}.desktop")),
            foreign,
        )?;
        register_at(dir.path(), Path::new("/newer/whiteboxed"))?;
        assert_eq!(desktop(dir.path())?, foreign);
        Ok(())
    }

    #[test]
    fn the_packaged_entry_matches_the_generated_one() -> TestResult {
        let packaged = include_str!("../packaging/linux/io.github.paxel.whiteboxed.desktop");
        let keys = |text: &str| -> Vec<String> {
            text.lines()
                .filter(|l| {
                    !l.starts_with('#') && !l.starts_with("Exec") && !l.starts_with("TryExec")
                })
                .map(str::to_owned)
                .collect()
        };
        assert_eq!(
            keys(packaged),
            keys(&desktop_entry(Path::new("whiteboxed")))
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn a_cellar_binary_registers_the_stable_bin_link() -> TestResult {
        let dir = tempfile::tempdir()?;
        let cellar = dir.path().join("Cellar/whiteboxed/0.1.0/bin");
        fs::create_dir_all(&cellar)?;
        let exe = cellar.join("whiteboxed");
        fs::write(&exe, b"binary")?;
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin)?;
        std::os::unix::fs::symlink(
            "../Cellar/whiteboxed/0.1.0/bin/whiteboxed",
            bin.join("whiteboxed"),
        )?;
        assert_eq!(stable_brew_path(&exe), Some(bin.join("whiteboxed")));
        assert_eq!(
            stable_brew_path(Path::new("/usr/local/bin/whiteboxed")),
            None
        );
        Ok(())
    }

    #[test]
    fn exec_paths_with_specials_are_quoted() {
        assert_eq!(quote_exec("/opt/bin/whiteboxed"), "/opt/bin/whiteboxed");
        assert_eq!(quote_exec("/home/me/My Apps/wb"), "\"/home/me/My Apps/wb\"");
        assert_eq!(quote_exec("/tmp/$HOME"), "\"/tmp/\\$HOME\"");
    }
}
