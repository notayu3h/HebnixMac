//! Applying and reverting file swaps, with the backups that make it reversible.
//!
//! Every cosmetic Hebnix or Rocket Swap offers on this side of the game is a
//! file in the install folder: a `.upk` in `TAGame/CookedPCConsole`, an audio
//! bank, an `.ini` value. None of it needs the game's process, which is why it
//! works on a Mac at all.
//!
//! What it does need is a way back. Obelisk's "Verify game files" repairs the
//! install, and a game update replaces what changed, so a swap silently
//! disappears. This keeps the original beside a manifest of hashes, which lets
//! [`SwapSet::state`] tell three cases apart: still swapped, already reverted,
//! and changed by something else.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// How a file relates to the swaps recorded for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Recorded as swapped, and the file on disk is the swapped one.
    Swapped,
    /// Not recorded as swapped (or never touched).
    Original,
    /// Recorded as swapped, but the file is neither the swap nor the original:
    /// an update, a verify, or an edit by something else.
    Changed,
}

/// One recorded swap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Path relative to the install root, as written by the caller.
    pub relative: String,
    /// Hash of the file before it was first swapped, `0` when it did not exist.
    pub original: u64,
    /// Hash of the file now installed.
    pub swapped: u64,
}

/// Swaps applied to one install, with their backups.
#[derive(Clone, Debug)]
pub struct SwapSet {
    install_root: PathBuf,
    backup_root: PathBuf,
}

const MANIFEST_HEADER: &str = "# hebnix-macos swaps v1: relative\toriginal\tswapped";
const BACKUP_SUFFIX: &str = ".hebnix-original";

impl SwapSet {
    /// `backup_root` should be outside the game folder, so a verify cannot
    /// remove it: the app's data directory is the natural home.
    pub fn new(install_root: impl Into<PathBuf>, backup_root: impl Into<PathBuf>) -> Self {
        Self {
            install_root: install_root.into(),
            backup_root: backup_root.into(),
        }
    }

    pub fn install_root(&self) -> &Path {
        &self.install_root
    }

    pub fn backup_root(&self) -> &Path {
        &self.backup_root
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.backup_root.join("swaps.tsv")
    }

    fn backup_path(&self, relative: &str) -> PathBuf {
        // Keep the game's directory structure so two files with the same name
        // in different folders cannot collide.
        let mut path = self.backup_root.join("originals");
        for part in relative.split(['/', '\\']).filter(|part| !part.is_empty()) {
            path.push(part);
        }
        let mut name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        name.push_str(BACKUP_SUFFIX);
        path.set_file_name(name);
        path
    }

    /// Every swap currently recorded, in the order they were first applied.
    pub fn entries(&self) -> Vec<Entry> {
        let Ok(text) = fs::read_to_string(self.manifest_path()) else {
            return Vec::new();
        };
        text.lines()
            .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
            .filter_map(|line| {
                let mut fields = line.split('\t');
                let relative = fields.next()?.to_string();
                let original = fields.next()?.parse().ok()?;
                let swapped = fields.next()?.parse().ok()?;
                Some(Entry {
                    relative,
                    original,
                    swapped,
                })
            })
            .collect()
    }

    fn write_entries(&self, entries: &[Entry]) -> io::Result<()> {
        let mut text = String::from(MANIFEST_HEADER);
        text.push('\n');
        for entry in entries {
            text.push_str(&format!(
                "{}\t{}\t{}\n",
                entry.relative, entry.original, entry.swapped
            ));
        }
        fs::create_dir_all(&self.backup_root)?;
        let path = self.manifest_path();
        let tmp = path.with_extension("tsv.tmp");
        fs::write(&tmp, text)?;
        fs::rename(tmp, path)
    }

    /// Replace `relative` with `source`, keeping the original file's bytes.
    ///
    /// A path already recorded keeps its first backup: re-applying a different
    /// variant of the same file must still restore what the game shipped, not
    /// the previous swap.
    pub fn apply_file(&self, relative: &str, source: impl AsRef<Path>) -> io::Result<()> {
        let destination = self.install_root.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut entries = self.entries();
        let index = entries.iter().position(|entry| entry.relative == relative);
        if index.is_none() && destination.exists() {
            let backup = self.backup_path(relative);
            if let Some(parent) = backup.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&destination, &backup)?;
        }
        let original = match index {
            Some(index) => entries[index].original,
            None if destination.exists() => hash_file(&destination)?,
            None => 0,
        };

        // Copy beside the target and rename, so a launch cannot catch a
        // half-written package.
        let staging = destination.with_extension("hebnix-staging");
        fs::copy(source.as_ref(), &staging)?;
        fs::rename(&staging, &destination)?;

        let swapped = hash_file(&destination)?;
        let entry = Entry {
            relative: relative.to_string(),
            original,
            swapped,
        };
        match index {
            Some(index) => entries[index] = entry,
            None => entries.push(entry),
        }
        self.write_entries(&entries)
    }

    /// Put the original back and forget the swap. Harmless when nothing is
    /// recorded for `relative`.
    pub fn restore_file(&self, relative: &str) -> io::Result<()> {
        let entries = self.entries();
        if !entries.iter().any(|entry| entry.relative == relative) {
            return Ok(());
        }
        let destination = self.install_root.join(relative);
        let backup = self.backup_path(relative);
        if backup.exists() {
            fs::copy(&backup, &destination)?;
            fs::remove_file(&backup)?;
        } else if destination.exists() {
            // Nothing was there before the swap, so restoring means removing it.
            fs::remove_file(&destination)?;
        }
        let kept: Vec<Entry> = entries
            .into_iter()
            .filter(|entry| entry.relative != relative)
            .collect();
        self.write_entries(&kept)
    }

    /// Restore everything recorded, returning how many were reverted.
    pub fn restore_all(&self) -> io::Result<usize> {
        let entries = self.entries();
        let mut restored = 0;
        for entry in &entries {
            self.restore_file(&entry.relative)?;
            restored += 1;
        }
        Ok(restored)
    }

    /// Whether the file on disk is still the swap, was reverted, or was changed
    /// by an update or a verify.
    pub fn state(&self, relative: &str) -> State {
        let Some(entry) = self
            .entries()
            .into_iter()
            .find(|entry| entry.relative == relative)
        else {
            return State::Original;
        };
        match hash_file(self.install_root.join(relative)) {
            Ok(hash) if hash == entry.swapped => State::Swapped,
            Ok(hash) if hash == entry.original => State::Original,
            _ => State::Changed,
        }
    }

    /// Swaps whose file was reverted or replaced since they were applied, which
    /// is what a "reapply" prompt is built from.
    pub fn broken(&self) -> Vec<Entry> {
        self.entries()
            .into_iter()
            .filter(|entry| self.state(&entry.relative) != State::Swapped)
            .collect()
    }
}

/// FNV-1a over the file's bytes. This only distinguishes our own files from
/// someone else's, so a fast non-cryptographic hash is enough and costs nothing
/// on a multi-hundred-megabyte package.
fn hash_file(path: impl AsRef<Path>) -> io::Result<u64> {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut file = fs::File::open(path)?;
    let mut hash = OFFSET;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(hash);
        }
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Fixture {
        dir: PathBuf,
        set: SwapSet,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "hebnix_macos_patcher_{name}_{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            let game = dir.join("game");
            fs::create_dir_all(game.join("TAGame/CookedPCConsole")).unwrap();
            fs::write(game.join("TAGame/CookedPCConsole/Lambo_P.upk"), b"vanilla").unwrap();
            let set = SwapSet::new(&game, dir.join("data"));
            Self { dir, set }
        }

        fn game_file(&self, name: &str) -> PathBuf {
            self.dir.join("game/TAGame/CookedPCConsole").join(name)
        }

        fn swap_source(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.dir.join(name);
            fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    const REL: &str = "TAGame/CookedPCConsole/Lambo_P.upk";

    #[test]
    fn apply_keeps_the_original_and_puts_the_swap_in_place() {
        let fixture = Fixture::new("apply");
        let source = fixture.swap_source("blue.upk", b"blue");

        fixture.set.apply_file(REL, &source).unwrap();

        assert_eq!(fs::read(fixture.game_file("Lambo_P.upk")).unwrap(), b"blue");
        assert_eq!(fixture.set.state(REL), State::Swapped);
        let backup = fixture.set.backup_path(REL);
        assert_eq!(fs::read(backup).unwrap(), b"vanilla");
    }

    #[test]
    fn restore_puts_the_original_bytes_back() {
        let fixture = Fixture::new("restore");
        let source = fixture.swap_source("blue.upk", b"blue");
        fixture.set.apply_file(REL, &source).unwrap();

        fixture.set.restore_file(REL).unwrap();

        assert_eq!(
            fs::read(fixture.game_file("Lambo_P.upk")).unwrap(),
            b"vanilla"
        );
        assert_eq!(fixture.set.state(REL), State::Original);
        assert!(fixture.set.entries().is_empty());
    }

    #[test]
    fn reapplying_keeps_the_first_backup() {
        let fixture = Fixture::new("reapply");
        let first = fixture.swap_source("blue.upk", b"blue");
        let second = fixture.swap_source("red.upk", b"red");

        fixture.set.apply_file(REL, &first).unwrap();
        fixture.set.apply_file(REL, &second).unwrap();
        fixture.set.restore_file(REL).unwrap();

        assert_eq!(
            fs::read(fixture.game_file("Lambo_P.upk")).unwrap(),
            b"vanilla",
            "restoring must return what the game shipped, not the previous swap"
        );
    }

    #[test]
    fn a_new_file_is_removed_on_restore() {
        let fixture = Fixture::new("newfile");
        let source = fixture.swap_source("extra.upk", b"extra");
        let rel = "TAGame/CookedPCConsole/Extra_SF.upk";

        fixture.set.apply_file(rel, &source).unwrap();
        assert!(fixture.game_file("Extra_SF.upk").exists());

        fixture.set.restore_file(rel).unwrap();
        assert!(!fixture.game_file("Extra_SF.upk").exists());
    }

    #[test]
    fn a_verify_that_reverts_the_file_is_reported_as_broken() {
        let fixture = Fixture::new("verify");
        let source = fixture.swap_source("blue.upk", b"blue");
        fixture.set.apply_file(REL, &source).unwrap();

        // What Obelisk's "Verify game files" or a game update does: the file is
        // back to exactly what the game shipped.
        fs::write(fixture.game_file("Lambo_P.upk"), b"vanilla").unwrap();

        assert_eq!(fixture.set.state(REL), State::Original);
        assert_eq!(
            fixture.set.broken().len(),
            1,
            "a reverted swap still has to be reported, or it is silently gone"
        );
    }

    #[test]
    fn restoring_reports_how_many_changed() {
        let fixture = Fixture::new("restoreall");
        let one = fixture.swap_source("one.upk", b"one");
        let two = fixture.swap_source("two.upk", b"two");
        fixture.set.apply_file(REL, &one).unwrap();
        fixture
            .set
            .apply_file("TAGame/CookedPCConsole/Ball_SF.upk", &two)
            .unwrap();

        assert_eq!(fixture.set.restore_all().unwrap(), 2);
        assert!(fixture.set.entries().is_empty());
        assert_eq!(
            fs::read(fixture.game_file("Lambo_P.upk")).unwrap(),
            b"vanilla"
        );
    }

    #[test]
    fn a_malformed_manifest_line_is_skipped_not_fatal() {
        let fixture = Fixture::new("malformed");
        let source = fixture.swap_source("blue.upk", b"blue");
        fixture.set.apply_file(REL, &source).unwrap();
        let manifest = fixture.set.manifest_path();
        let mut text = fs::read_to_string(&manifest).unwrap();
        text.push_str("garbage\nnot\ta\tnumber\n");
        fs::write(&manifest, text).unwrap();

        assert_eq!(fixture.set.entries().len(), 1);
    }
}
