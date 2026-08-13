#[cfg(target_os = "windows")]
use crate::gstreamer_backend::send_log;
#[cfg(target_os = "windows")]
use crate::protocol::NativeRenderRect;
use crate::protocol::{Event, NativeRenderSurface, NativeStreamerShortcutBindings};
#[cfg(target_os = "windows")]
use std::ffi::c_void;
use std::sync::atomic::AtomicBool;
#[cfg(target_os = "windows")]
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::sync::Arc;
#[cfg(target_os = "windows")]
use std::thread;
#[cfg(target_os = "windows")]
use std::time::Duration;

#[cfg(target_os = "windows")]
fn parse_window_handle(value: &str) -> Result<usize, String> {
    let trimmed = value.trim();
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"));
    let parsed = if let Some(hex) = hex {
        usize::from_str_radix(hex, 16)
    } else {
        trimmed.parse::<usize>()
    }
    .map_err(|error| format!("Invalid native render window handle {value:?}: {error}"))?;

    if parsed == 0 {
        return Err("Native render window handle is zero.".to_owned());
    }

    Ok(parsed)
}

#[cfg(target_os = "windows")]
fn normalized_render_rect(rect: Option<&NativeRenderRect>) -> NativeRenderRect {
    let Some(rect) = rect else {
        return NativeRenderRect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        };
    };

    NativeRenderRect {
        x: rect.x.max(0),
        y: rect.y.max(0),
        width: rect.width.max(2),
        height: rect.height.max(2),
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn start_external_renderer_window_guard(
    event_sender: Option<Sender<Event>>,
    stop: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let mut logged = false;
        while !stop.load(Ordering::SeqCst) {
            if stop.load(Ordering::SeqCst) {
                break;
            }

            let configured = unsafe { win32_renderer_window::protect_process_renderer_window() };
            if configured && !logged {
                send_log(
                    &event_sender,
                    "info",
                    "Configured external native renderer window for fullscreen DX11 input capture."
                        .to_owned(),
                );
                logged = true;
            }
            thread::sleep(if logged {
                Duration::from_millis(500)
            } else {
                Duration::from_millis(100)
            });
        }
    });
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn start_external_renderer_window_guard(
    _event_sender: Option<Sender<Event>>,
    _stop: Arc<AtomicBool>,
) {
}

#[cfg(target_os = "windows")]
pub(crate) fn set_native_shortcut_bindings(bindings: &NativeStreamerShortcutBindings) {
    unsafe {
        win32_renderer_window::set_shortcut_bindings(bindings.clone());
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn set_native_shortcut_bindings(_bindings: &NativeStreamerShortcutBindings) {}

#[cfg(target_os = "windows")]
pub(crate) fn clear_native_shortcut_bindings() {
    unsafe {
        win32_renderer_window::clear_shortcut_bindings();
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn clear_native_shortcut_bindings() {}

#[cfg(target_os = "windows")]
pub(crate) fn release_native_input_capture() {
    unsafe {
        win32_renderer_window::release_current_input_capture();
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn release_native_input_capture() {}

#[cfg(target_os = "windows")]
pub(crate) fn arm_internal_child_input(hwnd: usize) -> bool {
    unsafe { win32_renderer_window::arm_internal_child_input(hwnd) }
}

/// Arm mouse + keyboard RawInput capture on the stacked sink window so raw HID
/// events bypass the Electron bridge (renderer -> main -> stdin) entirely —
/// low-latency, in-process. No-op outside stacked render mode or when the
/// sink / shell / bridge are not ready (the stacked guard retries).
#[cfg(target_os = "windows")]
pub(crate) fn arm_stacked_sink_input_capture() -> bool {
    unsafe { win32_renderer_window::arm_stacked_sink_input_capture() }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn arm_stacked_sink_input_capture() -> bool {
    false
}

#[cfg(target_os = "windows")]
pub(crate) fn update_external_renderer_surface(surface: &NativeRenderSurface) {
    let target = surface
        .window_handle
        .as_deref()
        .and_then(|window_handle| parse_window_handle(window_handle).ok())
        .and_then(|window_handle| {
            surface
                .visible
                .then_some(())
                .and(surface.rect.as_ref())
                .map(|rect| (window_handle, normalized_render_rect(Some(rect))))
        });

    unsafe {
        win32_renderer_window::set_render_target_surface(target);
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn update_external_renderer_surface(_surface: &NativeRenderSurface) {}

/// GFN-style stacked renderer: keep the video sink's own top-level window,
/// size/position it to the stream rect (screen coordinates, derived from the
/// BrowserWindow client rect) and stack it directly below the Electron
/// window. No fullscreen, no foreground stealing, no RawInput capture — the
/// DOM overlay stays interactive above the video.
#[cfg(target_os = "windows")]
pub(crate) fn apply_stacked_renderer_surface(
    surface: &NativeRenderSurface,
    sink_window_handle: usize,
) {
    unsafe {
        win32_renderer_window::apply_stacked_renderer_surface(surface, sink_window_handle);
    }
}

/// Mark that the first decoded video frame arrived and, when the sink window
/// + BrowserWindow pair are known, position + show the sink right away
/// (GFN-parity: the video only becomes visible at its final rect, never at
/// GStreamer's default window position). Returns true when the sink is now
/// visible + positioned; false when the stacked guard must retry (the sink
/// window is created at the first present, just after the first-buffer probe).
#[cfg(target_os = "windows")]
pub(crate) fn reveal_stacked_renderer_window() -> bool {
    unsafe { win32_renderer_window::reveal_stacked_renderer_window() }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn reveal_stacked_renderer_window() -> bool {
    false
}

/// Record that the first-video-buffer probe delivered the streaming event
/// directly (used when the sink was already revealed at probe time).
#[cfg(target_os = "windows")]
pub(crate) fn stacked_mark_streaming_event_sent() {
    win32_renderer_window::stacked_mark_streaming_event_sent();
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn stacked_mark_streaming_event_sent() {}

/// Keep the stacked video sink aligned to the BrowserWindow while it moves or
/// resizes. The renderer only publishes surface updates on DOM resize/
/// fullscreen/visibility changes — dragging the shell never fires one — so a
/// WinEvent hook (EVENT_OBJECT_LOCATIONCHANGE) mirrors the browser rect into
/// the sink window the instant it moves, re-asserting z-order (sink stays
/// directly below the Electron window). Falls back to a slow poller only if
/// the hook cannot be installed.
#[cfg(target_os = "windows")]
pub(crate) fn start_stacked_renderer_window_guard(
    event_sender: Option<Sender<Event>>,
    stop: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        win32_renderer_window::set_stacked_guard_log_sender(event_sender.clone());
        // A new stacked session starts with the sink hidden (until its first
        // decoded frame) and the streaming event unsent.
        win32_renderer_window::reset_stacked_first_frame_state();
        let hooked = unsafe { win32_renderer_window::arm_stacked_renderer_event_hook() };
        if hooked {
            send_log(
                &event_sender,
                "info",
                "Stacked renderer window guard active: video follows BrowserWindow move/resize via WinEvent hook."
                    .to_owned(),
            );
            // Guard against a stop that raced ahead of arm() (thread id not yet
            // recorded, so no WM_QUIT was posted): exit immediately instead of
            // blocking on the message loop forever.
            if stop.load(Ordering::SeqCst) {
                unsafe {
                    win32_renderer_window::disarm_stacked_renderer_event_hook();
                }
                return;
            }
            // Periodic safety net (see STACKED_GUARD_TIMER_MS): re-run the style
            // + position + z-order assertions even when WinEvents are missed.
            unsafe {
                win32_renderer_window::arm_stacked_guard_timer();
            }
            // Event-driven sync. The hook callback is delivered on this thread's
            // message loop; keep pumping messages until WM_QUIT (posted by
            // stop_stacked_renderer_window_guard). WM_TIMER from the guard timer
            // is handled inline.
            loop {
                let mut msg = unsafe { std::mem::zeroed() };
                let result = unsafe { win32_renderer_window::get_message(&mut msg) };
                if result <= 0 {
                    break;
                }
                if unsafe { win32_renderer_window::stacked_guard_message_is_timer(&msg) } {
                    unsafe {
                        win32_renderer_window::stacked_guard_tick();
                    }
                }
            }
            unsafe {
                win32_renderer_window::disarm_stacked_renderer_event_hook();
            }
            return;
        }

        // Hook unavailable: fall back to a slow position poller.
        let mut logged = false;
        while !stop.load(Ordering::SeqCst) {
            unsafe {
                win32_renderer_window::sync_stacked_renderer_window_position();
            }
            if !logged {
                send_log(
                    &event_sender,
                    "info",
                    "Stacked renderer window guard active (polling fallback): video follows BrowserWindow position."
                        .to_owned(),
                );
                logged = true;
            }
            thread::sleep(Duration::from_millis(50));
        }
    });
}

/// Stop the stacked window guard (WinEvent hook thread) if one is running.
/// Safe to call when no stacked session is active — no-op.
#[cfg(target_os = "windows")]
pub(crate) fn stop_stacked_renderer_window_guard() {
    unsafe {
        win32_renderer_window::request_stacked_event_hook_stop();
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn stop_stacked_renderer_window_guard() {}

#[cfg(not(target_os = "windows"))]
pub(crate) fn start_stacked_renderer_window_guard(
    _event_sender: Option<Sender<Event>>,
    _stop: Arc<AtomicBool>,
) {
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn apply_stacked_renderer_surface(
    _surface: &NativeRenderSurface,
    _sink_window_handle: usize,
) {
}

#[cfg(target_os = "windows")]
pub(crate) mod win32_renderer_window {
    use crate::gstreamer_config::use_stacked_renderer;
    use crate::gstreamer_input::NativeWindowInputEvent;
    use crate::protocol::NativeRenderRect;
    use crate::protocol::{
        Event, NativeRenderSurface, NativeStreamerShortcutAction, NativeStreamerShortcutBindings,
    };
    use crate::shortcuts::NativeShortcutMatcher;
    use std::collections::{HashMap, HashSet};
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc::Sender;
    use std::sync::{Mutex, OnceLock};
    use std::thread;
    use std::time::{Duration, Instant};

    type Bool = i32;
    type Dword = u32;
    type Hcursor = *mut c_void;
    type Hhook = *mut c_void;
    type Hmonitor = *mut c_void;
    type Hrawinput = *mut c_void;
    type Hwnd = *mut c_void;
    type Lparam = isize;
    type Lresult = isize;
    type Uint = u32;
    type Wparam = usize;

    type WinEventProc = unsafe extern "system" fn(
        h_win_event_hook: Hhook,
        event: Dword,
        hwnd: Hwnd,
        id_object: i32,
        id_child: i32,
        id_event_thread: Dword,
        dwms_event_time: Dword,
    );

    const GWL_STYLE: i32 = -16;
    const GWL_EXSTYLE: i32 = -20;
    const GWLP_WNDPROC: i32 = -4;
    const GW_OWNER: Uint = 4;
    const GW_HWNDPREV: Uint = 3;
    const HTCLIENT: isize = 1;
    const HWND_NOTOPMOST: Hwnd = -2isize as Hwnd;
    const MA_ACTIVATE: isize = 1;
    const MONITOR_DEFAULTTONEAREST: Dword = 0x0000_0002;
    const RID_INPUT: Uint = 0x1000_0003;
    const RIDEV_REMOVE: Dword = 0x0000_0001;
    const RIDEV_NOLEGACY: Dword = 0x0000_0030;
    // Receive WM_INPUT even when this HWND is not foreground. Required for the
    // internal child surface: Electron stays the top-level foreground window,
    // so keyboard RawInput never arrives without INPUTSINK. Mouse still works
    // via RIDEV_CAPTUREMOUSE alone.
    const RIDEV_INPUTSINK: Dword = 0x0000_0100;
    const RIDEV_CAPTUREMOUSE: Dword = 0x0000_0200;
    const RIM_TYPEMOUSE: Dword = 0;
    const RIM_TYPEKEYBOARD: Dword = 1;
    const RI_KEY_BREAK: u16 = 0x0001;
    const RI_KEY_E0: u16 = 0x0002;
    const RI_KEY_E1: u16 = 0x0004;
    const RI_MOUSE_LEFT_BUTTON_DOWN: u16 = 0x0001;
    const RI_MOUSE_LEFT_BUTTON_UP: u16 = 0x0002;
    const RI_MOUSE_RIGHT_BUTTON_DOWN: u16 = 0x0004;
    const RI_MOUSE_RIGHT_BUTTON_UP: u16 = 0x0008;
    const RI_MOUSE_MIDDLE_BUTTON_DOWN: u16 = 0x0010;
    const RI_MOUSE_MIDDLE_BUTTON_UP: u16 = 0x0020;
    const RI_MOUSE_BUTTON_4_DOWN: u16 = 0x0040;
    const RI_MOUSE_BUTTON_4_UP: u16 = 0x0080;
    const RI_MOUSE_BUTTON_5_DOWN: u16 = 0x0100;
    const RI_MOUSE_BUTTON_5_UP: u16 = 0x0200;
    const RI_MOUSE_WHEEL: u16 = 0x0400;
    const VK_SHIFT: u16 = 0x10;
    const VK_ESCAPE: u16 = 0x1B;
    const VK_V: u16 = 0x56;
    const VK_TAB: u16 = 0x09;
    const VK_CONTROL: u16 = 0x11;
    const VK_MENU: u16 = 0x12;
    const VK_CAPITAL: i32 = 0x14;
    const VK_NUMLOCK: i32 = 0x90;
    const VK_SCROLL: i32 = 0x91;
    const VK_LSHIFT: u16 = 0xA0;
    const VK_RSHIFT: u16 = 0xA1;
    const VK_LCONTROL: u16 = 0xA2;
    const VK_RCONTROL: u16 = 0xA3;
    const VK_LMENU: u16 = 0xA4;
    const VK_RMENU: u16 = 0xA5;
    const VK_LWIN: u16 = 0x5B;
    const VK_RWIN: u16 = 0x5C;
    const WM_INPUT: Uint = 0x00FF;
    const WM_NCHITTEST: Uint = 0x0084;
    const WM_MOUSEACTIVATE: Uint = 0x0021;
    const WM_SETCURSOR: Uint = 0x0020;
    const WM_KILLFOCUS: Uint = 0x0008;
    const WM_ACTIVATE: Uint = 0x0006;
    const WM_QUIT: Uint = 0x0012;
    const WM_TIMER: Uint = 0x0113;
    const WA_INACTIVE: usize = 0;
    // WinEvent hook for the stacked renderer: fires whenever the BrowserWindow
    // moves or resizes, replacing the old 30 ms position poller with an
    // event-driven sync (cross-process safe — the callback runs on this
    // streamer's own message loop, never touching Electron's wndproc).
    // MOVESIZEEND is a final burst after a drag/resize/fullscreen transition
    // settles, catching any last-position drift the location-change stream
    // missed.
    const EVENT_OBJECT_CREATE: Dword = 0x8000;
    const EVENT_OBJECT_LOCATIONCHANGE: Dword = 0x800B;
    const EVENT_SYSTEM_MOVESIZEEND: Dword = 0x000B;
    const EVENT_SYSTEM_FOREGROUND: Dword = 0x0003;
    const WINEVENT_OUTOFCONTEXT: Dword = 0x0000;
    // Periodic stacked-guard safety net: WinEvents cover browser move/resize
    // and sink creation, but the sink window can be created before the first
    // renderer surface publish populates STACKED_TARGET, and GStreamer can
    // re-apply its default overlapped style when it recreates the window. A
    // slow thread timer re-runs the same assertions so a border or a stale
    // z-order can never linger.
    const STACKED_GUARD_TIMER_ID: usize = 0x4E53;
    const STACKED_GUARD_TIMER_MS: u32 = 400;
    const WM_KEYDOWN: Uint = 0x0100;
    const WM_KEYUP: Uint = 0x0101;
    const WM_SYSKEYDOWN: Uint = 0x0104;
    const WM_SYSKEYUP: Uint = 0x0105;
    const WM_LBUTTONDOWN: Uint = 0x0201;
    const WM_LBUTTONUP: Uint = 0x0202;
    const WM_RBUTTONDOWN: Uint = 0x0204;
    const WM_RBUTTONUP: Uint = 0x0205;
    const WM_MBUTTONDOWN: Uint = 0x0207;
    const WM_MBUTTONUP: Uint = 0x0208;
    const WM_XBUTTONDOWN: Uint = 0x020B;
    const WM_XBUTTONUP: Uint = 0x020C;
    const XBUTTON1: u16 = 0x0001;
    const XBUTTON2: u16 = 0x0002;
    const WS_CAPTION: isize = 0x00C0_0000;
    const WS_MAXIMIZEBOX: isize = 0x0001_0000;
    const WS_MINIMIZEBOX: isize = 0x0002_0000;
    const WS_SYSMENU: isize = 0x0008_0000;
    const WS_THICKFRAME: isize = 0x0004_0000;
    const WS_EX_NOACTIVATE: isize = 0x0800_0000;
    const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
    const WS_EX_APPWINDOW: isize = 0x0004_0000;
    const WS_EX_TRANSPARENT: isize = 0x0000_0020;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const SWP_HIDEWINDOW: u32 = 0x0080;
    const SW_HIDE: i32 = 0;
    const SW_SHOWNOACTIVATE: i32 = 4;
    const SW_MINIMIZE: i32 = 6;
    const ESCAPE_SCANCODE: u16 = 0x0001;
    const ESCAPE_HOLD_TO_MINIMIZE: Duration = Duration::from_secs(5);

    struct EnumState {
        process_id: u32,
        candidates: Vec<WindowCandidate>,
    }

    #[derive(Clone, Copy)]
    struct WindowCandidate {
        hwnd: Hwnd,
        area: i64,
    }

    /// Internal state for the EnumWindows callback that finds the streamer's
    /// own video-sink window when the sink does not expose a GstVideoOverlay
    /// window handle.
    struct Found {
        hwnd: Hwnd,
    }

    #[derive(Clone, Copy)]
    struct RenderTargetSurface {
        hwnd: isize,
        client_rect: Rect,
    }

    #[derive(Clone, Copy)]
    struct StackedTarget {
        /// HWNDs stored as isize (not raw pointers) so the struct stays
        /// Send/Sync for the static OnceLock slots — same pattern as
        /// CAPTURED_HWND / PROTECTED_HWND in this module.
        sink_hwnd: isize,
        browser_hwnd: isize,
        /// Last rect pushed to the sink, so repeat location-change events
        /// (fired in bursts during a drag/resize/fullscreen transition) don't
        /// spam SetWindowPos.
        last_rect: Option<Rect>,
    }

    #[repr(C)]
    #[derive(Clone, Copy, PartialEq)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Msg {
        hwnd: Hwnd,
        message: Uint,
        w_param: Wparam,
        l_param: Lparam,
        time: Dword,
        pt: Point,
        l_private: Dword,
    }

    #[repr(C)]
    struct MonitorInfo {
        cb_size: Dword,
        rc_monitor: Rect,
        rc_work: Rect,
        dw_flags: Dword,
    }

    #[repr(C)]
    struct RawInputDevice {
        us_usage_page: u16,
        us_usage: u16,
        dw_flags: Dword,
        hwnd_target: Hwnd,
    }

    #[repr(C)]
    struct RawInputHeader {
        dw_type: Dword,
        dw_size: Dword,
        h_device: *mut c_void,
        w_param: Wparam,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct RawMouse {
        us_flags: u16,
        buttons: u32,
        ul_raw_buttons: u32,
        l_last_x: i32,
        l_last_y: i32,
        ul_extra_information: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct RawKeyboard {
        make_code: u16,
        flags: u16,
        reserved: u16,
        vkey: u16,
        message: Uint,
        extra_information: u32,
    }

    #[derive(Clone, Copy)]
    struct PressedKey {
        keycode: u16,
        scancode: u16,
        suppressed: bool,
    }

    #[derive(Clone, Copy)]
    struct EscapeKeyPress {
        scancode: u16,
        hold_timer_armed: bool,
    }

    static INPUT_EVENT_SENDER: OnceLock<Mutex<Option<Sender<NativeWindowInputEvent>>>> =
        OnceLock::new();
    static ORIGINAL_WNDPROCS: OnceLock<Mutex<HashMap<isize, isize>>> = OnceLock::new();
    static CAPTURED_HWND: OnceLock<Mutex<Option<isize>>> = OnceLock::new();
    static PROTECTED_HWND: OnceLock<Mutex<Option<isize>>> = OnceLock::new();
    static PRESSED_KEYS: OnceLock<Mutex<HashMap<u16, PressedKey>>> = OnceLock::new();
    static LAST_LOCK_KEYS_STATE: OnceLock<Mutex<u8>> = OnceLock::new();
    static LEGACY_SUPPRESSED_KEYS: OnceLock<Mutex<HashSet<u16>>> = OnceLock::new();
    static ESCAPE_HOLD_HWND: OnceLock<Mutex<Option<isize>>> = OnceLock::new();
    static ESCAPE_HOLD_TOKEN: OnceLock<AtomicU64> = OnceLock::new();
    static ESCAPE_KEY_PRESS: OnceLock<Mutex<Option<EscapeKeyPress>>> = OnceLock::new();
    static SHORTCUT_MATCHER: OnceLock<Mutex<NativeShortcutMatcher>> = OnceLock::new();
    static RENDER_TARGET_SURFACE: OnceLock<Mutex<Option<RenderTargetSurface>>> = OnceLock::new();
    static STACKED_TARGET: OnceLock<Mutex<Option<StackedTarget>>> = OnceLock::new();
    /// Latest surface published by the renderer. The stacked guard re-applies
    /// it after the sink window is (re)created: the first publish can race
    /// ahead of window creation, and the renderer only publishes on DOM events
    /// — without this, the sink would sit at GStreamer's default window
    /// position until an overlay forces the next publish (the launch blur).
    static STACKED_PENDING_SURFACE: OnceLock<Mutex<Option<NativeRenderSurface>>> = OnceLock::new();
    /// Set once the first decoded video frame has arrived. Until then the sink
    /// is kept hidden (the shell is still opaque, so a visible window would
    /// only flash through once the shell goes transparent).
    static STACKED_FIRST_FRAME_REVEALED: OnceLock<AtomicBool> = OnceLock::new();
    /// One-shot: the first-video-buffer probe normally sends the streaming
    /// event, but defers it when the sink window does not exist yet (it is
    /// created at the first present, just after the probe). The stacked guard
    /// sends it the moment the sink is revealed, so the shell's transparent
    /// flip never precedes a visible, correctly-positioned video window.
    static STACKED_STREAMING_EVENT_SENT: OnceLock<AtomicBool> = OnceLock::new();
    // Hook handles as isize so the static Mutex slot stays Sync (raw *mut c_void
    // is not Send). Matches the HWND-as-isize pattern used by the other slots.
    static STACKED_EVENT_HOOK: OnceLock<Mutex<Vec<isize>>> = OnceLock::new();
    static STACKED_EVENT_HOOK_THREAD: OnceLock<Mutex<Option<Dword>>> = OnceLock::new();
    /// True while the Electron shell (browser_hwnd) is the Windows foreground
    /// window. The stacked sink's RawInput mouse registration uses
    /// RIDEV_INPUTSINK (deltas arrive even though the sink is never
    /// foreground), so this flag gates forwarding: when the user alt-tabs to
    /// another app, the game must never receive that app's mouse movement.
    /// Updated by the foreground WinEvent hook and the stacked guard tick.
    static STACKED_SHELL_FOREGROUND: OnceLock<AtomicBool> = OnceLock::new();
    /// Optional event sender used to surface stacked sink window transitions
    /// (style changes, SetWindowPos calls) in the Electron log, so flicker or
    /// border regressions can be matched against what the guard actually did.
    static STACKED_GUARD_LOG: OnceLock<Mutex<Option<Sender<Event>>>> = OnceLock::new();

    #[link(name = "user32")]
    unsafe extern "system" {
        fn CallWindowProcW(
            previous: isize,
            hwnd: Hwnd,
            message: Uint,
            wparam: Wparam,
            lparam: Lparam,
        ) -> Lresult;
        fn ClientToScreen(hwnd: Hwnd, point: *mut Point) -> Bool;
        fn ClipCursor(rect: *const Rect) -> Bool;
        fn DefWindowProcW(hwnd: Hwnd, message: Uint, wparam: Wparam, lparam: Lparam) -> Lresult;
        fn EnumWindows(
            callback: Option<unsafe extern "system" fn(Hwnd, Lparam) -> Bool>,
            lparam: Lparam,
        ) -> Bool;
        fn GetMonitorInfoW(monitor: Hmonitor, info: *mut MonitorInfo) -> Bool;
        fn GetRawInputData(
            raw_input: Hrawinput,
            command: Uint,
            data: *mut c_void,
            size: *mut u32,
            header_size: u32,
        ) -> u32;
        fn GetKeyState(virtual_key: i32) -> i16;
        fn GetMessageW(msg: *mut Msg, hwnd: Hwnd, min: Uint, max: Uint) -> Bool;
        fn GetWindow(hwnd: Hwnd, command: Uint) -> Hwnd;
        fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> Bool;
        fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> isize;
        fn GetWindowRect(hwnd: Hwnd, rect: *mut Rect) -> Bool;
        fn GetWindowThreadProcessId(hwnd: Hwnd, process_id: *mut u32) -> u32;
        fn IsIconic(hwnd: Hwnd) -> Bool;
        fn IsWindowVisible(hwnd: Hwnd) -> Bool;
        fn MonitorFromWindow(hwnd: Hwnd, flags: Dword) -> Hmonitor;
        fn PostThreadMessageW(
            thread_id: Dword,
            message: Uint,
            wparam: Wparam,
            lparam: Lparam,
        ) -> Bool;
        fn RegisterRawInputDevices(devices: *const RawInputDevice, count: u32, size: u32) -> Bool;
        fn ReleaseCapture() -> Bool;
        fn SetCapture(hwnd: Hwnd) -> Hwnd;
        fn SetCursor(cursor: Hcursor) -> Hcursor;
        fn SetFocus(hwnd: Hwnd) -> Hwnd;
        fn SetForegroundWindow(hwnd: Hwnd) -> Bool;
        fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, new_long: isize) -> isize;
        fn SetWindowPos(
            hwnd: Hwnd,
            insert_after: Hwnd,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> Bool;
        fn SetWinEventHook(
            event_min: Dword,
            event_max: Dword,
            module: *mut c_void,
            callback: Option<WinEventProc>,
            process_id: Dword,
            thread_id: Dword,
            flags: Dword,
        ) -> Hhook;
        fn ShowWindow(hwnd: Hwnd, command: i32) -> Bool;
        fn ShowCursor(show: Bool) -> i32;
        fn UnhookWinEvent(hook: Hhook) -> Bool;
        fn IsWindow(hwnd: Hwnd) -> Bool;
        fn GetForegroundWindow() -> Hwnd;
        fn SetTimer(
            hwnd: Hwnd,
            id: usize,
            elapse: u32,
            timer_proc: Option<unsafe extern "system" fn(Hwnd, Uint, usize, Dword)>,
        ) -> usize;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcessId() -> u32;
        fn GetCurrentThreadId() -> Dword;
    }

    /// Route diagnostics for the stacked renderer window (style/SetWindowPos
    /// transitions) back to the Electron log when a guard sender is attached.
    /// Safe to call with None; clears any previous sender.
    pub fn set_stacked_guard_log_sender(sender: Option<Sender<Event>>) {
        if let Ok(mut current) = STACKED_GUARD_LOG.get_or_init(|| Mutex::new(None)).lock() {
            *current = sender;
        }
    }

    fn stacked_guard_log(level: &'static str, message: String) {
        let Some(sender) = STACKED_GUARD_LOG
            .get()
            .and_then(|sender| sender.lock().ok().and_then(|sender| sender.clone()))
        else {
            eprintln!("[NativeStreamer] {message}");
            return;
        };
        let _ = sender.send(Event::Log { level, message });
    }

    /// Log at most one message per category within `interval`, so periodic
    /// paths (e.g. the 400 ms guard tick) cannot flood the session log.
    fn stacked_guard_log_throttled(
        category: &'static str,
        level: &'static str,
        message: String,
        interval: Duration,
    ) {
        static LAST_LOGGED: OnceLock<Mutex<HashMap<&'static str, Instant>>> = OnceLock::new();
        let now = Instant::now();
        let mut last_logged = LAST_LOGGED
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if last_logged
            .get(category)
            .is_some_and(|previous| now.duration_since(*previous) < interval)
        {
            return;
        }
        last_logged.insert(category, now);
        stacked_guard_log(level, message);
    }

    /// Whether a message from the stacked guard message loop is the periodic
    /// WM_TIMER tick.
    pub unsafe fn stacked_guard_message_is_timer(msg: &Msg) -> bool {
        msg.message == WM_TIMER
    }

    /// Arm the periodic stacked-guard safety-net timer on the current thread.
    /// A NULL hWnd timer posts WM_TIMER to the calling thread's message queue
    /// — the same queue the WinEvent hooks use — so the guard loop sees it via
    /// GetMessageW. Windows cleans the timer up when the thread exits.
    pub unsafe fn arm_stacked_guard_timer() {
        SetTimer(
            null_mut(),
            STACKED_GUARD_TIMER_ID,
            STACKED_GUARD_TIMER_MS,
            None,
        );
    }

    /// Reset the stacked reveal state at the start of a stacked session: a new
    /// session starts with the sink hidden (until its first decoded frame) and
    /// the streaming event unsent. The pending surface is deliberately kept —
    /// it is overwritten by the next renderer publish, and a stale copy is
    /// harmless because applies always read the live BrowserWindow rect.
    pub fn reset_stacked_first_frame_state() {
        STACKED_FIRST_FRAME_REVEALED
            .get_or_init(|| AtomicBool::new(false))
            .store(false, Ordering::SeqCst);
        STACKED_STREAMING_EVENT_SENT
            .get_or_init(|| AtomicBool::new(false))
            .store(false, Ordering::SeqCst);
    }

    fn stacked_first_frame_revealed() -> bool {
        STACKED_FIRST_FRAME_REVEALED
            .get()
            .map(|flag| flag.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    fn stacked_mark_first_frame_revealed() {
        STACKED_FIRST_FRAME_REVEALED
            .get_or_init(|| AtomicBool::new(false))
            .store(true, Ordering::SeqCst);
    }

    /// Mark that the streaming event was delivered to the renderer (used by
    /// the first-video-buffer probe when it sends the event directly).
    pub fn stacked_mark_streaming_event_sent() {
        STACKED_STREAMING_EVENT_SENT
            .get_or_init(|| AtomicBool::new(false))
            .store(true, Ordering::SeqCst);
    }

    /// Mark that the first decoded video frame arrived and, if the sink window
    /// and BrowserWindow pair are known, position + show the sink right away
    /// (GFN-parity: the video only becomes visible once it can appear at the
    /// final rect — no default-position flash through the transparent shell).
    /// Returns true when the sink is now visible + positioned; false when the
    /// caller must retry later (window not created yet, or no surface publish
    /// has populated the target pair).
    pub unsafe fn reveal_stacked_renderer_window() -> bool {
        stacked_mark_first_frame_revealed();
        let target_slot = STACKED_TARGET.get_or_init(|| Mutex::new(None));
        let Ok(mut target_guard) = target_slot.lock() else {
            return false;
        };
        let Some(target) = target_guard.as_mut() else {
            return false;
        };
        let browser_hwnd = target.browser_hwnd as Hwnd;
        let Some(sink_hwnd) = resolve_stacked_sink_hwnd(target) else {
            return false;
        };
        let mut window_rect = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(browser_hwnd, &mut window_rect) == 0 {
            return false;
        }
        enforce_stacked_renderer_window_style(sink_hwnd);
        SetWindowPos(
            sink_hwnd,
            browser_hwnd,
            window_rect.left,
            window_rect.top,
            window_rect.right.saturating_sub(window_rect.left),
            window_rect.bottom.saturating_sub(window_rect.top),
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        target.last_rect = Some(window_rect);
        // Diagnostics for the "slightly zoomed display" report: the sink is
        // sized to the browser's OUTER rect, but d3d12videosink renders into
        // the sink's CLIENT area. If the two differ (a leftover caption or
        // DWM resize border), the video is upscaled/cropped a few pixels —
        // a subtle zoom the recording (tapped at decode) never shows.
        // Compare client vs outer so the next session pinpoints it.
        let mut client_rect = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let client = GetClientRect(sink_hwnd, &mut client_rect);
        // Diagnostics for the "slightly zoomed display" report: compare the
        // sink rect (browser outer), the sink CLIENT rect, and the rect the
        // RENDERER published (video element CSS x dpr). If the renderer's
        // published rect ever differs from the browser rect (window not at the
        // origin, DPI mismatch, CSS overflow), the sink would be placed at the
        // wrong screen position / size — showing as a crop/zoom the recording
        // (tapped at decode) never has. The sink geometry itself is
        // browser-rect-driven, so this log is the tiebreaker for the report.
        let surface_rect = STACKED_PENDING_SURFACE
            .get()
            .and_then(|surface| surface.lock().ok().and_then(|surface| surface.as_ref().cloned()))
            .map(|surface| {
                surface.rect.map(|rect| {
                    format!(
                        "renderer_rect=({},{} {}x{}) dpr={}",
                        rect.x, rect.y, rect.width, rect.height, surface.device_scale_factor
                    )
                })
            })
            .flatten()
            .unwrap_or_else(|| "renderer_rect=none".to_owned());
        let client_dim = if client != 0 {
            format!(
                "{}x{}",
                client_rect.right.saturating_sub(client_rect.left),
                client_rect.bottom.saturating_sub(client_rect.top)
            )
        } else {
            "0x0".to_owned()
        };
        stacked_guard_log(
            "info",
            format!(
                "Stacked sink revealed at first decoded frame: rect=({},{} {}x{}) below browser; client={client_dim}; sink=0x{:X}; {surface_rect}",
                window_rect.left,
                window_rect.top,
                window_rect.right.saturating_sub(window_rect.left),
                window_rect.bottom.saturating_sub(window_rect.top),
                sink_hwnd as usize,
            ),
        );
        true
    }

    /// If the first decoded frame arrived but the probe could not show the
    /// sink yet (window not created at probe time), retry the reveal now that
    /// a window exists and — once it succeeds — deliver the deferred streaming
    /// event, so the renderer's transparent-shell flip never precedes a
    /// visible, correctly-positioned video window.
    unsafe fn maybe_finish_stacked_reveal() {
        let revealed = stacked_first_frame_revealed();
        let sent = STACKED_STREAMING_EVENT_SENT
            .get()
            .map(|flag| flag.load(Ordering::SeqCst))
            .unwrap_or(false);
        if !revealed || sent {
            return;
        }
        if !reveal_stacked_renderer_window() {
            return;
        }
        stacked_mark_streaming_event_sent();
        let message = if use_stacked_renderer() {
            "Native video frames reached the external low-latency GStreamer renderer window."
        } else {
            "Native video frames reached the internal child-surface GStreamer renderer."
        };
        let sender = STACKED_GUARD_LOG
            .get()
            .and_then(|sender| sender.lock().ok().and_then(|sender| sender.clone()));
        if let Some(sender) = sender {
            let _ = sender.send(Event::Status {
                status: "streaming",
                message: Some(message.to_owned()),
            });
        }
    }

    /// Periodic stacked-guard assertion: keep the sink borderless + hidden from
    /// alt-tab, aligned to the BrowserWindow, and directly below it in z-order.
    /// WinEvents cover the common cases, but the sink window can be created
    /// before the first surface publish populates STACKED_TARGET (that race
    /// left a bordered, alt-tab-visible video window until an overlay opened),
    /// and GStreamer can re-apply its default overlapped style when it recreates
    /// the window. The slow timer guarantees the border cannot linger and the
    /// video cannot stay stuck behind another window.
    pub unsafe fn stacked_guard_tick() {
        // Prefer the cached sink HWND; rediscover our own top-level window when
        // the cached one is gone (GStreamer recreated it) or not known yet (the
        // first surface publish has not populated STACKED_TARGET).
        let sink_hwnd = match STACKED_TARGET
            .get()
            .and_then(|target| target.lock().ok().and_then(|target| *target))
            .map(|target| target.sink_hwnd as Hwnd)
        {
            Some(hwnd) if IsWindow(hwnd) != 0 => hwnd,
            _ => match find_first_own_window() {
                Some(hwnd) => hwnd,
                None => return,
            },
        };
        // If the sink window exists but STACKED_TARGET was never populated
        // (the first surface publish raced ahead of window creation and the
        // renderer has not published since), re-apply the latest stored
        // surface: this positions the sink (+ shows it once the first frame
        // reveals it) without waiting for the next renderer publish — the gap
        // that left the video at GStreamer's default position for the first
        // ~tens of seconds of a session.
        let target_empty = STACKED_TARGET
            .get()
            .and_then(|target| target.lock().ok().and_then(|target| *target))
            .is_none();
        if target_empty {
            let pending = STACKED_PENDING_SURFACE
                .get()
                .and_then(|pending| pending.lock().ok().and_then(|pending| pending.clone()));
            if let Some(pending) = pending {
                apply_stacked_renderer_surface(&pending, sink_hwnd as usize);
            }
        }
        enforce_stacked_renderer_window_style(sink_hwnd);
        sync_stacked_renderer_window_position();
        // Re-assert z-order when the shell is the foreground window (alt-tab
        // back) OR the sink ended up hidden: a missed visibility flip can
        // leave the sink SW_HIDDEN while the shell is visible and foreground,
        // which shows the shell alone as blank until an overlay forces a
        // re-apply. The EVENT_SYSTEM_FOREGROUND hook is the primary trigger;
        // this periodic path is the safety net. Skip when the sink is already
        // directly below the BrowserWindow (GetWindow GW_HWNDPREV) — while
        // streaming normally that is the steady state, so without this check
        // every 400 ms tick would issue a SetWindowPos (DWM churn on a
        // window that is actively presenting).
        let browser_hwnd = STACKED_TARGET
            .get()
            .and_then(|target| target.lock().ok().and_then(|target| *target))
            .map(|target| target.browser_hwnd as Hwnd);
        let browser_foreground = browser_hwnd.is_some_and(|hwnd| hwnd == GetForegroundWindow());
        // Keep the stacked sink's RawInput mouse capture in sync with the shell
        // being foreground: arm it while streaming, release it when the user
        // alt-tabs away (so the other app's mouse never reaches the game). The
        // foreground WinEvent hook handles the common case; this tick is the
        // safety net.
        if browser_hwnd.is_some() {
            update_stacked_shell_foreground(browser_foreground);
        }
        let already_below = browser_hwnd
            .is_some_and(|browser| GetWindow(sink_hwnd, GW_HWNDPREV) as Hwnd == browser);
        if !already_below && (browser_foreground || IsWindowVisible(sink_hwnd) == 0) {
            reassert_stacked_renderer_window_zorder();
        }
        // If the first decoded frame arrived but the probe could not show the
        // sink yet (window created just after the probe), reveal it now and
        // deliver the deferred streaming event.
        maybe_finish_stacked_reveal();
    }

    /// Keep the sink window borderless and invisible to alt-tab/taskbar.
    /// GStreamer's d3d11videosink creates its window lazily when the first
    /// frame renders and re-applies its default overlapped style on recreation
    /// (codec change, state flips), so a caption can peek through the
    /// transparent shell and the sink can appear as a second alt-tab entry.
    /// This is idempotent and cheap, so it can run from the guard hook on
    /// every relevant event without flicker.
    unsafe fn enforce_stacked_renderer_window_style(sink_hwnd: Hwnd) {
        // Never steal focus, never appear in alt-tab/taskbar, and remove
        // WS_EX_APPWINDOW (which forces a taskbar entry) in case GStreamer
        // ever sets it.
        let ex_style = GetWindowLongPtrW(sink_hwnd, GWL_EXSTYLE);
        let desired_ex = (ex_style | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW;
        if desired_ex != ex_style {
            SetWindowLongPtrW(sink_hwnd, GWL_EXSTYLE, desired_ex);
            stacked_guard_log_throttled(
                "sink-exstyle",
                "info",
                format!(
                    "Stacked sink ex-style changed: 0x{:08X} -> 0x{:08X} (added NOACTIVATE/TOOLWINDOW, cleared APPWINDOW)",
                    ex_style as u32,
                    desired_ex as u32,
                ),
                Duration::from_millis(1000),
            );
        }

        // Strip the caption/frame so the sink is borderless.
        let style = GetWindowLongPtrW(sink_hwnd, GWL_STYLE);
        let borderless =
            style & !(WS_CAPTION | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU);
        if borderless != style {
            SetWindowLongPtrW(sink_hwnd, GWL_STYLE, borderless);
            SetWindowPos(
                sink_hwnd,
                null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            stacked_guard_log_throttled(
                "sink-style",
                "info",
                format!(
                    "Stacked sink style changed: 0x{:08X} -> 0x{:08X} (caption/frame stripped) + FRAMECHANGED",
                    style as u32,
                    borderless as u32,
                ),
                Duration::from_millis(1000),
            );
        }
    }

    /// Resolve the current sink HWND, re-discovering it if GStreamer recreated
    /// the window since it was cached. Returns None when no sink exists yet.
    unsafe fn resolve_stacked_sink_hwnd(target: &mut StackedTarget) -> Option<Hwnd> {
        let cached = target.sink_hwnd as Hwnd;
        if IsWindow(cached) != 0 {
            return Some(cached);
        }
        let Some(found) = find_first_own_window() else {
            return None;
        };
        target.sink_hwnd = found as isize;
        Some(found)
    }

    /// Position the video sink's own window to the stream rect and stack it
    /// directly below the Electron window. The rect arrives in BrowserWindow
    /// client coordinates, so it is translated to screen coordinates first.
    pub unsafe fn apply_stacked_renderer_surface(
        surface: &NativeRenderSurface,
        sink_window_handle: usize,
    ) {
        // Remember the latest surface so the stacked guard can re-apply it
        // after the sink window is (re)created — the first renderer publish
        // can race ahead of window creation, and the renderer only publishes
        // on DOM events, so without this the sink would stay at GStreamer's
        // default position until an overlay forced the next publish.
        if let Ok(mut pending) = STACKED_PENDING_SURFACE
            .get_or_init(|| Mutex::new(None))
            .lock()
        {
            *pending = Some(surface.clone());
        }

        let sink_hwnd = if sink_window_handle != 0 {
            sink_window_handle as Hwnd
        } else {
            // The renderer re-publishes the same surface every frame via rAF,
            // and the sink handle is stable for the whole session — reuse the
            // cached pair instead of running EnumWindows 60 times per second.
            let cached = STACKED_TARGET
                .get()
                .and_then(|target| target.lock().ok().and_then(|target| *target))
                .map(|target| target.sink_hwnd as Hwnd)
                .filter(|hwnd| IsWindow(*hwnd) != 0);
            match cached {
                Some(hwnd) => hwnd,
                None => {
                    let Some(found) = find_first_own_window() else {
                        return;
                    };
                    found
                }
            }
        };

        let Some(browser_hwnd) = surface
            .window_handle
            .as_deref()
            .and_then(|window_handle| super::parse_window_handle(window_handle).ok())
            .map(|window_handle| window_handle as Hwnd)
        else {
            return;
        };

        // Surface publishes arrive on every renderer event (overlay open,
        // resize, fullscreen, visibility) — log the transitions so a flicker
        // can be matched against what actually changed in the sink window.
        stacked_guard_log_throttled(
            "apply-entry",
            "info",
            format!(
                "Stacked renderer surface apply: sink=0x{:X} visible={}",
                sink_hwnd as usize, surface.visible,
            ),
            Duration::from_millis(500),
        );

        // Never steal focus, hide from alt-tab/taskbar, and strip the
        // caption/frame so the sink is borderless: when the shell is fullscreen
        // the sink must cover the whole monitor edge to edge, and a leftover
        // title bar or resize border would peek through the transparent shell
        // around the video. Mirrors protect_renderer_window's styling. Cheap
        // (two reads + compares) so it can run on every surface publish.
        enforce_stacked_renderer_window_style(sink_hwnd);

        if !surface.visible {
            ShowWindow(sink_hwnd, SW_HIDE);
            stacked_guard_log(
                "info",
                format!(
                    "Stacked sink hidden (surface not visible); sink=0x{:X}",
                    sink_hwnd as usize
                ),
            );
            STACKED_TARGET
                .get_or_init(|| Mutex::new(None))
                .lock()
                .ok()
                .map(|mut target| *target = None);
            return;
        }

        // Remember the pair so the stacked guard thread can keep the sink
        // aligned to the BrowserWindow while it moves or resizes (surface
        // updates only arrive on DOM resize/fullscreen/visibility changes,
        // never while the user drags the window). Preserve last_rect across
        // publishes — the renderer re-publishes the same surface on every
        // resize/fullscreen/visibility event, and resetting it here would
        // defeat the SetWindowPos dedup below (churn/flicker).
        let target = STACKED_TARGET.get_or_init(|| Mutex::new(None));
        if let Ok(mut current) = target.lock() {
            match current.as_mut() {
                Some(existing) => {
                    existing.sink_hwnd = sink_hwnd as isize;
                    existing.browser_hwnd = browser_hwnd as isize;
                }
                None => {
                    *current = Some(StackedTarget {
                        sink_hwnd: sink_hwnd as isize,
                        browser_hwnd: browser_hwnd as isize,
                        last_rect: None,
                    });
                }
            }
        }

        // The sink window mirrors the BrowserWindow's full outer rect so the
        // video fills the whole shell (the sink letterboxes it to the stream
        // aspect ratio) and stays perfectly aligned while the window moves or
        // resizes. hWndInsertAfter = browser window => the sink sits directly
        // below the Electron window in z-order, visible through its shell.
        // Dedupe against the last applied rect: the renderer re-publishes the
        // same surface on every frame via rAF while streaming, and each
        // SetWindowPos z-order reset here is a flicker candidate.
        let mut window_rect = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(browser_hwnd, &mut window_rect) == 0 {
            return;
        }
        let mut last_rect = None;
        if let Ok(current) = target.lock() {
            last_rect = current.as_ref().and_then(|target| target.last_rect);
        }
        // Until the first decoded frame reveals the sink, position it but keep
        // it hidden: the shell is still opaque (connecting screen), and a
        // visible window at GStreamer's default position would flash through
        // once the shell goes transparent — which must not happen until the
        // sink is shown at the final rect (reveal_stacked_renderer_window).
        let reveal_flags = if stacked_first_frame_revealed() {
            SWP_NOACTIVATE | SWP_SHOWWINDOW
        } else {
            SWP_NOACTIVATE | SWP_HIDEWINDOW
        };
        if last_rect != Some(window_rect) {
            SetWindowPos(
                sink_hwnd,
                browser_hwnd,
                window_rect.left,
                window_rect.top,
                window_rect.right.saturating_sub(window_rect.left),
                window_rect.bottom.saturating_sub(window_rect.top),
                reveal_flags,
            );
            stacked_guard_log(
                "info",
                format!(
                    "Stacked sink SetWindowPos (apply): rect=({},{} {}x{}) insert_after=browser flags=0x{:X} revealed={}",
                    window_rect.left,
                    window_rect.top,
                    window_rect.right.saturating_sub(window_rect.left),
                    window_rect.bottom.saturating_sub(window_rect.top),
                    reveal_flags,
                    stacked_first_frame_revealed(),
                ),
            );
        }
        if let Ok(mut current) = target.lock() {
            if let Some(target) = current.as_mut() {
                target.last_rect = Some(window_rect);
            }
        }
    }

    /// Re-assert the sink directly below the BrowserWindow without touching
    /// geometry. Non-activatable tool windows do not participate in Windows
    /// activation, so alt-tabbing away and back can leave the video stuck
    /// behind other apps — the transparent shell alone then shows blank.
    /// EVENT_SYSTEM_FOREGROUND fires when the shell regains the foreground;
    /// this pulls the sink back on top of whatever got stacked in between.
    pub unsafe fn reassert_stacked_renderer_window_zorder() {
        let target_slot = STACKED_TARGET.get_or_init(|| Mutex::new(None));
        let Ok(mut target_guard) = target_slot.lock() else {
            return;
        };
        let Some(target) = target_guard.as_mut() else {
            return;
        };
        let browser_hwnd = target.browser_hwnd as Hwnd;
        let Some(sink_hwnd) = resolve_stacked_sink_hwnd(target) else {
            return;
        };
        if IsIconic(browser_hwnd) != 0 {
            ShowWindow(sink_hwnd, SW_HIDE);
            stacked_guard_log_throttled(
                "sink-minimized",
                "info",
                format!(
                    "Stacked sink hidden (browser minimized); sink=0x{:X}",
                    sink_hwnd as usize
                ),
                Duration::from_secs(2),
            );
            return;
        }
        // Until the first decoded frame reveals the sink it must stay hidden
        // (z-order is irrelevant while invisible, and SWP_SHOWWINDOW here would
        // defeat the hold-hidden launch sequence).
        if !stacked_first_frame_revealed() {
            return;
        }
        // Keep the sink borderless / hidden from alt-tab while we are here.
        enforce_stacked_renderer_window_style(sink_hwnd);
        SetWindowPos(
            sink_hwnd,
            browser_hwnd,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        stacked_guard_log_throttled(
            "sink-zorder",
            "info",
            format!(
                "Stacked sink z-order re-asserted below browser (sink=0x{:X} browser=0x{:X})",
                sink_hwnd as usize, browser_hwnd as usize,
            ),
            Duration::from_secs(2),
        );
    }

    /// Re-apply the stacked sink position from the BrowserWindow's live outer
    /// rect. Polled by the stacked guard thread so dragging/resizing the shell
    /// keeps the video perfectly aligned (WM_MOVE/WM_SIZE are not observable
    /// from the renderer, so no surface update fires for them).
    pub unsafe fn sync_stacked_renderer_window_position() {
        let target_slot = STACKED_TARGET.get_or_init(|| Mutex::new(None));
        let Ok(mut target_guard) = target_slot.lock() else {
            return;
        };
        let Some(target) = target_guard.as_mut() else {
            return;
        };

        let mut window_rect = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let browser_hwnd = target.browser_hwnd as Hwnd;
        let Some(sink_hwnd) = resolve_stacked_sink_hwnd(target) else {
            return;
        };
        if GetWindowRect(browser_hwnd, &mut window_rect) == 0 {
            return;
        }
        if IsIconic(browser_hwnd) != 0 {
            // Browser minimized: keep the sink hidden too.
            ShowWindow(sink_hwnd, SW_HIDE);
            return;
        }
        // Keep the sink borderless / hidden from alt-tab while we are here.
        enforce_stacked_renderer_window_style(sink_hwnd);
        // Dedupe: location-change events fire in bursts while dragging/resizing
        // (and during fullscreen transitions), so skip SetWindowPos when the
        // browser rect has not actually changed.
        // Keep the sink hidden until the first decoded frame reveals it (see
        // apply_stacked_renderer_surface) — positioning alone must not show a
        // default-style window through the still-opaque shell.
        let reveal_flags = if stacked_first_frame_revealed() {
            SWP_NOACTIVATE | SWP_SHOWWINDOW
        } else {
            SWP_NOACTIVATE | SWP_HIDEWINDOW
        };
        if target.last_rect == Some(window_rect) {
            return;
        }
        target.last_rect = Some(window_rect);
        SetWindowPos(
            sink_hwnd,
            browser_hwnd,
            window_rect.left,
            window_rect.top,
            window_rect.right.saturating_sub(window_rect.left),
            window_rect.bottom.saturating_sub(window_rect.top),
            reveal_flags,
        );
        stacked_guard_log(
            "info",
            format!(
                "Stacked sink SetWindowPos (sync): rect=({},{} {}x{}) insert_after=browser flags=0x{:X} revealed={}",
                window_rect.left,
                window_rect.top,
                window_rect.right.saturating_sub(window_rect.left),
                window_rect.bottom.saturating_sub(window_rect.top),
                reveal_flags,
                stacked_first_frame_revealed(),
            ),
        );
    }

    /// Register WinEvent hooks for EVENT_OBJECT_LOCATIONCHANGE and
    /// EVENT_SYSTEM_MOVESIZEEND so the sink follows the BrowserWindow the
    /// instant it moves/resizes/finishes a fullscreen transition — no polling.
    /// The callback runs on this streamer's own message loop (OUTOFCONTEXT),
    /// so nothing is injected into the Electron process.
    pub unsafe fn arm_stacked_renderer_event_hook() -> bool {
        if STACKED_EVENT_HOOK
            .get()
            .is_some_and(|hook| hook.lock().map(|hook| !hook.is_empty()).unwrap_or(false))
        {
            return true;
        }

        // Span CREATE..LOCATIONCHANGE so the sink gets styled the moment
        // GStreamer creates/recreates it (not only when it later moves).
        let location_hook = SetWinEventHook(
            EVENT_OBJECT_CREATE,
            EVENT_OBJECT_LOCATIONCHANGE,
            null_mut(),
            Some(stacked_window_event_hook),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        let move_size_end_hook = SetWinEventHook(
            EVENT_SYSTEM_MOVESIZEEND,
            EVENT_SYSTEM_MOVESIZEEND,
            null_mut(),
            Some(stacked_window_event_hook),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        // Alt-tabbing back to the app makes the shell the foreground window.
        // The sink is a non-activatable tool window, so Windows activation
        // alone does not bring the video forward — re-assert its z-order.
        let foreground_hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            null_mut(),
            Some(stacked_window_event_hook),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        if location_hook.is_null() && move_size_end_hook.is_null() && foreground_hook.is_null() {
            return false;
        }
        let hooks = [location_hook, move_size_end_hook, foreground_hook]
            .into_iter()
            .filter(|hook| !hook.is_null())
            .map(|hook| hook as isize)
            .collect::<Vec<_>>();
        if let Ok(mut current) = STACKED_EVENT_HOOK
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
        {
            *current = hooks;
        }
        if let Ok(mut current) = STACKED_EVENT_HOOK_THREAD
            .get_or_init(|| Mutex::new(None))
            .lock()
        {
            *current = Some(GetCurrentThreadId());
        }
        true
    }

    pub unsafe fn disarm_stacked_renderer_event_hook() {
        let hooks = STACKED_EVENT_HOOK
            .get()
            .and_then(|hook| hook.lock().ok().map(|mut hook| std::mem::take(&mut *hook)))
            .unwrap_or_default();
        if let Ok(mut current) = STACKED_EVENT_HOOK_THREAD
            .get_or_init(|| Mutex::new(None))
            .lock()
        {
            *current = None;
        }
        for hook in hooks {
            UnhookWinEvent(hook as Hhook);
        }
    }

    unsafe extern "system" fn stacked_window_event_hook(
        _hook: Hhook,
        event: Dword,
        hwnd: Hwnd,
        _id_object: i32,
        _id_child: i32,
        _id_event_thread: Dword,
        _dwms_event_time: Dword,
    ) {
        // Sink-originated events first: GStreamer created/recreated/reshowed
        // its own window, which re-applies the default overlapped (bordered,
        // taskbar) style. Re-assert borderless + alt-tab invisibility
        // immediately so a caption never peeks through the transparent shell
        // and the video never shows up as a second alt-tab entry. This must not
        // depend on STACKED_TARGET: the sink window is created when the first
        // frame renders, which can precede the first renderer surface publish
        // (that race left the border visible at launch until an overlay was
        // opened). In stacked mode the sink is the only top-level window this
        // process creates, so any own-process, ownerless window lifecycle
        // event belongs to it.
        let sink_lifecycle_event = (EVENT_OBJECT_CREATE..=EVENT_OBJECT_LOCATIONCHANGE)
            .contains(&event)
            || event == EVENT_SYSTEM_MOVESIZEEND;
        if sink_lifecycle_event {
            let mut window_process_id: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut window_process_id);
            if window_process_id == GetCurrentProcessId() && GetWindow(hwnd, GW_OWNER).is_null() {
                enforce_stacked_renderer_window_style(hwnd);
                if let Ok(mut current) = STACKED_TARGET.get_or_init(|| Mutex::new(None)).lock() {
                    if let Some(current) = current.as_mut() {
                        current.sink_hwnd = hwnd as isize;
                    }
                }
                // The sink was just created/recreated. The first renderer
                // surface publish can race ahead of window creation (the
                // launch flash: video at GStreamer's default small/windowed
                // position for ~a second before the next publish positions
                // it). Re-apply the latest stored surface so the sink is
                // positioned the instant the window exists — the renderer
                // may not publish again until an overlay opens, and that gap
                // left the video at the default position for the first ~tens
                // of seconds of a session. This also reveals the sink (show +
                // position) once the first decoded frame has arrived, and
                // delivers the streaming event the probe deferred, so the
                // shell never goes transparent before the video is visible.
                let pending = STACKED_PENDING_SURFACE
                    .get()
                    .and_then(|pending| pending.lock().ok().and_then(|pending| pending.clone()));
                if let Some(pending) = pending {
                    apply_stacked_renderer_surface(&pending, hwnd as usize);
                }
                sync_stacked_renderer_window_position();
                reassert_stacked_renderer_window_zorder();
                maybe_finish_stacked_reveal();
                return;
            }
        }

        // Browser-window events (Electron process) need the tracked target.
        let Some(target) = STACKED_TARGET
            .get()
            .and_then(|target| target.lock().ok().and_then(|target| *target))
        else {
            return;
        };
        if hwnd as isize == target.browser_hwnd {
            if event == EVENT_SYSTEM_FOREGROUND {
                // Shell regained the foreground (e.g. alt-tab back): re-assert
                // the sink's z-order even though the rect may be unchanged, and
                // (re)arm the stacked sink's RawInput mouse capture.
                reassert_stacked_renderer_window_zorder();
                update_stacked_shell_foreground(true);
            } else {
                sync_stacked_renderer_window_position();
            }
        } else if event == EVENT_SYSTEM_FOREGROUND {
            // Some other window (another app) took the foreground: release the
            // stacked sink's mouse capture so the game never receives that
            // app's input. The sink itself is WS_EX_NOACTIVATE and can never
            // become foreground, so this branch is purely about alt-tab.
            update_stacked_shell_foreground(false);
        }
    }

    /// Wake the stacked event-hook thread (from the stop path) so its message
    /// loop exits and the hook is torn down promptly.
    pub unsafe fn request_stacked_event_hook_stop() {
        let Some(thread_id) = STACKED_EVENT_HOOK_THREAD
            .get()
            .and_then(|thread| thread.lock().ok().and_then(|thread| *thread))
        else {
            return;
        };
        PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
    }

    /// Blocking message pump for the stacked guard thread. Returns the
    /// GetMessageW result (0 on WM_QUIT, negative on error, positive on a
    /// delivered message) so the caller can decide when to exit.
    pub unsafe fn get_message(msg: &mut Msg) -> i32 {
        GetMessageW(msg, null_mut(), 0, 0) as i32
    }

    /// Fallback when the sink does not expose a window handle: any top-level
    /// window owned by this streamer process (the sink is the only one this
    /// process creates in stacked mode). Visibility is deliberately NOT
    /// required — the first surface update can arrive before the sink has
    /// painted its first frame, and the apply path shows the window anyway.
    fn find_first_own_window() -> Option<Hwnd> {
        let mut found = None::<Found>;
        unsafe {
            EnumWindows(
                Some(collect_own_window_candidate),
                &mut found as *mut Option<Found> as Lparam,
            );
        }
        found.map(|found| found.hwnd)
    }

    unsafe extern "system" fn collect_own_window_candidate(hwnd: Hwnd, lparam: Lparam) -> Bool {
        let found = &mut *(lparam as *mut Option<Found>);
        let mut window_process_id: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut window_process_id);
        if window_process_id == GetCurrentProcessId() && GetWindow(hwnd, GW_OWNER).is_null() {
            *found = Some(Found { hwnd });
            return 0;
        }
        1
    }

    pub unsafe fn set_render_target_surface(target: Option<(usize, NativeRenderRect)>) {
        let target_surface = target.map(|(window_handle, rect)| RenderTargetSurface {
            hwnd: window_handle as isize,
            client_rect: Rect {
                left: rect.x,
                top: rect.y,
                right: rect.x.saturating_add(rect.width.max(2)),
                bottom: rect.y.saturating_add(rect.height.max(2)),
            },
        });
        let slot = RENDER_TARGET_SURFACE.get_or_init(|| Mutex::new(None));
        if let Ok(mut current) = slot.lock() {
            *current = target_surface;
        }
    }

    pub unsafe fn set_input_event_sender(sender: Option<Sender<NativeWindowInputEvent>>) {
        let input_stopped = sender.is_none();
        let slot = INPUT_EVENT_SENDER.get_or_init(|| Mutex::new(None));
        if let Ok(mut current) = slot.lock() {
            *current = sender;
        }
        if input_stopped {
            unregister_raw_input_devices();
        }
    }

    pub unsafe fn set_shortcut_bindings(bindings: NativeStreamerShortcutBindings) {
        let matcher = SHORTCUT_MATCHER.get_or_init(|| Mutex::new(NativeShortcutMatcher::default()));
        if let Ok(mut current) = matcher.lock() {
            *current = NativeShortcutMatcher::from_bindings(&bindings);
        }
    }

    pub unsafe fn clear_shortcut_bindings() {
        let matcher = SHORTCUT_MATCHER.get_or_init(|| Mutex::new(NativeShortcutMatcher::default()));
        if let Ok(mut current) = matcher.lock() {
            *current = NativeShortcutMatcher::default();
        }
    }

    pub unsafe fn release_current_input_capture() {
        let Some(captured) = CAPTURED_HWND
            .get()
            .and_then(|captured| captured.lock().ok().and_then(|captured| *captured))
        else {
            if !crate::gstreamer_config::use_internal_renderer() {
                unregister_raw_input_devices();
            }
            return;
        };

        release_input_capture(captured as Hwnd);
    }

    /// Arm RawInput on the internal child HWND (sibling of Intermediate D3D).
    /// Chains over the child's existing wndproc so SET_BOUNDS still works.
    pub unsafe fn arm_internal_child_input(hwnd: usize) -> bool {
        if hwnd == 0 {
            return false;
        }
        let hwnd = hwnd as Hwnd;
        let protected_slot = PROTECTED_HWND.get_or_init(|| Mutex::new(None));
        if let Ok(mut protected) = protected_slot.lock() {
            *protected = Some(hwnd as isize);
        }
        let wndproc_installed = install_input_wndproc(hwnd);
        let keyboard_registered = register_internal_raw_keyboard(hwnd);
        wndproc_installed || keyboard_registered
    }

    /// Arm mouse + keyboard RawInput capture on the stacked sink window (the
    /// streamer's own top-level video window, positioned directly below the
    /// Electron shell). This is the low-latency input path for stacked mode:
    /// raw HID events travel sink window -> data channel entirely inside this
    /// process, bypassing the renderer -> main -> stdin bridge and its
    /// batching/delay. Escape and UI shortcuts stay with Electron (the raw
    /// path skips them), and keyboard uses INPUTSINK without NOLEGACY so
    /// Electron keeps receiving legacy keys for IME / browser UI.
    ///
    /// Guards: only in stacked render mode, only while the input bridge is
    /// running (data channels created) and the Electron shell is the foreground
    /// window (alt-tab must never leak input), and idempotent (re-arming an
    /// already-captured sink is a no-op).
    pub unsafe fn arm_stacked_sink_input_capture() -> bool {
        if !use_stacked_renderer() {
            return false;
        }
        // Opt-in toggle (settings > native streamer): the sink-native RawInput
        // bypass is experimental; by default stacked mode rides the Electron
        // bridge (addon mouse + DOM keyboard) like the web path.
        if !crate::gstreamer_input::native_sink_input_capture_enabled() {
            return false;
        }
        // Never re-arm while input is paused (an overlay / quick menu is open):
        // the guard tick and foreground hook call this repeatedly, and re-arming
        // would re-hide the cursor over the overlay and re-own input the game
        // should not receive (the visible arm/release cursor flicker).
        if crate::gstreamer_input::input_paused() {
            return false;
        }
        let bridge_running = INPUT_EVENT_SENDER
            .get()
            .and_then(|sender| sender.lock().ok().map(|sender| sender.is_some()))
            .unwrap_or(false);
        if !bridge_running {
            return false;
        }
        if !STACKED_SHELL_FOREGROUND
            .get_or_init(|| AtomicBool::new(false))
            .load(Ordering::SeqCst)
        {
            return false;
        }
        let Some(sink_hwnd) = resolve_stacked_sink_window() else {
            return false;
        };
        if is_input_captured(sink_hwnd) {
            return true;
        }
        install_input_wndproc(sink_hwnd);
        begin_input_capture(sink_hwnd);
        true
    }

    /// Resolve the stacked sink's top-level window: the cached STACKED_TARGET
    /// handle when still valid, otherwise rediscover the streamer's own window
    /// (the sink is the only top-level window this process owns in stacked
    /// mode).
    unsafe fn resolve_stacked_sink_window() -> Option<Hwnd> {
        let cached = STACKED_TARGET
            .get()
            .and_then(|target| target.lock().ok().and_then(|target| *target))
            .map(|target| target.sink_hwnd as Hwnd)
            .filter(|hwnd| IsWindow(*hwnd) != 0);
        if let Some(hwnd) = cached {
            return Some(hwnd);
        }
        find_first_own_window()
    }

    /// Track whether the Electron shell is the foreground window and keep the
    /// stacked sink's mouse capture in sync: arm it while the shell is
    /// foreground, release it the moment another window takes the foreground
    /// (alt-tab), so the game never receives another app's input. Called from
    /// the foreground WinEvent hook and the stacked guard tick.
    pub unsafe fn update_stacked_shell_foreground(foreground: bool) {
        STACKED_SHELL_FOREGROUND
            .get_or_init(|| AtomicBool::new(false))
            .store(foreground, Ordering::SeqCst);
        if foreground {
            arm_stacked_sink_input_capture();
        } else if captured_hwnd().is_some() {
            release_current_input_capture();
        }
    }

    pub unsafe fn protect_process_renderer_window() -> bool {
        let mut state = EnumState {
            process_id: GetCurrentProcessId(),
            candidates: Vec::new(),
        };
        EnumWindows(
            Some(collect_renderer_window_candidate),
            &mut state as *mut EnumState as Lparam,
        );

        let Some(candidate) = state
            .candidates
            .into_iter()
            .max_by_key(|candidate| candidate.area)
        else {
            return false;
        };

        protect_renderer_window(candidate.hwnd)
    }

    unsafe extern "system" fn collect_renderer_window_candidate(
        hwnd: Hwnd,
        lparam: Lparam,
    ) -> Bool {
        let state = &mut *(lparam as *mut EnumState);
        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, &mut process_id);
        if process_id != state.process_id || IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
            return 1;
        }

        if !GetWindow(hwnd, GW_OWNER).is_null() {
            return 1;
        }

        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if (ex_style & (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE)) != 0 {
            return 1;
        }

        let mut rect = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return 1;
        }
        let width = rect.right.saturating_sub(rect.left);
        let height = rect.bottom.saturating_sub(rect.top);
        if width < 320 || height < 180 {
            return 1;
        }

        state.candidates.push(WindowCandidate {
            hwnd,
            area: i64::from(width) * i64::from(height),
        });
        1
    }

    unsafe fn protect_renderer_window(hwnd: Hwnd) -> bool {
        let protected_slot = PROTECTED_HWND.get_or_init(|| Mutex::new(None));
        if let Ok(mut protected) = protected_slot.lock() {
            *protected = Some(hwnd as isize);
        }

        let mut configured = false;
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let desired = current & !(WS_EX_NOACTIVATE | WS_EX_TRANSPARENT);
        if desired != current {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired);
            configured = true;
        }

        let current_style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let fullscreen_style = current_style
            & !(WS_CAPTION | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU);
        if fullscreen_style != current_style {
            SetWindowLongPtrW(hwnd, GWL_STYLE, fullscreen_style);
            configured = true;
        }

        if install_input_wndproc(hwnd) {
            SetForegroundWindow(hwnd);
            SetFocus(hwnd);
            configured = true;
        }

        if let Some(rect) = target_renderer_rect().or_else(|| monitor_rect_for_window(hwnd)) {
            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                rect.left,
                rect.top,
                rect.right.saturating_sub(rect.left).max(2),
                rect.bottom.saturating_sub(rect.top).max(2),
                SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            configured = true;
        } else {
            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }

        configured
    }

    unsafe fn render_rect_to_screen_rect(hwnd: Hwnd, rect: Rect) -> Option<Rect> {
        if hwnd.is_null() {
            return None;
        }
        let mut origin = Point {
            x: rect.left,
            y: rect.top,
        };
        if ClientToScreen(hwnd, &mut origin) == 0 {
            return None;
        }
        let width = rect.right.saturating_sub(rect.left).max(2);
        let height = rect.bottom.saturating_sub(rect.top).max(2);

        Some(Rect {
            left: origin.x,
            top: origin.y,
            right: origin.x.saturating_add(width),
            bottom: origin.y.saturating_add(height),
        })
    }

    unsafe fn install_input_wndproc(hwnd: Hwnd) -> bool {
        let key = hwnd as isize;
        let map = ORIGINAL_WNDPROCS.get_or_init(|| Mutex::new(HashMap::new()));
        let Ok(mut map) = map.lock() else {
            return false;
        };
        if map.contains_key(&key) {
            return false;
        }

        let previous = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, renderer_window_wndproc as isize);
        if previous == 0 {
            return false;
        }
        map.insert(key, previous);
        true
    }

    unsafe extern "system" fn renderer_window_wndproc(
        hwnd: Hwnd,
        message: Uint,
        wparam: Wparam,
        lparam: Lparam,
    ) -> Lresult {
        if message == WM_NCHITTEST {
            return HTCLIENT;
        }
        if message == WM_MOUSEACTIVATE {
            begin_input_capture(hwnd);
            return MA_ACTIVATE;
        }
        if message == WM_SETCURSOR && is_input_captured(hwnd) {
            SetCursor(null_mut());
            return 1;
        }
        if message == WM_INPUT {
            handle_raw_input(lparam as Hrawinput);
            return 0;
        }
        let handled_legacy_shortcut = is_keyboard_message(message)
            && handle_legacy_shortcut_keyboard(message, wparam, lparam);
        if handled_legacy_shortcut {
            return 0;
        }
        if is_escape_keyboard_message(message, wparam) {
            if !is_input_captured(hwnd) {
                begin_input_capture(hwnd);
            }
            handle_legacy_escape_keyboard(message, lparam);
            return 0;
        }
        if message == WM_KILLFOCUS || (message == WM_ACTIVATE && (wparam & 0xffff) == WA_INACTIVE) {
            release_input_capture(hwnd);
        }
        if let Some((button, pressed)) = legacy_mouse_button(message, wparam) {
            let was_captured = is_input_captured(hwnd);
            if pressed && !was_captured {
                begin_input_capture(hwnd);
                emit_input_event(NativeWindowInputEvent::MouseButton {
                    pressed,
                    button,
                    timestamp_us: timestamp_us(),
                });
            }
        }

        let key = hwnd as isize;
        let previous = ORIGINAL_WNDPROCS
            .get()
            .and_then(|map| map.lock().ok().and_then(|map| map.get(&key).copied()));
        if let Some(previous) = previous {
            return CallWindowProcW(previous, hwnd, message, wparam, lparam);
        }

        DefWindowProcW(hwnd, message, wparam, lparam)
    }

    unsafe fn monitor_rect_for_window(hwnd: Hwnd) -> Option<Rect> {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return None;
        }

        let mut info = MonitorInfo {
            cb_size: std::mem::size_of::<MonitorInfo>() as Dword,
            rc_monitor: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            rc_work: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            dw_flags: 0,
        };
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }

        Some(info.rc_monitor)
    }

    unsafe fn target_renderer_rect() -> Option<Rect> {
        let target = RENDER_TARGET_SURFACE
            .get()
            .and_then(|surface| surface.lock().ok().and_then(|surface| *surface))?;
        let hwnd = target.hwnd as Hwnd;
        render_rect_to_screen_rect(hwnd, target.client_rect)
            .or_else(|| monitor_rect_for_window(hwnd))
    }

    unsafe fn begin_input_capture(hwnd: Hwnd) {
        let stacked = crate::gstreamer_config::use_stacked_renderer();
        // External floating window: take OS focus so RawInput + ClipCursor work
        // without INPUTSINK. Internal child / stacked sink: leave Electron as
        // foreground so its shortcut keydown handlers keep working; keyboard
        // arrives via INPUTSINK, and the stacked sink must never steal focus
        // (it sits below the shell, which stays the interactive layer).
        if !crate::gstreamer_config::use_internal_renderer() && !stacked {
            SetForegroundWindow(hwnd);
            SetFocus(hwnd);
        }
        if !stacked {
            // Child/external capture: redirect all mouse messages to the
            // capture window while active (the renderer pauses capture when an
            // overlay opens, so the shell UI still works).
            SetCapture(hwnd);
        }
        if stacked {
            // Stacked: mouse + keyboard RawInput against the sink window. No
            // SetCapture — with RIDEV_INPUTSINK the sink receives raw input
            // for the whole desktop while the Electron shell keeps receiving
            // (and acting on) its own legacy messages, so the shell UI stays
            // interactive above the video. Keyboard registers INPUTSINK
            // WITHOUT NOLEGACY so Electron's IME / layout handling keeps
            // working; the raw path skips Escape and UI shortcuts (Electron
            // owns those).
            register_stacked_raw_mouse(hwnd);
            register_stacked_raw_keyboard(hwnd);
        } else {
            register_raw_input_devices(hwnd);
        }
        if stacked {
            // Stacked: confine the cursor to the sink window's own rect (the
            // stream area). target_renderer_rect is not populated in stacked
            // mode — that surface path belongs to the embedded/external
            // renderers.
            let mut rect = Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if GetWindowRect(hwnd, &mut rect) != 0 {
                ClipCursor(&rect);
            }
        } else if let Some(rect) = target_renderer_rect().or_else(|| monitor_rect_for_window(hwnd))
        {
            ClipCursor(&rect);
        }
        hide_cursor();
        emit_input_capture_changed(true);
        crate::gstreamer_input::set_native_input_path(if stacked {
            "sink-native"
        } else if crate::gstreamer_config::use_internal_renderer() {
            "internal"
        } else {
            "external"
        });

        let slot = CAPTURED_HWND.get_or_init(|| Mutex::new(None));
        if let Ok(mut captured) = slot.lock() {
            *captured = Some(hwnd as isize);
        }
        sync_lock_keys_state(true);
    }

    /// Register mouse-only RawInput for the stacked sink window. INPUTSINK
    /// delivers WM_INPUT even though Electron remains the top-level foreground
    /// window; NOLEGACY keeps GStreamer's window from additionally processing
    /// legacy mouse messages.
    unsafe fn register_stacked_raw_mouse(hwnd: Hwnd) -> bool {
        let device = RawInputDevice {
            us_usage_page: 0x01,
            us_usage: 0x02,
            dw_flags: RIDEV_NOLEGACY | RIDEV_INPUTSINK,
            hwnd_target: hwnd,
        };

        RegisterRawInputDevices(&device, 1, std::mem::size_of::<RawInputDevice>() as u32) != 0
    }

    /// Register keyboard RawInput for the stacked sink window. INPUTSINK only
    /// (deliberately NOT NOLEGACY): Electron stays the foreground window and
    /// must keep receiving legacy WM_KEYDOWN/WM_CHAR for IME composition,
    /// layout APIs and the browser UI, while the sink receives the raw keys in
    /// parallel. Escape and UI shortcuts are skipped on the raw path so
    /// Electron's own keydown owns them.
    unsafe fn register_stacked_raw_keyboard(hwnd: Hwnd) -> bool {
        let device = RawInputDevice {
            us_usage_page: 0x01,
            us_usage: 0x06,
            dw_flags: RIDEV_INPUTSINK,
            hwnd_target: hwnd,
        };

        RegisterRawInputDevices(&device, 1, std::mem::size_of::<RawInputDevice>() as u32) != 0
    }

    unsafe fn release_input_capture(hwnd: Hwnd) {
        cancel_escape_hold_to_minimize_timer();
        clear_escape_key_press();
        let slot = CAPTURED_HWND.get_or_init(|| Mutex::new(None));
        let mut should_release = false;
        if let Ok(mut captured) = slot.lock() {
            should_release = captured.is_some_and(|captured| captured == hwnd as isize);
            if should_release {
                *captured = None;
            }
        }

        if !should_release {
            return;
        }

        release_pressed_keys();
        ReleaseCapture();
        ClipCursor(null());
        show_cursor();
        emit_input_capture_changed(false);
        // Capture released: input now travels over the renderer bridge
        // (addon / pointer-lock → IPC → stdin) until the next re-arm.
        crate::gstreamer_input::set_native_input_path("bridge");
        if crate::gstreamer_config::use_internal_renderer() {
            // F10/F8 release relative mouse capture, but the internal stream
            // must keep receiving keyboard input (especially Escape) while the
            // Electron window remains foreground.
            unregister_raw_mouse_device();
            register_internal_raw_keyboard(hwnd);
        } else {
            unregister_raw_input_devices();
        }
        SetWindowPos(
            hwnd,
            HWND_NOTOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }

    fn is_input_captured(hwnd: Hwnd) -> bool {
        CAPTURED_HWND
            .get()
            .and_then(|captured| captured.lock().ok().and_then(|captured| *captured))
            .is_some_and(|captured| captured == hwnd as isize)
    }

    fn captured_hwnd() -> Option<isize> {
        CAPTURED_HWND
            .get()
            .and_then(|captured| captured.lock().ok().and_then(|captured| *captured))
    }

    fn protected_hwnd() -> Option<isize> {
        PROTECTED_HWND
            .get()
            .and_then(|captured| captured.lock().ok().and_then(|captured| *captured))
    }

    unsafe fn start_escape_hold_to_minimize_timer() {
        let Some(hwnd) = captured_hwnd() else {
            return;
        };

        let token = ESCAPE_HOLD_TOKEN
            .get_or_init(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);
        let slot = ESCAPE_HOLD_HWND.get_or_init(|| Mutex::new(None));
        if let Ok(mut held_hwnd) = slot.lock() {
            *held_hwnd = Some(hwnd);
        }

        thread::spawn(move || {
            thread::sleep(ESCAPE_HOLD_TO_MINIMIZE);
            unsafe {
                minimize_window_if_escape_still_held(hwnd, token);
            }
        });
    }

    fn cancel_escape_hold_to_minimize_timer() {
        ESCAPE_HOLD_TOKEN
            .get_or_init(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::SeqCst);
        let slot = ESCAPE_HOLD_HWND.get_or_init(|| Mutex::new(None));
        if let Ok(mut held_hwnd) = slot.lock() {
            *held_hwnd = None;
        }
    }

    unsafe fn minimize_window_if_escape_still_held(hwnd: isize, token: u64) {
        let current_token = ESCAPE_HOLD_TOKEN
            .get_or_init(|| AtomicU64::new(0))
            .load(Ordering::SeqCst);
        if current_token != token {
            return;
        }

        let still_held = ESCAPE_HOLD_HWND
            .get()
            .and_then(|held_hwnd| held_hwnd.lock().ok().and_then(|held_hwnd| *held_hwnd))
            .is_some_and(|held_hwnd| held_hwnd == hwnd);
        if !still_held {
            return;
        }

        // Consume the held Escape so key-up does not also send a tap to GFN.
        clear_escape_key_press();
        cancel_escape_hold_to_minimize_timer();

        let hwnd = hwnd as Hwnd;
        release_input_capture(hwnd);

        ShowWindow(hwnd, SW_MINIMIZE);
    }

    unsafe fn register_raw_input_devices(hwnd: Hwnd) -> bool {
        let keyboard_flags = if crate::gstreamer_config::use_internal_renderer() {
            // Internal: Electron stays foreground and must keep receiving legacy
            // WM_KEYDOWN for UI shortcuts. Do NOT set RIDEV_NOLEGACY on keyboard
            // or Electron shortcuts die. INPUTSINK delivers WM_INPUT while Electron
            // remains the top-level foreground window.
            RIDEV_INPUTSINK
        } else {
            RIDEV_NOLEGACY
        };
        let mouse_flags = if crate::gstreamer_config::use_internal_renderer() {
            RIDEV_NOLEGACY | RIDEV_CAPTUREMOUSE | RIDEV_INPUTSINK
        } else {
            RIDEV_NOLEGACY | RIDEV_CAPTUREMOUSE
        };
        let devices = [
            RawInputDevice {
                us_usage_page: 0x01,
                us_usage: 0x02,
                dw_flags: mouse_flags,
                hwnd_target: hwnd,
            },
            RawInputDevice {
                us_usage_page: 0x01,
                us_usage: 0x06,
                dw_flags: keyboard_flags,
                hwnd_target: hwnd,
            },
        ];

        RegisterRawInputDevices(
            devices.as_ptr(),
            devices.len() as u32,
            std::mem::size_of::<RawInputDevice>() as u32,
        ) != 0
    }

    unsafe fn register_internal_raw_keyboard(hwnd: Hwnd) -> bool {
        let device = RawInputDevice {
            us_usage_page: 0x01,
            us_usage: 0x06,
            dw_flags: RIDEV_INPUTSINK,
            hwnd_target: hwnd,
        };

        RegisterRawInputDevices(&device, 1, std::mem::size_of::<RawInputDevice>() as u32) != 0
    }

    unsafe fn unregister_raw_mouse_device() -> bool {
        let device = RawInputDevice {
            us_usage_page: 0x01,
            us_usage: 0x02,
            dw_flags: RIDEV_REMOVE,
            hwnd_target: null_mut(),
        };

        RegisterRawInputDevices(&device, 1, std::mem::size_of::<RawInputDevice>() as u32) != 0
    }

    unsafe fn unregister_raw_input_devices() -> bool {
        let devices = [
            RawInputDevice {
                us_usage_page: 0x01,
                us_usage: 0x02,
                dw_flags: RIDEV_REMOVE,
                hwnd_target: null_mut(),
            },
            RawInputDevice {
                us_usage_page: 0x01,
                us_usage: 0x06,
                dw_flags: RIDEV_REMOVE,
                hwnd_target: null_mut(),
            },
        ];

        RegisterRawInputDevices(
            devices.as_ptr(),
            devices.len() as u32,
            std::mem::size_of::<RawInputDevice>() as u32,
        ) != 0
    }

    unsafe fn handle_raw_input(raw_input: Hrawinput) {
        let mut size = 0u32;
        let header_size = std::mem::size_of::<RawInputHeader>() as u32;
        let query = GetRawInputData(raw_input, RID_INPUT, null_mut(), &mut size, header_size);
        if query == u32::MAX || size < header_size {
            return;
        }

        let mut buffer = vec![0u8; size as usize];
        let read = GetRawInputData(
            raw_input,
            RID_INPUT,
            buffer.as_mut_ptr() as *mut c_void,
            &mut size,
            header_size,
        );
        if read == u32::MAX || read == 0 || buffer.len() < header_size as usize {
            return;
        }

        let header = &*(buffer.as_ptr() as *const RawInputHeader);
        let data = buffer.as_ptr().add(std::mem::size_of::<RawInputHeader>());
        match header.dw_type {
            RIM_TYPEMOUSE => handle_raw_mouse(&*(data as *const RawMouse)),
            RIM_TYPEKEYBOARD => handle_raw_keyboard(&*(data as *const RawKeyboard)),
            _ => {}
        }
    }

    unsafe fn handle_raw_mouse(raw: &RawMouse) {
        if CAPTURED_HWND
            .get()
            .and_then(|captured| captured.lock().ok().and_then(|captured| *captured))
            .is_none()
        {
            return;
        }
        // Stacked mode registers the sink with RIDEV_INPUTSINK, so raw deltas
        // arrive for the whole desktop — including while the user is alt-tabbed
        // to another app. Never forward those: only while the Electron shell is
        // the foreground window (the foreground WinEvent hook keeps this flag
        // current, and the stacked guard tick is the safety net).
        if crate::gstreamer_config::use_stacked_renderer()
            && !STACKED_SHELL_FOREGROUND
                .get_or_init(|| AtomicBool::new(false))
                .load(Ordering::SeqCst)
        {
            return;
        }

        let timestamp_us = timestamp_us();
        // Apply the configured mouse sensitivity / acceleration in-process (same
        // formula the renderer uses for the addon and DOM pointer-lock paths) so
        // the sink-native capture feels exactly like the mouse settings instead
        // of raw unscaled HID counts.
        let sensitivity = crate::gstreamer_input::native_mouse_sensitivity();
        let acceleration_percent = crate::gstreamer_input::native_mouse_acceleration_percent();
        let mut dx_f = f64::from(raw.l_last_x) * sensitivity;
        let mut dy_f = f64::from(raw.l_last_y) * sensitivity;
        if acceleration_percent > 1.0 {
            let speed = (dx_f * dx_f + dy_f * dy_f).sqrt();
            let strength = (acceleration_percent - 1.0) / 149.0;
            let accel_factor = 1.0 + (0.6 * strength).min((speed / 50.0) * strength);
            dx_f *= accel_factor;
            dy_f *= accel_factor;
        }
        let dx = clamp_i32_to_i16(dx_f.round() as i32);
        let dy = clamp_i32_to_i16(dy_f.round() as i32);
        if dx != 0 || dy != 0 {
            emit_input_event(NativeWindowInputEvent::MouseMove {
                dx,
                dy,
                timestamp_us,
            });
        }

        let button_flags = (raw.buttons & 0xffff) as u16;
        let button_data = (raw.buttons >> 16) as u16;
        emit_raw_mouse_button_events(button_flags, timestamp_us);
        if (button_flags & RI_MOUSE_WHEEL) != 0 {
            emit_input_event(NativeWindowInputEvent::MouseWheel {
                delta: button_data as i16,
                timestamp_us,
            });
        }
    }

    unsafe fn emit_raw_mouse_button_events(flags: u16, timestamp_us: u64) {
        let pairs = [
            (RI_MOUSE_LEFT_BUTTON_DOWN, 1, true),
            (RI_MOUSE_LEFT_BUTTON_UP, 1, false),
            (RI_MOUSE_MIDDLE_BUTTON_DOWN, 2, true),
            (RI_MOUSE_MIDDLE_BUTTON_UP, 2, false),
            (RI_MOUSE_RIGHT_BUTTON_DOWN, 3, true),
            (RI_MOUSE_RIGHT_BUTTON_UP, 3, false),
            (RI_MOUSE_BUTTON_4_DOWN, 4, true),
            (RI_MOUSE_BUTTON_4_UP, 4, false),
            (RI_MOUSE_BUTTON_5_DOWN, 5, true),
            (RI_MOUSE_BUTTON_5_UP, 5, false),
        ];

        for (flag, button, pressed) in pairs {
            if (flags & flag) != 0 {
                emit_input_event(NativeWindowInputEvent::MouseButton {
                    pressed,
                    button,
                    timestamp_us,
                });
            }
        }
    }

    unsafe fn handle_raw_keyboard(raw: &RawKeyboard) {
        if raw.vkey == 0xff {
            return;
        }
        // Same foreground gate as the stacked mouse: INPUTSINK delivers keys
        // for the whole desktop, so never forward them while the user is
        // alt-tabbed to another app (capture is also released on foreground
        // loss, this check covers queued WM_INPUTs in the gap).
        if crate::gstreamer_config::use_stacked_renderer()
            && !STACKED_SHELL_FOREGROUND
                .get_or_init(|| AtomicBool::new(false))
                .load(Ordering::SeqCst)
        {
            return;
        }

        let pressed = match raw.message {
            WM_KEYDOWN | WM_SYSKEYDOWN => true,
            WM_KEYUP | WM_SYSKEYUP => false,
            _ => (raw.flags & RI_KEY_BREAK) == 0,
        };
        let keycode = normalize_virtual_key(raw.vkey, raw.make_code, raw.flags);
        let mut scancode = normalize_scancode(raw.make_code, raw.flags);
        if keycode == VK_ESCAPE && scancode == 0 {
            scancode = ESCAPE_SCANCODE;
        }
        if keycode == 0 || scancode == 0 {
            return;
        }
        handle_keyboard_state(keycode, scancode, pressed);
    }

    unsafe fn handle_legacy_escape_keyboard(message: Uint, lparam: Lparam) {
        let pressed = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
        let mut scancode = legacy_keyboard_scancode(lparam);
        if scancode == 0 {
            scancode = ESCAPE_SCANCODE;
        }
        handle_keyboard_state(VK_ESCAPE, scancode, pressed);
    }

    fn is_escape_keyboard_message(message: Uint, wparam: Wparam) -> bool {
        matches!(message, WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP)
            && (wparam as u16) == VK_ESCAPE
    }

    fn is_keyboard_message(message: Uint) -> bool {
        matches!(message, WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP)
    }

    fn legacy_keyboard_scancode(lparam: Lparam) -> u16 {
        let scancode = ((lparam >> 16) & 0xff) as u16;
        if scancode == 0 {
            return 0;
        }
        if ((lparam >> 24) & 0x01) != 0 {
            0xe000 | scancode
        } else {
            scancode
        }
    }

    unsafe fn handle_legacy_shortcut_keyboard(
        message: Uint,
        wparam: Wparam,
        lparam: Lparam,
    ) -> bool {
        let keycode = wparam as u16;
        if keycode == VK_ESCAPE {
            return false;
        }

        let pressed = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
        let scancode = legacy_keyboard_scancode(lparam);
        let key_id = if scancode == 0 { keycode } else { scancode };
        let suppressed_keys = LEGACY_SUPPRESSED_KEYS.get_or_init(|| Mutex::new(HashSet::new()));
        let Ok(mut suppressed_keys) = suppressed_keys.lock() else {
            return false;
        };

        if !pressed {
            return suppressed_keys.remove(&key_id);
        }

        if suppressed_keys.contains(&key_id) {
            return true;
        }

        let modifiers = current_legacy_modifier_flags();
        if pressed && is_clipboard_paste_shortcut(keycode, modifiers) {
            suppressed_keys.insert(key_id);
            drop(suppressed_keys);
            emit_clipboard_paste_request();
            return true;
        }

        let Some(action) = shortcut_action_for_keypress(keycode, scancode, modifiers) else {
            return false;
        };

        suppressed_keys.insert(key_id);
        drop(suppressed_keys);
        handle_shortcut_action(action);
        true
    }

    unsafe fn handle_keyboard_state(keycode: u16, scancode: u16, pressed: bool) {
        sync_lock_keys_state(false);

        let keys = PRESSED_KEYS.get_or_init(|| Mutex::new(HashMap::new()));
        let Ok(mut keys) = keys.lock() else {
            return;
        };
        if pressed && keycode == VK_TAB && is_alt_modifier_down(&keys) {
            drop(keys);
            release_current_input_capture();
            return;
        }
        if keycode == VK_ESCAPE {
            drop(keys);
            // In the internal renderer AND the stacked sink, Electron must
            // intercept the legacy Escape before Chromium exits fullscreen,
            // then forward exactly one tap over IPC. RawInput still owns every
            // other key in these modes. The external native window has no
            // Electron interception and keeps this path.
            if crate::gstreamer_config::use_internal_renderer()
                || crate::gstreamer_config::use_stacked_renderer()
            {
                return;
            }
            handle_escape_keyboard_state(scancode, pressed);
            return;
        }
        let previous = keys.get(&scancode).copied();
        if pressed {
            if previous.is_some() {
                return;
            }
            let modifiers = pressed_key_modifier_flags(&keys, keycode);
            if is_clipboard_paste_shortcut(keycode, modifiers) {
                keys.insert(
                    scancode,
                    PressedKey {
                        keycode,
                        scancode,
                        suppressed: true,
                    },
                );
                drop(keys);
                emit_clipboard_paste_request();
                return;
            }
            if let Some(action) = shortcut_action_for_keypress(keycode, scancode, modifiers) {
                keys.insert(
                    scancode,
                    PressedKey {
                        keycode,
                        scancode,
                        suppressed: true,
                    },
                );
                drop(keys);
                handle_shortcut_action(action);
                return;
            }
            keys.insert(
                scancode,
                PressedKey {
                    keycode,
                    scancode,
                    suppressed: false,
                },
            );
        } else if let Some(previous) = keys.remove(&scancode) {
            if previous.suppressed {
                return;
            }
        }
        let modifiers = pressed_key_modifier_flags(&keys, keycode);
        drop(keys);

        emit_input_event(NativeWindowInputEvent::Key {
            pressed,
            keycode,
            scancode,
            modifiers,
            timestamp_us: timestamp_us(),
        });
    }

    unsafe fn handle_escape_keyboard_state(scancode: u16, pressed: bool) {
        let slot = ESCAPE_KEY_PRESS.get_or_init(|| Mutex::new(None));
        let Ok(mut escape_press) = slot.lock() else {
            return;
        };

        if pressed {
            let should_start_hold_timer = if let Some(current) = escape_press.as_mut() {
                let should_start = !crate::gstreamer_config::use_internal_renderer()
                    && !current.hold_timer_armed
                    && captured_hwnd().is_some();
                if should_start {
                    current.hold_timer_armed = true;
                }
                should_start
            } else {
                let hold_timer_armed =
                    !crate::gstreamer_config::use_internal_renderer() && captured_hwnd().is_some();
                *escape_press = Some(EscapeKeyPress {
                    scancode,
                    hold_timer_armed,
                });
                hold_timer_armed
            };
            drop(escape_press);
            if should_start_hold_timer {
                start_escape_hold_to_minimize_timer();
            }
            return;
        }

        let Some(escape_press) = escape_press.take() else {
            cancel_escape_hold_to_minimize_timer();
            return;
        };
        let scancode = escape_press.scancode;

        cancel_escape_hold_to_minimize_timer();
        send_escape_tap(scancode);
    }

    fn clear_escape_key_press() {
        let slot = ESCAPE_KEY_PRESS.get_or_init(|| Mutex::new(None));
        if let Ok(mut escape_press) = slot.lock() {
            *escape_press = None;
        }
    }

    fn send_escape_tap(scancode: u16) {
        let keydown_timestamp_us = timestamp_us();
        emit_input_event(NativeWindowInputEvent::Key {
            pressed: true,
            keycode: VK_ESCAPE,
            scancode,
            modifiers: 0,
            timestamp_us: keydown_timestamp_us,
        });
        emit_input_event(NativeWindowInputEvent::Key {
            pressed: false,
            keycode: VK_ESCAPE,
            scancode,
            modifiers: 0,
            timestamp_us: timestamp_us(),
        });
    }

    unsafe fn release_pressed_keys() {
        let keys = PRESSED_KEYS.get_or_init(|| Mutex::new(HashMap::new()));
        let Ok(mut keys) = keys.lock() else {
            return;
        };
        let pressed = keys.values().copied().collect::<Vec<_>>();
        keys.clear();
        drop(keys);

        let timestamp_us = timestamp_us();
        for key in pressed {
            if key.suppressed {
                continue;
            }
            emit_input_event(NativeWindowInputEvent::Key {
                pressed: false,
                keycode: key.keycode,
                scancode: key.scancode,
                modifiers: 0,
                timestamp_us,
            });
        }
    }

    fn normalize_virtual_key(vkey: u16, make_code: u16, flags: u16) -> u16 {
        match vkey {
            VK_SHIFT => match make_code {
                0x36 => VK_RSHIFT,
                _ => VK_LSHIFT,
            },
            VK_CONTROL => {
                if (flags & RI_KEY_E0) != 0 {
                    VK_RCONTROL
                } else {
                    VK_LCONTROL
                }
            }
            VK_MENU => {
                if (flags & RI_KEY_E0) != 0 {
                    VK_RMENU
                } else {
                    VK_LMENU
                }
            }
            _ => vkey,
        }
    }

    fn normalize_scancode(make_code: u16, flags: u16) -> u16 {
        if make_code == 0 {
            return 0;
        }
        if (flags & RI_KEY_E0) != 0 {
            0xe000 | make_code
        } else if (flags & RI_KEY_E1) != 0 {
            0xe100 | make_code
        } else {
            make_code
        }
    }

    /// Lock-key bitmask for INPUT_LOCK_KEYS_SYNC (official GFN iS() on desktop Windows).
    unsafe fn lock_keys_sync_state() -> u8 {
        let mut state = 0x10;
        if (GetKeyState(VK_CAPITAL) & 0x0001) != 0 {
            state |= 0x01;
        }
        state |= 0x20;
        state |= 0x40;
        if (GetKeyState(VK_NUMLOCK) & 0x0001) != 0 {
            state |= 0x02;
        }
        if (GetKeyState(VK_SCROLL) & 0x0001) != 0 {
            state |= 0x04;
        }
        state
    }

    unsafe fn sync_lock_keys_state(force: bool) {
        let state = lock_keys_sync_state();
        let slot = LAST_LOCK_KEYS_STATE.get_or_init(|| Mutex::new(0));
        let Ok(mut last) = slot.lock() else {
            return;
        };
        if !force && *last == state {
            return;
        }
        *last = state;
        drop(last);
        emit_input_event(NativeWindowInputEvent::LockKeysSync { state });
    }

    /// Per-key modifier byte from tracked pressed keys (official GFN yS()/Cb()).
    /// Lock keys sync separately via INPUT_LOCK_KEYS_SYNC, not here.
    unsafe fn pressed_key_modifier_flags(
        keys: &HashMap<u16, PressedKey>,
        active_keycode: u16,
    ) -> u16 {
        let mut modifiers = 0u16;
        let mut shift_tracked = false;
        let mut control_tracked = false;
        let mut alt_tracked = false;
        let mut win_tracked = false;

        for key in keys.values() {
            if key.keycode == active_keycode {
                continue;
            }
            match key.keycode {
                VK_LSHIFT | VK_RSHIFT | VK_SHIFT => {
                    shift_tracked = true;
                    modifiers |= 0x01;
                }
                VK_LCONTROL | VK_RCONTROL | VK_CONTROL => {
                    control_tracked = true;
                    modifiers |= 0x02;
                }
                VK_LMENU | VK_RMENU | VK_MENU => {
                    alt_tracked = true;
                    modifiers |= 0x04;
                }
                VK_LWIN | VK_RWIN => {
                    win_tracked = true;
                    modifiers |= 0x08;
                }
                _ => {}
            }
        }

        if !matches!(active_keycode, VK_LSHIFT | VK_RSHIFT | VK_SHIFT)
            && !shift_tracked
            && (is_key_down(VK_SHIFT) || is_key_down(VK_LSHIFT) || is_key_down(VK_RSHIFT))
        {
            modifiers |= 0x01;
        }
        if !matches!(active_keycode, VK_LCONTROL | VK_RCONTROL | VK_CONTROL)
            && !control_tracked
            && (is_key_down(VK_CONTROL) || is_key_down(VK_LCONTROL) || is_key_down(VK_RCONTROL))
        {
            modifiers |= 0x02;
        }
        if !matches!(active_keycode, VK_LMENU | VK_RMENU | VK_MENU)
            && !alt_tracked
            && (is_key_down(VK_MENU) || is_key_down(VK_LMENU) || is_key_down(VK_RMENU))
        {
            modifiers |= 0x04;
        }
        if !matches!(active_keycode, VK_LWIN | VK_RWIN)
            && !win_tracked
            && (is_key_down(VK_LWIN) || is_key_down(VK_RWIN))
        {
            modifiers |= 0x08;
        }

        modifiers
    }

    /// Legacy fallback for shortcut matching before a key enters the pressed-key map.
    unsafe fn keyboard_modifier_flags(active_keycode: u16) -> u16 {
        let keys = PRESSED_KEYS.get_or_init(|| Mutex::new(HashMap::new()));
        if let Ok(keys) = keys.lock() {
            if !keys.is_empty() {
                return pressed_key_modifier_flags(&keys, active_keycode);
            }
        }

        let mut modifiers = 0u16;
        if !matches!(active_keycode, VK_LSHIFT | VK_RSHIFT | VK_SHIFT)
            && (is_key_down(VK_SHIFT) || is_key_down(VK_LSHIFT) || is_key_down(VK_RSHIFT))
        {
            modifiers |= 0x01;
        }
        if !matches!(active_keycode, VK_LCONTROL | VK_RCONTROL | VK_CONTROL)
            && (is_key_down(VK_CONTROL) || is_key_down(VK_LCONTROL) || is_key_down(VK_RCONTROL))
        {
            modifiers |= 0x02;
        }
        if !matches!(active_keycode, VK_LMENU | VK_RMENU | VK_MENU)
            && (is_key_down(VK_MENU) || is_key_down(VK_LMENU) || is_key_down(VK_RMENU))
        {
            modifiers |= 0x04;
        }
        if !matches!(active_keycode, VK_LWIN | VK_RWIN)
            && (is_key_down(VK_LWIN) || is_key_down(VK_RWIN))
        {
            modifiers |= 0x08;
        }
        modifiers
    }

    unsafe fn current_legacy_modifier_flags() -> u16 {
        let mut modifiers = 0u16;
        if is_key_down(VK_SHIFT) || is_key_down(VK_LSHIFT) || is_key_down(VK_RSHIFT) {
            modifiers |= 0x01;
        }
        if is_key_down(VK_CONTROL) || is_key_down(VK_LCONTROL) || is_key_down(VK_RCONTROL) {
            modifiers |= 0x02;
        }
        if is_key_down(VK_MENU) || is_key_down(VK_LMENU) || is_key_down(VK_RMENU) {
            modifiers |= 0x04;
        }
        if is_key_down(VK_LWIN) || is_key_down(VK_RWIN) {
            modifiers |= 0x08;
        }
        if (GetKeyState(VK_CAPITAL) & 0x0001) != 0 {
            modifiers |= 0x10;
        }
        if (GetKeyState(VK_NUMLOCK) & 0x0001) != 0 {
            modifiers |= 0x20;
        }
        modifiers
    }

    unsafe fn is_key_down(keycode: u16) -> bool {
        ((GetKeyState(keycode as i32) as u16) & 0x8000) != 0
    }

    unsafe fn is_alt_modifier_down(keys: &HashMap<u16, PressedKey>) -> bool {
        keys.values()
            .any(|key| matches!(key.keycode, VK_LMENU | VK_RMENU | VK_MENU))
            || ((GetKeyState(VK_MENU as i32) as u16) & 0x8000) != 0
    }

    fn shortcut_action_for_keypress(
        keycode: u16,
        scancode: u16,
        modifiers: u16,
    ) -> Option<NativeStreamerShortcutAction> {
        SHORTCUT_MATCHER
            .get()
            .and_then(|matcher| matcher.lock().ok())
            .and_then(|matcher| matcher.match_keydown(keycode, scancode, modifiers))
    }

    unsafe fn handle_shortcut_action(action: NativeStreamerShortcutAction) {
        match action {
            NativeStreamerShortcutAction::TogglePointerLock => {
                if let Some(hwnd) = captured_hwnd().or_else(protected_hwnd) {
                    let hwnd = hwnd as Hwnd;
                    if is_input_captured(hwnd) {
                        release_input_capture(hwnd);
                    } else {
                        begin_input_capture(hwnd);
                    }
                }
                // Internal/stacked: Electron's own keydown owns the UI
                // shortcut; only toggle RawInput capture here and keep the key
                // out of GFN.
                if crate::gstreamer_config::use_internal_renderer()
                    || crate::gstreamer_config::use_stacked_renderer()
                {
                    return;
                }
            }
            _ => {
                if shortcut_action_releases_input_capture(action) {
                    release_current_input_capture();
                }
                // Internal/stacked: Electron already owns UI shortcuts via
                // keydown. Suppress the key from GFN (caller marks suppressed)
                // without emitting Shortcut, or Electron would double-fire.
                if crate::gstreamer_config::use_internal_renderer()
                    || crate::gstreamer_config::use_stacked_renderer()
                {
                    return;
                }
                emit_input_event(NativeWindowInputEvent::Shortcut { action });
            }
        }
    }

    fn shortcut_action_releases_input_capture(action: NativeStreamerShortcutAction) -> bool {
        matches!(
            action,
            NativeStreamerShortcutAction::ToggleFullscreen
                | NativeStreamerShortcutAction::StopStream
        )
    }

    fn legacy_mouse_button(message: Uint, wparam: Wparam) -> Option<(u8, bool)> {
        match message {
            WM_LBUTTONDOWN => Some((1, true)),
            WM_LBUTTONUP => Some((1, false)),
            WM_MBUTTONDOWN => Some((2, true)),
            WM_MBUTTONUP => Some((2, false)),
            WM_RBUTTONDOWN => Some((3, true)),
            WM_RBUTTONUP => Some((3, false)),
            WM_XBUTTONDOWN | WM_XBUTTONUP => {
                let xbutton = ((wparam >> 16) & 0xffff) as u16;
                let button = match xbutton {
                    XBUTTON1 => 4,
                    XBUTTON2 => 5,
                    _ => return None,
                };
                Some((button, message == WM_XBUTTONDOWN))
            }
            _ => None,
        }
    }

    fn emit_input_event(event: NativeWindowInputEvent) {
        let Some(sender) = INPUT_EVENT_SENDER
            .get()
            .and_then(|sender| sender.lock().ok().and_then(|sender| sender.clone()))
        else {
            return;
        };
        let _ = sender.send(event);
    }

    fn is_clipboard_paste_shortcut(keycode: u16, modifiers: u16) -> bool {
        keycode == VK_V
            && ((modifiers & 0x02) != 0 || unsafe { is_ctrl_modifier_down() })
            && (modifiers & 0x04) == 0
    }

    unsafe fn is_ctrl_modifier_down() -> bool {
        is_key_down(VK_CONTROL) || is_key_down(VK_LCONTROL) || is_key_down(VK_RCONTROL)
    }

    fn emit_clipboard_paste_request() {
        let Some(sender) = INPUT_EVENT_SENDER
            .get()
            .and_then(|sender| sender.lock().ok().and_then(|sender| sender.clone()))
        else {
            return;
        };
        let _ = sender.send(NativeWindowInputEvent::ClipboardPaste);
    }

    fn emit_input_capture_changed(captured: bool) {
        // Electron maps this to notifyPointerLockChange so main-process Escape
        // interception stays in sync with RawInput capture (tap→GFN, hold→exit).
        let Some(sender) = INPUT_EVENT_SENDER
            .get()
            .and_then(|sender| sender.lock().ok().and_then(|sender| sender.clone()))
        else {
            return;
        };
        let _ = sender.send(NativeWindowInputEvent::InputCaptureChanged { captured });
    }

    fn clamp_i32_to_i16(value: i32) -> i16 {
        value.clamp(i16::MIN as i32, i16::MAX as i32) as i16
    }

    fn timestamp_us() -> u64 {
        // Shared with the input thread so measured capture→send delta latency
        // is exact (a module-local baseline here would offset the subtraction).
        crate::gstreamer_input::native_input_clock_us()
    }

    unsafe fn hide_cursor() {
        while ShowCursor(0) >= 0 {}
    }

    unsafe fn show_cursor() {
        while ShowCursor(1) < 0 {}
    }
}

// The old BrowserWindow-HWND GstVideoOverlay path was removed. Internal mode
// uses `crate::internal_renderer::InternalRenderer` (child surface owned by the
// streamer). External mode uses the floating GStreamer window + window guard.

#[cfg(target_os = "windows")]
pub(crate) fn primary_display_refresh_hz() -> Option<u32> {
    const VREFRESH: i32 = 116;

    #[link(name = "user32")]
    extern "system" {
        fn GetDC(hwnd: *mut c_void) -> *mut c_void;
        fn ReleaseDC(hwnd: *mut c_void, hdc: *mut c_void) -> i32;
    }

    #[link(name = "gdi32")]
    extern "system" {
        fn GetDeviceCaps(hdc: *mut c_void, index: i32) -> i32;
    }

    let hdc = unsafe { GetDC(std::ptr::null_mut()) };
    if hdc.is_null() {
        return None;
    }

    let refresh = unsafe { GetDeviceCaps(hdc, VREFRESH) };
    unsafe {
        ReleaseDC(std::ptr::null_mut(), hdc);
    }

    (refresh > 1).then_some(refresh as u32)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn primary_display_refresh_hz() -> Option<u32> {
    None
}
