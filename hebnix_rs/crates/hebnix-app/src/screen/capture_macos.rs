//! Screen pixel capture for plugins (`screen.get_pixel` & friends), macOS.
//!
//! Uses CoreGraphics (the same framework the window manager composites with)
//! rather than introducing generated Objective-C bindings. The game window is
//! located through `CGWindowListCopyWindowInfo` (owner/title hinting at Rocket
//! League, Obelisk or its `wsd` host); its pixels are read with
//! `CGWindowListCreateImage`, or the main display with `CGDisplayCreateImage`
//! when no game window can be identified. A `CGBitmapContext` converts either
//! image into tightly packed BGRA8, the layout the shared [`Frame`] expects.
//!
//! Everything is reached through raw `extern "C"` declarations linked straight
//! to the CoreGraphics/CoreFoundation frameworks, so this file needs no new
//! crate dependencies. Screen Recording permission is required to read other
//! apps' windows; when it is missing CoreGraphics yields no image and the
//! worker simply logs once and stays idle instead of publishing wrong pixels.

use std::ffi::c_void;
use std::os::raw::c_char;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::{Frame, IDLE_STOP, Service};

/// How often a missing game window is looked for again.
const FIND_EVERY: Duration = Duration::from_secs(1);
/// Small windows (launchers, helpers) are never the game.
const MIN_SIDE: f64 = 200.0;

// CoreGraphics / CoreFoundation opaque object references.
type Ref = *mut c_void;
type ConstRef = *const c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

// Option/alpha/encoding bit values from CGWindow.h, CGImage.h and CFString.h.
const WINDOW_LIST_ON_SCREEN_ONLY: u32 = 1 << 5;
const WINDOW_LIST_EXCLUDE_DESKTOP: u32 = 1 << 4;
const WINDOW_LIST_INCLUDING_WINDOW: u32 = 1 << 3;
const WINDOW_IMAGE_IGNORE_FRAMING: u32 = 1 << 0;
const WINDOW_IMAGE_SHOULD_BE_OPAQUE: u32 = 1 << 1;
/// `kCGBitmapByteOrder32Little` (2 << 12). With alpha "first" in a little
/// endian word this lays pixels out as B, G, R, A in memory.
const BITMAP_BYTE_ORDER_32_LITTLE: u32 = 2 << 12;
const IMAGE_ALPHA_PREMULTIPLIED_FIRST: u32 = 2;
const CF_STRING_ENCODING_ASCII: u32 = 0x0600;
const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const CF_NUMBER_S_INT32: u32 = 3;
const CF_NUMBER_FLOAT64: u32 = 6;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> Ref;
    fn CGWindowListCreateImage(
        screen_bounds: CGRect,
        window_option: u32,
        window_id: u32,
        image_option: u32,
    ) -> Ref;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayCreateImage(display_id: u32) -> Ref;
    fn CGImageGetWidth(image: ConstRef) -> usize;
    fn CGImageGetHeight(image: ConstRef) -> usize;
    fn CGColorSpaceCreateDeviceRGB() -> Ref;
    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: ConstRef,
        bitmap_info: u32,
    ) -> Ref;
    fn CGContextDrawImage(context: ConstRef, rect: CGRect, image: ConstRef);
    fn CGBitmapContextGetData(context: ConstRef) -> *mut c_void;
    fn CGContextRelease(context: ConstRef);
    fn CGImageRelease(image: ConstRef);
    fn CGColorSpaceRelease(space: ConstRef);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: ConstRef);
    fn CFGetTypeID(cf: ConstRef) -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFArrayGetCount(array: ConstRef) -> isize;
    fn CFArrayGetValueAtIndex(array: ConstRef, index: isize) -> ConstRef;
    fn CFDictionaryGetValue(dict: ConstRef, key: ConstRef) -> ConstRef;
    fn CFStringCreateWithCString(alloc: ConstRef, c_str: *const c_char, encoding: u32) -> Ref;
    fn CFStringGetCString(
        string: ConstRef,
        buffer: *mut c_char,
        buffer_size: isize,
        encoding: u32,
    ) -> u8;
    fn CFNumberGetValue(number: ConstRef, number_type: u32, value_ptr: *mut c_void) -> u8;
}

/// A rectangle covering any plausible window/display position, used as the
/// `screenBounds` argument so the whole window is captured.
fn covering_bounds() -> CGRect {
    CGRect {
        origin: CGPoint {
            x: -1.0e7,
            y: -1.0e7,
        },
        size: CGSize {
            width: 2.0e7,
            height: 2.0e7,
        },
    }
}

unsafe fn make_key(name: &[u8]) -> Ref {
    unsafe {
        CFStringCreateWithCString(
            std::ptr::null(),
            name.as_ptr().cast(),
            CF_STRING_ENCODING_ASCII,
        )
    }
}

unsafe fn cf_text(value: ConstRef) -> Option<String> {
    if value.is_null() || unsafe { CFGetTypeID(value) } != unsafe { CFStringGetTypeID() } {
        return None;
    }
    let mut buf = [0 as c_char; 512];
    let ok = unsafe {
        CFStringGetCString(
            value,
            buf.as_mut_ptr(),
            buf.len() as isize,
            CF_STRING_ENCODING_UTF8,
        )
    };
    if ok == 0 {
        return None;
    }
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as u8)
        .collect();
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

unsafe fn cf_i32(value: ConstRef) -> Option<i32> {
    if value.is_null() || unsafe { CFGetTypeID(value) } != unsafe { CFNumberGetTypeID() } {
        return None;
    }
    let mut out: i32 = 0;
    let ok = unsafe { CFNumberGetValue(value, CF_NUMBER_S_INT32, (&mut out as *mut i32).cast()) };
    (ok != 0).then_some(out)
}

unsafe fn cf_f64(value: ConstRef) -> Option<f64> {
    if value.is_null() || unsafe { CFGetTypeID(value) } != unsafe { CFNumberGetTypeID() } {
        return None;
    }
    let mut out: f64 = 0.0;
    let ok = unsafe { CFNumberGetValue(value, CF_NUMBER_FLOAT64, (&mut out as *mut f64).cast()) };
    (ok != 0).then_some(out)
}

/// Owner/title hints that identify the game. Obelisk runs Rocket League through
/// its `wsd` host, whose window may be named after either one.
fn looks_like_game(owner: &str, title: &str) -> bool {
    let owner = owner.to_lowercase();
    let title = title.to_lowercase();
    owner.contains("rocket league")
        || owner.contains("rocketleague")
        || owner.contains("obelisk")
        || owner.starts_with("wsd")
        || title.contains("rocket league")
        || title.contains("steam_app_252950")
}

/// Largest on-screen window that looks like the game, or `None` to fall back
/// to capturing the whole display.
unsafe fn find_game_window() -> Option<u32> {
    unsafe {
        let list = CGWindowListCopyWindowInfo(
            WINDOW_LIST_ON_SCREEN_ONLY | WINDOW_LIST_EXCLUDE_DESKTOP,
            0,
        );
        if list.is_null() || CFGetTypeID(list) != CFArrayGetTypeID() {
            if !list.is_null() {
                CFRelease(list);
            }
            return None;
        }

        let keys = [
            make_key(b"kCGWindowOwnerName\0"),
            make_key(b"kCGWindowName\0"),
            make_key(b"kCGWindowNumber\0"),
            make_key(b"kCGWindowLayer\0"),
            make_key(b"kCGWindowBounds\0"),
            make_key(b"Width\0"),
            make_key(b"Height\0"),
        ];
        if keys.iter().any(|key| key.is_null()) {
            for key in keys {
                if !key.is_null() {
                    CFRelease(key);
                }
            }
            CFRelease(list);
            return None;
        }
        let (k_owner, k_title, k_id, k_layer, k_bounds, k_width, k_height) =
            (keys[0], keys[1], keys[2], keys[3], keys[4], keys[5], keys[6]);

        let mut best: Option<(u32, f64)> = None;
        let count = CFArrayGetCount(list);
        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(list, i);
            if dict.is_null() || CFGetTypeID(dict) != CFDictionaryGetTypeID() {
                continue;
            }
            let owner = cf_text(CFDictionaryGetValue(dict, k_owner)).unwrap_or_default();
            let title = cf_text(CFDictionaryGetValue(dict, k_title)).unwrap_or_default();
            if !looks_like_game(&owner, &title) {
                continue;
            }
            // layer 0 is a normal window; ignore menus, overlays and desktop
            if cf_i32(CFDictionaryGetValue(dict, k_layer)) != Some(0) {
                continue;
            }
            let bounds = CFDictionaryGetValue(dict, k_bounds);
            if bounds.is_null() || CFGetTypeID(bounds) != CFDictionaryGetTypeID() {
                continue;
            }
            let width = cf_f64(CFDictionaryGetValue(bounds, k_width)).unwrap_or(0.0);
            let height = cf_f64(CFDictionaryGetValue(bounds, k_height)).unwrap_or(0.0);
            let area = width * height;
            if width < MIN_SIDE || height < MIN_SIDE {
                continue;
            }
            if best.is_none_or(|(_, best_area)| area > best_area) {
                if let Some(id) = cf_i32(CFDictionaryGetValue(dict, k_id)) {
                    if id != 0 {
                        best = Some((id as u32, area));
                    }
                }
            }
        }

        for key in keys {
            if !key.is_null() {
                CFRelease(key);
            }
        }
        CFRelease(list);
        best.map(|(id, _)| id)
    }
}

/// Draw a CGImage into a BGRA8 bitmap context and copy the pixels out.
unsafe fn image_to_bgra(image: ConstRef) -> Result<(u32, u32, Vec<u8>), String> {
    unsafe {
        let width = CGImageGetWidth(image);
        let height = CGImageGetHeight(image);
        if width == 0 || height == 0 {
            return Err("the captured image is empty".into());
        }

        let space = CGColorSpaceCreateDeviceRGB();
        if space.is_null() {
            return Err("could not create an RGB color space".into());
        }
        let bitmap_info = BITMAP_BYTE_ORDER_32_LITTLE | IMAGE_ALPHA_PREMULTIPLIED_FIRST;
        let context = CGBitmapContextCreate(
            std::ptr::null_mut(),
            width,
            height,
            8,
            width * 4,
            space,
            bitmap_info,
        );
        CGColorSpaceRelease(space);
        if context.is_null() {
            return Err("could not create a BGRA bitmap context".into());
        }

        let rect = CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: width as f64,
                height: height as f64,
            },
        };
        CGContextDrawImage(context, rect, image);

        let data = CGBitmapContextGetData(context);
        if data.is_null() {
            CGContextRelease(context);
            return Err("the bitmap context returned no pixel data".into());
        }
        let mut bgra = vec![0u8; width * height * 4];
        std::ptr::copy_nonoverlapping(data as *const u8, bgra.as_mut_ptr(), bgra.len());
        // Window captures are opaque; make the alpha byte deterministic.
        for pixel in bgra.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        CGContextRelease(context);

        Ok((width as u32, height as u32, bgra))
    }
}

struct Capture {
    /// CGWindowID of the game window, re-resolved periodically. `None` means
    /// capture the main display as a fallback.
    window_id: Option<u32>,
    found_at: Instant,
}

impl Capture {
    fn new() -> Self {
        Self {
            window_id: None,
            found_at: Instant::now() - FIND_EVERY,
        }
    }

    /// One frame, or `None` when CoreGraphics has no image for us yet (missing
    /// permission, window minimised, no display).
    fn grab(&mut self) -> Result<Option<Frame>, String> {
        if self.window_id.is_none() && self.found_at.elapsed() >= FIND_EVERY {
            self.found_at = Instant::now();
            self.window_id = unsafe { find_game_window() };
        }

        let image = unsafe {
            match self.window_id {
                Some(id) => CGWindowListCreateImage(
                    covering_bounds(),
                    WINDOW_LIST_INCLUDING_WINDOW,
                    id,
                    WINDOW_IMAGE_IGNORE_FRAMING | WINDOW_IMAGE_SHOULD_BE_OPAQUE,
                ),
                None => CGDisplayCreateImage(CGMainDisplayID()),
            }
        };
        if image.is_null() {
            // Re-resolve the window; the display may also be temporarily gone.
            self.window_id = None;
            return Ok(None);
        }

        let result = unsafe { image_to_bgra(image) };
        unsafe { CGImageRelease(image) };
        let (width, height, bgra) = result?;
        Ok(Some(Frame {
            width,
            height,
            bgra,
            // No window-rect source exists on macOS yet (the SDK returns
            // None), so overlay coordinates already match capture coordinates.
            offset_x: 0,
            offset_y: 0,
            captured_at: Instant::now(),
        }))
    }
}

/// Runs on the capture thread started by [`Service::touch`].
pub fn run_worker(service: &crate::screen::Service) {
    let mut capture: Option<Capture> = None;
    let mut last_grab = Instant::now() - Duration::from_secs(60);
    let mut warned = false;

    loop {
        if service.idle_for() > IDLE_STOP {
            drop(capture.take());
            service.clear_frame();
            service.worker_running.store(false, Ordering::Release);
            // A request between the idle check and the store above saw the
            // worker as running and didn't spawn one, so pick it up here.
            if service.idle_for() <= IDLE_STOP
                && !service.worker_running.swap(true, Ordering::AcqRel)
            {
                continue;
            }
            return;
        }

        if capture.is_none() {
            capture = Some(Capture::new());
        }

        if last_grab.elapsed() >= service.frame_interval() {
            last_grab = Instant::now();
            if let Some(c) = capture.as_mut() {
                match c.grab() {
                    Ok(Some(frame)) => {
                        service.publish(frame);
                        warned = false;
                    }
                    Ok(None) => {
                        if !warned {
                            warned = true;
                            service.log(
                                "[Screen] No capturable window or display. Grant Screen Recording permission to Hebnix if this persists; screen reading stays unavailable until then.",
                            );
                        }
                    }
                    Err(e) => {
                        if !warned {
                            warned = true;
                            service.log(&format!("[Screen] macOS screen capture failed: {e}"));
                        }
                        service.clear_frame();
                    }
                }
            }
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}
