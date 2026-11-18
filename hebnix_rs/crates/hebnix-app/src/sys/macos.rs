//! macOS bits: single-instance lock, run-at-startup (LaunchAgent), focus
//! handoff, kill/restart RL, embedded-browser cache clearing.
//!
//! Launching is not ours to do here. On Linux this module shells out to the
//! configured launcher (Steam/Heroic/Epic); on macOS Obelisk owns the prefix
//! and the EAC bootstrapper, so "start Rocket League" means "bring Obelisk
//! up and let it start the profile" -- exec'ing the game exe ourselves would
//! bypass Voile and the prefix environment.
//!
//! Window management: macOS does not let a process raise or restack another
//! app's windows without Accessibility trust, and egui's viewport has no
//! always-on-top level we can flip after creation. So the topmost/focus
//! functions are honest no-ops that keep the state the rest of the app reads
//! (monitor.rs, the overlay's topmost decisions) rather than fake success.

use std::sync::atomic::{AtomicBool, Ordering};

/// macOS has no HWND; kept as an opaque unit so call sites
/// (`if let Some(hwnd) = winutil::main_window_hwnd()`) don't need to change.
pub type WindowHandle = ();

fn base_dir() -> std::path::PathBuf {
    hebnix_sdk::utils::paths::base_dir()
}

fn lock_path(name: &str) -> std::path::PathBuf {
    base_dir().join(format!("hebnix_{name}.lock"))
}

/// File-lock based single-instance guard. The handle is kept open for the
/// process lifetime; the OS drops the lock when the process exits or dies,
/// so there is nothing to clean up.
pub struct SingleInstanceLock {
    _file: std::fs::File,
}

fn acquire_lock(name: &str) -> Option<SingleInstanceLock> {
    let path = lock_path(name);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .ok()?;
    match file.try_lock() {
        Ok(()) => Some(SingleInstanceLock { _file: file }),
        Err(_) => None, // another instance holds it
    }
}

pub fn acquire_single_instance() -> Option<SingleInstanceLock> {
    acquire_lock("single_instance")
}

/// A second instance calls this before exiting. Raising another app's
/// window needs Accessibility trust, which we don't ask for just to
/// focus ourselves, so this only logs.
pub fn focus_existing_instance() {
    tracing::info!("hebnix is already running (another instance holds the single-instance lock)");
}

pub fn main_window_hwnd() -> Option<WindowHandle> {
    None
}

/// NSWindow sheet parenting is not exposed through rfd here; the dialog is
/// returned unchanged.
pub fn parent_file_dialog(dialog: rfd::FileDialog) -> rfd::FileDialog {
    dialog
}

pub fn set_main_window_topmost(_topmost: bool) {}

pub fn install_minimize_hook(_hwnd: WindowHandle, _ctx: &eframe::egui::Context) {}

// focus handoff

static MAIN_HIDDEN: AtomicBool = AtomicBool::new(false);
static CAME_FROM_GAME: AtomicBool = AtomicBool::new(false);

pub fn foreground_window_is_ours() -> bool {
    false
}

pub fn hebnix_window_focused() -> bool {
    false
}

pub fn main_window_hidden() -> bool {
    MAIN_HIDDEN.load(Ordering::Relaxed)
}

pub fn note_foreground() {
    CAME_FROM_GAME.store(
        hebnix_sdk::process::is_rocket_league_focused(),
        Ordering::Relaxed,
    );
}

pub fn restore_foreground() -> bool {
    CAME_FROM_GAME.load(Ordering::Relaxed) && hebnix_sdk::process::rocket_league_hwnd().is_some()
}

pub fn focus_main_window() -> bool {
    false
}

static MINIMIZE_REQUESTED: AtomicBool = AtomicBool::new(false);
static SHOW_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn take_minimize_request() -> bool {
    MINIMIZE_REQUESTED.swap(false, Ordering::Relaxed)
}

pub fn take_show_request() -> bool {
    SHOW_REQUESTED.swap(false, Ordering::Relaxed)
}

pub fn set_main_window_invisible(invisible: bool) {
    MAIN_HIDDEN.store(invisible, Ordering::Relaxed);
}

static MINIMIZE_TO_TRAY: AtomicBool = AtomicBool::new(false);

pub fn set_minimize_to_tray(enabled: bool) {
    MINIMIZE_TO_TRAY.store(enabled, Ordering::Relaxed);
}

pub fn minimize_to_tray() -> bool {
    MINIMIZE_TO_TRAY.load(Ordering::Relaxed)
}

/// The caller drives egui's `ViewportCommand::Visible`, which wakes a hidden
/// window on macOS by itself; nothing extra to do here.
pub fn wake_minimized_main_window() {}

// run-at-startup via a per-user LaunchAgent

const LAUNCH_AGENT_LABEL: &str = "com.hebnix.hebnix";

fn launch_agent_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("Library/LaunchAgents")
        .join(format!("{LAUNCH_AGENT_LABEL}.plist"))
}

pub fn is_startup_enabled() -> bool {
    launch_agent_path().is_file()
}

pub fn set_startup_enabled(enabled: bool) -> std::io::Result<()> {
    let path = launch_agent_path();
    if enabled {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let exe = std::env::current_exe()?;
        let contents = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
             \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\">\n\
             <dict>\n\
             \t<key>Label</key><string>{LAUNCH_AGENT_LABEL}</string>\n\
             \t<key>ProgramArguments</key>\n\
             \t<array><string>{}</string></array>\n\
             \t<key>RunAtLoad</key><true/>\n\
             </dict>\n\
             </plist>\n",
            exe.display()
        );
        std::fs::write(&path, contents)
    } else {
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

// kill / restart RL

pub fn kill_rocket_league() -> std::io::Result<()> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, Signal, System};
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    for proc_ in sys.processes().values() {
        let name = proc_.name().to_string_lossy().to_lowercase();
        if name.contains("rocketleague") {
            proc_.kill_with(Signal::Kill);
        }
    }
    Ok(())
}

/// Obelisk boots the profile (prefix + Voile + EAC), so that is what we open.
fn open_obelisk() -> std::io::Result<()> {
    std::process::Command::new("open")
        .args(["-b", "com.zagrios.obelisk"])
        .status()
        .map(|_| ())
}

pub fn start_rocket_league(
    _launch_cfg: &crate::config::RlLaunchCfg,
    _game_path: &std::path::Path,
) -> std::io::Result<()> {
    open_obelisk()
}

pub fn restart_rocket_league(
    _launch_cfg: &crate::config::RlLaunchCfg,
    _game_path: &std::path::Path,
) -> std::io::Result<()> {
    kill_rocket_league()?;
    for _ in 0..60 {
        if !hebnix_sdk::process::is_rocket_league_running() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    open_obelisk()
}

/// Workshop multiplayer "restart without -multihome" -- Obelisk owns the
/// launch arguments, so this is the plain restart.
pub fn restart_rocket_league_without_multihome(
    launch_cfg: &crate::config::RlLaunchCfg,
    game_path: &std::path::Path,
) -> Result<(), String> {
    restart_rocket_league(launch_cfg, game_path).map_err(|error| error.to_string())
}

/// `-multihome` is a game argument, and the profile Obelisk runs doesn't take
/// extra args from us; reported as unsupported rather than silently dropped.
pub fn restart_rocket_league_multihome(
    _launch_cfg: &crate::config::RlLaunchCfg,
    _game_path: &std::path::Path,
    _address: &str,
) -> Result<(), String> {
    Err("multiplayer LAN launch (-multihome) is not available on macOS".to_string())
}

/// Drop Rocket League's embedded-browser cache under the current user's
/// Documents folder.
pub fn clear_rocket_league_web_cache() -> std::io::Result<()> {
    let rel = std::path::Path::new("My Games/Rocket League/TAGame/Cache/WebCache");
    let Some(docs) = dirs::document_dir() else {
        return Ok(());
    };
    match std::fs::remove_dir_all(docs.join(rel)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn clear_rocket_league_multihome() -> Result<(), String> {
    Ok(())
}
