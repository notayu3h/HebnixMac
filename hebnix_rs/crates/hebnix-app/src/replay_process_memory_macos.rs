//! Reading another process's memory, macOS: deliberately not implemented.
//!
//! Windows uses this to pull the game's own state out of its address space
//! (`replay_process_memory.rs`). Reading another process's memory on macOS
//! requires either `task_for_pid` with debugging entitlements or disabling
//! SIP, both of which we are not going to ask for.
//!
//! These stubs keep the callers compiling and fail honestly, so the replay
//! export path reports "unavailable" instead of crashing.

pub fn find_process(_name: &str) -> Option<u32> {
    None
}

pub fn started_at(_pid: u32) -> Option<std::time::SystemTime> {
    None
}

pub fn scan_memory(_pid: u32, _needle: &[u8]) -> Vec<String> {
    Vec::new()
}
