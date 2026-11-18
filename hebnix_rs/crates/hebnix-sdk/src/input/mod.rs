//! keyboard, xinput (xbox) + generic dinput (ps4/ps5/8bitdo/etc, via gilrs) input detection.
//!
//! `gilrs` covers the same controllers on Linux and macOS, so those two backends
//! share one implementation; only the keyboard differs per platform.

pub mod bindings;
#[cfg(target_os = "windows")]
pub mod dinput;
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "dinput_gilrs.rs"]
pub mod dinput;
#[cfg(target_os = "windows")]
pub mod game_actions;
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "game_actions_portable.rs"]
pub mod game_actions;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod gilrs_hub;
#[cfg(target_os = "windows")]
pub mod keyboard;
#[cfg(target_os = "linux")]
#[path = "keyboard_linux.rs"]
pub mod keyboard;
#[cfg(target_os = "macos")]
#[path = "keyboard_macos.rs"]
pub mod keyboard;
#[cfg(target_os = "windows")]
pub mod xinput;
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "xinput_gilrs.rs"]
pub mod xinput;

pub use bindings::{
    ControllerType, HotkeyBind, bind_to_string, detect_any_hotkey, dinput_name_to_xinput_mask,
    get_button_display, init_controllers, is_bind_pressed, is_hotkey_pressed, resolve_bind_string,
    xinput_mask_to_dinput_name,
};
pub use dinput::{
    DINPUT_BUTTONS, Layout as DInputLayout, PsGen, current_dinput_layout, current_ps_gen,
    get_dinput_inputs, get_dinput_raw_inputs, is_dinput_connected, is_dinput_raw_pressed,
};
pub use game_actions::{
    action_binds, chat_channel_bind, clear_action_bind_cache, is_action_pressed, ui_scale,
};
pub use keyboard::{detect_hotkey, is_key_pressed, tap_key, type_text, type_text_while};
pub use xinput::{XINPUT_BUTTON_DISPLAY, XInputState, get_xinput_state};
