//! RL process detection on macOS.
//!
//! There is no native Rocket League here: the game runs as a Windows build
//! inside Obelisk's Voile runtime, so `sysinfo` reports the runtime's own
//! processes and the game folder lives under Obelisk's library instead of a
//! Steam/Epic install. Save data therefore follows the install's Wine prefix,
//! not `~/Documents`.

use std::path::{Path, PathBuf};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

use crate::utils::constants::{SAVE_PATH_EPIC, SAVE_PATH_STEAM};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RlPlatform {
    Steam,
    Epic,
    Unknown,
}

impl RlPlatform {
    pub fn as_str(&self) -> &'static str {
        match self {
            RlPlatform::Steam => "steam",
            RlPlatform::Epic => "epic",
            RlPlatform::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RlProcessInfo {
    pub pid: u32,
    pub exe_path: PathBuf,
    pub root_dir: PathBuf,
    pub platform: RlPlatform,
    pub save_data_path: PathBuf,
}

/// walk up from exe_path to the game root (the folder with Binaries/ and
/// Engine/ or TAGame/)
fn find_root_dir(exe_path: &Path) -> Option<PathBuf> {
    let mut current = exe_path.parent()?.to_path_buf();
    for _ in 0..10 {
        let bins = current.join("Binaries");
        let engine = current.join("Engine");
        let tagame = current.join("TAGame");
        if bins.is_dir() && (engine.is_dir() || tagame.is_dir()) {
            return Some(current);
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent.to_path_buf(),
            _ => break,
        }
    }
    None
}

/// Steam vs Epic from the install's own markers, then from Obelisk's layout
/// (`Games/Epic/epic-<id>-Windows`), then from the path text.
pub fn detect_platform(root_dir: &Path) -> RlPlatform {
    if root_dir.join("appinfo.vdf").exists() {
        return RlPlatform::Steam;
    }
    if root_dir.join(".egstore").is_dir() {
        return RlPlatform::Epic;
    }
    for ancestor in root_dir.ancestors() {
        let Some(name) = ancestor.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        match name.to_ascii_lowercase().as_str() {
            "epic" => return RlPlatform::Epic,
            "steam" => return RlPlatform::Steam,
            _ => {}
        }
    }
    let root_str = root_dir.to_string_lossy().to_lowercase();
    if root_str.contains("steamapps") || root_str.contains("steamlibrary") {
        return RlPlatform::Steam;
    }
    if root_str.contains("epic") {
        return RlPlatform::Epic;
    }
    RlPlatform::Unknown
}

fn documents_dir() -> PathBuf {
    crate::utils::paths::documents_dir().unwrap_or_else(|| PathBuf::from("Documents"))
}

/// `Documents` inside a Wine prefix, taken from its single user directory.
fn documents_in_prefix(prefix: &Path) -> Option<PathBuf> {
    let users = prefix.join("drive_c/users");
    let user = std::fs::read_dir(users)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.is_dir())?;
    Some(user.join("Documents"))
}

/// The install's own prefix, and so the Documents the game actually writes to.
fn install_documents_dir() -> Option<PathBuf> {
    let install = hebnix_macos::obelisk::find_rocket_league()?;
    documents_in_prefix(install.prefix.as_deref()?)
}

/// `DBE_Production/` under a Documents folder: the platform's own when known,
/// else whichever of Steam's and Epic's has the newest `.save`
pub fn save_data_dir_in(docs: &Path, platform: RlPlatform) -> PathBuf {
    match platform {
        RlPlatform::Steam => docs.join(SAVE_PATH_STEAM),
        RlPlatform::Epic => docs.join(SAVE_PATH_EPIC),
        RlPlatform::Unknown => [SAVE_PATH_STEAM, SAVE_PATH_EPIC]
            .into_iter()
            .map(|rel| docs.join(rel))
            .filter_map(|dir| Some((newest_save_time(&dir)?, dir)))
            .max_by_key(|(modified, _)| *modified)
            .map(|(_, dir)| dir)
            .unwrap_or_else(|| docs.join(SAVE_PATH_STEAM)),
    }
}

fn newest_save_time(dir: &Path) -> Option<std::time::SystemTime> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("save"))
        })
        .filter_map(|entry| entry.metadata().ok()?.modified().ok())
        .max()
}

/// full path to `DBE_Production/` for the platform
pub fn get_save_data_path(platform: RlPlatform) -> PathBuf {
    if let Some(docs) = install_documents_dir() {
        return save_data_dir_in(&docs, platform);
    }
    save_data_dir_in(&documents_dir(), platform)
}

/// Name-only check for a running Rocket League. Under Obelisk the guest process
/// shows up as `RocketLeague.exe`, so a name match is enough.
pub fn is_rocket_league_running() -> bool {
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    sys.processes().values().any(|process| {
        process
            .name()
            .to_string_lossy()
            .to_lowercase()
            .contains("rocketleague")
    })
}

/// Fallible name-only enumeration for operations that require confirmed closure.
/// An enumeration failure is unknown, never evidence that the game is closed.
pub fn rocket_league_running_checked() -> Result<bool, String> {
    let output = std::process::Command::new("/bin/ps")
        .args(["-Ao", "comm="])
        .output()
        .map_err(|error| format!("could not list processes: {error}"))?;
    let listing = String::from_utf8_lossy(&output.stdout);
    Ok(listing
        .lines()
        .any(|line| line.to_ascii_lowercase().contains("rocketleague")))
}

/// find the running RL process + its info. None if not running.
pub fn find_rocket_league() -> Option<RlProcessInfo> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::Always),
    );

    for (pid, process) in sys.processes() {
        let name = process.name().to_string_lossy().to_lowercase();
        if !name.contains("rocketleague") {
            continue;
        }
        let Some(exe) = process.exe() else {
            continue;
        };
        if !exe.exists() {
            continue;
        }
        let Some(root) = find_root_dir(exe) else {
            continue;
        };
        let platform = detect_platform(&root);
        // The game sits outside its prefix here (Obelisk keeps them in separate
        // folders), so the prefix is looked up from the install itself.
        let save_path = hebnix_macos::obelisk::install_at(&root)
            .and_then(|install| install.prefix)
            .and_then(|prefix| documents_in_prefix(&prefix))
            .map(|docs| save_data_dir_in(&docs, platform))
            .unwrap_or_else(|| get_save_data_path(platform));

        return Some(RlProcessInfo {
            pid: pid.as_u32(),
            exe_path: exe.to_path_buf(),
            root_dir: root,
            platform,
            save_data_path: save_path,
        });
    }
    None
}

/// pid of the first process whose exe name contains `name` (case-insensitive).
pub fn find_process(name: &str) -> Option<u32> {
    let needle = name.to_ascii_lowercase();
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::OnlyIfNotSet),
    );
    sys.processes()
        .iter()
        .find(|(_, process)| {
            process
                .name()
                .to_string_lossy()
                .to_lowercase()
                .contains(&needle)
                || process
                    .exe()
                    .and_then(|exe| exe.file_name())
                    .is_some_and(|exe| exe.to_string_lossy().to_lowercase().contains(&needle))
        })
        .map(|(pid, _)| pid.as_u32())
}
