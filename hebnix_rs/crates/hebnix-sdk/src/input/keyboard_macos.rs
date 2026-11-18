//! hotkey detection and synthetic typing on macOS.
//!
//! Key names match the python `keyboard` lib ("tab", "f2", "ctrl", ...) exactly
//! as the Windows and Linux backends do, so existing configs keep working; only
//! the code behind a name changes, to a macOS virtual key code.
//!
//! Reading a key's state uses `CGEventSourceKeyState`, which needs no
//! permissions. Posting keys uses `CGEventPost`, which macOS only delivers to
//! other apps when Hebnix has been granted **Accessibility**
//! (System Settings → Privacy & Security → Accessibility). Without it the calls
//! succeed and nothing arrives, so typing silently does nothing — that is the
//! platform's behaviour, not a bug to work around.

use std::time::{Duration, Instant};

/// A macOS virtual key code (`kVK_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyCode(pub u16);

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
    fn CGEventCreateKeyboardEvent(
        source: *mut std::ffi::c_void,
        key: u16,
        down: bool,
    ) -> *mut std::ffi::c_void;
    fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
    fn CFRelease(value: *mut std::ffi::c_void);
}

/// `kCGEventSourceStateHIDSystemState`: the real hardware state, not our own
/// posted events.
const HID_STATE: i32 = 1;
/// `kCGHIDEventTap`.
const HID_TAP: u32 = 0;

/// (name, macOS key code) for named keys, matching the Windows virtual-key and
/// Linux evdev tables name for name.
fn named_keys() -> &'static [(&'static str, KeyCode)] {
    &[
        ("backspace", KeyCode(51)),
        ("tab", KeyCode(48)),
        ("enter", KeyCode(36)),
        ("return", KeyCode(36)),
        ("shift", KeyCode(56)),
        ("ctrl", KeyCode(59)),
        ("control", KeyCode(59)),
        ("alt", KeyCode(58)),
        ("option", KeyCode(58)),
        ("cmd", KeyCode(55)),
        ("command", KeyCode(55)),
        ("pause", KeyCode(99)),
        ("caps lock", KeyCode(57)),
        ("esc", KeyCode(53)),
        ("escape", KeyCode(53)),
        ("space", KeyCode(49)),
        ("page up", KeyCode(116)),
        ("page down", KeyCode(121)),
        ("end", KeyCode(119)),
        ("home", KeyCode(115)),
        ("left", KeyCode(123)),
        ("up", KeyCode(126)),
        ("right", KeyCode(124)),
        ("down", KeyCode(125)),
        // No Print Screen or Insert key exists here; these are the nearest
        // keys a Mac keyboard actually has.
        ("print screen", KeyCode(105)),
        ("insert", KeyCode(114)),
        ("delete", KeyCode(117)),
        ("f1", KeyCode(122)),
        ("f2", KeyCode(120)),
        ("f3", KeyCode(99)),
        ("f4", KeyCode(118)),
        ("f5", KeyCode(96)),
        ("f6", KeyCode(97)),
        ("f7", KeyCode(98)),
        ("f8", KeyCode(100)),
        ("f9", KeyCode(101)),
        ("f10", KeyCode(109)),
        ("f11", KeyCode(103)),
        ("f12", KeyCode(111)),
        ("num lock", KeyCode(71)),
        ("scroll lock", KeyCode(107)),
        ("-", KeyCode(27)),
        ("=", KeyCode(24)),
        ("[", KeyCode(33)),
        ("]", KeyCode(30)),
        (";", KeyCode(41)),
        ("'", KeyCode(39)),
        (",", KeyCode(43)),
        (".", KeyCode(47)),
        ("/", KeyCode(44)),
        ("\\", KeyCode(42)),
        ("`", KeyCode(50)),
    ]
}

/// key name to macOS key code
pub fn name_to_code(key: &str) -> Option<KeyCode> {
    let key = key.trim().to_ascii_lowercase();
    if let Some((_, code)) = named_keys().iter().find(|(name, _)| *name == key) {
        return Some(*code);
    }
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return char_to_code(c).map(|(code, _)| code);
    }
    None
}

/// key code back to key name, used by hotkey capture
pub fn code_to_name(code: KeyCode) -> Option<String> {
    if let Some((name, _)) = named_keys().iter().find(|(_, found)| *found == code) {
        return Some((*name).to_string());
    }
    for c in ('a'..='z').chain('0'..='9') {
        if char_to_code(c).is_some_and(|(found, _)| found == code) {
            return Some(c.to_string());
        }
    }
    None
}

/// US-layout letter/digit to key code, the same char set chat text needs.
fn char_to_code(c: char) -> Option<(KeyCode, bool)> {
    let lower = c.to_ascii_lowercase();
    let shifted = c.is_ascii_uppercase();
    let code = match lower {
        'a' => 0,
        's' => 1,
        'd' => 2,
        'f' => 3,
        'h' => 4,
        'g' => 5,
        'z' => 6,
        'x' => 7,
        'c' => 8,
        'v' => 9,
        'b' => 11,
        'q' => 12,
        'w' => 13,
        'e' => 14,
        'r' => 15,
        'y' => 16,
        't' => 17,
        '1' => 18,
        '2' => 19,
        '3' => 20,
        '4' => 21,
        '6' => 22,
        '5' => 23,
        '9' => 25,
        '7' => 26,
        '8' => 28,
        '0' => 29,
        'o' => 31,
        'u' => 32,
        'i' => 34,
        'p' => 35,
        'l' => 37,
        'j' => 38,
        'k' => 40,
        'n' => 45,
        'm' => 46,
        ' ' => 49,
        _ => return None,
    };
    Some((KeyCode(code), shifted))
}

/// is the named key held down right now
pub fn is_key_pressed(key: &str) -> bool {
    let Some(code) = name_to_code(key) else {
        return false;
    };
    // SAFETY: pure C query, no pointers handed over.
    unsafe { CGEventSourceKeyState(HID_STATE, code.0) }
}

/// first named key currently held down
pub fn scan_pressed_key() -> Option<String> {
    named_keys()
        .iter()
        .find(|(_, code)| unsafe { CGEventSourceKeyState(HID_STATE, code.0) })
        .map(|(name, _)| (*name).to_string())
}

/// wait for a key press, returning its name. `None` on timeout.
///
/// Keys already held when the call starts are ignored, so the caller's own
/// shortcut does not come straight back.
pub fn detect_hotkey(timeout: Option<Duration>) -> Option<String> {
    let deadline = timeout.map(|limit| Instant::now() + limit);
    let started = Instant::now();
    // let go of whatever opened the capture
    while scan_pressed_key().is_some() {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = started;
    loop {
        if let Some(key) = scan_pressed_key() {
            return Some(key);
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return None;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn post(code: KeyCode, down: bool) {
    // SAFETY: the event is created and released here; no pointer escapes.
    unsafe {
        let event = CGEventCreateKeyboardEvent(std::ptr::null_mut(), code.0, down);
        if event.is_null() {
            return;
        }
        CGEventPost(HID_TAP, event);
        CFRelease(event);
    }
}

const TAP_GAP: Duration = Duration::from_millis(1);

/// press+release a named key ("enter", "t", "f1", ...)
pub fn tap_key(name: &str) -> bool {
    let Some(code) = name_to_code(name) else {
        return false;
    };
    post(code, true);
    std::thread::sleep(TAP_GAP);
    post(code, false);
    std::thread::sleep(TAP_GAP);
    true
}

/// type ascii text with synthetic key events
pub fn type_text(text: &str) {
    type_text_while(text, || true);
}

/// type ascii text, stopping when `should_continue` turns false between keys
pub fn type_text_while<F>(text: &str, mut should_continue: F) -> bool
where
    F: FnMut() -> bool,
{
    for c in text.chars() {
        if !should_continue() {
            return false;
        }
        let Some((code, shifted)) = char_to_code(c) else {
            continue;
        };
        let shift = KeyCode(56);
        if shifted {
            post(shift, true);
            std::thread::sleep(TAP_GAP);
        }
        post(code, true);
        std::thread::sleep(TAP_GAP);
        post(code, false);
        if shifted {
            std::thread::sleep(TAP_GAP);
            post(shift, false);
        }
        std::thread::sleep(TAP_GAP);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for name in ["tab", "f2", "ctrl", "space", "left", "a", "7"] {
            let code = name_to_code(name).unwrap_or_else(|| panic!("{name} should resolve"));
            assert_eq!(code_to_name(code).as_deref(), Some(name));
        }
    }

    #[test]
    fn unknown_names_do_not_resolve() {
        assert!(name_to_code("not-a-key").is_none());
        assert!(!is_key_pressed("not-a-key"));
    }

    #[test]
    fn shift_is_tracked_for_uppercase() {
        assert_eq!(char_to_code('a'), Some((KeyCode(0), false)));
        assert_eq!(char_to_code('A'), Some((KeyCode(0), true)));
    }
}
