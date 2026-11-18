//! Rocket League launch handling, macOS.
//!
//! The Linux build writes a launch script and hands it to Steam/Heroic/Epic;
//! the Windows one starts the exe with the configured arguments. Neither is
//! right here: Obelisk owns the Wine prefix, Voile and the EAC bootstrapper,
//! so launching means opening Obelisk and letting it start the profile.
//! Arguments such as `-multihome` can't be injected into a profile, so those
//! paths report "not available" instead of silently launching without them.

use std::path::PathBuf;

use crate::config::RlLaunchCfg;

/// a launcher shortcut found on disk, offered as a launch target in setup.
/// Nothing to discover on macOS (Heroic/`.desktop` entries don't exist), so
/// this is only kept so the settings type compiles.
#[derive(Clone, Debug)]
pub struct ShortcutCandidate {
    pub app_name: String,
    pub exe: String,
    pub rungameid: u64,
}

/// Steam's shortcut-ID algorithm: crc32(exe + appname), top bit forced set,
/// packed into the upper 32 bits with a fixed 0x02000000 suffix.
pub fn compute_shortcut_id(exe: &str, app_name: &str) -> u64 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(exe.as_bytes());
    hasher.update(app_name.as_bytes());
    (u64::from(hasher.finalize()) | 0x8000_0000) << 32 | 0x0200_0000
}

pub fn find_heroic_shortcuts() -> Vec<ShortcutCandidate> {
    Vec::new()
}

/// Obelisk resolves the install when it starts the game, and its prefix
/// layout is not something this app should guess at.
pub fn resolve_install_root(_cfg: &RlLaunchCfg) -> Option<PathBuf> {
    None
}

pub fn restart(cfg: &RlLaunchCfg) -> Result<(), String> {
    crate::winutil::start_rocket_league(cfg, std::path::Path::new(""))
        .map_err(|error| error.to_string())
}

pub fn restart_for_multiplayer_without_multihome(cfg: &RlLaunchCfg) -> Result<(), String> {
    restart(cfg)
}

pub fn restart_multihome(_cfg: &RlLaunchCfg, _address: &str) -> Result<(), String> {
    Err("multiplayer LAN launch (-multihome) is not available on macOS".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_id_matches_steams_layout() {
        // top bit set, fixed 0x02000000 suffix in the low word
        let id = compute_shortcut_id("\"/games/rl.exe\"", "Rocket League");
        assert_eq!(id & 0x8000_0000_0000_0000, 0x8000_0000_0000_0000);
        assert_eq!(id & 0x0200_0000, 0x0200_0000);
    }

    #[test]
    fn no_shortcuts_to_offer() {
        assert!(find_heroic_shortcuts().is_empty());
    }
}
