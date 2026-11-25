//! Where Paprika keeps its files.
//!
//! Default: the OS data dir (`~/.local/share/paprika` on Linux,
//! `%APPDATA%\paprika` on Windows). Put a file called `portable.txt` next to
//! the executable to keep everything in a `paprika-data` folder beside it, or
//! set `PAPRIKA_HOME` to choose any folder.

use std::path::PathBuf;
use std::sync::OnceLock;

static ROOT: OnceLock<PathBuf> = OnceLock::new();

pub fn root() -> &'static PathBuf {
    ROOT.get_or_init(|| {
        if let Ok(p) = std::env::var("PAPRIKA_HOME") {
            if !p.is_empty() {
                return PathBuf::from(p);
            }
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                if dir.join("portable.txt").exists() {
                    return dir.join("paprika-data");
                }
            }
        }
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("paprika")
    })
}

pub fn instances() -> PathBuf {
    root().join("instances")
}
pub fn libraries() -> PathBuf {
    root().join("libraries")
}
pub fn assets() -> PathBuf {
    root().join("assets")
}
pub fn versions() -> PathBuf {
    root().join("versions")
}
pub fn java() -> PathBuf {
    root().join("java")
}
pub fn cache() -> PathBuf {
    root().join("cache")
}
