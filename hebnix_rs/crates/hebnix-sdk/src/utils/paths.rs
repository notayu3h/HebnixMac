//! Shared Hebnix runtime and data paths.

use std::path::PathBuf;

/// Windows' Documents known folder includes OneDrive and other folder redirects.
/// Use the traditional location only when the known folder is unavailable.
pub fn documents_dir() -> Option<PathBuf> {
    // the emulated "My Documents" inside the game's Wine/Proton/Obelisk
    // prefix, where Rocket League actually keeps its configs, saves and logs
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if let Some(dir) = crate::process::rocket_league_documents_dir() {
        return Some(dir);
    }
    dirs::document_dir()
        .filter(|path| path.is_absolute())
        .or_else(|| dirs::home_dir().map(|home| home.join("Documents")))
        .filter(|path| path.is_absolute())
}

/// The root for all Hebnix-owned files.
///
/// On Windows this is `%AppData%\Hebnix`. `HEBNIX_BASE_DIR` remains available
/// for isolated development and test runs.
#[cfg(target_os = "windows")]
const BASE_DIR_NAME: &str = "Hebnix";
#[cfg(target_os = "linux")]
const BASE_DIR_NAME: &str = "hebnix";
#[cfg(target_os = "macos")]
const BASE_DIR_NAME: &str = "HebnixMac";
/// The folders this macOS build used before each rename, newest first.
#[cfg(target_os = "macos")]
const OLD_BASE_DIR_NAMES: &[&str] = &["BetterHebnix", "Hebnix"];

/// `%AppData%\Hebnix` on Windows, `~/.config/hebnix` (XDG) on Linux. Must
/// resolve to the same directory as the app crate's `config::base_dir()`.
fn default_base_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let root = dirs::data_dir();
    #[cfg(target_os = "linux")]
    let root = dirs::config_dir();
    // `~/Library/Application Support/HebnixMac`, taken over from the old
    // folder the first time this build runs so settings survive the rename.
    #[cfg(target_os = "macos")]
    let root = {
        let root = dirs::data_dir()?;
        migrate_macos_base_dir(&root);
        Some(root)
    };
    root.map(|dir| dir.join(BASE_DIR_NAME))
}

/// Move the pre-rename `~/Library/Application Support/Hebnix` to the new name,
/// once, so config, themes and plugins carry over.
#[cfg(target_os = "macos")]
fn migrate_macos_base_dir(root: &std::path::Path) {
    let dir = root.join(BASE_DIR_NAME);
    if dir.exists() {
        return;
    }
    for name in OLD_BASE_DIR_NAMES {
        let old = root.join(name);
        if old.is_dir() && std::fs::rename(&old, &dir).is_ok() {
            return;
        }
    }
}

pub fn base_dir() -> PathBuf {
    let dir = std::env::var_os("HEBNIX_BASE_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(portable_base_dir)
        .or_else(default_base_dir)
        .unwrap_or_else(|| std::env::temp_dir().join(BASE_DIR_NAME));

    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!(
            "failed to create Hebnix data directory {}: {error}",
            dir.display()
        );
    }
    dir
}

/// portable.txt next to the exe keeps Hebnix data in the exe folder, unless
/// it has a `portable=false` line (then it only holds options).
pub fn portable_base_dir() -> Option<PathBuf> {
    let path = portable_txt()?;
    if !path.exists() || portable_value("portable").is_some_and(|value| !truthy(&value)) {
        return None;
    }
    path.parent().map(PathBuf::from)
}

fn portable_txt() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("portable.txt"))
}

/// the value of `name` in portable.txt: `name=value`, or "true" for a bare
/// `name` line (case-insensitive). Other lines are ignored, so the file can
/// keep its note.
fn portable_value(name: &str) -> Option<String> {
    let contents = std::fs::read_to_string(portable_txt()?).ok()?;
    contents.lines().find_map(|line| {
        let line = line.trim().trim_start_matches('\u{feff}');
        match line.split_once('=') {
            Some((key, value)) if key.trim().eq_ignore_ascii_case(name) => {
                Some(value.trim().to_string())
            }
            None if line.eq_ignore_ascii_case(name) => Some("true".to_string()),
            _ => None,
        }
    })
}

fn truthy(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "true" | "1" | "yes")
}

/// true if portable.txt next to the exe has a line `name` or `name=true`.
pub fn portable_option(name: &str) -> bool {
    portable_value(name).is_some_and(|value| truthy(&value))
}
