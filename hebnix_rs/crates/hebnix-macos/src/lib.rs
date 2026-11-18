//! macOS platform layer for the Hebnix fork.
//!
//! Upstream Hebnix ships two backends, `hebnix-app/src/sys/windows.rs` and
//! `hebnix-app/src/sys/linux.rs`, and `main.rs` gates every platform module on
//! `target_os = "windows"` or `"linux"`. This crate is the start of the third
//! one, split out so it can be built and tested on its own instead of waiting on
//! the rest of the app to compile on macOS.
//!
//! Only what is specific to a Mac playing Rocket League through Obelisk lives
//! here:
//!
//! - [`obelisk`] finds the game inside Obelisk's library. The folder name is
//!   Obelisk's choice, so installs are located by their contents.
//! - [`patcher`] applies and reverts file swaps with backups, so a game update
//!   or Obelisk's "Verify game files" can be detected rather than silently
//!   undoing a user's setup.
//! - [`launch`] hands the launch back to Obelisk: it owns the prefix, the Voile
//!   runtime and the Easy Anti-Cheat bootstrapper, so nothing here execs the
//!   game's own executable.
//! - [`process`] answers "is Rocket League running", which every patch operation
//!   has to know before it touches files.
//!
//! Everything else upstream needs for a macOS target is not here yet; see the
//! README for the ordered list.

pub mod launch;
pub mod obelisk;
pub mod patcher;
pub mod process;

pub use obelisk::Install;
