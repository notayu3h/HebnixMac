//! HTML plugin overlays ("webview" pages), macOS backend.
//!
//! This is the macOS equivalent of `webview_linux.rs`: a single, transparent,
//! floating `NSPanel` hosts one `WKWebView` whose page is a small generated
//! shell containing one `<iframe>` per plugin that sets `overlay_page`. The
//! panel is non-activating, borderless and click-through (the game keeps the
//! mouse); plugin pages are purely visual HUD content, as on Linux.
//!
//! The JS bridge mirrors the Windows/Linux host shell:
//!   * the host page listens for `{__hebnixPointerHit, region}` `message`
//!     events from the iframes and forwards them to native via
//!     `window.webkit.messageHandlers.hebnixptr.postMessage(...)`. The native
//!     side registers a `WKScriptMessageHandler` named `hebnixptr` and records
//!     the reported regions (used to decide whether a page wants input).
//!   * Rust -> JS uses `evaluateJavaScript` to call
//!     `window.__hebnixDeliver(slug, data)`, which `postMessage`s into the
//!     matching iframe.
//!
//! Implementation note: the app owns the process main run loop through
//! winit/eframe, so `WKWebView` is naturally serviced by that loop (no
//! `NSApplication::run` here). Everything is done with `objc2` runtime message
//! sending (`msg_send!`) rather than generated bindings, so it does not depend
//! on the exact version of any `objc2-*` framework crate. Framework symbols
//! are linked directly below. All Cocoa interaction is main-thread only; the
//! shared state behind `GLOBAL` is a mutex and JS that arrives off-thread is
//! queued and flushed by `WebviewOverlay::pump()`.

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use objc2::encode::{Encode, Encoding};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{
    ClassType, DeclaredClass, ProtocolType, class, declare_class, extern_protocol, msg_send,
    msg_send_id, mutability, sel,
};

// Link the frameworks we message. winit already brings in AppKit/Foundation,
// but WebKit is not linked unless something (normally objc2-web-kit) asks for
// it; without this `class!(WKWebView)` would resolve to nil at runtime.
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}
#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {}
#[link(name = "WebKit", kind = "framework")]
unsafe extern "C" {}

/// (slug, overlay_page filename, plugin's own assets dir, needs input)
pub type PageLayer = (String, Option<String>, PathBuf, bool);

// ---------------------------------------------------------------------------
// CoreGraphics geometry (CGFloat is f64 on 64-bit macOS).
// ---------------------------------------------------------------------------

#[cfg(target_pointer_width = "64")]
type CGFloat = f64;
#[cfg(target_pointer_width = "32")]
type CGFloat = f32;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: CGFloat,
    y: CGFloat,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: CGFloat,
    height: CGFloat,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

// SAFETY: repr(C), encodings match the C declarations.
unsafe impl Encode for CGPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[CGFloat::ENCODING, CGFloat::ENCODING]);
}
unsafe impl Encode for CGSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[CGFloat::ENCODING, CGFloat::ENCODING]);
}
unsafe impl Encode for CGRect {
    const ENCODING: Encoding = Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
}

impl CGRect {
    fn zero() -> Self {
        Self {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: 0.0,
                height: 0.0,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Shared state. There is one webview/panel per process, and the Objective-C
// delegates need to reach the same data the Rust handles do.
// ---------------------------------------------------------------------------

/// Raw Objective-C object pointer. All sends happen on the main thread; the
/// `unsafe impl Send/Sync` only exists so it can live in the global mutex.
#[derive(Clone, Copy)]
struct ObjPtr(*mut AnyObject);
unsafe impl Send for ObjPtr {}
unsafe impl Sync for ObjPtr {}

#[derive(Default)]
struct Global {
    panel: Option<ObjPtr>,
    webview: Option<ObjPtr>,
    config: Option<ObjPtr>,
    /// Kept alive explicitly: WKWebView holds its navigation delegate weakly.
    nav_delegate: Option<ObjPtr>,
    /// WKUserContentController retains script handlers, but keep it anyway.
    handler: Option<ObjPtr>,
    error: Option<String>,
    /// Pointer-hit regions reported by plugin pages, keyed by escaped slug.
    regions: HashMap<String, [f64; 4]>,
    pages: Vec<PageLayer>,
    visible: bool,
    ready: bool,
    /// JS queued from a non-main thread, flushed by `pump()`.
    pending_js: Vec<String>,
}

static GLOBAL: OnceLock<Mutex<Global>> = OnceLock::new();

fn global() -> &'static Mutex<Global> {
    GLOBAL.get_or_init(|| Mutex::new(Global::default()))
}

fn webview_ptr() -> Option<*mut AnyObject> {
    global().lock().ok().and_then(|g| g.webview.map(|p| p.0))
}

fn panel_ptr() -> Option<*mut AnyObject> {
    global().lock().ok().and_then(|g| g.panel.map(|p| p.0))
}

fn is_main_thread() -> bool {
    // msg_send! converts the Objective-C BOOL return to a Rust bool.
    unsafe { msg_send![class!(NSThread), isMainThread] }
}

// ---------------------------------------------------------------------------
// Objective-C delegate classes.
// ---------------------------------------------------------------------------

extern_protocol!(
    unsafe trait HebnixScriptMessageHandler: NSObjectProtocol {
        #[method(userContentController:didReceiveScriptMessage:)]
        fn user_content_controller_did_receive_script_message(
            &self,
            controller: &AnyObject,
            message: &AnyObject,
        );
    }

    unsafe impl ProtocolType for dyn HebnixScriptMessageHandler {
        const NAME: &'static str = "WKScriptMessageHandler";
    }
);

extern_protocol!(
    unsafe trait HebnixNavigationDelegate: NSObjectProtocol {
        #[method(webView:didFailLoadWithError:)]
        fn web_view_did_fail_load_with_error(
            &self,
            web_view: &AnyObject,
            error: *const AnyObject,
        );
    }

    unsafe impl ProtocolType for dyn HebnixNavigationDelegate {
        const NAME: &'static str = "WKNavigationDelegate";
    }
);

declare_class!(
    struct HebnixMessageHandler;

    unsafe impl ClassType for HebnixMessageHandler {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "HebnixWKScriptMessageHandler";
    }

    impl DeclaredClass for HebnixMessageHandler {}

    unsafe impl HebnixScriptMessageHandler for HebnixMessageHandler {
        #[method(userContentController:didReceiveScriptMessage:)]
        fn user_content_controller_did_receive_script_message(
            &self,
            _controller: &AnyObject,
            message: &AnyObject,
        ) {
            let body: *mut AnyObject = unsafe { msg_send![message, body] };
            let Some(value) = (unsafe { body_to_json(body) }) else {
                return;
            };
            let Some(slug) = value.get("slug").and_then(|v| v.as_str()) else {
                return;
            };
            let Ok(mut g) = global().lock() else {
                return;
            };
            match value.get("region").filter(|r| r.is_object()) {
                Some(region) => {
                    let side = |key: &str| region.get(key).and_then(|v| v.as_f64());
                    if let (Some(left), Some(top), Some(right), Some(bottom)) =
                        (side("left"), side("top"), side("right"), side("bottom"))
                    {
                        g.regions
                            .insert(slug.to_string(), [left, top, right, bottom]);
                    }
                }
                None => {
                    g.regions.remove(slug);
                }
            }
        }
    }
);

unsafe impl NSObjectProtocol for HebnixMessageHandler {}

declare_class!(
    struct HebnixNavDelegate;

    unsafe impl ClassType for HebnixNavDelegate {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "HebnixWKNavigationDelegate";
    }

    impl DeclaredClass for HebnixNavDelegate {}

    unsafe impl HebnixNavigationDelegate for HebnixNavDelegate {
        #[method(webView:didFailLoadWithError:)]
        fn web_view_did_fail_load_with_error(
            &self,
            _web_view: &AnyObject,
            error: *const AnyObject,
        ) {
            if error.is_null() {
                return;
            }
            let description: *mut AnyObject = unsafe { msg_send![error, localizedDescription] };
            let text = unsafe { ns_to_string(description) };
            let text = if text.is_empty() {
                "unknown WebKit load error".to_string()
            } else {
                text
            };
            let message = format!("html overlay: webview load failed: {text}");
            tracing::warn!(%text, "html overlay: load failed");
            if let Ok(mut g) = global().lock() {
                g.error = Some(message);
            }
        }
    }
);

unsafe impl NSObjectProtocol for HebnixNavDelegate {}

// ---------------------------------------------------------------------------
// NSString / JSON helpers.
// ---------------------------------------------------------------------------

/// Build an `NSString` from a Rust string.
fn ns_string(text: &str) -> Retained<AnyObject> {
    let c_string = CString::new(text).unwrap_or_else(|_| CString::new("").unwrap());
    let result: Retained<AnyObject> = unsafe {
        msg_send_id![class!(NSString), stringWithUTF8String: c_string.as_ptr()]
    };
    result
}

/// Read an `NSString` (or anything responding to `UTF8String`) as a Rust string.
unsafe fn ns_to_string(object: *const AnyObject) -> String {
    if object.is_null() {
        return String::new();
    }
    let bytes: *const c_char = unsafe { msg_send![object, UTF8String] };
    if bytes.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(bytes) }.to_string_lossy().into_owned()
}

/// WKScriptMessage bodies are either an `NSString` (our host page posts a JSON
/// string) or a JS object bridged as `NSDictionary`; handle both.
unsafe fn body_to_json(body: *const AnyObject) -> Option<serde_json::Value> {
    if body.is_null() {
        return None;
    }
    let is_string: bool = unsafe { msg_send![body, isKindOfClass: class!(NSString)] };
    let text = if is_string {
        unsafe { ns_to_string(body) }
    } else {
        let data: *mut AnyObject = unsafe {
            msg_send![
                class!(NSJSONSerialization),
                dataWithJSONObject: body,
                options: 0u64,
                error: std::ptr::null_mut::<*mut AnyObject>(),
            ]
        };
        if data.is_null() {
            return None;
        }
        let bytes: *const u8 = unsafe { msg_send![data, bytes] };
        let length: usize = unsafe { msg_send![data, length] };
        if bytes.is_null() {
            return None;
        }
        String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(bytes, length) }).into_owned()
    };
    serde_json::from_str(&text).ok()
}

// ---------------------------------------------------------------------------
// URL / HTML generation.
// ---------------------------------------------------------------------------

fn file_url(path: &Path) -> String {
    url::Url::from_file_path(path)
        .map(|url| url.to_string())
        .unwrap_or_else(|_| format!("file://{}", path.display()))
}

/// `overlay_page` may carry a query string or fragment; only the path part is
/// a filename (mirrors `webview_linux.rs::page_url`).
fn page_url(assets_dir: &Path, page: &str) -> String {
    let split = page.find(['?', '#']).unwrap_or(page.len());
    let (name, suffix) = page.split_at(split);
    format!("{}{suffix}", file_url(&assets_dir.join(name)))
}

fn html_escape(slug: &str) -> String {
    slug.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

fn json_escape(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

const EMPTY_HTML: &str = "<!doctype html><html><body></body></html>";

fn build_html(pages: &[PageLayer]) -> String {
    let mut iframes = String::new();
    for (slug, page, assets_dir, _clickable) in pages {
        let Some(page) = page else { continue };
        let src = page_url(assets_dir, page);
        let id = html_escape(slug);
        tracing::info!(slug, src, "html overlay: iframe src");
        iframes.push_str(&format!(
            "<iframe id=\"frame-{id}\" data-slug=\"{id}\" src=\"{src}\" \
             style=\"position:absolute;inset:0;width:100%;height:100%;border:0;\
             pointer-events:none;background:transparent;\"></iframe>\n"
        ));
    }
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><style>
html,body{{margin:0;padding:0;background:transparent;overflow:hidden;}}
iframe{{pointer-events:none;background:transparent;}}
</style></head><body>{iframes}<script>
window.addEventListener('message', function(e) {{
  var d = e.data;
  if (!d || !d.__hebnixPointerHit) return;
  var frames = document.querySelectorAll('iframe');
  for (var i = 0; i < frames.length; i++) {{
    if (frames[i].contentWindow === e.source) {{
      if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.hebnixptr) {{
        window.webkit.messageHandlers.hebnixptr.postMessage(JSON.stringify({{
          slug: frames[i].getAttribute('data-slug') || '',
          region: d.region || null
        }}));
      }}
      return;
    }}
  }}
}});
window.__hebnixDeliver = function(slug, data) {{
  var el = document.getElementById('frame-' + slug);
  if (el && el.contentWindow) el.contentWindow.postMessage(data, '*');
}};
</script></body></html>"#
    )
}

// ---------------------------------------------------------------------------
// Native webview lifecycle.
// ---------------------------------------------------------------------------

/// Panel style mask: borderless + non-activating (`NSNonactivatingPanelMask`).
const PANEL_STYLE: u64 = 1 << 7;
/// `NSBackingStoreBuffered`.
const BACKING_BUFFERED: u64 = 2;
/// High enough to sit above a fullscreen game (`kCGScreenSaverWindowLevel`).
const OVERLAY_LEVEL: isize = 1000;
/// `NSWindowCollectionBehaviorCanJoinAllSpaces | FullScreenAuxiliary | Stationary`.
const COLLECTION_BEHAVIOR: u64 = (1 << 0) | (1 << 8) | (1 << 4);
const AUTORESIZE_WIDTH_HEIGHT: usize = (1 << 1) | (1 << 4);

fn create() -> Result<(), String> {
    // Make sure the shared application exists before creating windows.
    let _: *mut AnyObject = unsafe { msg_send![class!(NSApplication), sharedApplication] };

    let screen: *mut AnyObject = unsafe { msg_send![class!(NSScreen), mainScreen] };
    let frame = if screen.is_null() {
        CGRect::zero()
    } else {
        unsafe { msg_send![screen, frame] }
    };

    // Floating, non-activating, borderless, transparent, click-through panel.
    let panel: Retained<AnyObject> = unsafe {
        let allocated: Allocated<AnyObject> = msg_send_id![class!(NSPanel), alloc];
        msg_send_id![
            allocated,
            initWithContentRect: frame,
            styleMask: PANEL_STYLE,
            backing: BACKING_BUFFERED,
            defer: false,
        ]
    };
    unsafe {
        let _: () = msg_send![&*panel, setLevel: OVERLAY_LEVEL];
        let _: () = msg_send![&*panel, setOpaque: false];
        let _: () = msg_send![&*panel, setHasShadow: false];
        let _: () = msg_send![&*panel, setIgnoresMouseEvents: true];
        let _: () = msg_send![&*panel, setFloatingPanel: true];
        let _: () = msg_send![&*panel, setBecomesKeyOnlyIfNeeded: true];
        let _: () = msg_send![&*panel, setCollectionBehavior: COLLECTION_BEHAVIOR];
    }
    let clear_color: *mut AnyObject = unsafe { msg_send![class!(NSColor), clearColor] };
    let _: () = unsafe { msg_send![&*panel, setBackgroundColor: clear_color] };

    let config: Retained<AnyObject> = unsafe { msg_send_id![class!(WKWebViewConfiguration), new] };
    // WKPreferences has private KVC-only toggles for local file access. Guard
    // with respondsToSelector: so we never message an unknown selector.
    let preferences: *mut AnyObject = unsafe { msg_send![&*config, preferences] };
    if !preferences.is_null() {
        if unsafe { msg_send![preferences, respondsToSelector: sel!(setAllowFileAccessFromFileURLs:)] } {
            let _: () = unsafe { msg_send![preferences, setAllowFileAccessFromFileURLs: true] };
        }
        if unsafe { msg_send![preferences, respondsToSelector: sel!(setAllowUniversalAccessFromFileURLs:)] } {
            let _: () = unsafe { msg_send![preferences, setAllowUniversalAccessFromFileURLs: true] };
        }
    }

    // Register the `hebnixptr` script message handler.
    let handler: Retained<AnyObject> = unsafe { msg_send_id![HebnixMessageHandler::class(), new] };
    let user_content_controller: *mut AnyObject = unsafe { msg_send![&*config, userContentController] };
    let handler_name = ns_string("hebnixptr");
    if !user_content_controller.is_null() {
        let _: () = unsafe {
            msg_send![
                user_content_controller,
                addScriptMessageHandler: &*handler,
                forName: &*handler_name,
            ]
        };
    }

    let nav_delegate: Retained<AnyObject> = unsafe { msg_send_id![HebnixNavDelegate::class(), new] };

    let content_view: *mut AnyObject = unsafe { msg_send![&*panel, contentView] };
    let bounds: CGRect = if content_view.is_null() {
        CGRect::zero()
    } else {
        unsafe { msg_send![content_view, bounds] }
    };
    let webview: Retained<AnyObject> = unsafe {
        let allocated: Allocated<AnyObject> = msg_send_id![class!(WKWebView), alloc];
        msg_send_id![
            allocated,
            initWithFrame: bounds,
            configuration: &*config,
        ]
    };
    unsafe {
        if msg_send![&*webview, respondsToSelector: sel!(setDrawsBackground:)] {
            let _: () = msg_send![&*webview, setDrawsBackground: false];
        }
        // navigationDelegate is weak on WKWebView; the global keeps it alive.
        let _: () = msg_send![&*webview, setNavigationDelegate: &*nav_delegate];
        let _: () = msg_send![&*webview, setAutoresizingMask: AUTORESIZE_WIDTH_HEIGHT];
        if !content_view.is_null() {
            let _: () = msg_send![content_view, addSubview: &*webview];
        }
    }

    // Hand ownership to the global singleton (intentionally leaked for the
    // lifetime of the process).
    {
        let mut g = global().lock().map_err(|e| format!("overlay lock poisoned: {e}"))?;
        g.panel = Some(ObjPtr(Retained::into_raw(panel)));
        g.webview = Some(ObjPtr(Retained::into_raw(webview)));
        g.config = Some(ObjPtr(Retained::into_raw(config)));
        g.handler = Some(ObjPtr(Retained::into_raw(handler)));
        g.nav_delegate = Some(ObjPtr(Retained::into_raw(nav_delegate)));
        g.ready = true;
    }

    tracing::info!("html overlay: WKWebView panel created");
    Ok(())
}

fn eval_js(script: &str) -> Result<(), String> {
    let Some(webview) = webview_ptr() else {
        return Err("html overlay: webview not created yet".to_string());
    };
    let script_ns = ns_string(script);
    // completionHandler is nullable on macOS; passing nil is supported.
    let no_completion: *const c_void = std::ptr::null();
    let _: () = unsafe {
        msg_send![
            webview,
            evaluateJavaScript: &*script_ns,
            completionHandler: no_completion,
        ]
    };
    Ok(())
}

fn set_visible_impl(visible: bool) {
    let Some(panel) = panel_ptr() else { return };
    let sender: *const AnyObject = std::ptr::null();
    unsafe {
        if visible {
            let _: () = msg_send![panel, orderFront: sender];
        } else {
            let _: () = msg_send![panel, orderOut: sender];
        }
    }
    if let Ok(mut g) = global().lock() {
        g.visible = visible;
    }
}

fn resize_impl(width: u32, height: u32) {
    let (panel, webview) = {
        let Ok(g) = global().lock() else { return };
        (g.panel.map(|p| p.0), g.webview.map(|p| p.0))
    };
    let (Some(panel), Some(webview)) = (panel, webview) else {
        return;
    };
    let frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: width as CGFloat,
            height: height as CGFloat,
        },
    };
    unsafe {
        let _: () = msg_send![panel, setFrame: frame, display: true];
        let content_view: *mut AnyObject = msg_send![panel, contentView];
        if !content_view.is_null() {
            let bounds: CGRect = msg_send![content_view, bounds];
            let _: () = msg_send![webview, setFrame: bounds];
        }
    }
}

fn load_html(html: &str) {
    let Some(webview) = webview_ptr() else { return };
    let html_ns = ns_string(html);
    let base_url = ns_string("file:///");
    let _: () = unsafe { msg_send![webview, loadHTMLString: &*html_ns, baseURL: &*base_url] };
}

fn sync_pages_impl(pages: &[PageLayer]) {
    {
        let Ok(mut g) = global().lock() else { return };
        if g.webview.is_some() && g.pages.as_slice() == pages {
            return;
        }
        g.pages = pages.to_vec();
    }

    if pages.is_empty() {
        if webview_ptr().is_some() {
            load_html(EMPTY_HTML);
        }
        set_visible_impl(false);
        return;
    }

    if webview_ptr().is_none() {
        if let Err(error) = create() {
            if let Ok(mut g) = global().lock() {
                g.error = Some(error);
            }
            return;
        }
    }
    load_html(&build_html(pages));
}

fn deliver_impl(slug: &str, data: &serde_json::Value) -> Result<(), String> {
    let script = format!(
        "window.__hebnixDeliver && window.__hebnixDeliver({}, {});",
        json_escape(&html_escape(slug)),
        data,
    );
    if is_main_thread() {
        eval_js(&script)
    } else {
        // Cocoa must be touched on the main thread; queue for pump().
        let mut g = global().lock().map_err(|e| format!("overlay lock poisoned: {e}"))?;
        g.pending_js.push(script);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Public handles.
// ---------------------------------------------------------------------------

/// Handle to the process-wide webview overlay. Mirrors `webview_linux.rs`.
pub struct WebviewOverlay {
    active: Vec<PageLayer>,
    visible: bool,
    last_error: Option<String>,
}

impl WebviewOverlay {
    pub fn new() -> Self {
        Self {
            active: Vec::new(),
            visible: false,
            last_error: None,
        }
    }

    /// Drains JS queued from off-thread. WKWebView painting and JS themselves
    /// are driven by the main run loop winit already runs, so this is cheap.
    pub fn pump() {
        if !is_main_thread() {
            return;
        }
        let queued = {
            let Ok(mut g) = global().lock() else { return };
            std::mem::take(&mut g.pending_js)
        };
        for script in queued {
            if let Err(error) = eval_js(&script) {
                tracing::warn!(%error, "html overlay: deferred JS failed");
            }
        }
    }

    /// True once the WKWebView has been created.
    pub fn is_active(&self) -> bool {
        global().lock().map(|g| g.ready).unwrap_or(false)
    }

    pub fn take_error(&mut self) -> Option<String> {
        let global_error = global().lock().ok().and_then(|mut g| g.error.take());
        global_error.or_else(|| self.last_error.take())
    }

    pub fn sync_pages(&mut self, pages: &[PageLayer]) {
        self.active = pages.to_vec();
        sync_pages_impl(pages);
    }

    pub fn deliver(&self, slug: &str, data: &serde_json::Value) {
        if let Err(error) = deliver_impl(slug, data) {
            if let Ok(mut g) = global().lock() {
                g.error = Some(error);
            }
        }
    }

    pub fn show(&mut self) {
        set_visible_impl(true);
        self.visible = true;
    }

    pub fn hide(&mut self) {
        set_visible_impl(false);
        self.visible = false;
    }

    pub fn is_available() -> bool {
        true // WebKit is linked by this module.
    }

    pub fn ensure_present() {
        // Creation is lazy (first `sync_pages` with a non-empty page set).
    }

    /// Exposed for the UI's status line, which says whether pages are up.
    pub fn visible(&self) -> bool {
        global().lock().map(|g| g.visible).unwrap_or(self.visible)
    }

    pub fn pages(&self) -> &[PageLayer] {
        &self.active
    }
}

impl Default for WebviewOverlay {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether the platform can host plugin HTML at all.
pub mod runtime {
    pub fn is_available() -> bool {
        true
    }

    /// Pre-flight hook; WebKit is linked directly, so nothing to download.
    pub fn ensure_present() {}
}

/// Handle the app stores when a plugin page host exists. On macOS this is a
/// thin wrapper over the same process-wide WKWebView as `WebviewOverlay`.
pub struct WebviewHost {
    last_error: Option<String>,
    ready: bool,
}

impl WebviewHost {
    pub fn new(_target: ()) -> Self {
        Self {
            last_error: None,
            ready: false,
        }
    }

    /// Services the hosted pages: follow the overlay size/visibility and
    /// reflect the global webview state into this handle.
    pub fn tick(
        &mut self,
        _target: crate::winutil::WindowHandle,
        size: (u32, u32),
        active: bool,
    ) {
        if size.0 > 0 && size.1 > 0 {
            resize_impl(size.0, size.1);
        }
        set_visible_impl(active);
        if let Ok(g) = global().lock() {
            self.ready = g.ready;
        }
    }

    /// Whether a plugin page is asking for the overlay to be shown.
    pub fn wants_overlay(&self) -> bool {
        global()
            .lock()
            .map(|g| g.ready && g.pages.iter().any(|(_, page, _, _)| page.is_some()))
            .unwrap_or(false)
    }

    /// Whether a plugin page wants mouse input routed to it. The panel itself
    /// stays click-through (see module docs); this only reflects the pages'
    /// declared intent, as on Windows/Linux.
    pub fn wants_input(&self) -> bool {
        global()
            .lock()
            .map(|g| {
                g.ready
                    && g.pages
                        .iter()
                        .any(|(_, page, _, clickable)| page.is_some() && *clickable)
            })
            .unwrap_or(false)
    }

    pub fn take_error(&mut self) -> Option<String> {
        let global_error = global().lock().ok().and_then(|mut g| g.error.take());
        global_error.or_else(|| self.last_error.take())
    }

    pub fn sync_pages(&mut self, pages: &[PageLayer]) {
        sync_pages_impl(pages);
    }

    pub fn deliver(&self, slug: &str, data: serde_json::Value) -> Result<(), String> {
        deliver_impl(slug, &data)
    }

    pub fn set_visible(&self, visible: bool) {
        set_visible_impl(visible);
    }

    pub fn status(&self) -> String {
        match global().lock() {
            Ok(g) if g.ready => {
                let count = g.pages.iter().filter(|(_, page, _, _)| page.is_some()).count();
                format!("WKWebView ready, {count} page(s)")
            }
            Ok(_) => "WKWebView not created yet".to_string(),
            Err(_) => "webview state unavailable".to_string(),
        }
    }

    pub fn is_ready(&self) -> bool {
        global().lock().map(|g| g.ready).unwrap_or(false)
    }
}

pub mod host {
    pub use super::WebviewHost;
}
