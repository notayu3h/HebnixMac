//! Is Rocket League running?
//!
//! Every patch has to happen while the game is closed: it holds its packages
//! open, and a swap underneath a running game either does nothing or corrupts
//! what it reads next. The Windows backend asks Toolhelp; the Linux backend
//! walks `/proc`. Both are gone here, and macOS has no `/proc`, so this asks
//! `ps` once and matches names.
//!
//! The names are worth care: under Obelisk the game runs as a native arm64
//! process (`fexdarwin-run` translating the guest, `wsd` presenting it), so the
//! process table may show the runtime's own executables rather than
//! `RocketLeague.exe`. All of them are matched, because any of them means the
//! install is in use.

use std::process::Command;

/// Executable names that mean the game (or its runtime) is up. Compared
/// case-insensitively against the basename of `comm`, without the extension.
const MARKERS: [&str; 4] = ["rocketleague", "rocketleague_eac", "fexdarwin-run", "wsd"];

/// One row of `ps -Ao pid=,comm=`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Process {
    pub pid: i32,
    pub command: String,
}

impl Process {
    /// The executable's name without directories or a Windows extension. Both
    /// separators are handled, because the guest's own paths are Windows ones
    /// (`Z:\\opt\\game\\Binaries\\Win64\\RocketLeague.exe`) and `ps` can report
    /// either, depending on how the process was started.
    pub fn name(&self) -> String {
        let base = self
            .command
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&self.command)
            .to_ascii_lowercase();
        base.strip_suffix(".exe")
            .map(str::to_string)
            .unwrap_or(base)
    }
}

/// Every process `ps` will tell us about.
pub fn list() -> Vec<Process> {
    let Ok(output) = Command::new("/bin/ps").args(["-Ao", "pid=,comm="]).output() else {
        return Vec::new();
    };
    parse(&String::from_utf8_lossy(&output.stdout))
}

/// Whether anything that looks like Rocket League is running.
pub fn rocket_league_running() -> bool {
    running_in(&list())
}

/// The marker test on an already collected process list, so it can be tested.
pub fn running_in(processes: &[Process]) -> bool {
    processes
        .iter()
        .any(|process| MARKERS.contains(&process.name().as_str()))
}

fn parse(text: &str) -> Vec<Process> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let split = line.find(char::is_whitespace)?;
            let (pid, command) = line.split_at(split);
            Some(Process {
                pid: pid.parse().ok()?,
                command: command.trim().to_string(),
            })
        })
        .filter(|process| !process.command.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
  501 /sbin/launchd
  733 /System/Library/CoreServices/Finder.app/Contents/MacOS/Finder
 4242 /Users/example/Library/Application Support/com.zagrios.obelisk/Voile/current/linux/engine/fexdarwin-run
 4243 /Users/example/Library/Application Support/com.zagrios.obelisk/Voile/current/linux/engine/wsd
";

    #[test]
    fn parses_pid_and_command() {
        let processes = parse(SAMPLE);
        assert_eq!(processes.len(), 4);
        assert_eq!(processes[2].pid, 4242);
        assert_eq!(
            processes[2].command,
            "/Users/example/Library/Application Support/com.zagrios.obelisk/Voile/current/linux/engine/fexdarwin-run"
        );
    }

    #[test]
    fn strips_windows_extension_and_directories() {
        let process = Process {
            pid: 1,
            command: "Z:\\opt\\game\\Binaries\\Win64\\RocketLeague_EAC.exe".to_string(),
        };
        assert_eq!(process.name(), "rocketleague_eac");
    }

    #[test]
    fn detects_the_runtime_that_presents_the_game() {
        assert!(running_in(&parse(SAMPLE)));
    }

    #[test]
    fn ignores_unrelated_processes() {
        let processes =
            parse("  1 /sbin/launchd\n  2 /Applications/Obelisk.app/Contents/MacOS/Obelisk\n");
        assert!(!running_in(&processes));
    }
}
