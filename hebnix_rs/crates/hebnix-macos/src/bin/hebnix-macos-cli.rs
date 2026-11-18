//! Helper the Mac app shells out to, so the GUI does not need FFI into the crate.
//!
//! Output is `key=value` lines for `status`, and one `path<TAB>state` line per
//! swap for `swaps`, which is what `macos/HebnixMac/App.swift` parses.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use hebnix_macos::patcher::{State, SwapSet};
use hebnix_macos::{launch, obelisk, process};

fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    home.join("Library/Application Support/HebnixMac")
}

fn swaps_for(install: &obelisk::Install) -> SwapSet {
    SwapSet::new(&install.root, data_dir().join("swaps"))
}

/// A bare file name is taken to mean `TAGame/CookedPCConsole/<name>`, which is
/// where every package swap actually goes.
fn normalize(relative: &str) -> String {
    if relative.contains('/') || relative.contains('\\') {
        relative.to_string()
    } else {
        format!("TAGame/CookedPCConsole/{relative}")
    }
}

fn state_name(state: State) -> &'static str {
    match state {
        State::Swapped => "swapped",
        State::Original => "original",
        State::Changed => "changed",
    }
}

fn install() -> Result<obelisk::Install, String> {
    obelisk::find_rocket_league().ok_or_else(|| {
        "Rocket League was not found in Obelisk's library. Install it in Obelisk first.".to_string()
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("status");

    match command {
        "status" => match obelisk::find_rocket_league() {
            Some(found) => {
                let set = swaps_for(&found);
                let entries = set.entries();
                let broken = set.broken().len();
                println!("obelisk_installed={}", launch::is_installed());
                println!(
                    "obelisk_app={}",
                    launch::app_path().map_or_else(String::new, |p| p.display().to_string())
                );
                println!("install={}", found.root.display());
                println!("cooked={}", found.cooked.display());
                println!(
                    "prefix={}",
                    found
                        .prefix
                        .as_ref()
                        .map_or_else(String::new, |p| p.display().to_string())
                );
                println!(
                    "store={}",
                    match found.store {
                        Some(obelisk::Store::Steam) => "steam",
                        Some(obelisk::Store::Epic) => "epic",
                        Some(obelisk::Store::Gog) => "gog",
                        None => "unknown",
                    }
                );
                println!("running={}", process::rocket_league_running());
                println!("swaps={}", entries.len());
                println!("broken={broken}");
                println!("backups={}", set.backup_root().display());
                ExitCode::SUCCESS
            }
            None => {
                println!("obelisk_installed={}", launch::is_installed());
                println!("install=");
                println!("running={}", process::rocket_league_running());
                println!("swaps=0");
                println!("broken=0");
                ExitCode::from(3)
            }
        },
        "swaps" => {
            let found = match install() {
                Ok(found) => found,
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(3);
                }
            };
            for entry in swaps_for(&found).entries() {
                let state = swaps_for(&found).state(&entry.relative);
                println!("{}\t{}", entry.relative, state_name(state));
            }
            ExitCode::SUCCESS
        }
        "apply" => {
            let (Some(name), Some(source)) = (args.get(1), args.get(2)) else {
                eprintln!("usage: hebnix-macos-cli apply <file.upk> <source path>");
                return ExitCode::from(2);
            };
            let found = match install() {
                Ok(found) => found,
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(3);
                }
            };
            if process::rocket_league_running() {
                eprintln!("Rocket League is running. Close it, then apply the swap.");
                return ExitCode::from(4);
            }
            let relative = normalize(name);
            if !Path::new(source).is_file() {
                eprintln!("no such file: {source}");
                return ExitCode::from(2);
            }
            let set = swaps_for(&found);
            match set.apply_file(&relative, source) {
                Ok(()) => {
                    println!("applied {relative}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("could not apply {relative}: {error}");
                    ExitCode::from(5)
                }
            }
        }
        "restore" => {
            let Some(name) = args.get(1) else {
                eprintln!("usage: hebnix-macos-cli restore <file.upk>");
                return ExitCode::from(2);
            };
            let found = match install() {
                Ok(found) => found,
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(3);
                }
            };
            if process::rocket_league_running() {
                eprintln!("Rocket League is running. Close it, then restore.");
                return ExitCode::from(4);
            }
            let relative = normalize(name);
            match swaps_for(&found).restore_file(&relative) {
                Ok(()) => {
                    println!("restored {relative}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("could not restore {relative}: {error}");
                    ExitCode::from(5)
                }
            }
        }
        "restore-all" => {
            let found = match install() {
                Ok(found) => found,
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(3);
                }
            };
            if process::rocket_league_running() {
                eprintln!("Rocket League is running. Close it, then restore.");
                return ExitCode::from(4);
            }
            match swaps_for(&found).restore_all() {
                Ok(count) => {
                    println!("restored {count}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("could not restore everything: {error}");
                    ExitCode::from(5)
                }
            }
        }
        "open" => match launch::open_obelisk() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("could not open Obelisk: {error}");
                ExitCode::from(6)
            }
        },
        "help" | "--help" | "-h" => {
            println!("hebnix-macos-cli status");
            println!("hebnix-macos-cli swaps");
            println!("hebnix-macos-cli apply <file.upk> <source path>");
            println!("hebnix-macos-cli restore <file.upk>");
            println!("hebnix-macos-cli restore-all");
            println!("hebnix-macos-cli open");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown command: {other} (try: help)");
            ExitCode::from(2)
        }
    }
}
