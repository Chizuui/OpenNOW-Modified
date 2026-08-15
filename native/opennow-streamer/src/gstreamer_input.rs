use crate::gstreamer_backend::send_log;
#[cfg(target_os = "windows")]
use crate::gstreamer_platform::win32_renderer_window;
use crate::input::InputEncoder;
#[cfg(target_os = "windows")]
use crate::input::{
    finalize_reliable_single_input_packets, layout_mapped_keyboard_keycode,
    layout_mapped_keyboard_scancode, restamp_protocol_v3_outer_timestamp, GamepadInput,
    KeyboardPayload, MouseButtonPayload, MouseMovePayload, MouseWheelPayload,
    GAMEPAD_MAX_CONTROLLERS, PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL,
};
use crate::protocol::Event;
#[cfg(target_os = "windows")]
use crate::protocol::NativeStreamerShortcutAction;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use gst::glib;
use gst::prelude::*;
use gstreamer as gst;
use gstreamer_webrtc as gst_webrtc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
#[cfg(target_os = "windows")]
use std::sync::mpsc::{self, RecvError, TryRecvError};
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const RELIABLE_INPUT_CHANNEL_LABEL: &str = "input_channel_v1";
const PARTIALLY_RELIABLE_INPUT_CHANNEL_LABEL: &str = "input_channel_partially_reliable";
const STATS_CHANNEL_LABEL: &str = "stats_channel";
// Fallback only: the negotiated value (a=ri.partialReliableThresholdMs) is the
// official 16 ms and always wins in practice — 16 ms is maxPacketLifeTime, the
// retransmission window, NOT a send delay.
const DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS: u32 =
    crate::sdp::OFFICIAL_PARTIAL_RELIABLE_THRESHOLD_MS;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);
const HEARTBEAT_STOP_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Last server-reported game FPS read from the `stats_channel` data channel.
/// Mirrors the web client's statsChannel parser so the HUD "GAME" number in
/// native sessions reflects the server-side game render rate (which can exceed
/// the negotiated stream FPS) instead of falling back to the local decode rate.
static STATS_CHANNEL_GAME_FPS: OnceLock<AtomicU32> = OnceLock::new();

/// Server-reported network round-trip time (ms) read from the same stats
/// channel. Layout verified against real v5 frames (see the per-frame hex dump
/// in the input handler): the stats payload carries, before `avgGameFps`
/// (payload offset 25), three more little-endian float64 fields at payload
/// offsets 1, 9 and 17. The field at offset 17 holds a stable ~38-39 ms value
/// independent of game FPS — the server-side network RTT estimate — while the
/// offset 9 field is a small fraction (≈0.0001-0.0004) that looks like packet
/// loss. Both are 0 / -1 sentinel when the server has no measurement yet.
static STATS_CHANNEL_RTT_MS: OnceLock<AtomicU32> = OnceLock::new();
/// Monotonic instant of the last stats_channel frame that carried a valid
/// (non-zero) server RTT. The stats channel cadence is irregular — samples
/// arrive in bursts with arbitrary gaps — so this lets the HUD expire a
/// server RTT that stopped refreshing (the "ping frozen at an old spike"
/// symptom) instead of holding it as current forever.
static STATS_CHANNEL_RTT_LAST_SEEN_AT: OnceLock<Mutex<Instant>> = OnceLock::new();
/// Server-reported packet loss fraction (0..1) from payload offset 9, stored as
/// basis points (percent × 100) so it fits an atomic integer.
static STATS_CHANNEL_LOSS_BPS: OnceLock<AtomicU32> = OnceLock::new();

/// Parsed server network telemetry from a `stats_channel` frame.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StatsChannelNetwork {
    /// Server-reported round-trip time in ms (0 = not reported yet).
    pub rtt_ms: u32,
    /// Server-reported packet loss as a fraction (0..1); None when not reported.
    pub packet_loss_fraction: Option<f64>,
    /// Raw large counter at payload offset 1 — a candidate cumulative byte
    /// counter. Parsed on every frame so the per-message rate estimator can
    /// verify the semantics before surfacing a bitrate (see
    /// `StatsChannelRateEstimator`). None when the value is not a sane counter.
    pub counter: Option<f64>,
}

/// Shared monotonic clock baseline for native input timestamps. Both the
/// capture site (sink wndproc / raw-input handler in gstreamer_platform) and
/// the send site (this input thread) must measure against the SAME baseline,
/// otherwise the measured capture→send delta latency drifts by the offset
/// between their module-local clock initializations.
static NATIVE_INPUT_CLOCK: OnceLock<Instant> = OnceLock::new();

/// Microseconds since the shared native-input clock baseline.
pub(crate) fn native_input_clock_us() -> u64 {
    NATIVE_INPUT_CLOCK
        .get_or_init(Instant::now)
        .elapsed()
        .as_micros()
        .min(u128::from(u64::MAX)) as u64
}

/// Active input capture path in the native streamer:
/// - `"sink-native"` — stacked mode; the sink window owns RawInput (mouse +
///   keyboard) and feeds the data channel in-process (no renderer bridge).
/// - `"internal"` — embedded/internal child-window capture.
/// - `"external"` — floating sink window with its own RawInput.
/// - `"bridge"` — input arrives from the renderer over stdin (addon or
///   pointer-lock path).
/// - `"none"` — before any capture is armed.
static NATIVE_INPUT_PATH: OnceLock<Mutex<&'static str>> = OnceLock::new();

pub(crate) fn set_native_input_path(path: &'static str) {
    if let Ok(mut slot) = NATIVE_INPUT_PATH.get_or_init(|| Mutex::new("none")).lock() {
        *slot = path;
    }
}

pub(crate) fn native_input_path() -> &'static str {
    NATIVE_INPUT_PATH
        .get_or_init(|| Mutex::new("none"))
        .lock()
        .map(|path| *path)
        .unwrap_or("none")
}

/// Measured in-process mouse delta latency: WM_INPUT captured in the sink
/// wndproc → event mpsc → encode → data-channel send. Only meaningful on the
/// sink-native / internal / external paths (the renderer bridge latency is
/// measured on the renderer side instead).
#[derive(Debug, Clone, Copy)]
struct MouseDeltaLatency {
    ema_us: f64,
    last_us: u64,
    samples: u64,
}

static MOUSE_DELTA_LATENCY: OnceLock<Mutex<Option<MouseDeltaLatency>>> = OnceLock::new();

/// Current mouse delta latency as `(ema_us, last_us, samples)`, `None` until
/// the first delta has been measured.
pub(crate) fn mouse_delta_latency_us() -> Option<(u64, u64, u64)> {
    MOUSE_DELTA_LATENCY
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|slot| *slot)
        .map(|latency| {
            (
                latency.ema_us.round() as u64,
                latency.last_us,
                latency.samples,
            )
        })
}

/// Feed one capture→send latency sample into the EMA.
#[cfg(target_os = "windows")]
fn record_mouse_delta_latency_us(capture_to_send_us: u64) {
    if let Ok(mut slot) = MOUSE_DELTA_LATENCY.get_or_init(|| Mutex::new(None)).lock() {
        let next = match *slot {
            Some(previous) => MouseDeltaLatency {
                // EMA with ~50-sample smoothing; caps the influence of a single
                // scheduling hiccup so the reported value tracks the steady
                // 1:1 path latency.
                ema_us: previous.ema_us * 0.98 + capture_to_send_us as f64 * 0.02,
                last_us: capture_to_send_us,
                samples: previous.samples + 1,
            },
            None => MouseDeltaLatency {
                ema_us: capture_to_send_us as f64,
                last_us: capture_to_send_us,
                samples: 1,
            },
        };
        *slot = Some(next);
    }
}

/// Input-paused flag shared with the platform. While paused (an overlay is
/// open), the stacked guard must never re-arm the sink RawInput capture — doing
/// so would re-hide the cursor over the overlay and re-own input the game
/// should not receive (the visible arm/release cursor flicker).
static INPUT_PAUSED_FLAG: OnceLock<AtomicBool> = OnceLock::new();

pub(crate) fn set_input_paused_flag(paused: bool) {
    INPUT_PAUSED_FLAG
        .get_or_init(|| AtomicBool::new(false))
        .store(paused, Ordering::SeqCst);
}

pub(crate) fn input_paused() -> bool {
    INPUT_PAUSED_FLAG
        .get_or_init(|| AtomicBool::new(false))
        .load(Ordering::SeqCst)
}

/// Mouse sensitivity / acceleration applied to the sink-native RawInput path.
/// Same values the renderer uses for the addon / DOM pointer-lock paths, so
/// stacked-mode capture feels exactly like the configured mouse settings
/// instead of raw (unscaled) HID counts.
static NATIVE_MOUSE_SENSITIVITY: OnceLock<Mutex<f64>> = OnceLock::new();
static NATIVE_MOUSE_ACCELERATION_PERCENT: OnceLock<Mutex<f64>> = OnceLock::new();

pub(crate) fn set_native_mouse_settings(sensitivity: f64, acceleration_percent: f64) {
    if let Ok(mut slot) = NATIVE_MOUSE_SENSITIVITY
        .get_or_init(|| Mutex::new(1.0))
        .lock()
    {
        *slot = if sensitivity.is_finite() && sensitivity > 0.0 {
            sensitivity
        } else {
            1.0
        };
    }
    if let Ok(mut slot) = NATIVE_MOUSE_ACCELERATION_PERCENT
        .get_or_init(|| Mutex::new(1.0))
        .lock()
    {
        let clamped = acceleration_percent.clamp(1.0, 150.0);
        *slot = if clamped.is_finite() { clamped } else { 1.0 };
    }
}

pub(crate) fn native_mouse_sensitivity() -> f64 {
    NATIVE_MOUSE_SENSITIVITY
        .get_or_init(|| Mutex::new(1.0))
        .lock()
        .map(|value| *value)
        .unwrap_or(1.0)
}

pub(crate) fn native_mouse_acceleration_percent() -> f64 {
    NATIVE_MOUSE_ACCELERATION_PERCENT
        .get_or_init(|| Mutex::new(1.0))
        .lock()
        .map(|value| *value)
        .unwrap_or(1.0)
}

/// Server-side stream resolution (CSS/logical pixels, "WxH") used to
/// normalize sink-native RawInput deltas to the on-screen cursor. The DOM /
/// addon path scales deltas by server-width ÷ CSS-window-width
/// (getPointerScale in the renderer); stacked capture must apply the same
/// factor or the game cursor runs FASTER than the OS cursor on displays
/// larger than the stream (e.g. a 1080p stream fullscreen on a 1440p
/// monitor: raw counts × 1.0 vs the DOM path's × 0.75).
static NATIVE_MOUSE_SERVER_RESOLUTION: OnceLock<Mutex<Option<(u32, u32)>>> = OnceLock::new();

pub(crate) fn set_native_mouse_server_resolution(resolution: &str) {
    let mut parts = resolution.splitn(2, ['x', 'X']);
    let width = parts.next().and_then(|value| value.trim().parse::<u32>().ok());
    let height = parts.next().and_then(|value| value.trim().parse::<u32>().ok());
    if let (Some(width), Some(height)) = (width, height) {
        if width > 0 && height > 0 {
            if let Ok(mut slot) = NATIVE_MOUSE_SERVER_RESOLUTION
                .get_or_init(|| Mutex::new(None))
                .lock()
            {
                *slot = Some((width, height));
            }
        }
    }
}

pub(crate) fn native_mouse_server_resolution() -> Option<(u32, u32)> {
    NATIVE_MOUSE_SERVER_RESOLUTION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|slot| *slot)
}

/// Stacked sink-native RawInput capture toggle (settings > native streamer).
/// Default OFF: stacked mode rides the Electron bridge (addon mouse + DOM
/// keyboard) like the web path. The sink bypass is opt-in because raw HID
/// mouse/keyboard capture is experimental — re-arming is gated on this flag
/// (plus the foreground + paused guards in the platform).
static NATIVE_SINK_INPUT_CAPTURE_ENABLED: OnceLock<AtomicBool> = OnceLock::new();

pub(crate) fn set_native_sink_input_capture_enabled(enabled: bool) {
    NATIVE_SINK_INPUT_CAPTURE_ENABLED
        .get_or_init(|| AtomicBool::new(false))
        .store(enabled, Ordering::SeqCst);
}

pub(crate) fn native_sink_input_capture_enabled() -> bool {
    NATIVE_SINK_INPUT_CAPTURE_ENABLED
        .get_or_init(|| AtomicBool::new(false))
        .load(Ordering::SeqCst)
}

/// Read the most recently reported server-side game FPS (0 = none yet).
pub(crate) fn stats_channel_game_fps() -> u32 {
    STATS_CHANNEL_GAME_FPS
        .get_or_init(|| AtomicU32::new(0))
        .load(Ordering::Relaxed)
}

/// Read the most recently reported server-side network RTT in ms (0 = none yet).
pub(crate) fn stats_channel_rtt_ms() -> u32 {
    STATS_CHANNEL_RTT_MS
        .get_or_init(|| AtomicU32::new(0))
        .load(Ordering::Relaxed)
}

/// Age (ms) of the last server-reported RTT sample — time since the last
/// stats_channel frame carrying a valid (non-zero) RTT arrived. None when no
/// valid sample has been seen yet. Lets the renderer expire a server RTT
/// that stopped refreshing.
pub(crate) fn stats_channel_rtt_age_ms() -> Option<u32> {
    STATS_CHANNEL_RTT_LAST_SEEN_AT
        .get()
        .and_then(|slot| slot.lock().ok())
        .map(|last| last.elapsed().as_millis().min(u32::MAX as u128) as u32)
}

/// Read the most recently reported server-side packet loss fraction (0..1).
pub(crate) fn stats_channel_packet_loss_fraction() -> Option<f64> {
    let bps = STATS_CHANNEL_LOSS_BPS
        .get_or_init(|| AtomicU32::new(0))
        .load(Ordering::Relaxed);
    (bps > 0).then_some(f64::from(bps) / 10_000.0)
}

/// Parse a GFN `stats_channel` frame and return the server-side average game
/// render FPS. Format mirrors `statsChannel.ts` in the web client: byte 0 is a
/// TYPE discriminator (3 = 1-byte header + payload at byte 1, 4 = payload at
/// byte 0), the payload starts with a protocol VERSION (>= 4), and
/// `avgGameFps` is a little-endian float64 at payload offset 25.
/// Parse the GFN `stats_channel` header: byte 0 is a TYPE discriminator
/// (3 = 1-byte header + payload at byte 1, 4 = payload at byte 0), the payload
/// starts with the protocol VERSION (>= 4). Returns the payload offset or None.
fn stats_channel_payload_offset(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() {
        return None;
    }
    let offset = match bytes[0] {
        3 => 1,
        4 => 0,
        _ => return None,
    };
    if bytes.len() <= offset {
        return None;
    }
    if bytes[offset] < 4 {
        return None;
    }
    Some(offset)
}

pub(crate) fn parse_stats_channel_game_fps(bytes: &[u8]) -> Option<u32> {
    let offset = stats_channel_payload_offset(bytes)?;
    if bytes.len() < offset + 33 {
        return None;
    }
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&bytes[offset + 25..offset + 33]);
    let avg_game_fps = f64::from_le_bytes(raw);
    if !avg_game_fps.is_finite() || avg_game_fps <= 0.0 || avg_game_fps > 360.0 {
        return None;
    }
    Some(avg_game_fps.round() as u32)
}

/// Parse the server network telemetry (RTT + packet loss) from a stats frame.
///
/// Layout verified against real v5 frames (see the hex dump in the stats
/// channel handler): with the payload starting at `offset` (0 for type 4, 1 for
/// type 3), the server reports the following little-endian float64s before
/// `avgGameFps` (payload offset 25):
///
///   payload offset  1   — unknown (large counter, per-frame byte/interval)
///   payload offset  9   — packet loss fraction (0..1; ~0.0001-0.0004 healthy)
///   payload offset 17   — network RTT in ms (stable ~38-39 ms, fps-independent)
///   payload offset 25   — avgGameFps (parsed separately)
///
/// RTT is validated to (0, 2000] ms; the loss fraction to [0, 1].
pub(crate) fn parse_stats_channel_network(bytes: &[u8]) -> StatsChannelNetwork {
    let mut network = StatsChannelNetwork::default();
    let Some(offset) = stats_channel_payload_offset(bytes) else {
        return network;
    };
    if bytes.len() < offset + 33 {
        return network;
    }
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&bytes[offset + 17..offset + 25]);
    let rtt_ms = f64::from_le_bytes(raw);
    if rtt_ms.is_finite() && rtt_ms > 0.0 && rtt_ms <= 2000.0 {
        network.rtt_ms = rtt_ms.round() as u32;
    }
    raw.copy_from_slice(&bytes[offset + 9..offset + 17]);
    let loss = f64::from_le_bytes(raw);
    if loss.is_finite() && loss >= 0.0 && loss <= 1.0 {
        network.packet_loss_fraction = Some(loss);
    }
    raw.copy_from_slice(&bytes[offset + 1..offset + 9]);
    let counter = f64::from_le_bytes(raw);
    if counter.is_finite() && counter >= 0.0 && counter <= 2f64.powi(52) {
        network.counter = Some(counter);
    }
    network
}

/// Per-message estimator for the stats_channel counter (payload offset 1).
///
/// If that field is a cumulative byte counter, per-message deltas divided by
/// the inter-message time yield a stable session bitrate; the estimate is only
/// surfaced once enough consistent samples have accumulated. Counter resets
/// (non-monotonic deltas) and non-byte semantics (implausible rates) reset the
/// baseline so a bogus number never reaches the HUD — this is the "if it turns
/// out to be cumulative bytes" gate from the analysis: the field only drives
/// the HUD bitrate when its deltas actually behave like bytes.
#[derive(Debug, Default)]
struct StatsChannelRateEstimator {
    last_counter: Option<f64>,
    last_at: Option<Instant>,
    ema_kbps: f64,
    samples: u32,
}

impl StatsChannelRateEstimator {
    /// Feed one counter sample. Returns `(delta_bytes, dt_secs, kbps)` for the
    /// diagnostic log when the sample was accepted into the EMA.
    fn feed(&mut self, counter: f64, now: Instant) -> Option<(f64, f64, f64)> {
        let mut accepted = None;
        if let (Some(last_counter), Some(last_at)) = (self.last_counter, self.last_at) {
            let dt_secs = now.duration_since(last_at).as_secs_f64();
            let delta = counter - last_counter;
            if delta >= 0.0 && dt_secs >= 0.01 && dt_secs <= 10.0 {
                let kbps = delta * 8.0 / dt_secs / 1000.0;
                // Plausible session bitrate window: 0.1 kbps .. 2 Gbps. Anything
                // outside (wrap artifact, or the field is not bytes at all)
                // restarts the baseline so the estimate cannot lock onto junk.
                if (0.1..=2_000_000.0).contains(&kbps) {
                    self.ema_kbps = if self.samples == 0 {
                        kbps
                    } else {
                        self.ema_kbps * 0.9 + kbps * 0.1
                    };
                    self.samples = self.samples.saturating_add(1);
                    accepted = Some((delta, dt_secs, kbps));
                } else {
                    // Counter jumped (wrap or non-byte field) — start over so
                    // the stale EMA never survives a discontinuity.
                    self.samples = 0;
                    self.ema_kbps = 0.0;
                }
            } else {
                // Negative delta (server-side reset) or implausible inter-message
                // gap — drop the accumulated EMA as well.
                self.samples = 0;
                self.ema_kbps = 0.0;
            }
        }
        self.last_counter = Some(counter);
        self.last_at = Some(now);
        accepted
    }
}

static STATS_CHANNEL_RATE: OnceLock<Mutex<StatsChannelRateEstimator>> = OnceLock::new();

/// Feed one counter sample into the bitrate estimator; returns the accepted
/// `(delta_bytes, dt_secs, kbps)` sample for logging, if any.
fn feed_stats_channel_counter(counter: f64) -> Option<(f64, f64, f64)> {
    STATS_CHANNEL_RATE
        .get_or_init(|| Mutex::new(StatsChannelRateEstimator::default()))
        .lock()
        .ok()
        .and_then(|mut estimator| estimator.feed(counter, Instant::now()))
}

/// Current server-reported session bitrate estimate in kbps, only after at
/// least [`StatsChannelRateEstimator`] confidence samples (5) have been
/// observed; None while the counter semantics are unverified.
pub(crate) fn stats_channel_bitrate_kbps() -> Option<u32> {
    STATS_CHANNEL_RATE
        .get_or_init(|| Mutex::new(StatsChannelRateEstimator::default()))
        .lock()
        .ok()
        .filter(|estimator| estimator.samples >= 5)
        .map(|estimator| estimator.ema_kbps.round() as u32)
}
#[cfg(target_os = "windows")]
const NATIVE_INPUT_DRAIN_MAX_EVENTS: usize = 512;
#[cfg(target_os = "windows")]
const NATIVE_GAMEPAD_POLL_INTERVAL: Duration = Duration::from_millis(4);
#[cfg(target_os = "windows")]
const NATIVE_GAMEPAD_KEEPALIVE_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone)]
pub(crate) struct GstreamerInputState {
    encoder: Arc<Mutex<InputEncoder>>,
    pub(crate) ready: Arc<AtomicBool>,
    pub(crate) paused: Arc<AtomicBool>,
    heartbeat_stop: Arc<AtomicBool>,
    heartbeat_thread: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl std::fmt::Debug for GstreamerInputState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GstreamerInputState")
            .field("ready", &self.ready.load(Ordering::SeqCst))
            .field("paused", &self.paused.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}

impl Default for GstreamerInputState {
    fn default() -> Self {
        Self {
            encoder: Arc::new(Mutex::new(InputEncoder::default())),
            ready: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            heartbeat_stop: Arc::new(AtomicBool::new(false)),
            heartbeat_thread: Arc::new(Mutex::new(None)),
        }
    }
}

impl GstreamerInputState {
    pub(crate) fn reset(&self) {
        self.ready.store(false, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
        if let Ok(mut encoder) = self.encoder.lock() {
            encoder.set_protocol_version(2);
            encoder.reset_gamepad_sequences();
        }
    }

    pub(crate) fn stop_heartbeat(&self) {
        self.heartbeat_stop.store(true, Ordering::SeqCst);
        let Some(handle) = self
            .heartbeat_thread
            .lock()
            .ok()
            .and_then(|mut thread| thread.take())
        else {
            return;
        };

        if let Err(error) = handle.join() {
            eprintln!("[NativeStreamer] Input heartbeat thread panicked: {error:?}");
        }
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy)]
pub(crate) enum NativeWindowInputEvent {
    Shortcut {
        action: NativeStreamerShortcutAction,
    },
    ClipboardPaste,
    InputCaptureChanged {
        captured: bool,
    },
    Key {
        pressed: bool,
        keycode: u16,
        scancode: u16,
        modifiers: u16,
        timestamp_us: u64,
    },
    MouseMove {
        dx: i16,
        dy: i16,
        timestamp_us: u64,
    },
    MouseButton {
        pressed: bool,
        button: u8,
        timestamp_us: u64,
    },
    MouseWheel {
        delta: i16,
        timestamp_us: u64,
    },
    LockKeysSync {
        state: u8,
    },
}

#[cfg(target_os = "windows")]
enum EncodedNativeInputBatch {
    ReliableSingles(Vec<Vec<u8>>),
    MousePacket(Vec<u8>),
}

#[cfg(target_os = "windows")]
/// Win32 thread-priority helpers for the native input bridge. The mouse 1:1
/// path drains a delta the instant WM_INPUT arrives; raising the input thread
/// to TIME_CRITICAL keeps that drain/encode/send from being starved while the
/// render thread is busy presenting, and timeBeginPeriod(1) tightens the
/// process timer resolution so scheduling stays at ~1 ms granularity.
mod win32_priority {
    use std::ffi::c_void;

    type Hthread = *mut c_void;

    const THREAD_PRIORITY_TIME_CRITICAL: i32 = 15;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> Hthread;
        fn SetThreadPriority(thread: Hthread, priority: i32) -> i32;
    }
    #[link(name = "winmm")]
    unsafe extern "system" {
        fn timeBeginPeriod(period: u32) -> u32;
        fn timeEndPeriod(period: u32) -> u32;
    }

    /// Apply high priority + 1 ms timer resolution on the calling thread.
    pub unsafe fn apply() {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
        timeBeginPeriod(1);
    }

    /// Balance the timeBeginPeriod(1) taken in apply().
    pub unsafe fn clear() {
        timeEndPeriod(1);
    }
}

#[cfg(target_os = "windows")]
mod win32_xinput {
    use std::ffi::{c_char, c_void};

    type Dword = u32;
    type Hmodule = *mut c_void;
    type XInputGetStateFn = unsafe extern "system" fn(Dword, *mut XInputStateRaw) -> Dword;

    const ERROR_SUCCESS: Dword = 0;
    const XINPUT_DLLS: [&str; 3] = ["xinput1_4.dll", "xinput9_1_0.dll", "xinput1_3.dll"];

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct XInputGamepadRaw {
        buttons: u16,
        left_trigger: u8,
        right_trigger: u8,
        thumb_lx: i16,
        thumb_ly: i16,
        thumb_rx: i16,
        thumb_ry: i16,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct XInputStateRaw {
        packet_number: Dword,
        gamepad: XInputGamepadRaw,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct XInputGamepadSnapshot {
        pub buttons: u16,
        pub left_trigger: u8,
        pub right_trigger: u8,
        pub left_stick_x: i16,
        pub left_stick_y: i16,
        pub right_stick_x: i16,
        pub right_stick_y: i16,
    }

    #[derive(Clone, Copy)]
    pub struct XInput {
        get_state: XInputGetStateFn,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetProcAddress(module: Hmodule, proc_name: *const c_char) -> *mut c_void;
        fn LoadLibraryW(filename: *const u16) -> Hmodule;
    }

    impl XInput {
        pub unsafe fn load() -> Option<Self> {
            for dll in XINPUT_DLLS {
                let wide = wide_null(dll);
                let module = LoadLibraryW(wide.as_ptr());
                if module.is_null() {
                    continue;
                }

                let address = GetProcAddress(module, b"XInputGetState\0".as_ptr() as *const c_char);
                if !address.is_null() {
                    return Some(Self {
                        get_state: std::mem::transmute::<*mut c_void, XInputGetStateFn>(address),
                    });
                }
            }

            None
        }

        pub unsafe fn get_state(self, controller_id: u32) -> Option<XInputGamepadSnapshot> {
            let mut state = XInputStateRaw::default();
            if (self.get_state)(controller_id, &mut state) != ERROR_SUCCESS {
                return None;
            }

            Some(XInputGamepadSnapshot {
                buttons: state.gamepad.buttons,
                left_trigger: apply_trigger_deadzone(state.gamepad.left_trigger),
                right_trigger: apply_trigger_deadzone(state.gamepad.right_trigger),
                left_stick_x: apply_stick_deadzone(state.gamepad.thumb_lx, 7849),
                left_stick_y: apply_stick_deadzone(state.gamepad.thumb_ly, 7849),
                right_stick_x: apply_stick_deadzone(state.gamepad.thumb_rx, 8689),
                right_stick_y: apply_stick_deadzone(state.gamepad.thumb_ry, 8689),
            })
        }
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn apply_trigger_deadzone(value: u8) -> u8 {
        if value <= 30 {
            0
        } else {
            value
        }
    }

    fn apply_stick_deadzone(value: i16, deadzone: i16) -> i16 {
        if (value as i32).abs() <= deadzone as i32 {
            0
        } else {
            value
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GstreamerInputChannels {
    reliable: gst_webrtc::WebRTCDataChannel,
    partially_reliable: gst_webrtc::WebRTCDataChannel,
}

impl GstreamerInputChannels {
    pub(crate) fn labels(&self) -> (String, String) {
        (
            channel_label(&self.reliable),
            channel_label(&self.partially_reliable),
        )
    }

    pub(crate) fn send_packet(&self, payload: &[u8], partially_reliable: bool) -> bool {
        if payload.is_empty() {
            return false;
        }

        let channel = if partially_reliable {
            if self.partially_reliable.ready_state() != gst_webrtc::WebRTCDataChannelState::Open {
                return false;
            }
            &self.partially_reliable
        } else {
            &self.reliable
        };

        if channel.ready_state() != gst_webrtc::WebRTCDataChannelState::Open {
            return false;
        }

        let bytes = glib::Bytes::from_owned(payload.to_vec());
        channel.send_data_full(Some(&bytes)).is_ok()
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
pub(crate) struct NativeWindowInputBridge {
    stop: Arc<AtomicBool>,
    input_thread: Option<JoinHandle<()>>,
    gamepad_thread: Option<JoinHandle<()>>,
}

#[cfg(target_os = "windows")]
impl NativeWindowInputBridge {
    pub(crate) fn start(
        input_state: GstreamerInputState,
        input_channels: GstreamerInputChannels,
        event_sender: Option<Sender<Event>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel::<NativeWindowInputEvent>();
        unsafe {
            win32_renderer_window::set_input_event_sender(Some(sender));
        }

        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread_sender = event_sender.clone();
        let input_thread_state = input_state.clone();
        let input_thread_channels = input_channels.clone();
        let input_thread = thread::spawn(move || {
            // 1:1 input: this thread must never be starved by render/present
            // work, and its blocking recv must wake on ~1 ms scheduling. The
            // RAII guard restores timer resolution on any exit path.
            #[cfg(target_os = "windows")]
            struct InputPriorityGuard;
            #[cfg(target_os = "windows")]
            impl InputPriorityGuard {
                unsafe fn arm() -> Self {
                    win32_priority::apply();
                    Self
                }
            }
            #[cfg(target_os = "windows")]
            impl Drop for InputPriorityGuard {
                fn drop(&mut self) {
                    unsafe { win32_priority::clear(); }
                }
            }
            #[cfg(target_os = "windows")]
            let _priority = unsafe { InputPriorityGuard::arm() };

            let mut pending_events = Vec::with_capacity(NATIVE_INPUT_DRAIN_MAX_EVENTS);
            send_log(
                &thread_sender,
                "info",
                "Native DX11 window input capture bridge armed (high-priority input thread, 1 ms timer).".to_owned(),
            );

            while !thread_stop.load(Ordering::SeqCst) {
                pending_events.clear();
                loop {
                    match receiver.try_recv() {
                        Ok(event) => pending_events.push(event),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => return,
                    }
                    if pending_events.len() >= NATIVE_INPUT_DRAIN_MAX_EVENTS {
                        break;
                    }
                }

                if pending_events.is_empty() {
                    // Block until the next native input event arrives (the sender
                    // is dropped on stop, waking this recv), so a mouse delta is
                    // forwarded the instant it is captured — no polling interval
                    // adds even one millisecond to the 1:1 path.
                    match receiver.recv() {
                        Ok(event) => pending_events.push(event),
                        Err(RecvError) => break,
                    }

                    while pending_events.len() < NATIVE_INPUT_DRAIN_MAX_EVENTS {
                        match receiver.try_recv() {
                            Ok(event) => pending_events.push(event),
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => break,
                        }
                    }
                }

                send_native_window_input_events(
                    &input_thread_state,
                    &input_thread_channels,
                    &thread_sender,
                    &pending_events,
                );
            }
        });
        let gamepad_thread = Some(spawn_native_gamepad_thread(
            input_state,
            input_channels,
            event_sender,
            stop.clone(),
        ));

        Self {
            stop,
            input_thread: Some(input_thread),
            gamepad_thread,
        }
    }

    pub(crate) fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        unsafe {
            win32_renderer_window::release_current_input_capture();
            win32_renderer_window::set_input_event_sender(None);
        }

        if let Some(thread) = self.input_thread.take() {
            if let Err(error) = thread.join() {
                eprintln!("[NativeStreamer] Native window input bridge thread panicked: {error:?}");
            }
        }
        if let Some(thread) = self.gamepad_thread.take() {
            if let Err(error) = thread.join() {
                eprintln!("[NativeStreamer] Native XInput gamepad thread panicked: {error:?}");
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for NativeWindowInputBridge {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(target_os = "windows")]
fn send_native_window_input_events(
    input_state: &GstreamerInputState,
    input_channels: &GstreamerInputChannels,
    event_sender: &Option<Sender<Event>>,
    events: &[NativeWindowInputEvent],
) {
    if events.is_empty() {
        return;
    }

    // Forward shortcuts and host bridge events immediately (before input readiness check)
    // These are local control events and don't need the stream channel
    let mut other_events = Vec::new();
    for event in events.iter().copied() {
        match event {
            NativeWindowInputEvent::Shortcut { action } => {
                if let Some(sender) = event_sender.as_ref() {
                    let _ = sender.send(Event::Shortcut { action });
                }
            }
            NativeWindowInputEvent::ClipboardPaste => {
                if let Some(sender) = event_sender.as_ref() {
                    let _ = sender.send(Event::ClipboardPaste);
                }
            }
            NativeWindowInputEvent::InputCaptureChanged { captured } => {
                if let Some(sender) = event_sender.as_ref() {
                    let _ = sender.send(Event::InputCaptureChanged { captured });
                    // Diagnostic: confirm which input path is active per session
                    // (sink-native / internal / external / bridge) so a 1:1
                    // capture is verifiable from the log alone.
                    let path = native_input_path();
                    let _ = sender.send(Event::Log {
                        level: "info",
                        message: format!("Input path: {path} (captured={captured})"),
                    });
                }
            }
            _ => {
                other_events.push(event);
            }
        }
    }

    // Only process non-shortcut events if input is ready.
    if other_events.is_empty() || !input_state.ready.load(Ordering::SeqCst) {
        return;
    }
    if input_state.paused.load(Ordering::SeqCst) {
        other_events.retain(is_native_input_release_event);
        if other_events.is_empty() {
            return;
        }
    }

    let mut pending_mouse_move: Option<(i32, i32, u64)> = None;
    // Capture timestamp of the last mouse delta in this drain, used to measure
    // the in-process 1:1 path latency (capture → encode → data channel send).
    let mut last_mouse_capture_timestamp_us: Option<u64> = None;
    let mut current_reliable_singles: Vec<Vec<u8>> = Vec::new();
    let mut input_batches: Vec<EncodedNativeInputBatch> = Vec::new();

    {
        let Ok(encoder) = input_state.encoder.lock() else {
            return;
        };

        let mut flush_current_reliable_singles =
            |singles: &mut Vec<Vec<u8>>, batches: &mut Vec<EncodedNativeInputBatch>| {
                if singles.is_empty() {
                    return;
                }
                batches.push(EncodedNativeInputBatch::ReliableSingles(std::mem::take(
                    singles,
                )));
            };

        for event in other_events.iter().copied() {
            if let NativeWindowInputEvent::MouseMove {
                dx,
                dy,
                timestamp_us,
            } = event
            {
                let (pending_dx, pending_dy, pending_timestamp_us) =
                    pending_mouse_move.get_or_insert((0, 0, timestamp_us));
                *pending_dx = pending_dx.saturating_add(i32::from(dx));
                *pending_dy = pending_dy.saturating_add(i32::from(dy));
                *pending_timestamp_us = timestamp_us;
                last_mouse_capture_timestamp_us = Some(timestamp_us);
                continue;
            }

            if pending_mouse_move.is_some() {
                flush_current_reliable_singles(&mut current_reliable_singles, &mut input_batches);
                collect_pending_mouse_move_packets(
                    &encoder,
                    &mut pending_mouse_move,
                    &mut input_batches,
                );
            }
            if let Some(payload) = encode_native_window_input_payload(&encoder, event_sender, event)
            {
                current_reliable_singles.push(payload);
            }
        }

        flush_current_reliable_singles(&mut current_reliable_singles, &mut input_batches);
        collect_pending_mouse_move_packets(&encoder, &mut pending_mouse_move, &mut input_batches);
    }

    let send_timestamp_us = native_input_timestamp_us();
    if let Some(capture_timestamp_us) = last_mouse_capture_timestamp_us {
        // Same shared clock at both ends, so the difference is the true
        // in-process delta latency for the sink-native path.
        record_mouse_delta_latency_us(send_timestamp_us.saturating_sub(capture_timestamp_us));
    }
    for batch in input_batches {
        match batch {
            EncodedNativeInputBatch::ReliableSingles(reliable_singles) => {
                for payload in
                    finalize_reliable_single_input_packets(&reliable_singles, send_timestamp_us)
                {
                    let _ = input_channels.send_packet(&payload, false);
                }
            }
            EncodedNativeInputBatch::MousePacket(mut payload) => {
                restamp_protocol_v3_outer_timestamp(&mut payload, send_timestamp_us);
                let _ = input_channels.send_packet(&payload, true);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn is_native_input_release_event(event: &NativeWindowInputEvent) -> bool {
    matches!(
        event,
        NativeWindowInputEvent::Key { pressed: false, .. }
            | NativeWindowInputEvent::MouseButton { pressed: false, .. }
    )
}

#[cfg(target_os = "windows")]
fn collect_pending_mouse_move_packets(
    encoder: &InputEncoder,
    pending_mouse_move: &mut Option<(i32, i32, u64)>,
    input_batches: &mut Vec<EncodedNativeInputBatch>,
) {
    let Some((mut dx, mut dy, timestamp_us)) = pending_mouse_move.take() else {
        return;
    };

    while dx != 0 || dy != 0 {
        let chunk_dx = dx.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        let chunk_dy = dy.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        input_batches.push(EncodedNativeInputBatch::MousePacket(
            encoder.encode_mouse_move(MouseMovePayload {
                dx: chunk_dx,
                dy: chunk_dy,
                timestamp_us,
            }),
        ));
        dx = dx.saturating_sub(i32::from(chunk_dx));
        dy = dy.saturating_sub(i32::from(chunk_dy));
    }
}

#[cfg(target_os = "windows")]
fn encode_native_window_input_payload(
    encoder: &InputEncoder,
    event_sender: &Option<Sender<Event>>,
    event: NativeWindowInputEvent,
) -> Option<Vec<u8>> {
    match event {
        NativeWindowInputEvent::Shortcut { action } => {
            if let Some(sender) = event_sender.as_ref() {
                let _ = sender.send(Event::Shortcut { action });
            }
            None
        }
        NativeWindowInputEvent::ClipboardPaste => {
            if let Some(sender) = event_sender.as_ref() {
                let _ = sender.send(Event::ClipboardPaste);
            }
            None
        }
        NativeWindowInputEvent::InputCaptureChanged { captured } => {
            if let Some(sender) = event_sender.as_ref() {
                let _ = sender.send(Event::InputCaptureChanged { captured });
            }
            None
        }
        NativeWindowInputEvent::Key {
            pressed,
            keycode,
            scancode,
            modifiers,
            timestamp_us,
        } => {
            let payload = KeyboardPayload {
                keycode: layout_mapped_keyboard_keycode(keycode, scancode),
                scancode: layout_mapped_keyboard_scancode(scancode),
                modifiers,
                timestamp_us,
            };
            Some(if pressed {
                encoder.encode_key_down(payload)
            } else {
                encoder.encode_key_up(payload)
            })
        }
        NativeWindowInputEvent::MouseMove {
            dx,
            dy,
            timestamp_us,
        } => Some(encoder.encode_mouse_move(MouseMovePayload {
            dx,
            dy,
            timestamp_us,
        })),
        NativeWindowInputEvent::MouseButton {
            pressed,
            button,
            timestamp_us,
        } => {
            let payload = MouseButtonPayload {
                button,
                timestamp_us,
            };
            Some(if pressed {
                encoder.encode_mouse_button_down(payload)
            } else {
                encoder.encode_mouse_button_up(payload)
            })
        }
        NativeWindowInputEvent::MouseWheel {
            delta,
            timestamp_us,
        } => Some(encoder.encode_mouse_wheel(MouseWheelPayload {
            delta,
            timestamp_us,
        })),
        NativeWindowInputEvent::LockKeysSync { state } => {
            Some(encoder.encode_lock_keys_sync(state))
        }
    }
}

#[cfg(target_os = "windows")]
#[allow(dead_code)]
fn send_encoded_native_window_input_event(
    encoder: &InputEncoder,
    input_channels: &GstreamerInputChannels,
    event_sender: &Option<Sender<Event>>,
    event: NativeWindowInputEvent,
) {
    let Some(payload) = encode_native_window_input_payload(encoder, event_sender, event) else {
        return;
    };

    let partially_reliable = matches!(event, NativeWindowInputEvent::MouseMove { .. });
    let _ = input_channels.send_packet(&payload, partially_reliable);
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct NativeGamepadSnapshot {
    connected: bool,
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    left_stick_x: i16,
    left_stick_y: i16,
    right_stick_x: i16,
    right_stick_y: i16,
}

#[cfg(target_os = "windows")]
impl NativeGamepadSnapshot {
    fn from_xinput(snapshot: win32_xinput::XInputGamepadSnapshot) -> Self {
        Self {
            connected: true,
            buttons: snapshot.buttons,
            left_trigger: snapshot.left_trigger,
            right_trigger: snapshot.right_trigger,
            left_stick_x: snapshot.left_stick_x,
            left_stick_y: snapshot.left_stick_y,
            right_stick_x: snapshot.right_stick_x,
            right_stick_y: snapshot.right_stick_y,
        }
    }

    fn is_neutral(self) -> bool {
        self.buttons == 0
            && self.left_trigger == 0
            && self.right_trigger == 0
            && self.left_stick_x == 0
            && self.left_stick_y == 0
            && self.right_stick_x == 0
            && self.right_stick_y == 0
    }
}

#[cfg(target_os = "windows")]
fn spawn_native_gamepad_thread(
    input_state: GstreamerInputState,
    input_channels: GstreamerInputChannels,
    event_sender: Option<Sender<Event>>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let Some(xinput) = (unsafe { win32_xinput::XInput::load() }) else {
            send_log(
                &event_sender,
                "warn",
                "Native XInput gamepad bridge unavailable; controller input will require the web renderer fallback.".to_owned(),
            );
            return;
        };

        send_log(
            &event_sender,
            "info",
            "Native XInput gamepad bridge armed.".to_owned(),
        );

        let mut previous = [NativeGamepadSnapshot::default(); GAMEPAD_MAX_CONTROLLERS as usize];
        let mut last_sent = [Instant::now(); GAMEPAD_MAX_CONTROLLERS as usize];
        let mut suppress_until_neutral = [false; GAMEPAD_MAX_CONTROLLERS as usize];
        let mut was_paused = false;

        while !stop.load(Ordering::SeqCst) {
            if input_state.ready.load(Ordering::SeqCst) {
                let (snapshots, bitmap) = poll_xinput_gamepads(xinput);

                if input_state.paused.load(Ordering::SeqCst) {
                    if !was_paused {
                        send_neutral_gamepad_snapshots_for_pause(
                            &input_state,
                            &input_channels,
                            &previous,
                            &snapshots,
                        );
                    }
                    previous = snapshots;
                    for controller_id in 0..GAMEPAD_MAX_CONTROLLERS as usize {
                        last_sent[controller_id] = Instant::now();
                    }
                    was_paused = true;
                    thread::sleep(NATIVE_GAMEPAD_POLL_INTERVAL);
                    continue;
                }

                if was_paused {
                    for controller_id in 0..GAMEPAD_MAX_CONTROLLERS as usize {
                        let snapshot = snapshots[controller_id];
                        suppress_until_neutral[controller_id] =
                            snapshot.connected && !snapshot.is_neutral();
                        previous[controller_id] = snapshot;
                        last_sent[controller_id] = Instant::now();
                    }
                }
                was_paused = false;

                for controller_id in 0..GAMEPAD_MAX_CONTROLLERS as usize {
                    let snapshot = snapshots[controller_id];
                    if suppress_until_neutral[controller_id] {
                        previous[controller_id] = snapshot;
                        last_sent[controller_id] = Instant::now();
                        if !snapshot.connected || snapshot.is_neutral() {
                            suppress_until_neutral[controller_id] = false;
                        }
                        continue;
                    }
                    let state_changed = snapshot != previous[controller_id];
                    let keepalive_due = snapshot.connected
                        && last_sent[controller_id].elapsed() >= NATIVE_GAMEPAD_KEEPALIVE_INTERVAL;

                    if state_changed || keepalive_due {
                        send_native_gamepad_snapshot(
                            &input_state,
                            &input_channels,
                            controller_id as u8,
                            bitmap,
                            snapshot,
                        );
                        last_sent[controller_id] = Instant::now();

                        if snapshot.connected != previous[controller_id].connected {
                            send_log(
                                &event_sender,
                                "info",
                                format!(
                                    "Native XInput controller {controller_id} {}.",
                                    if snapshot.connected {
                                        "connected"
                                    } else {
                                        "disconnected"
                                    }
                                ),
                            );
                        }
                    }

                    previous[controller_id] = snapshot;
                }
            } else {
                was_paused = false;
            }

            thread::sleep(NATIVE_GAMEPAD_POLL_INTERVAL);
        }
    })
}

#[cfg(target_os = "windows")]
fn poll_xinput_gamepads(
    xinput: win32_xinput::XInput,
) -> (
    [NativeGamepadSnapshot; GAMEPAD_MAX_CONTROLLERS as usize],
    u16,
) {
    let mut snapshots = [NativeGamepadSnapshot::default(); GAMEPAD_MAX_CONTROLLERS as usize];
    let mut bitmap = 0u16;

    for controller_id in 0..GAMEPAD_MAX_CONTROLLERS as usize {
        if let Some(snapshot) = unsafe { xinput.get_state(controller_id as u32) } {
            snapshots[controller_id] = NativeGamepadSnapshot::from_xinput(snapshot);
            bitmap |= 1 << controller_id;
        }
    }

    (snapshots, bitmap)
}

#[cfg(target_os = "windows")]
fn send_neutral_gamepad_snapshots_for_pause(
    input_state: &GstreamerInputState,
    input_channels: &GstreamerInputChannels,
    previous: &[NativeGamepadSnapshot; GAMEPAD_MAX_CONTROLLERS as usize],
    current: &[NativeGamepadSnapshot; GAMEPAD_MAX_CONTROLLERS as usize],
) {
    let bitmap = previous.iter().zip(current.iter()).enumerate().fold(
        0u16,
        |bitmap, (controller_id, (previous, current))| {
            if previous.connected || current.connected {
                bitmap | (1 << controller_id)
            } else {
                bitmap
            }
        },
    );

    if bitmap == 0 {
        return;
    }

    for controller_id in 0..GAMEPAD_MAX_CONTROLLERS as usize {
        if (bitmap & (1 << controller_id)) == 0 {
            continue;
        }

        send_native_gamepad_snapshot(
            input_state,
            input_channels,
            controller_id as u8,
            bitmap,
            NativeGamepadSnapshot {
                connected: true,
                ..NativeGamepadSnapshot::default()
            },
        );
    }
}

#[cfg(target_os = "windows")]
fn send_native_gamepad_snapshot(
    input_state: &GstreamerInputState,
    input_channels: &GstreamerInputChannels,
    controller_id: u8,
    bitmap: u16,
    snapshot: NativeGamepadSnapshot,
) {
    if !input_state.ready.load(Ordering::SeqCst) {
        return;
    }

    let use_partially_reliable =
        (PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL & (1_u32 << u32::from(controller_id))) != 0;
    let input = GamepadInput {
        controller_id,
        buttons: snapshot.buttons,
        left_trigger: snapshot.left_trigger,
        right_trigger: snapshot.right_trigger,
        left_stick_x: snapshot.left_stick_x,
        left_stick_y: snapshot.left_stick_y,
        right_stick_x: snapshot.right_stick_x,
        right_stick_y: snapshot.right_stick_y,
        connected: snapshot.connected,
        timestamp_us: native_input_timestamp_us(),
    };

    let Ok(mut encoder) = input_state.encoder.lock() else {
        return;
    };
    let mut payload = encoder.encode_gamepad_state(bitmap, input, use_partially_reliable);
    drop(encoder);

    restamp_protocol_v3_outer_timestamp(&mut payload, native_input_timestamp_us());
    let _ = input_channels.send_packet(&payload, use_partially_reliable);
}

#[cfg(target_os = "windows")]
fn native_input_timestamp_us() -> u64 {
    native_input_clock_us()
}

pub(crate) fn wire_remote_data_channels(
    webrtc: &gst::Element,
    event_sender: Option<Sender<Event>>,
) {
    webrtc.connect("on-data-channel", false, move |values| {
        let Some(channel) = values
            .get(1)
            .and_then(|value| value.get::<gst_webrtc::WebRTCDataChannel>().ok())
        else {
            send_log(
                &event_sender,
                "warn",
                "GStreamer emitted on-data-channel without a channel.".to_owned(),
            );
            return None;
        };

        let label = channel_label(&channel);
        send_log(
            &event_sender,
            "info",
            format!(
                "Remote WebRTC data channel received: label={}, ordered={}.",
                label,
                channel.is_ordered()
            ),
        );
        connect_remote_data_channel_callbacks(&label, &channel, event_sender.clone());
        None
    });
}

pub(crate) fn create_input_data_channels(
    webrtc: &gst::Element,
    input_state: GstreamerInputState,
    event_sender: Option<Sender<Event>>,
    partial_reliable_threshold_ms: u32,
) -> Result<GstreamerInputChannels, String> {
    let reliable = create_data_channel(webrtc, RELIABLE_INPUT_CHANNEL_LABEL, None)?;
    connect_input_channel_callbacks(
        RELIABLE_INPUT_CHANNEL_LABEL,
        &reliable,
        input_state.clone(),
        event_sender.clone(),
    );

    let clamped_threshold_ms = if partial_reliable_threshold_ms == 0 {
        DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS
    } else {
        partial_reliable_threshold_ms.clamp(1, 5000)
    };
    let options = gst::Structure::builder("data-channel-options")
        .field("ordered", false)
        .field("max-packet-lifetime", clamped_threshold_ms as i32)
        .build();
    let partially_reliable = create_data_channel(
        webrtc,
        PARTIALLY_RELIABLE_INPUT_CHANNEL_LABEL,
        Some(options),
    )?;
    connect_input_channel_callbacks(
        PARTIALLY_RELIABLE_INPUT_CHANNEL_LABEL,
        &partially_reliable,
        input_state,
        event_sender.clone(),
    );

    // GFN `stats_channel`: the web client creates this channel itself
    // (createDataChannel("stats_channel", { ordered: false, maxRetransmits: 0 }))
    // so the server sends its telemetry on it (avgGameFps, network stats).
    // Without a locally-created stats channel the server never delivers stats
    // frames — which is why the native HUD gameFps stayed empty — so create it
    // with the exact same options for parity.
    let stats_options = gst::Structure::builder("data-channel-options")
        .field("ordered", false)
        .field("max-retransmits", 0)
        .build();
    let stats_channel = create_data_channel(webrtc, STATS_CHANNEL_LABEL, Some(stats_options))?;
    connect_remote_data_channel_callbacks(
        STATS_CHANNEL_LABEL,
        &stats_channel,
        event_sender.clone(),
    );

    send_log(
        &event_sender,
        "info",
        format!(
            "Created WebRTC input data channels ({}, {} maxPacketLifeTime={}ms) and stats channel ({STATS_CHANNEL_LABEL}).",
            RELIABLE_INPUT_CHANNEL_LABEL,
            PARTIALLY_RELIABLE_INPUT_CHANNEL_LABEL,
            clamped_threshold_ms
        ),
    );

    Ok(GstreamerInputChannels {
        reliable,
        partially_reliable,
    })
}

fn create_data_channel(
    webrtc: &gst::Element,
    label: &'static str,
    options: Option<gst::Structure>,
) -> Result<gst_webrtc::WebRTCDataChannel, String> {
    let channel = match options {
        Some(options) => {
            let options = Some(options);
            webrtc.emit_by_name::<gst_webrtc::WebRTCDataChannel>(
                "create-data-channel",
                &[&label, &options],
            )
        }
        None => webrtc.emit_by_name::<gst_webrtc::WebRTCDataChannel>(
            "create-data-channel",
            &[&label, &None::<gst::Structure>],
        ),
    };

    let actual_label = channel_label(&channel);
    if actual_label != label {
        return Err(format!(
            "GStreamer created data channel with unexpected label: expected {label}, got {actual_label}."
        ));
    }

    Ok(channel)
}

fn connect_input_channel_callbacks(
    label: &'static str,
    channel: &gst_webrtc::WebRTCDataChannel,
    input_state: GstreamerInputState,
    event_sender: Option<Sender<Event>>,
) {
    let open_sender = event_sender.clone();
    channel.connect_on_open(move |channel| {
        send_log(
            &open_sender,
            "info",
            format!(
                "Input data channel open: label={}, id={}, ordered={}, maxPacketLifeTime={}.",
                label,
                channel.id(),
                channel.is_ordered(),
                channel.max_packet_lifetime()
            ),
        );
    });

    let close_sender = event_sender.clone();
    let close_state = input_state.clone();
    channel.connect_on_close(move |_| {
        if label == RELIABLE_INPUT_CHANNEL_LABEL {
            close_state.ready.store(false, Ordering::SeqCst);
            close_state.heartbeat_stop.store(true, Ordering::SeqCst);
        }
        send_log(
            &close_sender,
            "info",
            format!("Input data channel closed: label={label}."),
        );
    });

    let error_sender = event_sender.clone();
    channel.connect_on_error(move |_, error| {
        send_log(
            &error_sender,
            "warn",
            format!("Input data channel error on {label}: {error}."),
        );
    });

    if label == RELIABLE_INPUT_CHANNEL_LABEL {
        let data_sender = event_sender.clone();
        let data_state = input_state.clone();
        channel.connect_on_message_data(move |channel, data| {
            let Some(bytes) = data else {
                return;
            };
            handle_input_handshake_message(
                channel,
                bytes.as_ref(),
                data_state.clone(),
                data_sender.clone(),
            );
        });

        let string_sender = event_sender.clone();
        let string_state = input_state;
        channel.connect_on_message_string(move |channel, message| {
            let Some(message) = message else {
                return;
            };
            handle_input_handshake_message(
                channel,
                message.as_bytes(),
                string_state.clone(),
                string_sender.clone(),
            );
        });
    }
}

/// Remote WebRTC data channels (created by the server, e.g. GFN's
/// `control_channel`) keyed by label. The input/stats channels are created
/// locally; anything else is registered here so the renderer can reply over
/// the same channel (clipboard control messages, etc.).
static REMOTE_DATA_CHANNELS: OnceLock<Mutex<HashMap<String, gst_webrtc::WebRTCDataChannel>>> =
    OnceLock::new();

/// Send a base64 payload on a remote data channel (e.g. GFN `control_channel`
/// clipboard responses). Returns an error when the channel is unknown or not
/// open so the caller can surface it instead of silently dropping the reply.
pub(crate) fn send_remote_data_channel_message(
    label: &str,
    payload_base64: &str,
) -> Result<(), String> {
    let payload = BASE64_STANDARD
        .decode(payload_base64)
        .map_err(|error| format!("Invalid data-channel payload base64: {error}"))?;
    let channels = REMOTE_DATA_CHANNELS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| "Remote data channel registry poisoned.".to_owned())?;
    let Some(channel) = channels.get(label) else {
        return Err(format!(
            "No remote data channel registered with label \"{label}\"."
        ));
    };
    if channel.ready_state() != gst_webrtc::WebRTCDataChannelState::Open {
        return Err(format!("Remote data channel \"{label}\" is not open."));
    }
    let bytes = glib::Bytes::from_owned(payload);
    channel
        .send_data_full(Some(&bytes))
        .map_err(|error| format!("Failed to send on remote data channel \"{label}\": {error}"))
}

fn connect_remote_data_channel_callbacks(
    label: &str,
    channel: &gst_webrtc::WebRTCDataChannel,
    event_sender: Option<Sender<Event>>,
) {
    let label = label.to_owned();
    // Register non-native channels so replies can be routed back to the server.
    if label != STATS_CHANNEL_LABEL
        && label != RELIABLE_INPUT_CHANNEL_LABEL
        && label != PARTIALLY_RELIABLE_INPUT_CHANNEL_LABEL
    {
        if let Ok(mut channels) = REMOTE_DATA_CHANNELS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
        {
            channels.insert(label.clone(), channel.clone());
        }
    }
    let open_sender = event_sender.clone();
    let open_label = label.clone();
    channel.connect_on_open(move |_| {
        send_log(
            &open_sender,
            "info",
            format!("Remote data channel open: label={open_label}."),
        );
    });

    let close_sender = event_sender.clone();
    let close_label = label.clone();
    channel.connect_on_close(move |_| {
        send_log(
            &close_sender,
            "info",
            format!("Remote data channel closed: label={close_label}."),
        );
    });

    let stats_sender = event_sender.clone();
    let error_sender = event_sender.clone();
    let error_label = label.clone();
    channel.connect_on_error(move |_, error| {
        send_log(
            &error_sender,
            "warn",
            format!("Remote data channel error on {error_label}: {error}."),
        );
    });

    // Non-native remote channels (GFN `control_channel` and friends): relay
    // every message verbatim (base64) to the renderer so server-initiated
    // protocols — clipboard paste requests, etc. — work in native mode exactly
    // like the web client's data channel handler.
    if label != STATS_CHANNEL_LABEL {
        let relay_sender = event_sender.clone();
        let relay_label = label.clone();
        channel.connect_on_message_data(move |_channel, data| {
            let Some(bytes) = data else {
                return;
            };
            let payload_base64 = BASE64_STANDARD.encode(bytes.as_ref());
            if let Some(sender) = relay_sender.as_ref() {
                let _ = sender.send(Event::DataChannelMessage {
                    label: relay_label.clone(),
                    payload_base64,
                });
            }
        });

        let relay_sender = event_sender.clone();
        let relay_label = label.clone();
        channel.connect_on_message_string(move |_channel, message| {
            let Some(message) = message else {
                return;
            };
            let payload_base64 = BASE64_STANDARD.encode(message.as_bytes());
            if let Some(sender) = relay_sender.as_ref() {
                let _ = sender.send(Event::DataChannelMessage {
                    label: relay_label.clone(),
                    payload_base64,
                });
            }
        });
    }

    // GFN `stats_channel`: parse the server-reported game FPS + network
    // telemetry (RTT, packet loss) so the native HUD shows the same numbers as
    // the official clients. The full frame is hex-dumped on the first message,
    // then once a minute, to keep verifying the payload layout against real
    // sessions (the decoded RTT / loss are printed on the same line).
    if label == STATS_CHANNEL_LABEL {
        channel.connect_on_message_data(move |_channel, data| {
            let Some(bytes) = data else {
                return;
            };
            let bytes = bytes.as_ref();
            let fps = parse_stats_channel_game_fps(bytes);
            if let Some(fps) = fps {
                STATS_CHANNEL_GAME_FPS
                    .get_or_init(|| AtomicU32::new(0))
                    .store(fps, Ordering::Relaxed);
            }
            let network = parse_stats_channel_network(bytes);
            if network.rtt_ms > 0 {
                STATS_CHANNEL_RTT_MS
                    .get_or_init(|| AtomicU32::new(0))
                    .store(network.rtt_ms, Ordering::Relaxed);
                if let Ok(mut slot) = STATS_CHANNEL_RTT_LAST_SEEN_AT
                    .get_or_init(|| Mutex::new(Instant::now()))
                    .lock()
                {
                    *slot = Instant::now();
                }
            }
            if let Some(loss) = network.packet_loss_fraction {
                STATS_CHANNEL_LOSS_BPS
                    .get_or_init(|| AtomicU32::new(0))
                    .store((loss * 10_000.0).round() as u32, Ordering::Relaxed);
            }
            // Feed the payload-offset-1 counter into the bitrate estimator on
            // EVERY frame (not just the logged ones): the estimator needs
            // per-message deltas to verify whether the counter is cumulative
            // bytes, and to keep the HUD bitrate live once verified.
            let rate_sample = network.counter.and_then(feed_stats_channel_counter);
            let bitrate_kbps = stats_channel_bitrate_kbps();
            static MESSAGE_COUNT: OnceLock<AtomicU32> = OnceLock::new();
            let message_count = MESSAGE_COUNT
                .get_or_init(|| AtomicU32::new(0))
                .fetch_add(1, Ordering::Relaxed)
                + 1;
            static FIRST_DUMP: OnceLock<AtomicBool> = OnceLock::new();
            static LAST_DUMP_AT: OnceLock<Mutex<Instant>> = OnceLock::new();
            let first = !FIRST_DUMP
                .get_or_init(|| AtomicBool::new(false))
                .swap(true, Ordering::Relaxed);
            let due = LAST_DUMP_AT
                .get_or_init(|| Mutex::new(Instant::now()))
                .lock()
                .map(|last| last.elapsed() >= Duration::from_secs(60))
                .unwrap_or(false);
            // Burst-log the first 25 frames (~6-25s of samples) with the raw
            // per-message counter delta + derived rate — this is the data that
            // settles whether payload offset 1 is a cumulative byte counter
            // (steady positive deltas) or something else (resets / wild rates).
            let burst = message_count <= 25;
            if burst {
                send_log(
                    &stats_sender,
                    "info",
                    format!(
                        "Stats channel sample: #{} counter={} Δ={} dt={:.0}ms rate={:.2}Mbps ema={}Mbps",
                        message_count,
                        network
                            .counter
                            .map(|counter| format!("{counter:.0}"))
                            .unwrap_or_else(|| "?".to_owned()),
                        rate_sample.map(|(delta, _, _)| format!("{delta:.0}")).unwrap_or_else(|| "?".to_owned()),
                        rate_sample.map(|(_, dt, _)| dt * 1000.0).unwrap_or(0.0),
                        rate_sample.map(|(_, _, kbps)| kbps / 1000.0).unwrap_or(-1.0),
                        bitrate_kbps
                            .map(|kbps| format!("{:.2}", f64::from(kbps) / 1000.0))
                            .unwrap_or_else(|| "-".to_owned()),
                    ),
                );
            }
            if first || due {
                if let Ok(mut last) = LAST_DUMP_AT
                    .get_or_init(|| Mutex::new(Instant::now()))
                    .lock()
                {
                    *last = Instant::now();
                }
                let hex: String = bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
                send_log(
                    &stats_sender,
                    "info",
                    format!(
                        "Stats channel frame: len={} gameFps={} rtt={}ms loss={:.4}% counter={} bitrate={}Mbps payload={hex}",
                        bytes.len(),
                        fps.map(|fps| fps.to_string())
                            .unwrap_or_else(|| "?".to_owned()),
                        network.rtt_ms,
                        network
                            .packet_loss_fraction
                            .map(|loss| loss * 100.0)
                            .unwrap_or(-1.0),
                        network
                            .counter
                            .map(|counter| format!("{counter:.0}"))
                            .unwrap_or_else(|| "?".to_owned()),
                        bitrate_kbps
                            .map(|kbps| format!("{:.2}", f64::from(kbps) / 1000.0))
                            .unwrap_or_else(|| "-".to_owned()),
                    ),
                );
            }
        });
    }
}

fn handle_input_handshake_message(
    channel: &gst_webrtc::WebRTCDataChannel,
    bytes: &[u8],
    input_state: GstreamerInputState,
    event_sender: Option<Sender<Event>>,
) {
    let Some(protocol_version) = parse_input_handshake_version(bytes) else {
        return;
    };

    let encoder_version = protocol_version.min(u8::MAX as u16) as u8;
    if let Ok(mut encoder) = input_state.encoder.lock() {
        encoder.set_protocol_version(encoder_version);
    }
    let was_ready = input_state.ready.swap(true, Ordering::SeqCst);
    if was_ready {
        return;
    }

    // Stacked mode: input is ready — arm the sink-window RawInput mouse +
    // keyboard capture (if the sink/shell are ready; the stacked guard retries
    // otherwise). No-op in other render modes.
    crate::gstreamer_platform::arm_stacked_sink_input_capture();

    send_log(
        &event_sender,
        "info",
        format!(
            "Input handshake complete on {} (protocol v{}).",
            channel_label(channel),
            protocol_version
        ),
    );
    if let Some(sender) = event_sender.as_ref() {
        let _ = sender.send(Event::InputReady { protocol_version });
    }
    start_input_heartbeat(input_state, channel.clone(), event_sender);
}

pub(crate) fn parse_input_handshake_version(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 2 {
        return None;
    }

    let first_word = u16::from_le_bytes([bytes[0], bytes[1]]);
    if first_word == 526 {
        return Some(if bytes.len() >= 4 {
            u16::from_le_bytes([bytes[2], bytes[3]])
        } else {
            2
        });
    }

    if bytes[0] == 0x0e {
        return Some(first_word);
    }

    None
}

fn start_input_heartbeat(
    input_state: GstreamerInputState,
    channel: gst_webrtc::WebRTCDataChannel,
    event_sender: Option<Sender<Event>>,
) {
    let Ok(mut heartbeat_thread) = input_state.heartbeat_thread.lock() else {
        send_log(
            &event_sender,
            "warn",
            "Failed to acquire input heartbeat thread lock.".to_owned(),
        );
        return;
    };
    if heartbeat_thread
        .as_ref()
        .is_some_and(|thread| !thread.is_finished())
    {
        return;
    }
    if let Some(thread) = heartbeat_thread.take() {
        let _ = thread.join();
    }

    input_state.heartbeat_stop.store(false, Ordering::SeqCst);
    let encoder = input_state.encoder.clone();
    let stop = input_state.heartbeat_stop.clone();
    let thread_sender = event_sender.clone();
    *heartbeat_thread = Some(thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            send_input_heartbeat(&channel, &encoder, &thread_sender);

            let mut slept = Duration::ZERO;
            while slept < HEARTBEAT_INTERVAL {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let remaining = HEARTBEAT_INTERVAL.saturating_sub(slept);
                let interval = remaining.min(HEARTBEAT_STOP_POLL_INTERVAL);
                thread::sleep(interval);
                slept += interval;
            }
        }
    }));
}

fn send_input_heartbeat(
    channel: &gst_webrtc::WebRTCDataChannel,
    encoder: &Arc<Mutex<InputEncoder>>,
    event_sender: &Option<Sender<Event>>,
) {
    if channel.ready_state() != gst_webrtc::WebRTCDataChannelState::Open {
        return;
    }

    let Ok(encoder) = encoder.lock() else {
        send_log(
            event_sender,
            "warn",
            "Failed to acquire input encoder for heartbeat.".to_owned(),
        );
        return;
    };
    let bytes = glib::Bytes::from_owned(encoder.encode_heartbeat());
    if let Err(error) = channel.send_data_full(Some(&bytes)) {
        send_log(
            event_sender,
            "warn",
            format!("Failed to send input heartbeat: {error}."),
        );
    }
}

pub(crate) fn channel_label(channel: &gst_webrtc::WebRTCDataChannel) -> String {
    channel
        .label()
        .map(|label| label.to_string())
        .unwrap_or_else(|| "<unlabeled>".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_stats_frame(frame_type: u8, version: u8, fps: f64) -> Vec<u8> {
        // Mirrors buildStatsChannelMessage in the web client tests.
        let payload_offset = if frame_type == 3 { 1 } else { 0 };
        let mut bytes = vec![0u8; payload_offset + 33];
        bytes[0] = frame_type;
        bytes[payload_offset] = version;
        bytes[payload_offset + 25..payload_offset + 33].copy_from_slice(&fps.to_le_bytes());
        bytes
    }

    #[test]
    fn parses_stats_channel_game_fps() {
        assert_eq!(
            parse_stats_channel_game_fps(&build_stats_frame(4, 4, 119.7)),
            Some(120)
        );
        assert_eq!(
            parse_stats_channel_game_fps(&build_stats_frame(4, 4, 60.2)),
            Some(60)
        );
        // Type 3 (1-byte header) with protocol v5.
        assert_eq!(
            parse_stats_channel_game_fps(&build_stats_frame(3, 5, 144.0)),
            Some(144)
        );
    }

    #[test]
    fn rejects_invalid_stats_channel_frames() {
        assert_eq!(parse_stats_channel_game_fps(&[]), None);
        // Bad TYPE discriminator frames are dropped.
        assert_eq!(parse_stats_channel_game_fps(&[9, 4, 0, 0]), None);
        // Version < 4 carries no avgGameFps.
        assert_eq!(
            parse_stats_channel_game_fps(&build_stats_frame(4, 3, 60.0)),
            None
        );
        // Too short for the fixed payload layout.
        assert_eq!(parse_stats_channel_game_fps(&vec![4u8; 20]), None);
        // NaN fps.
        assert_eq!(
            parse_stats_channel_game_fps(&build_stats_frame(4, 4, f64::NAN)),
            None
        );
        // Out-of-range fps.
        assert_eq!(
            parse_stats_channel_game_fps(&build_stats_frame(4, 4, 5000.0)),
            None
        );
    }

    fn build_network_frame(rtt_ms: f64, loss_fraction: f64, counter: f64) -> Vec<u8> {
        // Same fixed layout as build_stats_frame, with the counter + network
        // float64s at payload offsets 1 (counter), 9 (loss) and 17 (RTT).
        let payload_offset = 1; // type 3, v5
        let mut bytes = vec![0u8; payload_offset + 33];
        bytes[0] = 3;
        bytes[payload_offset] = 5;
        bytes[payload_offset + 1..payload_offset + 9].copy_from_slice(&counter.to_le_bytes());
        bytes[payload_offset + 9..payload_offset + 17]
            .copy_from_slice(&loss_fraction.to_le_bytes());
        bytes[payload_offset + 17..payload_offset + 25].copy_from_slice(&rtt_ms.to_le_bytes());
        bytes
    }

    #[test]
    fn parses_stats_channel_network_telemetry() {
        // Type 3, v5: counter 74,629,000, loss fraction 0.000173, RTT 39.0 ms.
        let network =
            parse_stats_channel_network(&build_network_frame(39.0, 0.000173, 74_629_000.0));
        assert_eq!(network.rtt_ms, 39);
        let loss = network.packet_loss_fraction.expect("loss reported");
        assert!((loss - 0.000173).abs() < 1e-9);
        assert_eq!(network.counter, Some(74_629_000.0));

        // Zero RTT (server has no measurement yet) → 0, loss + counter still parsed.
        let network = parse_stats_channel_network(&build_network_frame(0.0, 0.0002, 1_000_000.0));
        assert_eq!(network.rtt_ms, 0);
        assert!(network.packet_loss_fraction.is_some());
        assert_eq!(network.counter, Some(1_000_000.0));

        // Absurd RTT (> 2s), negative loss and negative counter are rejected.
        let network = parse_stats_channel_network(&build_network_frame(5000.0, -0.5, -1.0));
        assert_eq!(network.rtt_ms, 0);
        assert!(network.packet_loss_fraction.is_none());
        assert!(network.counter.is_none());

        // Real frame captured from a live session (counter=74,629,000, fps=120,
        // rtt=39ms, loss=0.000173) — guards the decoded layout against regressions.
        let real = [
            0x03u8, 0x05, 0x00, 0x00, 0x00, 0x20, 0xfe, 0xca, 0x91, 0x41, 0x2e, 0x93, 0xd6, 0xae,
            0x7e, 0xb9, 0x26, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x43, 0x40, 0xaa, 0xed,
            0xa7, 0x05, 0xbd, 0xfb, 0x5d, 0x40,
        ];
        assert_eq!(parse_stats_channel_game_fps(&real), Some(120));
        let network = parse_stats_channel_network(&real);
        assert_eq!(network.counter, Some(74_629_000.0));
        assert_eq!(network.rtt_ms, 39);
        let loss = network.packet_loss_fraction.expect("loss reported");
        assert!((loss - 0.000173).abs() < 1e-6);
    }

    #[test]
    fn rate_estimator_tracks_stable_byte_deltas() {
        let t0 = Instant::now();
        let mut estimator = StatsChannelRateEstimator::default();
        // First sample only establishes the baseline (no delta yet).
        assert!(estimator.feed(0.0, t0).is_none());
        // 1 MB per 500 ms → 16,000 kbps = 16 Mbps, steady over 8 samples.
        let mut counter = 0.0;
        for i in 1..=8 {
            counter += 1_000_000.0;
            let accepted = estimator.feed(counter, t0 + Duration::from_millis(i * 500));
            assert!(accepted.is_some(), "sample {i} should be accepted");
        }
        assert!(estimator.samples >= 5);
        assert!((estimator.ema_kbps - 16_000.0).abs() < 2_000.0);
    }

    #[test]
    fn rate_estimator_resets_on_counter_reset_or_wrap() {
        let t0 = Instant::now();
        let mut estimator = StatsChannelRateEstimator::default();
        estimator.feed(1_000_000.0, t0);
        // Counter goes backwards (server-side reset) → baseline restarts and
        // any accumulated EMA is dropped.
        assert!(estimator
            .feed(500_000.0, t0 + Duration::from_millis(500))
            .is_none());
        assert_eq!(estimator.samples, 0);
        // A wild forward jump (wrap artifact → absurd rate) also resets.
        estimator.feed(1_000_000.0, t0 + Duration::from_millis(1000));
        assert!(estimator
            .feed(
                1_000_000.0 + 2f64.powi(40),
                t0 + Duration::from_millis(1500)
            )
            .is_none());
        assert_eq!(estimator.samples, 0);
        // Sane deltas after a reset recover the estimate from scratch.
        // (Re-baseline first: the rejected wrap sample left a huge last counter.)
        estimator.feed(1_000_000.0, t0 + Duration::from_millis(2000));
        let mut counter = 1_000_000.0;
        for i in 1..=6 {
            counter += 1_000_000.0;
            assert!(estimator
                .feed(counter, t0 + Duration::from_millis(2000 + i * 500))
                .is_some());
        }
        assert!(estimator.samples >= 5);
    }
}
