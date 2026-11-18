//! Window and screen helpers on macOS.
//!
//! The Windows build owns its own windows through Win32 and the Linux build
//! talks to X11/Wayland; here the game's window belongs to Obelisk's `wsd`,
//! which presents the guest's frames with Metal. Nothing in this file may take
//! that window over.
//!
//! What is real here: the display geometry and the cursor position, which come
//! from CoreGraphics and are all the UI needs for placing its own windows.
//! What is not implemented yet: window rects, focus tracking and the
//! always-on-top dance for our own window, which belong with the overlay port
//! (an `NSPanel` above `wsd`'s surface). Those answer conservatively instead of
//! guessing, so a caller never acts on a wrong rectangle.

use core_graphics::display::CGDisplay;
use core_graphics::event::CGEvent;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

/// No HWND equivalent: a window handle here belongs to `wsd`, and the overlay
/// port supplies its own once it exists.
pub fn rocket_league_hwnd() -> Option<u32> {
    None
}

/// Focus tracking needs the overlay's window to compare against, so this answers
/// "not focused" until that exists.
pub fn is_rocket_league_focused() -> bool {
    false
}

/// See [`is_rocket_league_focused`].
pub fn get_rocket_league_window_rect() -> Option<(i32, i32, i32, i32)> {
    None
}

/// The main display's size in points; the game renders into that display.
pub fn rocket_league_monitor_size() -> (i32, i32) {
    let display = CGDisplay::main();
    (
        display.pixels_wide().try_into().unwrap_or(0),
        display.pixels_high().try_into().unwrap_or(0),
    )
}

/// The main display's bounds, in the same order as the Windows build.
pub fn rocket_league_monitor_rect() -> Option<(i32, i32, i32, i32)> {
    let bounds = CGDisplay::main().bounds();
    Some((
        bounds.origin.x as i32,
        bounds.origin.y as i32,
        bounds.size.width as i32,
        bounds.size.height as i32,
    ))
}

/// Cursor position in screen coordinates, or `None` when CoreGraphics refuses.
pub fn cursor_position() -> Option<(i32, i32)> {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok()?;
    let event = CGEvent::new(source).ok()?;
    let point = event.location();
    Some((point.x as i32, point.y as i32))
}

/// No window rect to test against yet.
pub fn is_cursor_inside_rl_window() -> bool {
    false
}

/// See [`is_cursor_inside_rl_window`].
pub fn cursor_in_rl_window() -> Option<(i32, i32)> {
    None
}

// Our own-window management arrives with the overlay. These stay explicit
// no-ops rather than pretending a change happened.

pub fn can_track_own_window() -> bool {
    false
}

pub fn exempt_own_window_decorations() {}

pub fn reassert_own_window_decorations(_own_pid: u32) {}

pub fn place_own_window(_own_pid: u32, _x: i32, _y: i32, _w: i32, _h: i32) -> bool {
    false
}

pub fn focus_own_window_over_game(_own_pid: u32) -> bool {
    false
}

pub fn unminimize_own_window() -> bool {
    false
}

pub fn set_own_always_on_top(_own_pid: u32, _on_top: bool) -> bool {
    false
}

pub fn raise_other_instance(_own_pid: u32) -> bool {
    false
}

/// A Wayland concept; there is nothing equivalent to hide here.
pub fn hyprland_layer_visible(_namespace: &str) -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_reports_a_real_display() {
        let (width, height) = rocket_league_monitor_size();
        assert!(
            width > 0 && height > 0,
            "CoreGraphics should know the display"
        );
    }

    #[test]
    fn unimplemented_window_helpers_answer_conservatively() {
        assert!(rocket_league_hwnd().is_none());
        assert!(!is_rocket_league_focused());
        assert!(!is_cursor_inside_rl_window());
        assert!(!can_track_own_window());
    }
}
