//! Finding Rocket League inside an Obelisk library.
//!
//! Obelisk installs every Windows game into its own folder under
//! `~/Library/Application Support/com.zagrios.obelisk/Games`, and gives it a
//! Wine prefix under `Prefixes/` named after that folder. Rocket League has no
//! usable native macOS build, so this is the only install the patchers can touch
//! on a Mac.
//!
//! The install folder's name is chosen by Obelisk, not by us (it follows the
//! store's product name, which changes with localisation in some stores), so
//! installs are located by their contents instead of by a guessed name. This is
//! also how the store itself is inferred: a Steam install carries
//! `Binaries/Win64/RocketLeague.exe`, an Epic install runs
//! `Binaries/Win64/RocketLeague_EAC.exe` through the anti-cheat bootstrapper.

use std::fs;
use std::path::{Path, PathBuf};

/// Obelisk's bundle identifier, which is also its data directory name.
pub const BUNDLE_ID: &str = "com.zagrios.obelisk";

/// Rocket League's Steam app id (see Obelisk's `crates/game-runtime/profiles.json`).
pub const STEAM_APP_ID: u32 = 252_950;

/// Rocket League's Epic product, from the same profile table.
pub const EPIC_NAMESPACE: &str = "9773aa1aa54f4f7b80e44bef04986cea";
pub const EPIC_APP_NAME: &str = "Sugar";

/// Which store an install came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Store {
    Steam,
    Epic,
    Gog,
}

/// One Rocket League install found in an Obelisk library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Install {
    /// The game's install folder, e.g. `.../Games/Rocket League`.
    pub root: PathBuf,
    /// `TAGame/CookedPCConsole`, where every `.upk` swap goes.
    pub cooked: PathBuf,
    /// `Binaries/Win64` when it exists.
    pub binaries: Option<PathBuf>,
    /// The game's Wine prefix, shared by both engines.
    pub prefix: Option<PathBuf>,
    /// What the files suggest, when they say anything.
    pub store: Option<Store>,
}

impl Install {
    /// The install folder's own name, which is how Obelisk names the prefix.
    pub fn folder_name(&self) -> Option<&str> {
        self.root.file_name().and_then(|name| name.to_str())
    }

    /// A path inside the game folder, resolved as the game would see it.
    ///
    /// APFS is case-insensitive by default, so unlike the Linux backend this
    /// does not have to search for the on-disk casing; it only has to cope with
    /// a case-insensitive volume refusing to distinguish two names. A
    /// case-sensitive APFS volume is not handled yet.
    pub fn game_path(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.root.join(relative)
    }

    /// `TAGame/CookedPCConsole/<name>`, the directory the swapper writes to.
    pub fn cooked_file(&self, name: impl AsRef<Path>) -> PathBuf {
        self.cooked.join(name)
    }
}

/// `<home>/Library/Application Support/com.zagrios.obelisk`, when it exists.
pub fn library_dir() -> Option<PathBuf> {
    library_dir_in(std::env::var_os("HOME").map(PathBuf::from)?)
}

/// The same lookup against an explicit home, so it can be tested.
pub fn library_dir_in(home: impl AsRef<Path>) -> Option<PathBuf> {
    let dir = home
        .as_ref()
        .join("Library/Application Support")
        .join(BUNDLE_ID);
    dir.is_dir().then_some(dir)
}

/// `<library>/Games`, where Obelisk puts installed games.
pub fn games_dir() -> Option<PathBuf> {
    let dir = library_dir()?.join("Games");
    dir.is_dir().then_some(dir)
}

/// `<library>/Prefixes`, one Wine prefix per install, named after its folder.
pub fn prefixes_dir() -> Option<PathBuf> {
    let dir = library_dir()?.join("Prefixes");
    dir.is_dir().then_some(dir)
}

/// `<library>/Voile/current`, the Windows runtime, when a Windows game has run.
pub fn voile_dir() -> Option<PathBuf> {
    let dir = library_dir()?.join("Voile").join("current");
    dir.is_dir().then_some(dir)
}

/// Every Rocket League install in the library, most recently modified first.
pub fn find_all() -> Vec<Install> {
    let Some(games) = games_dir() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in fs::read_dir(&games).into_iter().flatten().flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(install) = install_at(&path) {
            found.push(install);
            continue;
        }
        // Obelisk nests installs under the store that owns them:
        // `Games/Epic/epic-<id>-Windows`. One level is enough; nothing is
        // nested deeper than that today.
        for child in fs::read_dir(&path).into_iter().flatten().flatten() {
            if let Some(install) = install_at(child.path()) {
                found.push(install);
            }
        }
    }
    // A library can hold a Steam and an Epic copy side by side; the one played
    // last is the one the user means.
    found.sort_by_key(|install| std::cmp::Reverse(modified(&install.root)));
    found
}

/// The install to patch: the most recently modified one, if any.
pub fn find_rocket_league() -> Option<Install> {
    find_all().into_iter().next()
}

/// Inspect one folder and describe it as an install, if it is one.
pub fn install_at(root: impl AsRef<Path>) -> Option<Install> {
    let root = root.as_ref();
    let tagame = child_ignoring_case(root, "TAGame")?;
    let cooked = child_ignoring_case(&tagame, "CookedPCConsole")?;
    if !cooked.is_dir() {
        return None;
    }
    let binaries = child_ignoring_case(root, "Binaries")
        .and_then(|binaries| child_ignoring_case(&binaries, "Win64"))
        .filter(|dir| dir.is_dir());

    // The path is the reliable source: a real install carries both
    // `RocketLeague.exe` and `RocketLeague_EAC.exe`, so the executables alone
    // cannot tell Epic from Steam. They are only a fallback for a flat layout.
    let store = store_from_path(root).or_else(|| {
        let names = names_in(binaries.as_deref()?);
        if names.iter().any(|name| name == "rocketleague_eac.exe") {
            Some(Store::Epic)
        } else if names.iter().any(|name| name == "rocketleague.exe") {
            Some(Store::Steam)
        } else {
            None
        }
    });

    let prefix = prefix_for(root);

    Some(Install {
        root: root.to_path_buf(),
        cooked,
        binaries,
        prefix,
        store,
    })
}

/// The store an install belongs to, read from the folder Obelisk filed it under.
fn store_from_path(root: &Path) -> Option<Store> {
    root.ancestors()
        .filter_map(|dir| dir.file_name())
        .filter_map(|name| name.to_str())
        .find_map(|name| match name.to_ascii_lowercase().as_str() {
            "epic" => Some(Store::Epic),
            "steam" => Some(Store::Steam),
            "gog" => Some(Store::Gog),
            _ => None,
        })
}

/// The prefix Obelisk gave an install.
///
/// Prefixes sit beside `Games`, named after the install folder, with the engine
/// appended when a game only ever runs on the Linux engine: Rocket League's real
/// prefix here is `epic-<id>-Windows-linux`, because its anti-cheat refuses the
/// macOS Wine. A library can live anywhere, so the location is derived from the
/// install itself rather than from the environment.
fn prefix_for(root: &Path) -> Option<PathBuf> {
    let name = root.file_name()?.to_str()?;
    let mut current = root.parent();
    while let Some(dir) = current {
        if dir.file_name().and_then(|part| part.to_str()) == Some("Games") {
            let prefixes = dir.parent()?.join("Prefixes");
            return [
                name.to_string(),
                format!("{name}-linux"),
                format!("{name}-wine"),
            ]
            .into_iter()
            .map(|candidate| prefixes.join(candidate))
            .find(|candidate| candidate.is_dir());
        }
        current = dir.parent();
    }
    None
}

fn modified(path: &Path) -> std::time::SystemTime {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .unwrap_or(std::time::UNIX_EPOCH)
}

fn names_in(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(str::to_ascii_lowercase))
        .collect()
}

/// A child of `dir` whose name matches ignoring ASCII case, as the game's own
/// lookups do on a case-insensitive volume.
fn child_ignoring_case(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.exists() {
        return Some(exact);
    }
    let wanted = name.to_ascii_lowercase();
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|found| found.to_ascii_lowercase() == wanted)
        })
        .map(|entry| entry.path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "hebnix_macos_obelisk_{name}_{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Build a fake Obelisk library holding one install.
    fn fake_library(name: &str, folder: &str, store: Store) -> (TempDir, PathBuf) {
        let temp = TempDir::new(name);
        let library = temp
            .path()
            .join("Library/Application Support")
            .join(BUNDLE_ID);
        let install = library.join("Games").join(folder);
        fs::create_dir_all(install.join("TAGame/CookedPCConsole")).unwrap();
        fs::create_dir_all(install.join("Binaries/Win64")).unwrap();
        fs::create_dir_all(library.join("Prefixes").join(folder)).unwrap();
        let exe = match store {
            Store::Epic => "RocketLeague_EAC.exe",
            Store::Steam | Store::Gog => "RocketLeague.exe",
        };
        fs::write(install.join("Binaries/Win64").join(exe), b"x").unwrap();
        (temp, library)
    }

    #[test]
    fn finds_install_by_its_contents_not_its_name() {
        let (temp, library) = fake_library("contents", "Rocket League", Store::Steam);
        let install = install_at(library.join("Games").join("Rocket League")).unwrap();
        assert_eq!(install.folder_name(), Some("Rocket League"));
        assert!(install.cooked.ends_with("TAGame/CookedPCConsole"));
        assert_eq!(install.store, Some(Store::Steam));
        assert!(install
            .cooked_file("Lambo_P.upk")
            .starts_with(&install.cooked));
        drop(temp);
    }

    #[test]
    fn finds_a_localised_folder_name() {
        let (temp, library) = fake_library("localised", "Rocket League™", Store::Epic);
        let install = install_at(library.join("Games").join("Rocket League™")).unwrap();
        assert_eq!(install.store, Some(Store::Epic));
        drop(temp);
    }

    #[test]
    fn maps_prefix_by_folder_name() {
        let (temp, library) = fake_library("prefix", "Rocket League", Store::Steam);
        let install = install_at(library.join("Games").join("Rocket League")).unwrap();
        let prefix = install.prefix.expect("prefix");
        assert_eq!(
            prefix,
            library.join("Prefixes").join("Rocket League"),
            "Obelisk names the prefix after the install folder"
        );
        drop(temp);
    }

    /// The layout Obelisk actually uses on this machine:
    /// `Games/Epic/epic-<id>-Windows`, prefix `epic-<id>-Windows-linux`.
    #[test]
    fn finds_the_nested_store_layout_and_its_linux_prefix() {
        let temp = TempDir::new("nested_store");
        let library = temp
            .path()
            .join("Library/Application Support")
            .join(BUNDLE_ID);
        let folder = "epic-52edd447ec86748fc2afecc6cd67acc7748a5461-Windows";
        let install = library.join("Games/Epic").join(folder);
        fs::create_dir_all(install.join("TAGame/CookedPCConsole")).unwrap();
        fs::create_dir_all(install.join("Binaries/Win64")).unwrap();
        fs::create_dir_all(library.join("Prefixes").join(format!("{folder}-linux"))).unwrap();
        fs::write(install.join("Binaries/Win64/RocketLeague.exe"), b"x").unwrap();
        fs::write(install.join("Binaries/Win64/RocketLeague_EAC.exe"), b"x").unwrap();

        let found = install_at(&install).unwrap();
        assert_eq!(found.store, Some(Store::Epic), "the path says Epic");
        assert_eq!(
            found.prefix,
            Some(library.join("Prefixes").join(format!("{folder}-linux")))
        );
        drop(temp);
    }

    #[test]
    fn ignores_folders_that_are_not_installs() {
        let (temp, library) = fake_library("notinstall", "Rocket League", Store::Steam);
        let other = library.join("Games").join("Something Else");
        fs::create_dir_all(other.join("Binaries/Win64")).unwrap();
        assert!(install_at(&other).is_none());
        drop(temp);
    }

    #[test]
    fn library_lookup_needs_the_bundle_directory() {
        let temp = TempDir::new("nolibrary");
        assert!(library_dir_in(temp.path()).is_none());
        fs::create_dir_all(
            temp.path()
                .join("Library/Application Support")
                .join(BUNDLE_ID),
        )
        .unwrap();
        assert!(library_dir_in(temp.path()).is_some());
        drop(temp);
    }
}
