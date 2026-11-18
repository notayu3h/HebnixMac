//! macOS overlay: API-compatible with the Windows and Linux backends, but the
//! drawing surface is not implemented yet.
//!
//! The real target is an `NSPanel` (borderless, non-activating, `.screenSaver`
//! or `.statusBar` level) containing a transparent layer, composited above
//! `wsd`'s Metal surface. That needs `objc2`/`objc2-app-kit` bindings, so it
//! is a separate piece of work from this file.
//!
//! Until then every draw call is a no-op and the overlay reports itself as
//! hidden, which is what the callers in the egui UI already handle (they draw
//! into the app window when the overlay has no render focus). `measure_text`
//! returns an estimate so layouts that size text still work; it is not
//! pixel-accurate.
#![allow(clippy::too_many_arguments)]

pub mod gif;

/// rgba color, straight (non-premultiplied) alpha 0-255
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

// Drawing primitives called from the Lua `draw` table.

pub fn line(_x1: f32, _y1: f32, _x2: f32, _y2: f32, _color: Rgba, _width: f32) {}

pub fn rect(_x: f32, _y: f32, _w: f32, _h: f32, _color: Rgba, _width: f32, _filled: bool) {}

pub fn rect_styled(
    _x: f32,
    _y: f32,
    _w: f32,
    _h: f32,
    _fill: Rgba,
    _border: Rgba,
    _width: f32,
    _filled: bool,
    _radius: f32,
) {
}

pub fn gradient(
    _x: f32,
    _y: f32,
    _w: f32,
    _h: f32,
    _c1: Rgba,
    _c2: Rgba,
    _radius: f32,
    _angle: f32,
) {
}

pub fn circle(_x: f32, _y: f32, _radius: f32, _color: Rgba, _width: f32, _filled: bool) {}

pub fn text(
    _x: f32,
    _y: f32,
    _s: &str,
    _color: Rgba,
    _size: f32,
    _halign: &str,
    _font: &str,
    _bold: bool,
    _clip: Option<(f32, f32)>,
) {
}

pub fn polygon(_points: &[(f32, f32)], _color: Rgba) {}

pub fn image(_path: &str, _x: f32, _y: f32, _w: f32, _h: f32, _opacity: f32, _radius: f32) {}

pub fn capture_image(
    _handle: u64,
    _frame: std::sync::Arc<crate::plugins::window_capture::CapturedFrame>,
    _x: f32,
    _y: f32,
    _w: f32,
    _h: f32,
    _opacity: f32,
) {
}

pub fn set_font(_path: Option<&std::path::Path>, _weight: f32, _bold_weight: f32) {}

/// Rough advance width for a string at `size`px. Upper bound for most text,
/// which keeps right-aligned plugin layouts inside their boxes.
pub fn measure_text(s: &str, size: f32, bold: bool) -> f32 {
    let per_char = if bold { 0.62 } else { 0.56 };
    let mut width = 0.0;
    for ch in s.chars() {
        width += match ch {
            'i' | 'l' | 'j' | '.' | ',' | '\'' | '!' | '|' => 0.32,
            'm' | 'w' | 'M' | 'W' => 0.95,
            ' ' => 0.30,
            _ => per_char,
        } * size;
    }
    width
}

/// Whether a plugin page may load a media URL. No overlay page can render
/// yet, so nothing is allowed; this matches the Linux backend.
pub fn media_host_allowed(_uri: &str) -> bool {
    false
}

pub fn set_allow_draw_on_hebnix_focus(_allow: bool) {}

pub fn set_webview_clickable(_clickable: bool) {}

pub fn webview_clickable() -> bool {
    false
}

pub fn has_render_focus() -> bool {
    false
}

/// Platform overlay handle. Nothing owns a surface yet, so placement always
/// fails and callers keep their "no overlay" path.
pub struct NativeOverlay;

impl NativeOverlay {
    pub fn new() -> Self {
        Self
    }

    pub fn frame(&mut self, _rect: (i32, i32, i32, i32), _draw_fn: impl FnOnce(f32, f32)) {}

    pub fn hide(&mut self) {}
}

impl Default for NativeOverlay {
    fn default() -> Self {
        Self::new()
    }
}

static FORCE_HIDE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Ask the main loop to hide the overlay; safe from any thread.
pub fn enforce_hidden() {
    FORCE_HIDE.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Consumed by the main loop each tick; true means `NativeOverlay::hide()`
/// should be called (and the flag is cleared).
#[doc(hidden)]
pub fn take_force_hide() -> bool {
    FORCE_HIDE.swap(false, std::sync::atomic::Ordering::Relaxed)
}

pub mod native {
    pub use super::NativeOverlay;
}

/// HTML plugin pages are hosted in a native child window on Windows. macOS
/// would need a `WKWebView` in the panel; until that exists this reports no
/// target, so plugin pages stay off rather than rendering nowhere.
pub struct Overlay;

impl Overlay {
    pub fn new() -> Self {
        Self
    }

    pub fn webview_target(&self) -> Option<((), ())> {
        // The macOS WKWebView lives in its own floating NSPanel and needs no
        // game-window child target: giving the host a target lets plugin
        // overlays actually start. The panel places itself via WebviewHost::tick.
        Some(((), ()))
    }

    pub fn commit(&self) {}

    pub fn place(&mut self, _rect: (i32, i32, i32, i32)) -> bool {
        false
    }

    pub fn hide(&mut self) {}
}

impl Default for Overlay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_text_grows_with_length_and_size() {
        assert!(measure_text("hello", 20.0, false) > measure_text("hi", 20.0, false));
        assert!(measure_text("hello", 40.0, false) > measure_text("hello", 20.0, false));
        assert_eq!(measure_text("", 20.0, false), 0.0);
    }

    #[test]
    fn nothing_renders_yet() {
        assert!(!has_render_focus());
        assert!(!webview_clickable());
        assert!(Overlay::new().webview_target().is_some());
        assert!(!Overlay::new().place((0, 0, 10, 10)));
    }

    #[test]
    fn force_hide_is_consumed_once() {
        enforce_hidden();
        assert!(take_force_hide());
        assert!(!take_force_hide());
    }
}
