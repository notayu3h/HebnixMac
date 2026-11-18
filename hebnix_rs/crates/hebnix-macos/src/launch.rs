//! Handing the launch back to Obelisk.
//!
//! The Windows and Linux backends start Rocket League themselves, because they
//! own the prefix and the store session. Here that belongs to Obelisk: it builds
//! the Wine prefix, downloads and versions the Voile runtime, and on Rocket
//! League starts `Binaries/Win64/RocketLeague_EAC.exe` under the Linux engine so
//! Easy Anti-Cheat loads its Linux module.
//!
//! Executing the game's executable from this app would skip all of that and
//! would not be the supported path, so this deliberately cannot do it. It can
//! only bring Obelisk to the front and let the player press Play.

use std::io;
use std::path::{Path, PathBuf};

/// Where Obelisk is installed, when it is in one of the usual places.
pub fn app_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut candidates = vec![PathBuf::from("/Applications/Obelisk.app")];
    if let Some(home) = home {
        candidates.push(home.join("Applications/Obelisk.app"));
    }
    candidates.into_iter().find(|path| path.is_dir())
}

/// Whether Obelisk is installed where we can find it.
pub fn is_installed() -> bool {
    app_path().is_some()
}

/// Bring Obelisk to the front. The player still chooses the game there.
pub fn open_obelisk() -> io::Result<()> {
    let app = app_path().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Obelisk is not in /Applications")
    })?;
    open_path(app)
}

/// `open` is the only supported way to start another app from here: it respects
/// the app's own launch services registration and does not inherit our
/// environment.
pub fn open_path(path: impl AsRef<Path>) -> io::Result<()> {
    let status = std::process::Command::new("/usr/bin/open")
        .arg(path.as_ref())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("open exited with {status}")))
    }
}

/// Why the launch is not ours, for the UI to explain instead of failing.
pub const HANDOFF_NOTE: &str = "Obelisk owns the prefix, the Voile runtime and the \
anti-cheat bootstrapper, so it starts Rocket League. Hebnix prepares the files and \
asks Obelisk to come forward.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_path_is_application_bundle_or_nothing() {
        match app_path() {
            Some(path) => assert!(path.ends_with("Obelisk.app")),
            None => assert!(!is_installed()),
        }
    }

    #[test]
    fn note_explains_the_handoff() {
        assert!(HANDOFF_NOTE.contains("Obelisk"));
    }
}
