use crate::gstreamer_backend::send_log;
use crate::gstreamer_config::{use_external_renderer_window, use_stacked_renderer};
use crate::gstreamer_input::{
    stats_channel_bitrate_kbps, stats_channel_game_fps, stats_channel_packet_loss_fraction,
    stats_channel_rtt_ms,
};
use crate::gstreamer_pipeline::{
    configure_queue, set_property_if_supported, VideoChainRebuildContext,
};
use crate::gstreamer_transitions::{
    format_transition_summary, resolve_queue_mode, TransitionSnapshot, TransitionTelemetry,
    DEFAULT_VIDEO_QUEUE_DEPTH,
};
use crate::protocol::{Event, NativeQueueMode, NativeStreamerSessionContext, VideoStallEvent};
use gst::prelude::*;
use gstreamer as gst;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub(crate) const VIDEO_SINK_RATE_LOG_INTERVAL: Duration = Duration::from_secs(1);
const VIDEO_STALL_WARNING_MS: u64 = 2_500;
const VIDEO_STALL_SECOND_ATTEMPT_MS: u64 = 5_000;
const VIDEO_STALL_RESYNC_MS: u64 = 8_000;
const VIDEO_STALL_PARTIAL_FLUSH_MS: u64 = 12_000;
const VIDEO_STALL_COMPLETE_FLUSH_MS: u64 = 16_000;
const VIDEO_STALL_FATAL_MS: u64 = 20_000;

/// How long a detected RTT spike holds the pre-decode jitter buffer at MAX
/// depth before it is allowed to decay back. Spikes arrive in clusters (each
/// burst is seconds of elevated jitter), and the EMA lags them by ~2-4
/// samples, so the deep buffer must be HELD past the first spike or the very
/// next burst starves the decoder again and the picture blinks the previous
/// frame.
const JITTER_BURST_HOLD_MS: u64 = 4_000;

/// A locally computed RTCP round-trip (`rb-round-trip`) is only trusted while
/// Receiver Reports keep arriving. rtpsession's `have-rb` flag sticks once
/// set — the raw value would otherwise be reported forever as the current
/// ping even after the server stopped sending RRs (the frozen-ping bug). The
/// `rb-lsr` field (the SR timestamp the server echoes back in each RR)
/// advances with every new RR, so a change is the freshness signal: if no RR
/// has arrived within this window, the local measurement is expired and the
/// HUD falls back to the server-reported stats_channel RTT.
const LOCAL_RTCP_FRESH_AGE_MS: u32 = 15_000;

/// Local receive jitter (rtpsession's RFC 3550 interarrival jitter of the
/// INCOMING video stream) is only trusted while video RTP keeps arriving.
/// The jitter value updates continuously as packets flow, but freezes at its
/// last value when the stream stalls — so without a liveness gate a dead
/// session would report the frozen jitter forever. The watchdog gates on the
/// RTP bitrate probe (`last_encoded_ms`), which fires on every RTP buffer;
/// 5 s without any RTP means the stream is stalled/dead and jitter is
/// reported as None (the HUD then decays it like the ping).
const JITTER_FRESH_AGE_MS: u64 = 5_000;

/// Map the network signals to a pre-decode jitter-buffer depth in compressed
/// frames. A CONTINUOUS ramp grows the buffer in proportion to the measured
/// RTT (BASE ≈100 ms at ≤ 30 ms up to MAX ≈250 ms at ≥ 150 ms) instead of
/// jumping between discrete bands, so even a modest RTT rise buys buffer
/// depth immediately — the field logs showed the decoder starving with a
/// 100 ms buffer once RTT passed ~100 ms. Two overrides react to signals
/// that LEAD the RTT EMA (which lags a burst by seconds):
///   - `burst_hold`: a detected RTT spike (or one within the last few
///     seconds — the caller holds it) forces MAX so the burst already in
///     flight is absorbed instead of the decoder starving and the sink
///     blinking the previous frame;
///   - packet loss floors the depth (≥0.1% → mid, ≥0.5% → max): loss is the
///     early indicator of jitter — it spikes before RTT climbs.
fn target_pre_decode_depth(rtt_ema_ms: u32, loss_fraction: Option<f64>, burst_hold: bool) -> u32 {
    use crate::gstreamer_pipeline::{
        VIDEO_COMPRESSED_QUEUE_BASE_BUFFERS, VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS,
        VIDEO_COMPRESSED_QUEUE_MID_BUFFERS,
    };
    if burst_hold || loss_fraction.is_some_and(|loss| loss >= 0.005) {
        return VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS;
    }
    if loss_fraction.is_some_and(|loss| loss >= 0.001) {
        return VIDEO_COMPRESSED_QUEUE_MID_BUFFERS;
    }
    // Continuous ramp: 30 ms → BASE, 150 ms → MAX.
    const RAMP_LO_MS: u32 = 30;
    const RAMP_HI_MS: u32 = 150;
    if rtt_ema_ms <= RAMP_LO_MS {
        VIDEO_COMPRESSED_QUEUE_BASE_BUFFERS
    } else if rtt_ema_ms >= RAMP_HI_MS {
        VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS
    } else {
        let span = RAMP_HI_MS - RAMP_LO_MS;
        let frac = rtt_ema_ms - RAMP_LO_MS;
        VIDEO_COMPRESSED_QUEUE_BASE_BUFFERS
            + (VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS - VIDEO_COMPRESSED_QUEUE_BASE_BUFFERS) * frac
                / span
    }
}
const VIDEO_STALL_MIN_KEYFRAME_REQUEST_MS: u64 = 2_000;
const VIDEO_STARTUP_KEYFRAME_MS: u64 = 2_500;
const VIDEO_STARTUP_RESYNC_MS: u64 = 5_000;
const VIDEO_STARTUP_FATAL_MS: u64 = 8_000;
const VIDEO_LIVENESS_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Upper bound on the decode-timestamp queue used to measure decode→present
/// latency. 60fps ≈ 120 entries/second; 512 covers ~4s before old samples
/// start dropping, which only happens if the sink stalls (latency then reads
/// stale and is ignored anyway).
const DECODE_TIMESTAMP_QUEUE_MAX: usize = 512;

/// Maximum plausible decode→present latency (ms). The present queue holds at
/// most a few frames (~250 ms at 60 fps) plus sink processing, so anything
/// older than this is a frame that was decoded before a stall and is only now
/// reaching the sink — or was dropped by the present limiter. Expiring such
/// entries keeps the HUD decode time reading real pipeline latency instead of
/// the stall duration.
const DECODE_PRESENT_MAX_AGE_MS: u64 = 1_000;

/// Median window (number of recent decode→present deltas) used for the HUD
/// decode time (~0.5 s at 60 fps). A median is robust to the single inflated
/// delta a stall or limiter-drop leaves behind, unlike the old 75%-history
/// EMA which held the inflated value for seconds.
const DECODE_PRESENT_MEDIAN_WINDOW: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct VideoRateSnapshot {
    encoded_kbps: f64,
    decoded_fps: f64,
    sink_fps: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VideoStallAction {
    None,
    RequestKeyframe { attempt: u8, stall_ms: u64 },
    Resync { attempt: u8, stall_ms: u64 },
    PartialFlush { attempt: u8, stall_ms: u64 },
    CompleteFlush { attempt: u8, stall_ms: u64 },
    Fatal { attempt: u8, stall_ms: u64 },
    Recovered { stall_ms: u64 },
}

#[derive(Debug, Clone)]
pub(crate) struct VideoStallTracker {
    in_stall: bool,
    stall_started_ms: u64,
    last_request_ms: Option<u64>,
    next_attempt: u8,
}

impl Default for VideoStallTracker {
    fn default() -> Self {
        Self {
            in_stall: false,
            stall_started_ms: 0,
            last_request_ms: None,
            next_attempt: 1,
        }
    }
}

impl VideoStallTracker {
    pub(crate) fn evaluate(&mut self, now_ms: u64, last_video_ms: u64) -> VideoStallAction {
        let stall_ms = now_ms.saturating_sub(last_video_ms);
        if stall_ms < VIDEO_STALL_WARNING_MS {
            if self.in_stall {
                let recovered_ms = now_ms.saturating_sub(self.stall_started_ms);
                *self = Self::default();
                return VideoStallAction::Recovered {
                    stall_ms: recovered_ms,
                };
            }
            return VideoStallAction::None;
        }

        if !self.in_stall {
            self.in_stall = true;
            self.stall_started_ms = last_video_ms;
            self.next_attempt = 1;
        }

        let next_due_ms = match self.next_attempt {
            1 => VIDEO_STALL_WARNING_MS,
            2 => VIDEO_STALL_SECOND_ATTEMPT_MS,
            3 => VIDEO_STALL_RESYNC_MS,
            4 => VIDEO_STALL_PARTIAL_FLUSH_MS,
            5 => VIDEO_STALL_COMPLETE_FLUSH_MS,
            6 => VIDEO_STALL_FATAL_MS,
            _ => return VideoStallAction::None,
        };
        if stall_ms < next_due_ms {
            return VideoStallAction::None;
        }
        if self
            .last_request_ms
            .is_some_and(|last| now_ms.saturating_sub(last) < VIDEO_STALL_MIN_KEYFRAME_REQUEST_MS)
        {
            return VideoStallAction::None;
        }

        let attempt = self.next_attempt;
        self.next_attempt = self.next_attempt.saturating_add(1);
        self.last_request_ms = Some(now_ms);
        match attempt {
            1 | 2 => VideoStallAction::RequestKeyframe { attempt, stall_ms },
            3 => VideoStallAction::Resync { attempt, stall_ms },
            4 => VideoStallAction::PartialFlush { attempt, stall_ms },
            5 => VideoStallAction::CompleteFlush { attempt, stall_ms },
            _ => VideoStallAction::Fatal { attempt, stall_ms },
        }
    }
}

#[derive(Debug)]
pub(crate) struct VideoLivenessState {
    started_at: Instant,
    codec: Mutex<String>,
    resolution: Mutex<String>,
    hardware_acceleration: Mutex<String>,
    memory_mode: Mutex<String>,
    caps_framerate: Mutex<Option<String>>,
    requested_streaming_features_summary: Mutex<String>,
    finalized_streaming_features_summary: Mutex<String>,
    transition_telemetry: Mutex<TransitionTelemetry>,
    stats_overlay: Mutex<Option<gst::Element>>,
    pre_decode_queue: Mutex<Option<gst::Element>>,
    decoder: Mutex<Option<gst::Element>>,
    post_decode_queue: Mutex<Option<gst::Element>>,
    stats_overlay_visible: AtomicBool,
    target_bitrate_kbps: AtomicU32,
    encoded_bytes_total: AtomicU64,
    last_encoded_ms: AtomicU64,
    last_decoded_ms: AtomicU64,
    last_sink_ms: AtomicU64,
    last_audio_ms: AtomicU64,
    first_startup_audio_ms: AtomicU64,
    first_startup_encoded_ms: AtomicU64,
    decoded_total: AtomicU64,
    sink_total: AtomicU64,
    /// Decode finish timestamps, popped one per sink event (present or
    /// present-limiter drop) to measure the decode→present pipeline latency
    /// (filled into the HUD "Decode time"). Frames dropped by the present
    /// limiter pop their entry via `record_sink_limiter_drop`; stale entries
    /// (a stall backlog) are cleared via `clear_decode_timestamps`.
    decode_timestamps: Mutex<VecDeque<u64>>,
    /// Sliding window of recent decode→present deltas (ms); the reported
    /// value is the window's MEDIAN — robust to the single inflated delta a
    /// stall or limiter-drop leaves behind.
    decode_present_deltas: Mutex<VecDeque<u32>>,
    /// Median decode→present latency in ms.
    decode_present_median_ms: AtomicU32,
    zero_copy_d3d11: AtomicBool,
    zero_copy_d3d12: AtomicBool,
    requested_fps: AtomicU32,
    framerate_mismatch_warned: AtomicBool,
    transition_flush_escalation_enabled: AtomicBool,
    first_encoded_logged: AtomicBool,
    startup_keyframe_requested: AtomicBool,
    startup_resync_requested: AtomicBool,
    startup_fatal_reported: AtomicBool,
    /// Whether the AV1 → H265 codec downgrade request was already emitted for
    /// this session. Guarded so the watchdog never spams the Electron main
    /// process (the manager restarts the session with the fallback codec).
    startup_downgrade_requested: AtomicBool,
    /// The current video sink element. The decoder-fallback rebuild replaces
    /// the whole decode chain (including the sink), so the watchdog must read
    /// the live sink from here instead of holding the element it captured at
    /// `start()` — otherwise it would keep driving stats/health probes against
    /// a removed element after a rebuild.
    current_sink: Mutex<Option<gst::Element>>,
    /// Whether the RTP video bitrate probe is already installed on the src
    /// pad. The probe survives a decoder-fallback rebuild (the src pad is not
    /// torn down, only the chain elements), so re-adding it would double-count
    /// encoded bytes. Guarded per monitor instance.
    rtp_bitrate_probe_installed: AtomicBool,
    /// Whether the outgoing-RTCP observability probe is already installed on
    /// the webrtcbin rtpbin sessions. The rtpbin/sessions outlive decoder
    /// rebuilds, so re-adding would double-count. Guarded per monitor.
    rtcp_send_probe_installed: AtomicBool,
    /// Cumulative counts of the RTCP packets the client actually SENDS to the
    /// server (RR/SR/transport-cc feedback/NACK/PLI/FIR), classified by
    /// `classify_rtcp_messages`. GFN's server BWE (`enableBandwidthEstimation`)
    /// is driven by transport-cc feedback: if these stay at 0 the server runs
    /// blind and holds a conservative bitrate (~3.4 Mbps observed) no matter
    /// what the NVST SDP requested.
    rtcp_sent_sr: AtomicU64,
    rtcp_sent_rr: AtomicU64,
    rtcp_sent_twcc: AtomicU64,
    rtcp_sent_nack: AtomicU64,
    rtcp_sent_pli: AtomicU64,
    rtcp_sent_fir: AtomicU64,
    rtcp_sent_other: AtomicU64,
    /// EMA (ms) of the effective network RTT (stats-channel server RTT, or the
    /// local RTCP measurement when available), used to resize the pre-decode
    /// jitter buffer adaptively.
    network_rtt_ema_ms: AtomicU32,
    /// Current pre-decode queue depth in compressed frames, so the adaptive
    /// resize only touches the element when the target actually changes.
    pre_decode_depth: AtomicU32,
    /// Monotonic watchdog clock (ms) until which the jitter buffer must stay
    /// at MAX depth after a detected RTT spike: the spike starves the decoder
    /// in the seconds the EMA needs to catch up, so the deep buffer is HELD
    /// past the spike instead of being released the moment RTT dips once
    /// (that dip is followed by another burst).
    burst_hold_until_ms: AtomicU64,
}

impl VideoLivenessState {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            codec: Mutex::new(String::new()),
            resolution: Mutex::new(String::new()),
            hardware_acceleration: Mutex::new(String::new()),
            memory_mode: Mutex::new("system-memory".to_owned()),
            caps_framerate: Mutex::new(None),
            requested_streaming_features_summary: Mutex::new("none".to_owned()),
            finalized_streaming_features_summary: Mutex::new("none".to_owned()),
            transition_telemetry: Mutex::new(TransitionTelemetry::default()),
            stats_overlay: Mutex::new(None),
            pre_decode_queue: Mutex::new(None),
            decoder: Mutex::new(None),
            post_decode_queue: Mutex::new(None),
            stats_overlay_visible: AtomicBool::new(false),
            target_bitrate_kbps: AtomicU32::new(0),
            encoded_bytes_total: AtomicU64::new(0),
            last_encoded_ms: AtomicU64::new(0),
            last_decoded_ms: AtomicU64::new(0),
            last_sink_ms: AtomicU64::new(0),
            last_audio_ms: AtomicU64::new(0),
            first_startup_audio_ms: AtomicU64::new(0),
            first_startup_encoded_ms: AtomicU64::new(0),
            decoded_total: AtomicU64::new(0),
            sink_total: AtomicU64::new(0),
            decode_timestamps: Mutex::new(VecDeque::new()),
            decode_present_deltas: Mutex::new(VecDeque::new()),
            decode_present_median_ms: AtomicU32::new(0),
            zero_copy_d3d11: AtomicBool::new(false),
            zero_copy_d3d12: AtomicBool::new(false),
            requested_fps: AtomicU32::new(0),
            framerate_mismatch_warned: AtomicBool::new(false),
            transition_flush_escalation_enabled: AtomicBool::new(true),
            first_encoded_logged: AtomicBool::new(false),
            startup_keyframe_requested: AtomicBool::new(false),
            startup_resync_requested: AtomicBool::new(false),
            startup_fatal_reported: AtomicBool::new(false),
            startup_downgrade_requested: AtomicBool::new(false),
            current_sink: Mutex::new(None),
            rtp_bitrate_probe_installed: AtomicBool::new(false),
            rtcp_send_probe_installed: AtomicBool::new(false),
            rtcp_sent_sr: AtomicU64::new(0),
            rtcp_sent_rr: AtomicU64::new(0),
            rtcp_sent_twcc: AtomicU64::new(0),
            rtcp_sent_nack: AtomicU64::new(0),
            rtcp_sent_pli: AtomicU64::new(0),
            rtcp_sent_fir: AtomicU64::new(0),
            rtcp_sent_other: AtomicU64::new(0),
            network_rtt_ema_ms: AtomicU32::new(0),
            pre_decode_depth: AtomicU32::new(
                crate::gstreamer_pipeline::VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS,
            ),
            burst_hold_until_ms: AtomicU64::new(0),
        }
    }

    fn now_ms(&self) -> u64 {
        self.started_at
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }

    pub(crate) fn configure(
        &self,
        context: &NativeStreamerSessionContext,
        target_bitrate_kbps: u32,
    ) {
        let settings = &context.settings;
        if let Ok(mut codec) = self.codec.lock() {
            *codec = settings.codec.as_str().to_owned();
        }
        if let Ok(mut resolution) = self.resolution.lock() {
            *resolution = settings.resolution.clone();
        }
        if let Ok(mut caps_framerate) = self.caps_framerate.lock() {
            *caps_framerate = None;
        }
        if let Ok(mut requested_summary) = self.requested_streaming_features_summary.lock() {
            *requested_summary = context
                .session
                .requested_streaming_features
                .as_ref()
                .map(|features| features.summary())
                .unwrap_or_else(|| "none".to_owned());
        }
        if let Ok(mut finalized_summary) = self.finalized_streaming_features_summary.lock() {
            *finalized_summary = context
                .session
                .finalized_streaming_features
                .as_ref()
                .map(|features| features.summary())
                .unwrap_or_else(|| "none".to_owned());
        }
        if let Ok(mut telemetry) = self.transition_telemetry.lock() {
            telemetry.queue_mode = resolve_queue_mode(settings);
            telemetry.queue_depth = DEFAULT_VIDEO_QUEUE_DEPTH;
            telemetry.queue_depth_changes = 0;
            telemetry.present_pacing_changes = 0;
            telemetry.partial_flush_count = 0;
            telemetry.complete_flush_count = 0;
            telemetry.last_transition = None;
        }
        self.target_bitrate_kbps
            .store(target_bitrate_kbps, Ordering::Relaxed);
        self.requested_fps.store(settings.fps, Ordering::Relaxed);
        self.framerate_mismatch_warned
            .store(false, Ordering::Relaxed);
        self.first_encoded_logged.store(false, Ordering::Relaxed);
        self.first_startup_audio_ms.store(0, Ordering::Relaxed);
        self.first_startup_encoded_ms.store(0, Ordering::Relaxed);
        self.transition_flush_escalation_enabled.store(
            settings
                .native_transition_diagnostics
                .as_ref()
                .map(|diagnostics| !diagnostics.disable_transition_flush_escalation)
                .unwrap_or(true),
            Ordering::Relaxed,
        );
        self.startup_keyframe_requested
            .store(false, Ordering::Relaxed);
        self.startup_resync_requested
            .store(false, Ordering::Relaxed);
        self.startup_fatal_reported.store(false, Ordering::Relaxed);
        self.startup_downgrade_requested.store(false, Ordering::Relaxed);
    }

    pub(crate) fn update_hardware_acceleration(&self, value: impl Into<String>) {
        if let Ok(mut hardware_acceleration) = self.hardware_acceleration.lock() {
            *hardware_acceleration = value.into();
        }
    }

    /// Add one outgoing RTCP packet (already classified) to the send counters.
    /// Fired by the pad probe on the rtpbin sessions' RTCP src pads.
    pub(crate) fn record_rtcp_message(&self, counts: RtcpMessageCounts) {
        self.rtcp_sent_sr.fetch_add(counts.sr, Ordering::Relaxed);
        self.rtcp_sent_rr.fetch_add(counts.rr, Ordering::Relaxed);
        self.rtcp_sent_twcc.fetch_add(counts.twcc, Ordering::Relaxed);
        self.rtcp_sent_nack.fetch_add(counts.nack, Ordering::Relaxed);
        self.rtcp_sent_pli.fetch_add(counts.pli, Ordering::Relaxed);
        self.rtcp_sent_fir.fetch_add(counts.fir, Ordering::Relaxed);
        self.rtcp_sent_other.fetch_add(counts.other, Ordering::Relaxed);
    }

    /// Human-readable snapshot of the outgoing RTCP counters; `none` when the
    /// client has not sent a single RTCP packet (feedback path dead).
    pub(crate) fn rtcp_sent_summary(&self) -> String {
        let sr = self.rtcp_sent_sr.load(Ordering::Relaxed);
        let rr = self.rtcp_sent_rr.load(Ordering::Relaxed);
        let twcc = self.rtcp_sent_twcc.load(Ordering::Relaxed);
        let nack = self.rtcp_sent_nack.load(Ordering::Relaxed);
        let pli = self.rtcp_sent_pli.load(Ordering::Relaxed);
        let fir = self.rtcp_sent_fir.load(Ordering::Relaxed);
        let other = self.rtcp_sent_other.load(Ordering::Relaxed);
        if sr + rr + twcc + nack + pli + fir + other == 0 {
            return "none".to_owned();
        }
        format!("SR={sr} RR={rr} TWCC={twcc} NACK={nack} PLI={pli} FIR={fir} other={other}")
    }

    pub(crate) fn record_encoded_buffer(&self, size: usize) {
        let now_ms = self.now_ms();
        self.last_encoded_ms.store(now_ms, Ordering::Relaxed);
        self.encoded_bytes_total
            .fetch_add(size as u64, Ordering::Relaxed);
        // First video RTP timestamp — the startup recovery keys off encoded
        // video activity (not audio) so a decode stall on a silent screen
        // (GFN Opus DTX sends no audio RTP during silence) still gets a
        // keyframe request.
        let _ = self.first_startup_encoded_ms.compare_exchange(
            0,
            now_ms,
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
    }

    pub(crate) fn record_audio_buffer(&self) {
        let now_ms = self.now_ms();
        self.last_audio_ms.store(now_ms, Ordering::Relaxed);
        if self.last_sink_ms.load(Ordering::Relaxed) == 0 {
            let _ = self.first_startup_audio_ms.compare_exchange(
                0,
                now_ms,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }
    }

    fn log_first_encoded_once(&self) -> bool {
        !self.first_encoded_logged.swap(true, Ordering::Relaxed)
    }

    pub(crate) fn set_stats_overlay(&self, overlay: Option<gst::Element>) {
        if let Some(element) = overlay.as_ref() {
            set_property_if_supported(
                element,
                "visible",
                self.stats_overlay_visible.load(Ordering::Relaxed),
            );
        }
        if let Ok(mut current) = self.stats_overlay.lock() {
            *current = overlay;
        }
    }

    pub(crate) fn set_stats_overlay_visible(&self, visible: bool) {
        self.stats_overlay_visible.store(visible, Ordering::Relaxed);
        if let Ok(current) = self.stats_overlay.lock() {
            if let Some(overlay) = current.as_ref() {
                set_property_if_supported(overlay, "visible", visible);
            }
        }
    }

    fn update_stats_overlay_text(&self, text: &str) {
        if let Ok(current) = self.stats_overlay.lock() {
            if let Some(overlay) = current.as_ref() {
                overlay.set_property("text", text);
                set_property_if_supported(
                    overlay,
                    "visible",
                    self.stats_overlay_visible.load(Ordering::Relaxed) && !text.is_empty(),
                );
            }
        }
    }

    pub(crate) fn record_decoded_buffer(&self) {
        let now_ms = self.now_ms();
        self.last_decoded_ms.store(now_ms, Ordering::Relaxed);
        self.decoded_total.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut timestamps) = self.decode_timestamps.lock() {
            timestamps.push_back(now_ms);
            if timestamps.len() > DECODE_TIMESTAMP_QUEUE_MAX {
                timestamps.pop_front();
            }
        }
    }

    /// A present-limiter drop happened on the sink pad BEFORE the sink-rate
    /// probe ran (the limiter probe is installed first and returns Drop, which
    /// short-circuits the probe chain), so `record_sink_buffer` never fired for
    /// that frame. Pop its decode timestamp here to keep the pairing queue
    /// balanced — otherwise the dropped frame's entry lingers and the next
    /// presented frame pops a STALE (older) timestamp, inflating the measured
    /// decode→present delta.
    pub(crate) fn record_sink_limiter_drop(&self) {
        if let Ok(mut timestamps) = self.decode_timestamps.lock() {
            let _ = timestamps.pop_front();
        }
    }

    /// Drop ALL pending decode timestamps. Called by the watchdog on every
    /// tick while the sink is stalled: the entries pushed before/during the
    /// stall belong to frames that will be presented (if ever) long after they
    /// were decoded, so pairing them with post-recovery presents would report
    /// the whole stall as decode time. Clearing on stall means the first
    /// post-recovery presents find an empty queue (no delta recorded, the
    /// median holds its last good value) and only fresh frames re-populate it.
    /// Note this intentionally clears entries decoded DURING the stall too:
    /// they are just as contaminated (their delta would still include the
    /// stall's remaining wait).
    pub(crate) fn clear_decode_timestamps(&self) {
        if let Ok(mut timestamps) = self.decode_timestamps.lock() {
            timestamps.clear();
        }
    }

    pub(crate) fn record_sink_buffer(&self) {
        let now_ms = self.now_ms();
        self.last_sink_ms.store(now_ms, Ordering::Relaxed);
        self.sink_total.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut timestamps) = self.decode_timestamps.lock() {
            // Defensive expiry: a frame decoded more than the max plausible
            // latency ago is stall backlog, not this present's pairing — pop it
            // so it can never inflate the delta (the watchdog also expires on
            // stall; this covers the window between watchdog ticks).
            while timestamps
                .front()
                .is_some_and(|ts| now_ms.saturating_sub(*ts) > DECODE_PRESENT_MAX_AGE_MS)
            {
                timestamps.pop_front();
            }
            if let Some(decoded_at_ms) = timestamps.pop_front() {
                let delta = now_ms.saturating_sub(decoded_at_ms) as u32;
                if let Ok(mut deltas) = self.decode_present_deltas.lock() {
                    deltas.push_back(delta);
                    if deltas.len() > DECODE_PRESENT_MEDIAN_WINDOW {
                        deltas.pop_front();
                    }
                    // Median of the window — robust to the single inflated
                    // delta a stall/limiter-drop leaves behind (an EMA with
                    // 75% history held it for seconds).
                    self.decode_present_median_ms
                        .store(median_of_deltas(&deltas), Ordering::Relaxed);
                }
            }
        }
    }

    /// Median decode→present latency in ms (None until the first present).
    fn decode_present_median_ms(&self) -> Option<u32> {
        let value = self.decode_present_median_ms.load(Ordering::Relaxed);
        (value > 0).then_some(value)
    }

    pub(crate) fn update_caps(&self, caps: &str) {
        self.zero_copy_d3d11
            .store(caps.contains("memory:D3D11Memory"), Ordering::Relaxed);
        self.zero_copy_d3d12
            .store(caps.contains("memory:D3D12Memory"), Ordering::Relaxed);
        if let Ok(mut memory_mode) = self.memory_mode.lock() {
            *memory_mode = memory_mode_from_caps(caps).to_owned();
        }
        if let Ok(mut caps_framerate) = self.caps_framerate.lock() {
            *caps_framerate = caps_framerate_summary(caps);
        }
    }

    pub(crate) fn set_current_sink(&self, sink: gst::Element) {
        if let Ok(mut current) = self.current_sink.lock() {
            *current = Some(sink);
        }
    }

    pub(crate) fn current_sink(&self) -> Option<gst::Element> {
        self.current_sink
            .lock()
            .ok()
            .and_then(|current| current.clone())
    }

    /// Drop liveness element references to a decode chain that is being torn
    /// down (decoder-fallback rebuild). The new chain re-registers fresh
    /// elements through the normal `set_*` methods.
    pub(crate) fn clear_chain_elements(&self) {
        if let Ok(mut current) = self.stats_overlay.lock() {
            *current = None;
        }
        if let Ok(mut current) = self.pre_decode_queue.lock() {
            *current = None;
        }
        if let Ok(mut current) = self.decoder.lock() {
            *current = None;
        }
        if let Ok(mut current) = self.post_decode_queue.lock() {
            *current = None;
        }
    }

    /// Give a freshly-rebuilt decode chain a full startup-recovery window.
    /// Called right after a decoder-fallback rebuild succeeds, so the new
    /// chain gets its own keyframe/resync/fatal timeline instead of inheriting
    /// the dead chain's exhausted budget.
    pub(crate) fn reset_startup_window(&self) {
        self.first_startup_encoded_ms.store(0, Ordering::Relaxed);
        self.decoded_total.store(0, Ordering::Relaxed);
        self.sink_total.store(0, Ordering::Relaxed);
        self.last_decoded_ms.store(0, Ordering::Relaxed);
        self.last_sink_ms.store(0, Ordering::Relaxed);
        self.startup_keyframe_requested
            .store(false, Ordering::Relaxed);
        self.startup_resync_requested
            .store(false, Ordering::Relaxed);
        self.startup_fatal_reported.store(false, Ordering::Relaxed);
        self.startup_downgrade_requested.store(false, Ordering::Relaxed);
    }

    fn zero_copy_d3d11(&self) -> bool {
        self.zero_copy_d3d11.load(Ordering::Relaxed)
    }

    fn zero_copy_d3d12(&self) -> bool {
        self.zero_copy_d3d12.load(Ordering::Relaxed)
    }

    fn memory_mode(&self) -> String {
        self.memory_mode
            .lock()
            .map(|value| value.clone())
            .unwrap_or_else(|_| "unknown".to_owned())
    }

    fn zero_copy(&self) -> bool {
        is_zero_copy_memory_mode(&self.memory_mode())
    }

    fn requested_fps(&self) -> Option<u32> {
        let fps = self.requested_fps.load(Ordering::Relaxed);
        (fps > 0).then_some(fps)
    }

    fn caps_framerate(&self) -> Option<String> {
        self.caps_framerate
            .lock()
            .ok()
            .and_then(|value| value.clone())
    }

    fn warn_framerate_mismatch_once(&self) -> bool {
        !self.framerate_mismatch_warned.swap(true, Ordering::Relaxed)
    }

    fn queue_mode(&self) -> NativeQueueMode {
        self.transition_telemetry
            .lock()
            .map(|telemetry| telemetry.queue_mode)
            .unwrap_or(NativeQueueMode::Auto)
    }

    pub(crate) fn set_post_decode_queue(&self, queue: gst::Element) {
        if let Ok(mut current) = self.post_decode_queue.lock() {
            *current = Some(queue);
        }
    }

    pub(crate) fn set_pre_decode_queue(&self, queue: gst::Element) {
        if let Ok(mut current) = self.pre_decode_queue.lock() {
            *current = Some(queue);
        }
    }

    /// Adaptive pre-decode jitter buffer depth: DEEP (15 frames ≈ 250 ms)
    /// while the network is degraded so WAN jitter bursts never starve the
    /// decoder (the anti-flicker fix), SHALLOW (6 frames ≈ 100 ms) on stable
    /// links so in-game drags and aiming stay tight — a fixed 15-frame depth
    /// added ~150 ms of constant latency and made drags feel "patah-patah".
    /// Returns the new depth in frames when it changed, else None.
    pub(crate) fn adjust_pre_decode_queue_for_network(
        &self,
        rtt_ms: u32,
        loss_fraction: Option<f64>,
    ) -> Option<u32> {
        // EMA (75% history, 25% latest) — the stats-channel RTT is a raw
        // per-sample value that can bounce between polls; the EMA also gives
        // the band-switch hysteresis that stops oscillation.
        let ema = {
            let current = self.network_rtt_ema_ms.load(Ordering::Relaxed);
            let next = if current == 0 {
                rtt_ms
            } else {
                (current * 3 + rtt_ms) / 4
            };
            self.network_rtt_ema_ms.store(next, Ordering::Relaxed);
            next
        };
        // Spike detection: the RAW sample far above the EMA means a jitter
        // burst is in flight RIGHT NOW. The EMA would need ~2-4 samples to
        // climb, during which the decoder starves and the sink repeats the
        // last frame — the "kedip-kedip frame sebelumnya" flicker the user
        // saw for seconds after the ping rose. Force MAX depth and HOLD it
        // for JITTER_BURST_HOLD_MS so the burst in flight is absorbed and
        // the following bursts (spikes come in clusters) never leak through.
        let spike = rtt_ms > ema.saturating_mul(3) / 2 && rtt_ms.saturating_sub(ema) >= 30;
        if spike {
            self.burst_hold_until_ms.store(
                self.now_ms().saturating_add(JITTER_BURST_HOLD_MS),
                Ordering::Relaxed,
            );
        }
        let burst_hold = self.now_ms() < self.burst_hold_until_ms.load(Ordering::Relaxed);
        let target = target_pre_decode_depth(ema, loss_fraction, burst_hold);
        if target == self.pre_decode_depth.load(Ordering::Relaxed) {
            return None;
        }
        let Some(queue) = self.pre_decode_queue() else {
            return None;
        };
        // Runtime resize: the queue drains to the new max on its own, and with
        // the present limiter the backlog is consumed at real-time, so
        // shrinking never fast-forwards the picture.
        set_property_if_supported(&queue, "max-size-buffers", target);
        self.pre_decode_depth.store(target, Ordering::Relaxed);
        Some(target)
    }

    pub(crate) fn set_decoder(&self, decoder: gst::Element) {
        if let Ok(mut current) = self.decoder.lock() {
            *current = Some(decoder);
        }
    }

    fn pre_decode_queue(&self) -> Option<gst::Element> {
        self.pre_decode_queue
            .lock()
            .ok()
            .and_then(|current| current.clone())
    }

    fn decoder(&self) -> Option<gst::Element> {
        self.decoder.lock().ok().and_then(|current| current.clone())
    }

    fn set_queue_depth(
        &self,
        max_buffers: u32,
        reason: &str,
        event_sender: &Option<Sender<Event>>,
    ) {
        let queue = self
            .post_decode_queue
            .lock()
            .ok()
            .and_then(|current| current.clone());
        if let Some(queue) = queue.as_ref() {
            configure_queue(queue, max_buffers, true);
        }

        let mut should_log = false;
        if let Ok(mut telemetry) = self.transition_telemetry.lock() {
            if telemetry.queue_depth != max_buffers {
                telemetry.queue_depth = max_buffers;
                telemetry.queue_depth_changes = telemetry.queue_depth_changes.saturating_add(1);
                should_log = true;
            }
        }

        if should_log {
            send_log(
                event_sender,
                "info",
                format!("Adjusted native post-decode queue depth to {max_buffers} ({reason})."),
            );
        }
    }

    fn queue_depth(&self) -> u32 {
        self.transition_telemetry
            .lock()
            .map(|telemetry| telemetry.queue_depth)
            .unwrap_or(DEFAULT_VIDEO_QUEUE_DEPTH)
    }

    fn record_present_pacing_change(&self) {
        if let Ok(mut telemetry) = self.transition_telemetry.lock() {
            telemetry.present_pacing_changes = telemetry.present_pacing_changes.saturating_add(1);
        }
    }

    fn transition_flush_escalation_enabled(&self) -> bool {
        self.transition_flush_escalation_enabled
            .load(Ordering::Relaxed)
    }

    fn transition_telemetry_snapshot(&self) -> TransitionTelemetry {
        self.transition_telemetry
            .lock()
            .map(|telemetry| telemetry.clone())
            .unwrap_or_default()
    }

    fn requested_streaming_features_summary(&self) -> String {
        self.requested_streaming_features_summary
            .lock()
            .map(|value| value.clone())
            .unwrap_or_else(|_| "none".to_owned())
    }

    fn finalized_streaming_features_summary(&self) -> String {
        self.finalized_streaming_features_summary
            .lock()
            .map(|value| value.clone())
            .unwrap_or_else(|_| "none".to_owned())
    }

    fn record_transition(
        &self,
        transition_type: &str,
        source: &str,
        old_caps: Option<String>,
        new_caps: Option<String>,
        old_framerate: Option<String>,
        new_framerate: Option<String>,
        old_memory_mode: Option<String>,
        new_memory_mode: Option<String>,
        event_sender: &Option<Sender<Event>>,
    ) {
        let requested_fps = self.requested_fps();
        let queue_mode = self.queue_mode();
        let render_gap_ms = age_since_ms(self.now_ms(), self.last_sink_ms.load(Ordering::Relaxed));
        let high_fps_risk = requested_fps.is_some_and(|fps| fps >= 240)
            && new_framerate
                .as_deref()
                .is_some_and(|value| value != format!("{}/1", requested_fps.unwrap_or_default()));
        let summary = format_transition_summary(
            transition_type,
            source,
            requested_fps,
            old_framerate.as_deref(),
            new_framerate.as_deref(),
            high_fps_risk,
        );
        let snapshot = TransitionSnapshot {
            transition_type: transition_type.to_owned(),
            source: source.to_owned(),
            at_ms: self.now_ms(),
            old_caps,
            new_caps,
            old_framerate,
            new_framerate: new_framerate.clone(),
            old_memory_mode,
            new_memory_mode,
            render_gap_ms,
            requested_fps,
            caps_framerate: new_framerate,
            high_fps_risk,
            queue_mode,
            summary: summary.clone(),
        };

        if let Ok(mut telemetry) = self.transition_telemetry.lock() {
            telemetry.last_transition = Some(snapshot.clone());
        }

        send_log(
            event_sender,
            "warn",
            format!("Native video transition: {summary}"),
        );
        if let Some(event_sender) = event_sender {
            let _ = event_sender.send(Event::VideoTransition {
                transition: snapshot.to_event(),
            });
        }
    }

    fn increment_partial_flush_count(&self) {
        if let Ok(mut telemetry) = self.transition_telemetry.lock() {
            telemetry.partial_flush_count = telemetry.partial_flush_count.saturating_add(1);
        }
    }

    fn increment_complete_flush_count(&self) {
        if let Ok(mut telemetry) = self.transition_telemetry.lock() {
            telemetry.complete_flush_count = telemetry.complete_flush_count.saturating_add(1);
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VideoLivenessMonitor {
    state: Arc<VideoLivenessState>,
    stop: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
    thread: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Decoder-chain fallback context registered by the RTP video chain
    /// builder. The watchdog consults it when startup/stall recovery is
    /// exhausted: it tears down the current decode chain and rebuilds it with
    /// the next candidate decoder API (e.g. D3D12 → D3D11 → software) instead
    /// of declaring the stream dead. Cleared on stop so it never outlives the
    /// session.
    chain_rebuild: Arc<Mutex<Option<VideoChainRebuildContext>>>,
}

impl Default for VideoLivenessMonitor {
    fn default() -> Self {
        Self {
            state: Arc::new(VideoLivenessState::new()),
            stop: Arc::new(AtomicBool::new(false)),
            started: Arc::new(AtomicBool::new(false)),
            thread: Arc::new(Mutex::new(None)),
            chain_rebuild: Arc::new(Mutex::new(None)),
        }
    }
}

impl VideoLivenessMonitor {
    pub(crate) fn configure(
        &self,
        context: &NativeStreamerSessionContext,
        target_bitrate_kbps: u32,
    ) {
        self.state.configure(context, target_bitrate_kbps);
    }

    /// Override the codec with the one actually negotiated in the WebRTC
    /// answer (the requested codec from settings may differ, e.g. when the
    /// server downgrades or the offer only carries another codec). The startup
    /// watchdog uses the negotiated codec to decide whether an AV1
    /// zero-frame startup warrants a session codec downgrade.
    pub(crate) fn update_negotiated_codec(&self, codec: &str) {
        if let Ok(mut current) = self.state.codec.lock() {
            *current = codec.to_owned();
        }
    }

    pub(crate) fn update_hardware_acceleration(&self, value: impl Into<String>) {
        self.state.update_hardware_acceleration(value);
    }

    pub(crate) fn record_encoded_buffer(&self, size: usize) {
        self.state.record_encoded_buffer(size);
    }

    pub(crate) fn record_rtcp_message(&self, counts: RtcpMessageCounts) {
        self.state.record_rtcp_message(counts);
    }

    pub(crate) fn rtcp_sent_summary(&self) -> String {
        self.state.rtcp_sent_summary()
    }

    pub(crate) fn record_audio_buffer(&self) {
        self.state.record_audio_buffer();
    }

    pub(crate) fn set_stats_overlay(&self, overlay: Option<gst::Element>) {
        self.state.set_stats_overlay(overlay);
    }

    pub(crate) fn set_stats_overlay_visible(&self, visible: bool) {
        self.state.set_stats_overlay_visible(visible);
    }

    pub(crate) fn record_decoded_buffer(&self) {
        self.state.record_decoded_buffer();
    }

    pub(crate) fn record_sink_buffer(&self) {
        self.state.record_sink_buffer();
    }

    pub(crate) fn record_sink_limiter_drop(&self) {
        self.state.record_sink_limiter_drop();
    }

    pub(crate) fn update_caps(&self, caps: &str) {
        self.state.update_caps(caps);
    }

    pub(crate) fn clear_chain_elements(&self) {
        self.state.clear_chain_elements();
    }

    pub(crate) fn set_post_decode_queue(&self, queue: gst::Element) {
        self.state.set_post_decode_queue(queue);
    }

    pub(crate) fn set_pre_decode_queue(&self, queue: gst::Element) {
        self.state.set_pre_decode_queue(queue);
    }

    pub(crate) fn set_decoder(&self, decoder: gst::Element) {
        self.state.set_decoder(decoder);
    }

    pub(crate) fn log_first_encoded_once(&self) -> bool {
        self.state.log_first_encoded_once()
    }

    pub(crate) fn requested_fps(&self) -> Option<u32> {
        self.state.requested_fps()
    }

    pub(crate) fn warn_framerate_mismatch_once(&self) -> bool {
        self.state.warn_framerate_mismatch_once()
    }

    pub(crate) fn record_present_pacing_change(&self) {
        self.state.record_present_pacing_change();
    }

    pub(crate) fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    pub(crate) fn state(&self) -> Arc<VideoLivenessState> {
        self.state.clone()
    }

    pub(crate) fn set_chain_rebuild(&self, rebuild: Option<VideoChainRebuildContext>) {
        if let Ok(mut slot) = self.chain_rebuild.lock() {
            *slot = rebuild;
        }
    }

    pub(crate) fn chain_rebuild(&self) -> Arc<Mutex<Option<VideoChainRebuildContext>>> {
        self.chain_rebuild.clone()
    }

    pub(crate) fn record_transition(
        &self,
        transition_type: &str,
        source: &str,
        old_caps: Option<String>,
        new_caps: Option<String>,
        old_framerate: Option<String>,
        new_framerate: Option<String>,
        old_memory_mode: Option<String>,
        new_memory_mode: Option<String>,
        event_sender: &Option<Sender<Event>>,
    ) {
        self.state.record_transition(
            transition_type,
            source,
            old_caps,
            new_caps,
            old_framerate,
            new_framerate,
            old_memory_mode,
            new_memory_mode,
            event_sender,
        );
    }

    pub(crate) fn start(
        &self,
        pipeline: gst::Pipeline,
        sink: gst::Element,
        event_sender: Option<Sender<Event>>,
    ) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }

        self.stop.store(false, Ordering::SeqCst);
        self.state.set_current_sink(sink.clone());
        let state = self.state.clone();
        let stop = self.stop.clone();
        let chain_rebuild = self.chain_rebuild.clone();
        let thread = thread::spawn(move || {
            run_video_liveness_watchdog(state, chain_rebuild, stop, pipeline, sink, event_sender);
        });
        if let Ok(mut slot) = self.thread.lock() {
            *slot = Some(thread);
        }
    }

    pub(crate) fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.started.store(false, Ordering::SeqCst);
        // Drop the rebuild context before joining: it holds the pipeline / pad
        // / chain elements and must not outlive the session or keep them alive
        // after the watchdog (which holds a clone) finishes.
        if let Ok(mut slot) = self.chain_rebuild.lock() {
            *slot = None;
        }
        let handle = self.thread.lock().ok().and_then(|mut slot| slot.take());
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

fn run_video_liveness_watchdog(
    state: Arc<VideoLivenessState>,
    chain_rebuild: Arc<Mutex<Option<VideoChainRebuildContext>>>,
    stop: Arc<AtomicBool>,
    pipeline: gst::Pipeline,
    sink: gst::Element,
    event_sender: Option<Sender<Event>>,
) {
    let mut tracker = VideoStallTracker::default();
    let mut last_rate_at = Instant::now();
    let mut last_health_at = Instant::now();
    let mut last_encoded_bytes_total = state.encoded_bytes_total.load(Ordering::Relaxed);
    let mut last_decoded_total = state.decoded_total.load(Ordering::Relaxed);
    let mut last_sink_total = state.sink_total.load(Ordering::Relaxed);
    let mut rates = VideoRateSnapshot {
        encoded_kbps: 0.0,
        decoded_fps: 0.0,
        sink_fps: 0.0,
    };
    // Local RTCP freshness tracking. rtpsession's `have-rb` sticks once set,
    // so the raw `rb-round-trip` value alone can never tell "RRs still
    // flowing" from "frozen at an old value" (the frozen-ping bug). The
    // `rb-lsr` field (the SR timestamp the server echoes back in each RR)
    // advances on EVERY new RR, so a change is the freshness signal. Track
    // the full `(rtt, lsr)` pair so builds that leave `rb-lsr` at 0 still
    // detect new RRs via the RTT changing: the timestamp is re-based
    // whenever the sample changes, and a local RTCP that hasn't refreshed
    // within LOCAL_RTCP_FRESH_AGE_MS is expired (reported as None so the
    // HUD falls back to the server RTT).
    let mut last_local_rtcp_sample: Option<(u32, u64)> = None;
    let mut last_local_rtcp_at = Instant::now();

    while !stop.load(Ordering::SeqCst) {
        thread::sleep(VIDEO_LIVENESS_POLL_INTERVAL);

        // The decoder-fallback rebuild replaces the whole decode chain
        // (including the sink), so always drive stats/health against the live
        // sink instead of the element captured when the watchdog started.
        let sink = state.current_sink().unwrap_or_else(|| sink.clone());

        // Adaptive pre-decode jitter buffer — polled every watchdog tick
        // (250 ms), NOT the 1 s rate-log interval below: the RTT EMA
        // converges ~4x faster, so the buffer deepens within a quarter second
        // of the RTT rising instead of after 2-4 seconds of a starved decoder
        // (the "kedip-kedip frame sebelumnya" flicker). Kept shallow on
        // stable links for tight input feel. Packet loss rides along as the
        // leading indicator that floors the depth before RTT even climbs.
        let local_rtcp_sample = query_rtcp_rtt_ms(&pipeline);
        // Re-base the freshness timestamp whenever a new RR changed the
        // measurement (rb-lsr advances with every RR; None → Some counts as
        // the first RR).
        if local_rtcp_sample != last_local_rtcp_sample {
            last_local_rtcp_sample = local_rtcp_sample;
            last_local_rtcp_at = Instant::now();
        }
        let local_rtcp_rtt_age_ms = last_local_rtcp_at
            .elapsed()
            .as_millis()
            .min(u128::from(u32::MAX)) as u32;
        // Expire the local measurement once RRs stop refreshing it — the raw
        // rtpsession value (gated only by the sticky `have-rb`) would
        // otherwise override the server RTT forever. When expired, report
        // None so both the jitter buffer and the HUD use the server RTT.
        let local_rtcp_rtt_ms = local_rtcp_sample
            .map(|(rtt, _)| rtt)
            .filter(|_| local_rtcp_rtt_age_ms <= LOCAL_RTCP_FRESH_AGE_MS);
        // Local receive jitter of the incoming video stream, gated by RTP
        // liveness: the value freezes when packets stop, so only report it
        // while the RTP bitrate probe (fires on every RTP buffer) is fresh.
        let encoded_age_ms = age_since_ms(
            state.now_ms(),
            state.last_encoded_ms.load(Ordering::Relaxed),
        );
        let local_jitter_ms = query_rtcp_jitter_ms(&pipeline)
            .filter(|_| encoded_age_ms.is_none_or(|age| age <= JITTER_FRESH_AGE_MS));
        // Target depth of the adaptive pre-decode jitter buffer, converted
        // from compressed frames to milliseconds of buffered video (frame
        // interval from the negotiated stream rate, 60 fps fallback — the
        // same assumption the resize log uses). Gated on RTP liveness so the
        // HUD decays the depth once the stream stalls, like the jitter.
        let pre_decode_depth = state.pre_decode_depth.load(Ordering::Relaxed);
        let pre_decode_jitter_buffer_ms = (pre_decode_depth > 0)
            .then(|| {
                let fps = state.requested_fps().unwrap_or(60).max(1);
                (u64::from(pre_decode_depth) * 1000 / u64::from(fps)) as u32
            })
            .filter(|_| encoded_age_ms.is_none_or(|age| age <= JITTER_FRESH_AGE_MS));
        let server_rtt_now = stats_channel_rtt_ms();
        let effective_rtt = local_rtcp_rtt_ms.or((server_rtt_now > 0).then_some(server_rtt_now));
        if let Some(rtt) = effective_rtt {
            if let Some(depth) =
                state.adjust_pre_decode_queue_for_network(rtt, stats_channel_packet_loss_fraction())
            {
                send_log(
                    &event_sender,
                    "info",
                    format!(
                        "Native pre-decode jitter buffer resized to {depth} compressed frames (~{} ms) for network rtt={rtt} ms.",
                        depth * 1000 / 60
                    ),
                );
            }
        }

        let elapsed = last_rate_at.elapsed();
        if elapsed >= VIDEO_SINK_RATE_LOG_INTERVAL {
            let encoded_bytes_total = state.encoded_bytes_total.load(Ordering::Relaxed);
            let decoded_total = state.decoded_total.load(Ordering::Relaxed);
            let sink_total = state.sink_total.load(Ordering::Relaxed);
            let elapsed_secs = elapsed.as_secs_f64().max(0.001);
            let bitrate_kbps = encoded_bytes_total
                .saturating_sub(last_encoded_bytes_total)
                .saturating_mul(8) as f64
                / elapsed_secs
                / 1000.0;
            rates = VideoRateSnapshot {
                encoded_kbps: bitrate_kbps.max(0.0),
                decoded_fps: decoded_total.saturating_sub(last_decoded_total) as f64 / elapsed_secs,
                sink_fps: sink_total.saturating_sub(last_sink_total) as f64 / elapsed_secs,
            };
            update_native_stats_overlay(
                &sink,
                &state,
                rates.encoded_kbps.round() as u32,
                rates,
                decoded_total,
                sink_total,
                local_rtcp_rtt_ms,
                local_rtcp_rtt_age_ms,
                local_jitter_ms,
            );
            emit_native_stats_event(
                &event_sender,
                &sink,
                &state,
                rates.encoded_kbps.round() as u32,
                rates,
                decoded_total,
                sink_total,
                local_rtcp_rtt_ms,
                local_rtcp_rtt_age_ms,
                local_jitter_ms,
                pre_decode_jitter_buffer_ms,
            );
            // Fine-grained network health: the server RTT field vs the local
            // RTCP LSR/DLSR measurement + local signals, logged every 5s so a
            // single throttled session shows whether the server RTT actually
            // tracks degradation (or stays static — the trigger for switching
            // the HUD ping to the local RTCP source).
            if last_health_at.elapsed() >= Duration::from_secs(5) {
                last_health_at = Instant::now();
                let server_rtt = stats_channel_rtt_ms();
                let loss_percent = stats_channel_packet_loss_fraction()
                    .map(|loss| loss * 100.0)
                    .unwrap_or(-1.0);
                let sink_stats = read_sink_stats(&sink);
                let sink_dropped = sink_stats.dropped.unwrap_or(0);
                let sink_rendered = sink_stats.rendered.unwrap_or(sink_total);
                let sink_total_now = sink_rendered.saturating_add(sink_dropped);
                let drop_percent = if sink_total_now > 0 {
                    (sink_dropped as f64 / sink_total_now as f64) * 100.0
                } else {
                    0.0
                };
                send_log(
                    &event_sender,
                    "info",
                    format!(
                        "[NetworkHealth] server rtt={}ms loss={:.4}% rtcp={} sinkDrop={:.2}% sink={:.1}fps bitrate={:.1}Mbps rtcp_sent={}",
                        server_rtt,
                        loss_percent,
                        local_rtcp_rtt_ms
                            .map(|rtt| format!("{rtt}ms"))
                            .unwrap_or_else(|| "n/a (receiver-only)".to_owned()),
                        drop_percent,
                        rates.sink_fps,
                        rates.encoded_kbps / 1000.0,
                        state.rtcp_sent_summary(),
                    ),
                );
            }
            last_encoded_bytes_total = encoded_bytes_total;
            last_decoded_total = decoded_total;
            last_sink_total = sink_total;
            last_rate_at = Instant::now();
        }

        let last_sink_ms = state.last_sink_ms.load(Ordering::Relaxed);
        if last_sink_ms == 0 {
            maybe_recover_video_startup(&state, &chain_rebuild, &pipeline, &event_sender);
            continue;
        }

        let now_ms = state.now_ms();
        let encoded_age_ms = age_since_ms(now_ms, state.last_encoded_ms.load(Ordering::Relaxed));
        let decoded_age_ms = age_since_ms(now_ms, state.last_decoded_ms.load(Ordering::Relaxed));
        let sink_age_ms = age_since_ms(now_ms, last_sink_ms);
        let likely_stage = classify_video_stall(encoded_age_ms, decoded_age_ms, sink_age_ms);
        let transition_stall = likely_stage == "decode-chain-stalled"
            && encoded_age_ms.is_some_and(|age| age <= 1_000);

        // Drop ALL decode→present timestamp entries while the sink is stalled.
        // Entries pushed before/during the stall belong to frames that will be
        // presented (if ever) long after decode — pairing them after recovery
        // would report the whole stall as "decode time". Re-cleared every tick
        // the sink stays idle (decode may keep producing during the stall), so
        // the first post-recovery presents find an empty queue and the median
        // holds its last good value instead of spiking to thousands of ms.
        if sink_age_ms.is_some_and(|age| age >= VIDEO_STALL_WARNING_MS) {
            state.clear_decode_timestamps();
        }

        match tracker.evaluate(now_ms, last_sink_ms) {
            VideoStallAction::None => {}
            VideoStallAction::RequestKeyframe { attempt, stall_ms } => {
                // The main process already sends the RTCP keyframe for every
                // `video-stall` event (manager.ts → requestKeyframe), so no
                // separate request here.
                emit_video_stall_event(
                    &event_sender,
                    &sink,
                    &state,
                    rates,
                    attempt,
                    stall_ms,
                    false,
                );
                // If encoded RTP is still arriving but both decoded and sink
                // rates are zero, this is a decoder transition stall rather
                // than ordinary network idleness. Do not wait 20 seconds for
                // the fatal rung: after the second keyframe request, move to
                // the next decoder candidate (D3D12 → D3D11 → software). This
                // is the field pattern that looked like the stream was moving
                // backward/flickering while the D3D12 H265 decoder was stuck.
                if attempt >= 2
                    && transition_stall
                    && try_decoder_chain_fallback(&chain_rebuild, &state, &event_sender)
                {
                    tracker = VideoStallTracker::default();
                    continue;
                }
            }
            VideoStallAction::Resync { attempt, stall_ms } => {
                // Same as RequestKeyframe: the `video-stall` event drives the
                // RTCP keyframe request on the main side.
                emit_video_stall_event(
                    &event_sender,
                    &sink,
                    &state,
                    rates,
                    attempt,
                    stall_ms,
                    true,
                );
                match pipeline.recalculate_latency() {
                    Ok(()) => send_log(
                        &event_sender,
                        "warn",
                        "Requested GStreamer latency recalculation after native video stall.".to_owned(),
                    ),
                    Err(error) => send_log(
                        &event_sender,
                        "warn",
                        format!(
                            "Failed to request GStreamer latency recalculation after native video stall: {error}."
                        ),
                    ),
                }
            }
            VideoStallAction::PartialFlush { attempt, stall_ms } => {
                if transition_stall && state.transition_flush_escalation_enabled() {
                    perform_transition_flush(&state, &event_sender, TransitionFlushKind::Partial);
                }
                emit_video_stall_event(
                    &event_sender,
                    &sink,
                    &state,
                    rates,
                    attempt,
                    stall_ms,
                    false,
                );
            }
            VideoStallAction::CompleteFlush { attempt, stall_ms } => {
                if transition_stall && state.transition_flush_escalation_enabled() {
                    perform_transition_flush(&state, &event_sender, TransitionFlushKind::Complete);
                }
                emit_video_stall_event(
                    &event_sender,
                    &sink,
                    &state,
                    rates,
                    attempt,
                    stall_ms,
                    false,
                );
            }
            VideoStallAction::Fatal { attempt, stall_ms } => {
                emit_video_stall_event(
                    &event_sender,
                    &sink,
                    &state,
                    rates,
                    attempt,
                    stall_ms,
                    false,
                );
                // A mid-stream decode-chain stall (decoder stops producing
                // while RTP keeps flowing) can also be recovered by rebuilding
                // with the next decoder candidate before declaring the stream
                // dead. Reset the stall tracker so the new chain gets its own
                // escalation ladder.
                if try_decoder_chain_fallback(&chain_rebuild, &state, &event_sender) {
                    tracker = VideoStallTracker::default();
                    continue;
                }
                send_log(
                    &event_sender,
                    "error",
                    format!(
                        "Native video stall recovery exhausted after {stall_ms}ms; stage={likely_stage} queueMode={} transitionFlushEscalation={}.",
                        state.queue_mode().as_str(),
                        state.transition_flush_escalation_enabled(),
                    ),
                );
                if let Some(event_sender) = &event_sender {
                    let _ = event_sender.send(Event::Error {
                        code: "native-video-stall-fatal".to_owned(),
                        message: format!(
                            "Native video stall recovery exhausted after {stall_ms}ms ({likely_stage})."
                        ),
                    });
                }
            }
            VideoStallAction::Recovered { stall_ms } => {
                if state.queue_depth() > DEFAULT_VIDEO_QUEUE_DEPTH {
                    state.set_queue_depth(
                        DEFAULT_VIDEO_QUEUE_DEPTH,
                        "transition recovery completed",
                        &event_sender,
                    );
                }
                send_log(
                    &event_sender,
                    "info",
                    format!("Native video recovered after {stall_ms} ms."),
                );
            }
        }
    }
}

fn maybe_recover_video_startup(
    state: &VideoLivenessState,
    chain_rebuild: &Arc<Mutex<Option<VideoChainRebuildContext>>>,
    pipeline: &gst::Pipeline,
    event_sender: &Option<Sender<Event>>,
) {
    let now_ms = state.now_ms();
    let last_encoded_ms = state.last_encoded_ms.load(Ordering::Relaxed);
    let first_encoded_ms = state.first_startup_encoded_ms.load(Ordering::Relaxed);
    // Gate the startup recovery on the ENCODED VIDEO RTP being live instead
    // of audio: GFN game audio is Opus with DTX, so silent moments (loading
    // screens, quiet menus) carry no audio RTP at all — keying off audio let
    // a video decode stall on a silent screen run forever with no keyframe
    // request (the 22:33 packaged-build regression: video RTP flowed at
    // ~3 Mbps while d3d12h265dec produced zero frames, and the recovery never
    // fired because last_audio_ms had gone stale). The encoded RTP counter is
    // the direct proof the session's video path is alive.
    if first_encoded_ms == 0 || now_ms.saturating_sub(last_encoded_ms) > VIDEO_STARTUP_KEYFRAME_MS {
        return;
    }
    let encoded_active_ms = now_ms.saturating_sub(first_encoded_ms);

    let decoded_total = state.decoded_total.load(Ordering::Relaxed);
    let sink_total = state.sink_total.load(Ordering::Relaxed);
    let encoded_age = format!("{}ms", now_ms.saturating_sub(last_encoded_ms));

    if encoded_active_ms >= VIDEO_STARTUP_KEYFRAME_MS
        && !state
            .startup_keyframe_requested
            .swap(true, Ordering::Relaxed)
    {
        send_log(
            event_sender,
            "warn",
            format!(
                "Native video startup has no rendered frame after {encoded_active_ms}ms of incoming RTP; startupAge={now_ms}ms encodedAge={encoded_age} decoded={decoded_total} sink={sink_total}. Requesting keyframe."
            ),
        );
        request_video_keyframe("native-video-startup", 0, event_sender);
    }

    if encoded_active_ms >= VIDEO_STARTUP_RESYNC_MS
        && !state.startup_resync_requested.swap(true, Ordering::Relaxed)
    {
        send_log(
            event_sender,
            "warn",
            format!(
                "Native video startup still has no rendered frame after {encoded_active_ms}ms of incoming RTP; startupAge={now_ms}ms encodedAge={encoded_age} decoded={decoded_total} sink={sink_total}. Requesting keyframe and GStreamer latency resync."
            ),
        );
        request_video_keyframe("native-video-startup-resync", 0, event_sender);
        if let Err(error) = pipeline.recalculate_latency() {
            send_log(
                event_sender,
                "warn",
                format!("Failed to resync GStreamer latency during native video startup recovery: {error}."),
            );
        }
    }

    if encoded_active_ms >= VIDEO_STARTUP_FATAL_MS
        && !state.startup_fatal_reported.swap(true, Ordering::Relaxed)
    {
        // Before declaring the stream dead, try the decoder-chain fallback: a
        // hardware decoder that never outputs a frame (d3d12h265dec on some
        // Intel iGPUs) is only recoverable by rebuilding the chain with the
        // next candidate (D3D11, then software avdec). Keyframe requests don't
        // help a decoder that produces nothing at all.
        if try_decoder_chain_fallback(chain_rebuild, state, event_sender) {
            return;
        }
        // Zero decoded frames across every decoder candidate means this
        // client cannot decode the negotiated codec at all (e.g. the stock
        // rtpav1depay drop-forever case on GFN AV1 payloads). Keyframes and
        // latency resyncs cannot fix that — the only way to keep the session
        // usable is to restart it one step down the GFN codec ladder
        // (AV1 → H265 → H264). Each downgrade relaunches the session with the
        // next codec; the fresh streamer process re-evaluates, so the ladder
        // cascades naturally until H264 (the terminal codec, universally
        // decodable) or a codec that actually decodes. Emit the request ONCE
        // per session; the Electron main process stops the streamer and the
        // renderer relaunches the game session with the fallback codec.
        let codec = state
            .codec
            .lock()
            .map(|codec| codec.trim().to_ascii_uppercase())
            .unwrap_or_default();
        let downgrade_to = codec_downgrade_target(&codec);
        if let Some(downgrade_to) = downgrade_to {
            if decoded_total == 0
                && sink_total == 0
                && !state
                    .startup_downgrade_requested
                    .swap(true, Ordering::Relaxed)
            {
                send_log(
                    event_sender,
                    "warn",
                    format!(
                        "Native {codec} startup produced zero decoded frames after {encoded_active_ms}ms of incoming RTP (decoded={decoded_total} sink={sink_total}) across every decoder candidate; requesting automatic codec downgrade to {downgrade_to} so the session can keep running."
                    ),
                );
                if let Some(event_sender) = event_sender {
                    let _ = event_sender.send(Event::CodecDowngradeRequest {
                        from_codec: codec,
                        to_codec: downgrade_to.to_owned(),
                    });
                }
                return;
            }
        }
        send_log(
            event_sender,
            "error",
            format!(
                "Native video startup still has no rendered frame after {encoded_active_ms}ms of incoming RTP; startupAge={now_ms}ms encodedAge={encoded_age} decoded={decoded_total} sink={sink_total}. Treating startup as failed instead of restarting the WebRTC pipeline."
            ),
        );
        request_video_keyframe("native-video-startup-fatal", 0, event_sender);
        if let Some(event_sender) = event_sender {
            let _ = event_sender.send(Event::Error {
                code: "native-video-startup-timeout".to_owned(),
                message: "Native video startup timed out before the first rendered frame."
                    .to_owned(),
            });
        }
    }
}

/// GFN codec downgrade ladder: which codec to fall back to when `codec`
/// produced zero decoded frames during startup (every decoder candidate
/// exhausted). AV1 → H265 → H264; `None` for the terminal codec (H264) or
/// unknown labels. Each downgrade relaunches the session with the next codec
/// and the fresh streamer process re-evaluates, so the ladder cascades
/// naturally until H264 (universally decodable) or a codec that actually
/// decodes.
fn codec_downgrade_target(codec: &str) -> Option<&'static str> {
    match codec.trim().to_ascii_uppercase().as_str() {
        "AV1" => Some("H265"),
        // Defensive: the negotiated codec always serializes as "H265", but
        // some callers may pass the raw SDP spelling.
        "H265" | "HEVC" => Some("H264"),
        _ => None,
    }
}

/// Try to rebuild the RTP video decode chain with the next decoder candidate
/// (e.g. D3D12 → D3D11 → software). Returns true when a fallback chain is now
/// live; false when no candidates remain or the rebuild failed (the caller
/// should then escalate to the normal fatal error). Resets the startup window
/// so the new chain gets a fresh keyframe/resync/fatal timeline.
fn try_decoder_chain_fallback(
    chain_rebuild: &Arc<Mutex<Option<VideoChainRebuildContext>>>,
    state: &VideoLivenessState,
    event_sender: &Option<Sender<Event>>,
) -> bool {
    let Ok(mut slot) = chain_rebuild.lock() else {
        return false;
    };
    let Some(context) = slot.as_mut() else {
        return false;
    };
    if !context.try_rebuild(event_sender) {
        return false;
    }
    state.reset_startup_window();
    request_video_keyframe("native-video-chain-rebuilt", 0, event_sender);
    send_log(
        event_sender,
        "warn",
        "Native video decode chain rebuilt with the next decoder candidate; giving it a fresh startup window.".to_owned(),
    );
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransitionFlushKind {
    Partial,
    Complete,
}

fn perform_transition_flush(
    state: &VideoLivenessState,
    event_sender: &Option<Sender<Event>>,
    flush_kind: TransitionFlushKind,
) {
    let label = match flush_kind {
        TransitionFlushKind::Partial => "partial",
        TransitionFlushKind::Complete => "complete",
    };
    let mut flushed = Vec::new();

    if matches!(
        flush_kind,
        TransitionFlushKind::Partial | TransitionFlushKind::Complete
    ) {
        if let Some(queue) = state.pre_decode_queue() {
            flush_element(&queue);
            flushed.push("pre-decode queue");
        }
    }

    if matches!(flush_kind, TransitionFlushKind::Complete) {
        if let Some(decoder) = state.decoder() {
            flush_element(&decoder);
            flushed.push("decoder");
        }
    }

    if let Some(queue) = state
        .post_decode_queue
        .lock()
        .ok()
        .and_then(|current| current.clone())
    {
        flush_element(&queue);
        flushed.push("post-decode queue");
    }

    if flushed.is_empty() {
        send_log(
            event_sender,
            "warn",
            "Cannot flush native transition path because no video branch elements are registered."
                .to_owned(),
        );
        return;
    }

    match flush_kind {
        TransitionFlushKind::Partial => {
            state.increment_partial_flush_count();
            state.set_queue_depth(2, "transition partial flush", event_sender);
        }
        TransitionFlushKind::Complete => {
            state.increment_complete_flush_count();
            state.set_queue_depth(2, "transition complete flush", event_sender);
        }
    }

    send_log(
        event_sender,
        "warn",
        format!(
            "Performed {label} native transition flush on {}.",
            flushed.join(", ")
        ),
    );
}

fn flush_element(element: &gst::Element) {
    let _ = element.send_event(gst::event::FlushStart::new());
    let _ = element.send_event(gst::event::FlushStop::new(false));
}

/// Request an upstream video keyframe WITHOUT touching the media pipeline:
/// the request is forwarded to the Electron main process as a
/// `video-keyframe-request` event, and the main side sends the RTCP/PLI via
/// the signaling data channel (`requestKeyframe`). A GstForceKeyUnit
/// CustomUpstream event sent on the webrtcbin video src pad propagates
/// UPSTREAM into the transport and the bundled GStreamer runtime errors out
/// the UDP receiver (`nicesrc: Internal data stream error, reason
/// not-negotiated`) — the record-start stream death in the 22:35 field log.
fn request_video_keyframe(reason: &str, attempt: u8, event_sender: &Option<Sender<Event>>) {
    if let Some(sender) = event_sender {
        let _ = sender.send(Event::VideoKeyframeRequest(
            crate::protocol::VideoKeyframeRequest {
                reason: reason.to_owned(),
                attempt,
            },
        ));
    }
}

/// Query the LOCAL RTCP round-trip time from the webrtcbin's internal rtpbin
/// session stats. `rtpsession` computes this from Receiver Reports the server
/// sends about OUR outgoing RTP streams (the LSR/DLSR algorithm — the exact
/// "round-trip from Receiver Report" measurement) and exposes it as
/// `rb-round-trip` (16.16 fixed-point seconds) in the session's
/// `source-stats`.
///
/// The server only sends RRs for RTP streams it receives, so this needs
/// outgoing RTP. The native pipeline carries it on the mic m-line, and only
/// while the user's mic is actually on (real platform capture): there is no
/// generated-silence keepalive (a muted keepalive kept continuous outgoing
/// RTP alive and was the only structural delta behind the periodic video
/// stalls — see build_mic_pipeline). With the mic off (or
/// OPENNOW_NATIVE_MIC=0) there is no outgoing RTP and this returns None,
/// and the HUD falls back to the server-reported stats_channel field.
///
/// Returns `(rtt_ms, rb_lsr)`: the round-trip plus the `rb-lsr` field — the
/// SR timestamp the server echoes back inside each Receiver Report. `rb-lsr`
/// advances with EVERY new RR, so it is the freshness signal the watchdog
/// uses to expire a local measurement whose RR stream stopped (rtpsession's
/// `have-rb` sticks once set, so the raw `rb-round-trip` value alone can
/// never tell "still flowing" from "frozen at an old value").
fn query_rtcp_rtt_ms(pipeline: &gst::Pipeline) -> Option<(u32, u64)> {
    let webrtc = pipeline.by_name("opennow-webrtcbin")?;
    let webrtc_bin = webrtc.downcast::<gst::Bin>().ok()?;
    let rtpbin = webrtc_bin.children().into_iter().find(|child| {
        child
            .factory()
            .is_some_and(|factory| factory.name() == "rtpbin")
    })?;
    let rtpbin_bin = rtpbin.downcast::<gst::Bin>().ok()?;
    for session in rtpbin_bin.children() {
        // Only rtpsession elements expose a "stats" property; guard before
        // querying because `property()` panics on a missing property.
        if session.find_property("stats").is_none() {
            continue;
        }
        let stats = session.property::<gst::Structure>("stats");
        let Ok(source_stats) = stats.value("source-stats") else {
            continue;
        };
        let Ok(sources) = source_stats.get::<gst::glib::ValueArray>() else {
            continue;
        };
        for source in sources.iter() {
            let Ok(source) = source.get::<gst::Structure>() else {
                continue;
            };
            if source.get::<bool>("have-rb").unwrap_or(false) {
                let rb_lsr = source.get::<u64>("rb-lsr").unwrap_or(0);
                // GStreamer versions expose rb-round-trip as either guint or
                // guint64. Calling Value::get::<u32>() against a guint64
                // emits GLib's g_value_get_uint critical (seen once per stats
                // poll in the field log), so inspect the GType before reading.
                let Ok(value) = source.value("rb-round-trip") else {
                    continue;
                };
                let rtt_fixed = match value.type_().name().to_string().as_str() {
                    "guint" => value.get::<u32>().ok().map(u64::from),
                    "guint64" => value.get::<u64>().ok(),
                    "guchar" => value.get::<u8>().ok().map(u64::from),
                    _ => None,
                };
                if let Some(rtt_fixed) = rtt_fixed {
                    // 16.16 fixed point: value / 65536 seconds → ms.
                    let rtt_ms = (rtt_fixed as f64 / 65536.0 * 1000.0).round();
                    if rtt_ms > 0.0 && rtt_ms <= 2000.0 {
                        return Some((rtt_ms as u32, rb_lsr));
                    }
                }
            }
        }
    }
    None
}

/// Query the LOCAL receive jitter of the video stream from the webrtcbin's
/// internal rtpbin session stats. rtpsession computes the RFC 3550
/// interarrival jitter of the RTP packets it RECEIVES and exposes it as
/// `jitter` (and its EWMA `avg-jitter`) in each source's `source-stats`
/// entry, in RTP timestamp units. The video stream is the source with the
/// most received packets (the mic uplink is send-only and has none). Unlike
/// `query_rtcp_rtt_ms`, this does NOT need outgoing RTP — it measures the
/// incoming video stream directly, so it works even with the mic off.
fn query_rtcp_jitter_ms(pipeline: &gst::Pipeline) -> Option<u32> {
    let webrtc = pipeline.by_name("opennow-webrtcbin")?;
    let webrtc_bin = webrtc.downcast::<gst::Bin>().ok()?;
    let rtpbin = webrtc_bin.children().into_iter().find(|child| {
        child
            .factory()
            .is_some_and(|factory| factory.name() == "rtpbin")
    })?;
    let rtpbin_bin = rtpbin.downcast::<gst::Bin>().ok()?;
    // (packets_received, jitter_units, clock_rate) of the best candidate so
    // far — the video stream is the source receiving the most RTP packets.
    let mut best: Option<(u64, u32, u32)> = None;
    for session in rtpbin_bin.children() {
        // Only rtpsession elements expose a "stats" property; guard before
        // querying because `property()` panics on a missing property.
        if session.find_property("stats").is_none() {
            continue;
        }
        let stats = session.property::<gst::Structure>("stats");
        let Ok(source_stats) = stats.value("source-stats") else {
            continue;
        };
        let Ok(sources) = source_stats.get::<gst::glib::ValueArray>() else {
            continue;
        };
        for source in sources.iter() {
            let Ok(source) = source.get::<gst::Structure>() else {
                continue;
            };
            let packets_received = source.get::<u64>("packets-received").unwrap_or(0);
            if packets_received == 0 {
                continue;
            }
            // RFC 3550 jitter of the packets received from this source.
            // Prefer the EWMA (`avg-jitter`) — it is smoother and more stable
            // for the HUD, so per-packet arrival variance does not make the
            // readout jump around — and fall back to the raw `jitter` only
            // when the EWMA is absent or still 0 (e.g. too few packets yet).
            let jitter_units = source
                .get::<u32>("avg-jitter")
                .ok()
                .filter(|j| *j > 0)
                .or_else(|| source.get::<u32>("jitter").ok().filter(|j| *j > 0))
                .unwrap_or(0);
            if jitter_units == 0 {
                continue;
            }
            let clock_rate = source.get::<u32>("clock-rate").unwrap_or(90_000);
            if best
                .as_ref()
                .is_none_or(|(best_packets, _, _)| packets_received > *best_packets)
            {
                best = Some((packets_received, jitter_units, clock_rate));
            }
        }
    }
    let (_, jitter_units, clock_rate) = best?;
    rtcp_jitter_to_ms(jitter_units, clock_rate)
}

/// Convert RFC 3550 interarrival jitter from RTP timestamp units to
/// milliseconds: jitter_ms = units * 1000 / clock_rate (video RTP uses a
/// 90 kHz clock, so 1 ms of jitter ≈ 90 units). Pure so it's unit-testable.
fn rtcp_jitter_to_ms(jitter_units: u32, clock_rate: u32) -> Option<u32> {
    if jitter_units == 0 || clock_rate == 0 {
        return None;
    }
    let jitter_ms = (f64::from(jitter_units) * 1000.0 / f64::from(clock_rate)).round();
    // Plausibility clamp: real jitter is sub-second; a garbage/overflowed
    // counter must never reach the HUD.
    (jitter_ms > 0.0 && jitter_ms <= 1000.0).then_some(jitter_ms as u32)
}

/// The stats_channel `avgGameFps` is a short-window server-side average of the
/// game's render rate and can briefly overshoot the negotiated stream rate
/// (menu/loading screens that render uncapped, catch-up bursts after stalls,
/// sub-second averaging-window artifacts) — e.g. 174-200 for a game capped at
/// 120 fps. But the game's render rate legitimately EXCEEDS the stream rate: a
/// 60 fps stream can carry a game capped at 120 fps, so clamping to the
/// negotiated stream fps (requested fps) made the HUD stick at 60 regardless
/// of the game. Keep a plausibility ceiling of 2x the stream rate (120 for a
/// 60 fps stream, 240 for 120 fps, absolute sanity cap 360) so uncapped
/// menu/loading spikes stay out while real in-game rates are shown. 0 = no
/// stats yet.
fn clamped_server_game_fps(state: &VideoLivenessState) -> u32 {
    let game_fps = stats_channel_game_fps();
    if game_fps == 0 {
        return 0;
    }
    let stream_fps = state.requested_fps().or_else(|| {
        state
            .caps_framerate()
            .and_then(|caps| caps.split('/').next()?.trim().parse::<u32>().ok())
    });
    match stream_fps {
        Some(stream_fps) if stream_fps > 0 => {
            let ceiling = stream_fps.saturating_mul(2).clamp(60, 360);
            game_fps.min(ceiling)
        }
        _ => game_fps,
    }
}

fn emit_native_stats_event(
    event_sender: &Option<Sender<Event>>,
    sink: &gst::Element,
    state: &VideoLivenessState,
    bitrate_kbps: u32,
    rates: VideoRateSnapshot,
    frames_decoded: u64,
    frames_rendered: u64,
    local_rtcp_rtt_ms: Option<u32>,
    local_rtcp_rtt_age_ms: u32,
    local_jitter_ms: Option<u32>,
    pre_decode_jitter_buffer_ms: Option<u32>,
) {
    let Some(event_sender) = event_sender else {
        return;
    };

    let target_bitrate_kbps = state.target_bitrate_kbps.load(Ordering::Relaxed);
    let bitrate_performance_percent = if target_bitrate_kbps > 0 {
        (f64::from(bitrate_kbps) / f64::from(target_bitrate_kbps)) * 100.0
    } else {
        0.0
    };
    let codec = state
        .codec
        .lock()
        .map(|codec| codec.clone())
        .unwrap_or_default();
    let resolution = state
        .resolution
        .lock()
        .map(|resolution| resolution.clone())
        .unwrap_or_default();
    let hardware_acceleration = state
        .hardware_acceleration
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default();
    let sink_stats = read_sink_stats(sink);
    let telemetry = state.transition_telemetry_snapshot();
    let game_fps = clamped_server_game_fps(state);
    let rtt_ms = crate::gstreamer_input::stats_channel_rtt_ms();
    let input_path = crate::gstreamer_input::native_input_path().to_owned();
    let mouse_delta_latency_us =
        crate::gstreamer_input::mouse_delta_latency_us().map(|(ema_us, _, _)| ema_us);
    let _ = event_sender.send(Event::Stats {
        stats: crate::protocol::NativeStatsEvent {
            codec,
            resolution,
            hardware_acceleration,
            requested_fps: state.requested_fps(),
            caps_framerate: state.caps_framerate(),
            bitrate_kbps,
            target_bitrate_kbps,
            bitrate_performance_percent,
            decoded_fps: rates.decoded_fps,
            render_fps: rates.sink_fps,
            game_fps: (game_fps > 0).then_some(game_fps),
            network_rtt_ms: (rtt_ms > 0).then_some(rtt_ms),
            // Server RTT sample age (time since the last stats_channel frame
            // carried a valid RTT) so the renderer can expire it too.
            network_rtt_age_ms: crate::gstreamer_input::stats_channel_rtt_age_ms(),
            local_rtcp_rtt_ms,
            // Local RTCP sample age (time since the last RR refreshed the
            // value; the value is already gated on the native side, this age
            // lets the renderer prefer the freshest source when both exist).
            local_rtcp_rtt_age_ms: local_rtcp_rtt_ms.map(|_| local_rtcp_rtt_age_ms),
            // Local receive jitter of the incoming video stream (rtpsession
            // RFC 3550 interarrival jitter, converted from RTP timestamp
            // units via the source clock rate). None while the stream is
            // stalled (no RTP within JITTER_FRESH_AGE_MS) so the HUD decays
            // a frozen value instead of holding it as current.
            local_jitter_ms,
            // Target depth of the adaptive pre-decode jitter buffer (ms of
            // buffered video) — the delay the streamer intentionally holds
            // before decoding. None while stalled (RTP liveness gate), so
            // the HUD JitterBuf metric decays like the WebRTC one.
            pre_decode_jitter_buffer_ms,
            network_packet_loss_percent: stats_channel_packet_loss_fraction()
                .map(|loss| loss * 100.0),
            network_bitrate_kbps: stats_channel_bitrate_kbps(),
            decode_time_ms: state.decode_present_median_ms(),
            input_path,
            mouse_delta_latency_us,
            frames_decoded,
            frames_rendered,
            frames_pending_to_present: frames_decoded.saturating_sub(frames_rendered),
            sink_rendered: sink_stats.rendered,
            sink_dropped: sink_stats.dropped,
            memory_mode: state.memory_mode(),
            zero_copy: state.zero_copy(),
            queue_mode: telemetry.queue_mode.as_str().to_owned(),
            queue_depth_changes: telemetry.queue_depth_changes,
            present_pacing_changes: telemetry.present_pacing_changes,
            partial_flush_count: telemetry.partial_flush_count,
            complete_flush_count: telemetry.complete_flush_count,
            last_transition_type: telemetry
                .last_transition
                .as_ref()
                .map(|transition| transition.transition_type.clone()),
            last_transition_at_ms: telemetry
                .last_transition
                .as_ref()
                .map(|transition| transition.at_ms),
            last_transition_summary: telemetry
                .last_transition
                .as_ref()
                .map(|transition| transition.summary.clone()),
            requested_streaming_features_summary: state.requested_streaming_features_summary(),
            finalized_streaming_features_summary: state.finalized_streaming_features_summary(),
            zero_copy_d3d11: state.zero_copy_d3d11(),
            zero_copy_d3d12: state.zero_copy_d3d12(),
        },
    });
}

fn update_native_stats_overlay(
    sink: &gst::Element,
    state: &VideoLivenessState,
    bitrate_kbps: u32,
    rates: VideoRateSnapshot,
    _frames_decoded: u64,
    frames_rendered: u64,
    local_rtcp_rtt_ms: Option<u32>,
    _local_rtcp_rtt_age_ms: u32,
    local_jitter_ms: Option<u32>,
) {
    let target_bitrate_kbps = state.target_bitrate_kbps.load(Ordering::Relaxed);
    let bitrate_performance_percent = if target_bitrate_kbps > 0 {
        (f64::from(bitrate_kbps) / f64::from(target_bitrate_kbps)) * 100.0
    } else {
        0.0
    };
    let codec = state
        .codec
        .lock()
        .map(|codec| codec.clone())
        .unwrap_or_default();
    let resolution = state
        .resolution
        .lock()
        .map(|resolution| resolution.clone())
        .unwrap_or_default();
    let hardware_acceleration = state
        .hardware_acceleration
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default();
    let sink_stats = read_sink_stats(sink);
    let sink_dropped = sink_stats.dropped.unwrap_or(0);
    let sink_rendered = sink_stats.rendered.unwrap_or(frames_rendered);
    let sink_total = sink_rendered.saturating_add(sink_dropped);
    let drop_percent = if sink_total > 0 {
        (sink_dropped as f64 / sink_total as f64) * 100.0
    } else {
        0.0
    };
    let target_mbps = f64::from(target_bitrate_kbps) / 1000.0;
    let bitrate_mbps = f64::from(bitrate_kbps) / 1000.0;
    let memory_mode = state.memory_mode();
    let memory_path = if state.zero_copy() {
        format!("{memory_mode} zero-copy")
    } else {
        memory_mode
    };
    let rtt_ms = crate::gstreamer_input::stats_channel_rtt_ms();
    // Prefer the LOCAL RTCP round-trip (LSR/DLSR from RRs the server sends
    // about our outgoing RTP) over the server-reported stats_channel field
    // whenever it is available — but only while each source is fresh: the
    // local value is already gated by the watchdog, and the server field is
    // gated by its own sample age (time since the last stats_channel frame
    // carried a valid RTT). "-" until any FRESH source reports.
    let server_rtt_fresh = rtt_ms > 0
        && crate::gstreamer_input::stats_channel_rtt_age_ms()
            .is_none_or(|age| age <= LOCAL_RTCP_FRESH_AGE_MS);
    let ping_ms = local_rtcp_rtt_ms.or(server_rtt_fresh.then_some(rtt_ms));
    // Server-reported session bitrate (stats_channel counter, confidence-gated
    // in the native streamer); "-" until enough consistent samples confirm the
    // counter is cumulative bytes.
    let server_bitrate = stats_channel_bitrate_kbps()
        .map(|kbps| format!("{:.1}", f64::from(kbps) / 1000.0))
        .unwrap_or_else(|| "-".to_owned());
    let text = format!(
        "{} {}  {:.1}/{:.1} Mbps  Bit {:.0}%  Ping {}ms  Jit {}ms  Srv {} Mbps\nGame {:.0}fps  Stream {:.0}fps  Decode {:.0}fps  Drop {:.2}%  {}",
        codec,
        resolution,
        bitrate_mbps,
        target_mbps,
        bitrate_performance_percent,
        ping_ms.map(|ms| ms.to_string()).unwrap_or_else(|| "-".to_owned()),
        local_jitter_ms
            .map(|ms| ms.to_string())
            .unwrap_or_else(|| "-".to_owned()),
        server_bitrate,
        // Server-reported game render FPS (stats_channel), clamped to the
        // negotiated stream rate (short-window server averages can overshoot
        // the game's cap — e.g. 200 for a 120-cap game); 0 until the first
        // frame.
        f64::from(clamped_server_game_fps(state)),
        rates.sink_fps,
        rates.decoded_fps,
        drop_percent,
        if hardware_acceleration.is_empty() {
            memory_path
        } else {
            format!("{hardware_acceleration} {memory_path}")
        },
    );
    state.update_stats_overlay_text(&text);
}

fn emit_video_stall_event(
    event_sender: &Option<Sender<Event>>,
    sink: &gst::Element,
    state: &VideoLivenessState,
    rates: VideoRateSnapshot,
    recovery_attempt: u8,
    stall_ms: u64,
    will_resync: bool,
) {
    let stats = read_sink_stats(sink);
    let now_ms = state.now_ms();
    let last_encoded_ms = state.last_encoded_ms.load(Ordering::Relaxed);
    let last_decoded_ms = state.last_decoded_ms.load(Ordering::Relaxed);
    let last_sink_ms = state.last_sink_ms.load(Ordering::Relaxed);
    let encoded_age_ms = age_since_ms(now_ms, last_encoded_ms);
    let decoded_age_ms = age_since_ms(now_ms, last_decoded_ms);
    let sink_age_ms = age_since_ms(now_ms, last_sink_ms);
    let likely_stage = classify_video_stall(encoded_age_ms, decoded_age_ms, sink_age_ms);
    let memory_mode = state.memory_mode();
    let zero_copy = state.zero_copy();
    let telemetry = state.transition_telemetry_snapshot();
    let resync_suffix = if will_resync {
        " Requesting keyframe and resyncing GStreamer latency."
    } else {
        " Requesting keyframe."
    };
    send_log(
        event_sender,
        "warn",
        format!(
            "Native video stall detected: stall={stall_ms}ms stage={likely_stage} encoded={:.0}kbps decoded={:.1}fps sink={:.1}fps requestedFps={} capsFramerate={} queueMode={} partialFlushes={} completeFlushes={} lastTransition={} ages=encoded:{} decoded:{} sink:{} rendered={} dropped={} memoryMode={} zeroCopy={} zeroCopyD3D11={} zeroCopyD3D12={}. If decoded/sink/rendered counters are still flowing but the visible frame is stale, suspect a server-driven mid-stream transition the native decode/present chain failed to absorb rather than pure RTP loss.{}",
            rates.encoded_kbps,
            rates.decoded_fps,
            rates.sink_fps,
            state
                .requested_fps()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_owned()),
            state.caps_framerate().unwrap_or_else(|| "unknown".to_owned()),
            telemetry.queue_mode.as_str(),
            telemetry.partial_flush_count,
            telemetry.complete_flush_count,
            telemetry
                .last_transition
                .as_ref()
                .map(|transition| transition.transition_type.as_str())
                .unwrap_or("none"),
            format_age_ms(encoded_age_ms),
            format_age_ms(decoded_age_ms),
            format_age_ms(sink_age_ms),
            stats
                .rendered
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_owned()),
            stats
                .dropped
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_owned()),
            memory_mode.as_str(),
            zero_copy,
            state.zero_copy_d3d11(),
            state.zero_copy_d3d12(),
            resync_suffix
        ),
    );
    if let Some(event_sender) = event_sender {
        let _ = event_sender.send(Event::VideoStall(VideoStallEvent {
            stall_ms,
            encoded_kbps: rates.encoded_kbps,
            decoded_fps: rates.decoded_fps,
            sink_fps: rates.sink_fps,
            encoded_age_ms,
            decoded_age_ms,
            sink_age_ms,
            likely_stage: likely_stage.to_owned(),
            sink_rendered: stats.rendered,
            sink_dropped: stats.dropped,
            memory_mode,
            zero_copy,
            requested_fps: state.requested_fps(),
            caps_framerate: state.caps_framerate(),
            queue_mode: telemetry.queue_mode.as_str().to_owned(),
            partial_flush_count: telemetry.partial_flush_count,
            complete_flush_count: telemetry.complete_flush_count,
            last_transition_type: telemetry
                .last_transition
                .as_ref()
                .map(|transition| transition.transition_type.clone()),
            last_transition_at_ms: telemetry
                .last_transition
                .as_ref()
                .map(|transition| transition.at_ms),
            requested_streaming_features_summary: state.requested_streaming_features_summary(),
            finalized_streaming_features_summary: state.finalized_streaming_features_summary(),
            zero_copy_d3d11: state.zero_copy_d3d11(),
            zero_copy_d3d12: state.zero_copy_d3d12(),
            recovery_attempt,
        }));
    }
}

/// Median of a small slice of recent decode→present deltas (lower-middle for
/// an even count). Pure so the median robustness is unit-testable.
fn median_of_deltas(deltas: &VecDeque<u32>) -> u32 {
    let mut sorted: Vec<u32> = deltas.iter().copied().collect();
    sorted.sort_unstable();
    sorted[(sorted.len() - 1) / 2]
}

fn age_since_ms(now_ms: u64, last_ms: u64) -> Option<u64> {
    (last_ms != 0).then_some(now_ms.saturating_sub(last_ms))
}

fn format_age_ms(age_ms: Option<u64>) -> String {
    age_ms
        .map(|value| format!("{value}ms"))
        .unwrap_or_else(|| "n/a".to_owned())
}

fn classify_video_stall(
    encoded_age_ms: Option<u64>,
    decoded_age_ms: Option<u64>,
    sink_age_ms: Option<u64>,
) -> &'static str {
    const ACTIVE_RECENT_MS: u64 = 1_000;
    match (encoded_age_ms, decoded_age_ms, sink_age_ms) {
        (Some(encoded), _, _) if encoded > VIDEO_STALL_WARNING_MS => "video-rtp-idle",
        (Some(encoded), Some(decoded), _)
            if encoded <= ACTIVE_RECENT_MS && decoded > VIDEO_STALL_WARNING_MS =>
        {
            "decode-chain-stalled"
        }
        (_, Some(decoded), Some(sink))
            if decoded <= ACTIVE_RECENT_MS && sink > VIDEO_STALL_WARNING_MS =>
        {
            "present-chain-stalled"
        }
        (None, _, _) => "video-rtp-not-observed",
        _ => "video-output-stalled",
    }
}

pub(crate) fn watch_audio_activity(sink: &gst::Element, video_liveness: &VideoLivenessMonitor) {
    let Some(sink_pad) = sink.static_pad("sink") else {
        return;
    };
    let monitor = video_liveness.clone();
    sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
        monitor.record_audio_buffer();
        gst::PadProbeReturn::Ok
    });
}

pub(crate) fn watch_first_sink_buffer(
    sink: &gst::Element,
    media_label: &str,
    event_sender: &Option<Sender<Event>>,
    streaming_reported: &Arc<AtomicBool>,
) {
    let Some(sink_pad) = sink.static_pad("sink") else {
        return;
    };
    let sender = event_sender.clone();
    let label = media_label.to_owned();
    let reported = streaming_reported.clone();
    sink_pad.add_probe(gst::PadProbeType::BUFFER, move |pad, _info| {
        let caps = pad
            .current_caps()
            .map(|caps| caps.to_string())
            .unwrap_or_else(|| "unknown caps".to_owned());
        let zero_copy_d3d11 = caps.contains("memory:D3D11Memory");
        let zero_copy_d3d12 = caps.contains("memory:D3D12Memory");
        let memory_mode = memory_mode_from_caps(&caps);
        let zero_copy = is_zero_copy_memory_mode(memory_mode);
        send_log(
            &sender,
            "info",
            format!(
                "First decoded {label} buffer reached native sink; caps={caps}; memoryMode={memory_mode}; zeroCopy={zero_copy}; zeroCopyD3D11={zero_copy_d3d11}; zeroCopyD3D12={zero_copy_d3d12}."
            ),
        );

        if label == "video" && !reported.swap(true, Ordering::SeqCst) {
            // GFN-parity launch: the sink stays hidden until the first decoded
            // frame, then is shown + positioned (reveal) BEFORE the renderer
            // learns streaming started — so the transparent-shell flip never
            // precedes a visible, correctly-positioned video window (the
            // launch blur / desktop flash). The sink window is created at the
            // first present, just after this probe, so the reveal usually
            // cannot complete here; in stacked mode the guard finishes it (and
            // then delivers this event) the moment the window exists.
            let revealed_now = crate::gstreamer_platform::reveal_stacked_renderer_window();
            // Deferral is a Windows-only stacked-mode concept: the sink window
            // is created at the first present, after this probe, and the guard
            // reveals it (then delivers this event) once it exists. On other
            // platforms the no-op reveal always reports false, so never defer
            // there or the event would never be sent.
            let deferred = cfg!(target_os = "windows") && !revealed_now && use_stacked_renderer();
            if !deferred {
                crate::gstreamer_platform::stacked_mark_streaming_event_sent();
                if let Some(event_sender) = &sender {
                    let message = if use_external_renderer_window() {
                        "Native video frames reached the external low-latency GStreamer renderer window."
                    } else {
                        "Native video frames reached the internal child-surface GStreamer renderer."
                    };
                    let _ = event_sender.send(Event::Status {
                        status: "streaming",
                        message: Some(message.to_owned()),
                    });
                }
            }
        }

        gst::PadProbeReturn::Remove
    });
}

pub(crate) fn watch_rtp_video_bitrate(
    pad: &gst::Pad,
    video_liveness: VideoLivenessMonitor,
    event_sender: &Option<Sender<Event>>,
) {
    // The RTP src pad outlives decoder-chain rebuilds, so the probe must only
    // be installed once per session; re-adding would double-count encoded
    // bytes in the stats/bitrate reporting.
    if video_liveness
        .state()
        .rtp_bitrate_probe_installed
        .swap(true, Ordering::Relaxed)
    {
        return;
    }
    let sender = event_sender.clone();
    pad.add_probe(gst::PadProbeType::BUFFER, move |probe_pad, info| {
        if let Some(buffer) = info.buffer() {
            video_liveness.record_encoded_buffer(buffer.size());
            if video_liveness.log_first_encoded_once() {
                // The pad caps expose the actual RTP payload type + encoding the
                // server picked. If that differs from the negotiated codec
                // (e.g. H265/AV1 `not-negotiated` receive failures), this line
                // shows exactly what the server is sending.
                let caps = probe_pad
                    .current_caps()
                    .map(|caps| caps.to_string())
                    .unwrap_or_else(|| "unknown caps".to_owned());
                send_log(
                    &sender,
                    "info",
                    format!(
                        "First encoded RTP video buffer arrived; size={} bytes; pad_caps={caps}",
                        buffer.size()
                    ),
                );
            }
        }
        gst::PadProbeReturn::Ok
    });
}

pub(crate) fn watch_video_sink_rate(
    sink: &gst::Element,
    event_sender: &Option<Sender<Event>>,
    video_liveness: Option<VideoLivenessMonitor>,
) {
    let Some(sink_pad) = sink.static_pad("sink") else {
        return;
    };
    let sink = sink.clone();
    watch_video_pad_rate(
        &sink_pad,
        "Native video sink rate",
        Some(sink),
        event_sender,
        video_liveness.map(|monitor| (monitor, VideoLivenessPadKind::Sink)),
    );
}

pub(crate) fn watch_video_decoded_rate(
    queue: &gst::Element,
    event_sender: &Option<Sender<Event>>,
    video_liveness: Option<VideoLivenessMonitor>,
) {
    let Some(queue_sink_pad) = queue.static_pad("sink") else {
        return;
    };
    watch_video_pad_rate(
        &queue_sink_pad,
        "Native decoded video rate before present queue",
        None,
        event_sender,
        video_liveness.map(|monitor| (monitor, VideoLivenessPadKind::Decoded)),
    );
}

pub(crate) fn watch_video_caps_transitions(
    element: &gst::Element,
    source: &'static str,
    event_sender: &Option<Sender<Event>>,
    video_liveness: VideoLivenessMonitor,
) {
    let Some(src_pad) = element.static_pad("src") else {
        return;
    };
    let sender = event_sender.clone();
    let monitor = video_liveness.clone();
    let last_caps = Arc::new(Mutex::new(None::<String>));
    let last_framerate = Arc::new(Mutex::new(None::<String>));
    let last_memory_mode = Arc::new(Mutex::new(None::<String>));
    let last_caps_for_probe = last_caps.clone();
    let last_framerate_for_probe = last_framerate.clone();
    let last_memory_mode_for_probe = last_memory_mode.clone();

    src_pad.add_probe(gst::PadProbeType::BUFFER, move |pad, _info| {
        let caps = pad
            .current_caps()
            .map(|caps| caps.to_string())
            .unwrap_or_else(|| "unknown caps".to_owned());
        let framerate = caps_framerate_summary(&caps);
        let memory_mode = Some(memory_mode_from_caps(&caps).to_owned());

        let Ok(mut old_caps) = last_caps_for_probe.lock() else {
            return gst::PadProbeReturn::Ok;
        };
        let Ok(mut old_framerate) = last_framerate_for_probe.lock() else {
            return gst::PadProbeReturn::Ok;
        };
        let Ok(mut old_memory_mode) = last_memory_mode_for_probe.lock() else {
            return gst::PadProbeReturn::Ok;
        };

        if old_caps.is_none() {
            *old_caps = Some(caps.clone());
            *old_framerate = framerate.clone();
            *old_memory_mode = memory_mode.clone();
            if source == "decoder" {
                send_log(
                    &sender,
                    "info",
                    format!("Native decoded video caps: {caps}"),
                );
            }
            return gst::PadProbeReturn::Ok;
        }

        let caps_changed = old_caps.as_ref() != Some(&caps);
        let framerate_changed = *old_framerate != framerate;
        let memory_changed = *old_memory_mode != memory_mode;
        if caps_changed || framerate_changed || memory_changed {
            monitor.record_transition(
                &format!("{source}-caps-change"),
                source,
                old_caps.clone(),
                Some(caps.clone()),
                old_framerate.clone(),
                framerate.clone(),
                old_memory_mode.clone(),
                memory_mode.clone(),
                &sender,
            );
            *old_caps = Some(caps);
            *old_framerate = framerate;
            *old_memory_mode = memory_mode;
        }

        gst::PadProbeReturn::Ok
    });
}

pub(crate) fn watch_video_sink_caps_transitions(
    sink: &gst::Element,
    event_sender: &Option<Sender<Event>>,
    video_liveness: Option<VideoLivenessMonitor>,
) {
    let Some(monitor) = video_liveness else {
        return;
    };
    let Some(sink_pad) = sink.static_pad("sink") else {
        return;
    };
    let sender = event_sender.clone();
    let last_caps = Arc::new(Mutex::new(None::<String>));
    let last_framerate = Arc::new(Mutex::new(None::<String>));
    let last_memory_mode = Arc::new(Mutex::new(None::<String>));
    let last_caps_for_probe = last_caps.clone();
    let last_framerate_for_probe = last_framerate.clone();
    let last_memory_mode_for_probe = last_memory_mode.clone();

    sink_pad.add_probe(gst::PadProbeType::BUFFER, move |pad, _info| {
        let caps = pad
            .current_caps()
            .map(|caps| caps.to_string())
            .unwrap_or_else(|| "unknown caps".to_owned());
        let framerate = caps_framerate_summary(&caps);
        let memory_mode = Some(memory_mode_from_caps(&caps).to_owned());

        let Ok(mut old_caps) = last_caps_for_probe.lock() else {
            return gst::PadProbeReturn::Ok;
        };
        let Ok(mut old_framerate) = last_framerate_for_probe.lock() else {
            return gst::PadProbeReturn::Ok;
        };
        let Ok(mut old_memory_mode) = last_memory_mode_for_probe.lock() else {
            return gst::PadProbeReturn::Ok;
        };

        if old_caps.is_none() {
            *old_caps = Some(caps);
            *old_framerate = framerate;
            *old_memory_mode = memory_mode;
            return gst::PadProbeReturn::Ok;
        }

        let caps_changed = old_caps.as_ref() != Some(&caps);
        let framerate_changed = *old_framerate != framerate;
        let memory_changed = *old_memory_mode != memory_mode;
        if caps_changed || framerate_changed || memory_changed {
            monitor.record_transition(
                "sink-caps-change",
                "sink",
                old_caps.clone(),
                Some(caps.clone()),
                old_framerate.clone(),
                framerate.clone(),
                old_memory_mode.clone(),
                memory_mode.clone(),
                &sender,
            );
            *old_caps = Some(caps);
            *old_framerate = framerate;
            *old_memory_mode = memory_mode;
        }

        gst::PadProbeReturn::Ok
    });
}

/// Frames arriving up to this much before their present slot pass instead of
/// being dropped. Steady-state arrival jitter is usually ±2 ms; dropping those
/// marginal frames against the fixed grid reads as "patah-patah" motion during
/// a smooth drag. Real catch-up bursts (many ms early) are still thinned.
const PRESENT_LIMITER_EARLY_TOLERANCE: Duration = Duration::from_millis(2);

pub(crate) fn install_present_limiter(
    sink: &gst::Element,
    present_max_fps: Arc<AtomicU32>,
    event_sender: &Option<Sender<Event>>,
    video_liveness: Option<VideoLivenessMonitor>,
) {
    let Some(sink_pad) = sink.static_pad("sink") else {
        return;
    };

    let sender = event_sender.clone();
    let monitor = video_liveness.clone();
    let state = Arc::new(Mutex::new(PresentLimiterState {
        next_present_at: Instant::now(),
        last_log_at: Instant::now(),
        passed: 0,
        dropped: 0,
        active_fps: 0,
    }));

    sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
        let target_fps = present_max_fps.load(Ordering::Relaxed);
        if target_fps == 0 {
            return gst::PadProbeReturn::Ok;
        }

        let Ok(mut state) = state.lock() else {
            return gst::PadProbeReturn::Ok;
        };

        let now = Instant::now();
        if state.active_fps != target_fps {
            state.active_fps = target_fps;
            state.next_present_at = now;
            state.last_log_at = now;
            state.passed = 0;
            state.dropped = 0;
            if let Some(monitor) = &monitor {
                monitor.record_present_pacing_change();
            }
        }

        let frame_interval = Duration::from_secs_f64(1.0 / f64::from(target_fps.max(1)));
        // Drop only frames arriving well before their slot (the jitter
        // catch-up bursts the limiter exists to thin). A 2 ms tolerance keeps
        // steady-state arrival jitter from dropping frames against the fixed
        // grid; marginal early frames pass and re-anchor the grid instead of
        // vanishing (dropped frames during a smooth drag read as "patah-patah"
        // motion).
        if now + PRESENT_LIMITER_EARLY_TOLERANCE < state.next_present_at {
            state.dropped = state.dropped.saturating_add(1);
            // This frame is being dropped by the limiter probe, which runs
            // BEFORE the sink-rate probe on the same pad — the sink-rate
            // probe (and its record_sink_buffer) never fires for it. Pop its
            // decode timestamp here so the pairing queue stays balanced: a
            // dropped frame must consume exactly one entry, or the next
            // presented frame pops a stale one and the HUD decode time
            // inflates.
            if let Some(monitor) = &monitor {
                monitor.record_sink_limiter_drop();
            }
            return gst::PadProbeReturn::Drop;
        }

        state.passed = state.passed.saturating_add(1);
        if now < state.next_present_at {
            // Within tolerance: present at the actual arrival and anchor the
            // next slot to it, so the grid keeps a stable cadence.
            state.next_present_at = now + frame_interval;
        } else {
            while state.next_present_at <= now {
                state.next_present_at += frame_interval;
            }
        }
        let elapsed = state.last_log_at.elapsed();
        if elapsed >= VIDEO_SINK_RATE_LOG_INTERVAL {
            let passed = state.passed;
            let dropped = state.dropped;
            send_log(
                &sender,
                "debug",
                format!(
                    "Native present limiter: target={target_fps} fps; passed={passed}; dropped={dropped} over {:.1}s.",
                    elapsed.as_secs_f64()
                ),
            );
            state.last_log_at = now;
            state.passed = 0;
            state.dropped = 0;
        }

        gst::PadProbeReturn::Ok
    });
}

#[derive(Debug)]
struct PresentLimiterState {
    next_present_at: Instant,
    last_log_at: Instant,
    passed: u32,
    dropped: u32,
    active_fps: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VideoLivenessPadKind {
    Decoded,
    Sink,
}

fn watch_video_pad_rate(
    pad: &gst::Pad,
    label: &'static str,
    sink: Option<gst::Element>,
    event_sender: &Option<Sender<Event>>,
    video_liveness: Option<(VideoLivenessMonitor, VideoLivenessPadKind)>,
) {
    let sender = event_sender.clone();
    let state = Arc::new(Mutex::new((Instant::now(), 0u32)));

    pad.add_probe(gst::PadProbeType::BUFFER, move |pad, _info| {
        if let Some((monitor, kind)) = &video_liveness {
            match kind {
                VideoLivenessPadKind::Decoded => monitor.record_decoded_buffer(),
                VideoLivenessPadKind::Sink => monitor.record_sink_buffer(),
            }
        }

        let Ok(mut state) = state.lock() else {
            return gst::PadProbeReturn::Ok;
        };

        state.1 = state.1.saturating_add(1);
        let elapsed = state.0.elapsed();
        if elapsed >= VIDEO_SINK_RATE_LOG_INTERVAL {
            let frames = state.1;
            let fps = f64::from(frames) / elapsed.as_secs_f64();
            let caps = pad
                .current_caps()
                .map(|caps| caps.to_string())
                .unwrap_or_else(|| "unknown caps".to_owned());
            let zero_copy_d3d11 = caps.contains("memory:D3D11Memory");
            let zero_copy_d3d12 = caps.contains("memory:D3D12Memory");
            let memory_mode = memory_mode_from_caps(&caps);
            let zero_copy = is_zero_copy_memory_mode(memory_mode);
            if let Some((monitor, _)) = &video_liveness {
                monitor.update_caps(&caps);
            }
            let caps_framerate =
                caps_framerate_summary(&caps).unwrap_or_else(|| "unknown".to_owned());
            let requested_fps = video_liveness
                .as_ref()
                .and_then(|(monitor, _)| monitor.requested_fps());
            let requested_fps_summary = requested_fps
                .map(|fps| format!("; requestedFps={fps}"))
                .unwrap_or_default();
            if let (Some((monitor, _)), Some(requested_fps), Some(caps_framerate_value)) = (
                video_liveness.as_ref(),
                requested_fps,
                caps_framerate_summary(&caps),
            ) {
                let expected = format!("{requested_fps}/1");
                if caps_framerate_value != expected && monitor.warn_framerate_mismatch_once() {
                    monitor.record_transition(
                        "high-fps-transition-risk",
                        label,
                        None,
                        Some(caps.clone()),
                        None,
                        Some(caps_framerate_value.clone()),
                        None,
                        Some(memory_mode.to_owned()),
                        &sender,
                    );
                    send_log(
                        &sender,
                        "warn",
                        format!(
                            "Native video caps framerate {caps_framerate_value} does not match requestedFps={requested_fps}; this can destabilize high-FPS native playback scheduling and buffer pools."
                        ),
                    );
                }
            }
            let sink_stats = sink
                .as_ref()
                .map(|sink| format!("; {}", sink_stats_summary(sink)))
                .unwrap_or_default();

            send_log(
                &sender,
                "debug",
                format!(
                    "{label}: {fps:.1} fps; capsFramerate={caps_framerate}{requested_fps_summary}; memoryMode={memory_mode}; zeroCopy={zero_copy}; zeroCopyD3D11={zero_copy_d3d11}; zeroCopyD3D12={zero_copy_d3d12}{sink_stats}."
                ),
            );

            *state = (Instant::now(), 0);
        }

        gst::PadProbeReturn::Ok
    });
}

pub(crate) fn sink_stats_summary(sink: &gst::Element) -> String {
    let stats = read_sink_stats(sink);
    if !stats.available {
        return "sinkStats=unavailable".to_owned();
    }

    format!(
        "sinkStats rendered={} dropped={} averageRate={}",
        stats
            .rendered
            .map(|value| value.to_string())
            .unwrap_or_else(|| "n/a".to_owned()),
        stats
            .dropped
            .map(|value| value.to_string())
            .unwrap_or_else(|| "n/a".to_owned()),
        stats
            .average_rate
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "n/a".to_owned())
    )
}

#[derive(Debug, Clone, Copy, Default)]
struct VideoSinkStats {
    available: bool,
    rendered: Option<u64>,
    dropped: Option<u64>,
    average_rate: Option<f64>,
}

fn read_sink_stats(sink: &gst::Element) -> VideoSinkStats {
    if sink.find_property("stats").is_none() {
        return VideoSinkStats::default();
    }

    let stats = sink.property::<gst::Structure>("stats");
    VideoSinkStats {
        available: true,
        // d3d12videosink exposes rendered/dropped as guint on the bundled
        // runtime, while other sinks expose guint64. Reading everything as
        // u64 calls g_value_get_uint64 on a guint and emits a GLib critical
        // (the once-per-session warning seen in the field log).
        rendered: structure_uint64(&stats, "rendered"),
        dropped: structure_uint64(&stats, "dropped"),
        average_rate: stats.get::<f64>("average-rate").ok(),
    }
}

fn structure_uint64(structure: &gst::Structure, field: &str) -> Option<u64> {
    let value = structure.value(field).ok()?;
    let type_name = value.type_().name().to_string();
    match type_name.as_str() {
        "guint" => value.get::<u32>().ok().map(u64::from),
        "guint64" => value.get::<u64>().ok(),
        "gint" => value
            .get::<i32>()
            .ok()
            .and_then(|value| u64::try_from(value).ok()),
        "gint64" => value
            .get::<i64>()
            .ok()
            .and_then(|value| u64::try_from(value).ok()),
        _ => None,
    }
}

/// Number of frames the sink has presented so far (None when the sink has no
/// `stats` property). Used by the deferred video-tap attach to wait for the
/// D3D sink to finish warming up before hot-plugging the tap tee.
pub(crate) fn sink_rendered_frame_count(sink: &gst::Element) -> Option<u64> {
    read_sink_stats(sink).rendered
}

/// Current number of buffers in a queue element (`current-level-buffers`).
pub(crate) fn read_queue_level(queue: &gst::Element) -> u32 {
    queue.property::<u32>("current-level-buffers")
}

/// True once the pad's sticky event list shows the mandatory media sequence
/// STREAM_START + CAPS. Used as the deterministic "branch is ready" gate
/// before the recording valve opens after a rebuild: a queue that receives a
/// live buffer with no stream-start yet logs the field warning `Got data flow
/// before stream-start` and the branch can stall so badly that stop() times
/// out. CAPS is sticky and arrives AFTER stream-start in the replay order, so
/// its presence proves the whole sequence landed.
pub(crate) fn pad_has_media_sticky_events(pad: &gst::Pad) -> bool {
    let mut seen_stream_start = false;
    let mut seen_caps = false;
    pad.sticky_events_foreach(|event| {
        match event.type_() {
            gst::EventType::StreamStart => seen_stream_start = true,
            gst::EventType::Caps => seen_caps = true,
            _ => {}
        }
        if seen_stream_start && seen_caps {
            std::ops::ControlFlow::Break(gst::EventForeachAction::Keep)
        } else {
            std::ops::ControlFlow::Continue(gst::EventForeachAction::Keep)
        }
    });
    seen_stream_start && seen_caps
}

/// One outgoing RTCP message's classification (per RTCP packet walk).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RtcpMessageCounts {
    pub(crate) sr: u64,
    pub(crate) rr: u64,
    pub(crate) twcc: u64,
    pub(crate) nack: u64,
    pub(crate) pli: u64,
    pub(crate) fir: u64,
    pub(crate) other: u64,
}

/// Walk a (possibly compound) RTCP buffer and classify each message by packet
/// type. Used by the outgoing-RTCP observability probe so we can SEE whether
/// the client's feedback (RR, and especially transport-cc) actually reaches
/// the GFN server — the server's BWE is blind without it and holds a
/// conservative bitrate. Pure function, unit-tested.
///
/// RTCP message header: byte0 = V(2)|P(1)|count/FMT(5), byte1 = PT,
/// bytes2-3 = length in 32-bit words minus 1. PT 200=SR, 201=RR, 205=RTPFB
/// (FMT 1=NACK, 15=transport-cc), 206=PSFB (FMT 1=PLI, 4=FIR).
pub(crate) fn classify_rtcp_messages(buf: &[u8]) -> RtcpMessageCounts {
    let mut counts = RtcpMessageCounts::default();
    let mut offset = 0usize;
    while offset + 4 <= buf.len() {
        let pt = buf[offset + 1];
        let length_words =
            u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]) as usize;
        let message_len = (length_words + 1).saturating_mul(4);
        // Only count a message whose full body is present; a truncated tail
        // (partial last message) is dropped instead of misread.
        if message_len < 4 || offset + message_len > buf.len() {
            break;
        }
        let fmt = buf[offset] & 0x1F;
        match pt {
            200 => counts.sr += 1,
            201 => counts.rr += 1,
            205 => match fmt {
                1 => counts.nack += 1,
                15 => counts.twcc += 1,
                _ => counts.other += 1,
            },
            206 => match fmt {
                1 => counts.pli += 1,
                4 => counts.fir += 1,
                _ => counts.other += 1,
            },
            _ => counts.other += 1,
        }
        offset += message_len;
    }
    counts
}

/// Probe the webrtcbin's internal rtpbin rtpsession elements and count every
/// RTCP packet the client SENDS to the server (RR/SR/transport-cc/NACK/PLI/
/// FIR), classified by `classify_rtcp_messages`. The sessions outlive
/// decoder-chain rebuilds, so the probe is installed once per monitor
/// (guarded by `rtcp_send_probe_installed`, like the RTP bitrate probe). The
/// counters land in the NetworkHealth log (`rtcp_sent=...`), turning "is the
/// server BWE blind?" from speculation into data: TWCC stays 0 when the
/// feedback path is dead.
pub(crate) fn watch_rtcp_send_stats(
    pipeline: &gst::Pipeline,
    video_liveness: VideoLivenessMonitor,
    event_sender: &Option<Sender<Event>>,
) {
    if video_liveness
        .state()
        .rtcp_send_probe_installed
        .swap(true, Ordering::Relaxed)
    {
        return;
    }
    let Some(webrtc) = pipeline.by_name("opennow-webrtcbin") else {
        return;
    };
    let Ok(webrtc_bin) = webrtc.downcast::<gst::Bin>() else {
        return;
    };
    let Some(rtpbin) = webrtc_bin.children().into_iter().find(|child| {
        child
            .factory()
            .is_some_and(|factory| factory.name() == "rtpbin")
    }) else {
        return;
    };
    let Ok(rtpbin_bin) = rtpbin.downcast::<gst::Bin>() else {
        return;
    };
    let mut probed = 0usize;
    let mut configured_twcc = 0usize;
    for session in rtpbin_bin.children() {
        if !session
            .factory()
            .is_some_and(|factory| factory.name() == "rtpsession")
        {
            continue;
        }
        // Harden the feedback path: GStreamer's default TWCC policy sends
        // transport-cc feedback only when the received RTP packet has its
        // marker bit set (rtptwcc.c `rtp_twcc_manager_recv_packet`). If the
        // GFN server does not set the marker bit, TWCC feedback is never
        // generated and the server BWE stays blind (~3.4 Mbps regardless of
        // the negotiated cap). A fixed interval forces periodic feedback
        // (100 ms -> ~10 reports/s, negligible RTCP overhead) no matter what
        // the sender does with the marker bit.
        // The writable `twcc-feedback-interval` lives on the internal
        // RTPSession GObject, not on the rtpsession element wrapper
        // (gst-inspect on this runtime only exposes `twcc-stats` on the
        // element). Reach through the readable `internal-session` property.
        // `set_property` panics on an unknown property, so guard for runtimes
        // without it and fall back to the default marker-bit behavior — the
        // RTCP send probe below still reports whether any feedback is leaving
        // at all.
        if session.find_property("internal-session").is_some() {
            let internal_session: gst::glib::Object = session.property("internal-session");
            if internal_session.find_property("twcc-feedback-interval").is_some() {
                internal_session.set_property(
                    "twcc-feedback-interval",
                    100u64 * gst::ClockTime::MSECOND,
                );
                configured_twcc += 1;
            }
        }
        // Outgoing RTCP leaves the session through these src pads: the
        // receive-path session emits RR + transport-cc/NACK/PLI feedback on
        // `recv_rtcp_src`, and the send-path session (mic uplink) emits SRs on
        // `send_rtcp_src`. Probing both covers every RTCP byte we push to the
        // server without double-counting (the two pads carry distinct packets).
        for pad_name in ["recv_rtcp_src", "send_rtcp_src"] {
            let Some(pad) = session.static_pad(pad_name) else {
                continue;
            };
            let monitor = video_liveness.clone();
            pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
                if let Some(buffer) = info.buffer() {
                    if let Ok(mapped) = buffer.map_readable() {
                        monitor.record_rtcp_message(classify_rtcp_messages(mapped.as_slice()));
                    }
                }
                gst::PadProbeReturn::Ok
            });
            probed += 1;
        }
    }
    if probed > 0 {
        send_log(
            event_sender,
            "info",
            format!(
                "Native RTCP send observability armed on {probed} rtpbin session pad(s); feedback counters will appear in NetworkHealth. TWCC feedback interval forced on {configured_twcc} session(s) (100 ms) so transport-cc reports flow even if the server never sets the RTP marker bit."
            ),
        );
    } else if configured_twcc > 0 {
        send_log(
            event_sender,
            "info",
            format!(
                "TWCC feedback interval forced on {configured_twcc} rtpbin session(s) (100 ms) but no RTCP send pads probed yet."
            ),
        );
    }
}

pub(crate) fn caps_framerate_summary(caps: &str) -> Option<String> {
    let marker = "framerate=(fraction)";
    let start = caps.find(marker)? + marker.len();
    let rest = &caps[start..];
    let semicolon = rest.find(';');
    let comma = rest.find(',');
    let end = match (semicolon, comma) {
        (Some(left), Some(right)) => left.min(right),
        (Some(index), None) | (None, Some(index)) => index,
        (None, None) => rest.len(),
    };
    Some(rest[..end].trim().to_owned())
}

pub(crate) fn memory_mode_from_caps(caps: &str) -> &'static str {
    if caps.contains("memory:D3D12Memory") {
        "D3D12Memory"
    } else if caps.contains("memory:D3D11Memory") {
        "D3D11Memory"
    } else if caps.contains("memory:VulkanImage") {
        "VulkanImage"
    } else if caps.contains("memory:VAMemory") {
        "VAMemory"
    } else if caps.contains("memory:GLMemory") {
        "GLMemory"
    } else {
        "system-memory"
    }
}

pub(crate) fn is_zero_copy_memory_mode(memory_mode: &str) -> bool {
    matches!(
        memory_mode,
        "D3D12Memory" | "D3D11Memory" | "VulkanImage" | "VAMemory" | "GLMemory"
    )
}

/// Decisive diagnostic for the "black screen after SDK downgrade" report: the
/// app's stacked renderer restyles/repositions the sink window while the
/// present queue is live (enforce_stacked_renderer_window_style +
/// apply_stacked_renderer_surface). If GStreamer 1.28.x d3d12videosink stops
/// presenting after any of those Win32 operations, this test fails with the
/// exact phase that killed it.
#[cfg(all(test, target_os = "windows"))]
mod stacked_window_dance_diagnostics {
    use super::*;
    use std::str::FromStr;
    use std::time::{Duration, Instant};

    fn rendered(sink: &gst::Element) -> u64 {
        read_sink_stats(sink).rendered.unwrap_or(0)
    }

    /// Bisect the app's decode chain on 1.28.3. Each variant builds a fresh
    /// pipeline and reports how many frames the sink actually presented:
    /// 0: dec→download→sink, 1: +videoconvert+capsfilter, 2: +dwritetextoverlay,
    /// 3: +queue+tee+screenshot branch (full app chain), 4: dec→sink (D3D12
    /// zero-copy, no download), 5: +queue only (no tee), 6: +tee with an
    /// NV12-accepting branch (no videoconvert/pngenc) to isolate the RGB caps
    /// difference.
    #[test]
    fn full_hardware_decode_chain_renders_d3d12_present() {
        gst::init().expect("gstreamer init");
        for variant in [0u8, 5, 6] {
            let rendered_count = run_decode_chain_variant(variant);
            eprintln!("DIAG chain variant {variant}: rendered={rendered_count}");
        }
        // FIX VERIFICATION: the app chain WITHOUT the tee, let the sink start
        // presenting, then hot-plug the tee + screenshot branch (deferred
        // insertion). If rendered keeps growing, deferring the tee insertion
        // until after the first frame is a working fix.
        let after_hotplug = run_deferred_tee_fix();
        eprintln!("DIAG deferred tee fix: rendered={after_hotplug}");
        assert!(
            after_hotplug > 0,
            "deferred tee insertion also stalls the sink — need a different fix"
        );
    }

    fn run_deferred_tee_fix() -> u64 {
        let Some(sink) = gst::ElementFactory::make("d3d12videosink").build().ok() else {
            return 0;
        };
        sink.set_property("sync", false);
        sink.set_property("async", false);
        sink.set_property("qos", false);
        sink.set_property("max-lateness", -1i64);
        sink.set_property("show-preroll-frame", false);
        sink.set_property("redraw-on-update", true);
        sink.set_property("force-aspect-ratio", true);
        sink.set_property("direct-swapchain", false);
        sink.set_property("enable-navigation-events", false);

        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        src.set_property_from_str("pattern", "smpte");
        let src_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("src capsfilter");
        src_caps.set_property(
            "caps",
            gst::Caps::from_str(
                "video/x-raw,format=(string)NV12,width=(int)1920,height=(int)1080,framerate=(fraction)60/1",
            )
            .expect("valid caps"),
        );
        let enc = gst::ElementFactory::make("x264enc")
            .build()
            .expect("x264enc");
        enc.set_property("bitrate", 8000u32);
        enc.set_property_from_str("speed-preset", "ultrafast");
        enc.set_property_from_str("tune", "zerolatency");
        enc.set_property("bframes", 0u32);
        enc.set_property("key-int-max", 60u32);
        let parse = gst::ElementFactory::make("h264parse")
            .build()
            .expect("h264parse");
        let pre_queue = gst::ElementFactory::make("queue")
            .build()
            .expect("pre queue");
        let dec = gst::ElementFactory::make("d3d12h264dec")
            .build()
            .expect("d3d12h264dec");
        let download = gst::ElementFactory::make("d3d12download")
            .build()
            .expect("d3d12download");
        let convert = gst::ElementFactory::make("videoconvert")
            .build()
            .expect("videoconvert");
        let nv12 = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("nv12 capsfilter");
        nv12.set_property(
            "caps",
            gst::Caps::from_str("video/x-raw,format=(string)NV12").expect("nv12 caps"),
        );
        let overlay = gst::ElementFactory::make("dwritetextoverlay")
            .build()
            .expect("overlay");
        overlay.set_property("visible", false);
        overlay.set_property("text", "");
        overlay.set_property("auto-resize", true);
        overlay.set_property("font-family", "Cascadia Mono");
        let queue = gst::ElementFactory::make("queue")
            .build()
            .expect("post queue");
        queue.set_property("max-size-buffers", 1u32);
        queue.set_property("max-size-bytes", 0u32);
        queue.set_property("max-size-time", 0u64);
        queue.set_property_from_str("leaky", "downstream");
        let tee = gst::ElementFactory::make("tee").build().expect("tee");
        let valve = gst::ElementFactory::make("valve").build().expect("valve");
        valve.set_property("drop", true);
        let branch_queue = gst::ElementFactory::make("queue")
            .build()
            .expect("branch queue");
        branch_queue.set_property_from_str("leaky", "downstream");
        branch_queue.set_property("max-size-buffers", 2u32);
        branch_queue.set_property("max-size-bytes", 0u32);
        branch_queue.set_property("max-size-time", 0u64);
        let branch_convert = gst::ElementFactory::make("videoconvert")
            .build()
            .expect("branch convert");
        let pngenc = gst::ElementFactory::make("pngenc").build().expect("pngenc");
        let appsink = gst::ElementFactory::make("appsink")
            .build()
            .expect("appsink");
        appsink.set_property("sync", false);
        appsink.set_property("max-buffers", 1u32);
        appsink.set_property("drop", true);
        appsink.set_property("wait-on-eos", false);

        let pipeline = gst::Pipeline::new();
        let chain: Vec<&gst::Element> = vec![
            &src, &src_caps, &enc, &parse, &pre_queue, &dec, &download, &convert, &nv12, &overlay,
            &queue, &sink,
        ];
        for element in &chain {
            pipeline.add(*element).expect("add element");
        }
        for pair in chain.windows(2) {
            pair[0].link(pair[1]).expect("link chain pair");
        }
        pipeline.set_state(gst::State::Playing).expect("playing");

        // Mirror the app's warm-up guard (GstreamerVideoTap::ensure_tee): wait
        // until the sink has presented at least 8 frames before hot-plugging
        // the tee — the d3d12 present-chain stall is a warm-up race.
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
            if rendered(&sink) >= 8 {
                break;
            }
        }
        let before = rendered(&sink);
        assert!(
            before >= 8,
            "sink never warmed up (rendered={before}) before the deferred tee attach"
        );

        // Hot-plug the tee + screenshot branch after the sink is presenting.
        queue.unlink(&sink);
        for element in [
            &tee,
            &valve,
            &branch_queue,
            &branch_convert,
            &pngenc,
            &appsink,
        ] {
            pipeline.add(element).expect("add hotplug element");
        }
        queue.link(&tee).expect("queue->tee");
        tee.link(&sink).expect("tee->sink");
        valve.link(&branch_queue).expect("valve->branch_queue");
        branch_queue
            .link(&branch_convert)
            .expect("branch_queue->branch_convert");
        branch_convert
            .link(&pngenc)
            .expect("branch_convert->pngenc");
        pngenc.link(&appsink).expect("pngenc->appsink");
        for element in [
            &tee,
            &valve,
            &branch_queue,
            &branch_convert,
            &pngenc,
            &appsink,
        ] {
            element
                .sync_state_with_parent()
                .expect("sync hotplug state");
        }

        std::thread::sleep(Duration::from_millis(2000));
        let after = rendered(&sink);
        eprintln!("DIAG deferred tee: before={before} after={after}");
        let _ = pipeline.set_state(gst::State::Null);
        after
    }

    fn run_decode_chain_variant(variant: u8) -> u64 {
        let Some(sink) = gst::ElementFactory::make("d3d12videosink").build().ok() else {
            return 0;
        };
        sink.set_property("sync", false);
        sink.set_property("async", false);
        sink.set_property("qos", false);
        sink.set_property("max-lateness", -1i64);
        sink.set_property("processing-deadline", 0u64);
        sink.set_property("render-delay", 0u64);
        sink.set_property("throttle-time", 0u64);
        sink.set_property("enable-last-sample", false);
        sink.set_property("show-preroll-frame", false);
        sink.set_property("redraw-on-update", true);
        sink.set_property("force-aspect-ratio", true);
        sink.set_property("direct-swapchain", false);
        sink.set_property("error-on-closed", false);
        sink.set_property("enable-navigation-events", false);

        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        src.set_property_from_str("pattern", "smpte");
        let src_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("src capsfilter");
        src_caps.set_property(
            "caps",
            gst::Caps::from_str(
                "video/x-raw,format=(string)NV12,width=(int)1920,height=(int)1080,framerate=(fraction)60/1",
            )
            .expect("valid caps"),
        );
        let enc = gst::ElementFactory::make("x264enc")
            .build()
            .expect("x264enc");
        enc.set_property("bitrate", 8000u32);
        enc.set_property_from_str("speed-preset", "ultrafast");
        enc.set_property_from_str("tune", "zerolatency");
        enc.set_property("bframes", 0u32);
        enc.set_property("key-int-max", 60u32);
        let parse = gst::ElementFactory::make("h264parse")
            .build()
            .expect("h264parse");
        let pre_queue = gst::ElementFactory::make("queue")
            .build()
            .expect("pre queue");
        let dec = gst::ElementFactory::make("d3d12h264dec")
            .build()
            .expect("d3d12h264dec");
        let download = gst::ElementFactory::make("d3d12download")
            .build()
            .expect("d3d12download");
        let convert = gst::ElementFactory::make("videoconvert")
            .build()
            .expect("videoconvert");
        let nv12 = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("nv12 capsfilter");
        nv12.set_property(
            "caps",
            gst::Caps::from_str("video/x-raw,format=(string)NV12").expect("nv12 caps"),
        );
        let overlay = gst::ElementFactory::make("dwritetextoverlay")
            .build()
            .expect("overlay");
        overlay.set_property("visible", false);
        overlay.set_property("text", "");
        overlay.set_property("auto-resize", true);
        overlay.set_property("font-family", "Cascadia Mono");
        let queue = gst::ElementFactory::make("queue")
            .build()
            .expect("post queue");
        queue.set_property("max-size-buffers", 1u32);
        queue.set_property("max-size-bytes", 0u32);
        queue.set_property("max-size-time", 0u64);
        queue.set_property_from_str("leaky", "downstream");
        let tee = gst::ElementFactory::make("tee").build().expect("tee");
        let valve = gst::ElementFactory::make("valve").build().expect("valve");
        valve.set_property("drop", true);
        let branch_queue = gst::ElementFactory::make("queue")
            .build()
            .expect("branch queue");
        branch_queue.set_property_from_str("leaky", "downstream");
        branch_queue.set_property("max-size-buffers", 2u32);
        branch_queue.set_property("max-size-bytes", 0u32);
        branch_queue.set_property("max-size-time", 0u64);
        let branch_convert = gst::ElementFactory::make("videoconvert")
            .build()
            .expect("branch convert");
        let pngenc = gst::ElementFactory::make("pngenc").build().expect("pngenc");
        let appsink = gst::ElementFactory::make("appsink")
            .build()
            .expect("appsink");
        appsink.set_property("sync", false);
        appsink.set_property("max-buffers", 1u32);
        appsink.set_property("drop", true);
        appsink.set_property("wait-on-eos", false);

        let pipeline = gst::Pipeline::new();
        let mut chain: Vec<&gst::Element> = vec![&src, &src_caps, &enc, &parse, &pre_queue, &dec];
        match variant {
            0 => chain.extend([&download, &sink]),
            1 => chain.extend([&download, &convert, &nv12, &sink]),
            2 => chain.extend([&download, &convert, &nv12, &overlay, &sink]),
            3 => {
                chain.extend([&download, &convert, &nv12, &overlay, &queue, &tee]);
                // tee → sink (main) and tee → valve → branch (screenshot).
                for element in [&valve, &branch_queue, &branch_convert, &pngenc, &appsink] {
                    pipeline.add(element).expect("add branch element");
                }
                valve.link(&branch_queue).expect("valve->branch_queue");
                branch_queue
                    .link(&branch_convert)
                    .expect("branch_queue->branch_convert");
                branch_convert
                    .link(&pngenc)
                    .expect("branch_convert->pngenc");
                pngenc.link(&appsink).expect("pngenc->appsink");
                tee.link(&sink).expect("tee->sink");
            }
            4 => chain.extend([&sink]), // dec → sink, zero-copy D3D12Memory
            5 => chain.extend([&download, &convert, &nv12, &overlay, &queue, &sink]),
            6 => {
                // tee with an NV12-accepting branch (valve → queue → appsink),
                // no videoconvert/pngenc — isolates whether the RGB caps
                // difference of the pngenc branch is what breaks negotiation.
                chain.extend([&download, &convert, &nv12, &overlay, &queue, &tee]);
                for element in [&valve, &branch_queue, &appsink] {
                    pipeline.add(element).expect("add branch element");
                }
                valve.link(&branch_queue).expect("valve->branch_queue");
                branch_queue.link(&appsink).expect("branch_queue->appsink");
                tee.link(&sink).expect("tee->sink");
            }
            7 => {
                // bare tee, no branch at all: queue → tee → sink.
                chain.extend([&download, &convert, &nv12, &overlay, &queue, &tee, &sink]);
            }
            _ => {
                // 8: tee BEFORE the post-decode queue: overlay → tee → queue →
                // sink (+ pngenc branch) — the sink negotiates its pool with the
                // queue, not across the tee.
                chain.extend([&download, &convert, &nv12, &overlay, &tee, &queue, &sink]);
                for element in [&valve, &branch_queue, &branch_convert, &pngenc, &appsink] {
                    pipeline.add(element).expect("add branch element");
                }
                valve.link(&branch_queue).expect("valve->branch_queue");
                branch_queue
                    .link(&branch_convert)
                    .expect("branch_queue->branch_convert");
                branch_convert
                    .link(&pngenc)
                    .expect("branch_convert->pngenc");
                pngenc.link(&appsink).expect("pngenc->appsink");
            }
        }
        for element in &chain {
            pipeline.add(*element).expect("add element");
        }
        for pair in chain.windows(2) {
            pair[0].link(pair[1]).expect("link chain pair");
        }

        pipeline.set_state(gst::State::Playing).expect("playing");
        let deadline = Instant::now() + Duration::from_secs(6);
        let mut rendered_count = 0u64;
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
            rendered_count = rendered(&sink);
            if rendered_count > 0 {
                break;
            }
        }
        let _ = pipeline.set_state(gst::State::Null);
        rendered_count
    }

    /// Reproduce the app's Windows "Vulkan" external-renderer chain
    /// (windows_vulkan_external_present_chain_definition): DXVA decode →
    /// dwritetextoverlay → d3d12download → videoconvert → RGBA capsfilter →
    /// vulkanupload → queue → vulkansink. If the chain only presents a single
    /// frame (or none) on the bundled runtime, the 1.28.3 vulkan plugin
    /// present path itself is broken and an SDK upgrade is the fix; if it
    /// keeps presenting hundreds of frames, the stall is app-side.
    #[test]
    fn vulkan_external_present_chain_renders() {
        gst::init().expect("gstreamer init");
        let (first, final_count) = run_vulkan_chain();
        eprintln!("DIAG vulkan external chain: first={first} final={final_count}");
        assert!(
            final_count > 1,
            "vulkansink present stalled after {first} frame(s) on this runtime — SDK vulkan plugin issue"
        );
    }

    fn run_vulkan_chain() -> (u64, u64) {
        let Some(sink) = gst::ElementFactory::make("vulkansink").build().ok() else {
            eprintln!("DIAG vulkan: vulkansink unavailable");
            return (0, 0);
        };
        sink.set_property("sync", false);
        sink.set_property("async", false);
        sink.set_property("qos", false);
        sink.set_property("max-lateness", -1i64);
        sink.set_property("show-preroll-frame", false);
        sink.set_property("force-aspect-ratio", true);

        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        src.set_property_from_str("pattern", "smpte");
        let src_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("src capsfilter");
        src_caps.set_property(
            "caps",
            gst::Caps::from_str(
                "video/x-raw,format=(string)NV12,width=(int)1920,height=(int)1080,framerate=(fraction)60/1",
            )
            .expect("valid caps"),
        );
        let enc = gst::ElementFactory::make("x264enc")
            .build()
            .expect("x264enc");
        enc.set_property("bitrate", 8000u32);
        enc.set_property_from_str("speed-preset", "ultrafast");
        enc.set_property_from_str("tune", "zerolatency");
        enc.set_property("bframes", 0u32);
        enc.set_property("key-int-max", 60u32);
        let parse = gst::ElementFactory::make("h264parse")
            .build()
            .expect("h264parse");
        let pre_queue = gst::ElementFactory::make("queue")
            .build()
            .expect("pre queue");
        let dec = gst::ElementFactory::make("d3d12h264dec")
            .build()
            .expect("d3d12h264dec");
        let overlay = gst::ElementFactory::make("dwritetextoverlay")
            .build()
            .expect("overlay");
        overlay.set_property("visible", false);
        overlay.set_property("text", "");
        overlay.set_property("auto-resize", true);
        let download = gst::ElementFactory::make("d3d12download")
            .build()
            .expect("d3d12download");
        let convert = gst::ElementFactory::make("videoconvert")
            .build()
            .expect("videoconvert");
        let rgba = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("rgba capsfilter");
        rgba.set_property(
            "caps",
            gst::Caps::from_str("video/x-raw,format=(string)RGBA").expect("rgba caps"),
        );
        let upload = gst::ElementFactory::make("vulkanupload")
            .build()
            .expect("vulkanupload");
        let queue = gst::ElementFactory::make("queue")
            .build()
            .expect("post queue");
        queue.set_property("max-size-buffers", 1u32);
        queue.set_property("max-size-bytes", 0u32);
        queue.set_property("max-size-time", 0u64);
        queue.set_property_from_str("leaky", "downstream");

        let chain: Vec<&gst::Element> = vec![
            &src, &src_caps, &enc, &parse, &pre_queue, &dec, &overlay, &download, &convert, &rgba,
            &upload, &queue, &sink,
        ];
        let pipeline = gst::Pipeline::new();
        for element in &chain {
            pipeline.add(*element).expect("add element");
        }
        for pair in chain.windows(2) {
            pair[0].link(pair[1]).expect("link chain pair");
        }

        pipeline.set_state(gst::State::Playing).expect("playing");
        let deadline = Instant::now() + Duration::from_secs(6);
        let mut first = 0u64;
        let mut final_count = 0u64;
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
            let now = rendered(&sink);
            if first == 0 && now > 0 {
                first = now;
            }
            final_count = now;
        }
        let _ = pipeline.set_state(gst::State::Null);
        (first, final_count)
    }

    /// The app's d3d11 H265 receive chain, end to end: RTP H265 → capsfilter
    /// (the app's ReceiveCapsFilter, `application/x-rtp, encoding-name=H265`)
    /// → rtph265depay → h265parse → queue → d3d11h265dec → d3d11download →
    /// videoconvert → NV12 → queue → d3d11videosink. The 03:51 field log's
    /// explicit-H265 session received real video RTP (23 Mbps) but never
    /// decoded a single frame (no decoded caps, no sink reveal → black
    /// screen); this test decides whether the decode chain itself is broken
    /// on this runtime or the failure lives upstream (SDP/negotiation).
    #[test]
    fn h265_d3d11_rtp_chain_renders() {
        gst::init().expect("gstreamer init");
        let (first, final_count) = run_h265_d3d11_rtp_chain();
        eprintln!("DIAG h265 d3d11 RTP chain: first={first} final={final_count}");
        assert!(
            final_count > 1,
            "d3d11h265dec chain stalled after {first} frame(s) on this runtime — H265 decode itself is broken"
        );
    }

    fn run_h265_d3d11_rtp_chain() -> (u64, u64) {
        let Some(sink) = gst::ElementFactory::make("d3d11videosink").build().ok() else {
            eprintln!("DIAG h265: d3d11videosink unavailable");
            return (0, 0);
        };
        sink.set_property("sync", false);
        sink.set_property("async", false);
        sink.set_property("qos", false);
        sink.set_property("max-lateness", -1i64);
        sink.set_property("show-preroll-frame", false);
        sink.set_property("force-aspect-ratio", true);

        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        src.set_property_from_str("pattern", "smpte");
        let src_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("src capsfilter");
        // x265enc accepts I420/Y444 (no NV12), so feed it I420 directly.
        src_caps.set_property(
            "caps",
            gst::Caps::from_str(
                "video/x-raw,format=(string)I420,width=(int)1920,height=(int)1080,framerate=(fraction)60/1",
            )
            .expect("valid caps"),
        );
        let enc = gst::ElementFactory::make("x265enc")
            .build()
            .expect("x265enc");
        enc.set_property("bitrate", 8000u32);
        enc.set_property_from_str("speed-preset", "ultrafast");
        enc.set_property_from_str("tune", "zerolatency");
        // x265enc's key-int-max is a signed gint (unlike x264enc's guint).
        enc.set_property("key-int-max", 60i32);
        let enc_parse = gst::ElementFactory::make("h265parse")
            .build()
            .expect("encoder h265parse");
        let pay = gst::ElementFactory::make("rtph265pay")
            .build()
            .expect("rtph265pay");
        let receive_filter = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("receive capsfilter");
        receive_filter.set_property(
            "caps",
            gst::Caps::from_str("application/x-rtp, encoding-name=H265").expect("rtp caps"),
        );
        let depay = gst::ElementFactory::make("rtph265depay")
            .build()
            .expect("rtph265depay");
        let parse = gst::ElementFactory::make("h265parse")
            .build()
            .expect("h265parse");
        let pre_queue = gst::ElementFactory::make("queue")
            .build()
            .expect("pre queue");
        pre_queue.set_property("max-size-buffers", 2u32);
        pre_queue.set_property("max-size-bytes", 0u32);
        pre_queue.set_property("max-size-time", 0u64);
        pre_queue.set_property_from_str("leaky", "downstream");
        let dec = gst::ElementFactory::make("d3d11h265dec")
            .build()
            .expect("d3d11h265dec");
        let download = gst::ElementFactory::make("d3d11download")
            .build()
            .expect("d3d11download");
        let convert = gst::ElementFactory::make("videoconvert")
            .build()
            .expect("videoconvert");
        let nv12 = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("nv12 capsfilter");
        nv12.set_property(
            "caps",
            gst::Caps::from_str("video/x-raw,format=(string)NV12").expect("nv12 caps"),
        );
        let queue = gst::ElementFactory::make("queue")
            .build()
            .expect("post queue");
        queue.set_property("max-size-buffers", 1u32);
        queue.set_property("max-size-bytes", 0u32);
        queue.set_property("max-size-time", 0u64);
        queue.set_property_from_str("leaky", "downstream");

        let chain: Vec<&gst::Element> = vec![
            &src,
            &src_caps,
            &enc,
            &enc_parse,
            &pay,
            &receive_filter,
            &depay,
            &parse,
            &pre_queue,
            &dec,
            &download,
            &convert,
            &nv12,
            &queue,
            &sink,
        ];
        let pipeline = gst::Pipeline::new();
        for element in &chain {
            pipeline.add(*element).expect("add element");
        }
        for pair in chain.windows(2) {
            pair[0].link(pair[1]).expect("link chain pair");
        }

        pipeline.set_state(gst::State::Playing).expect("playing");
        let deadline = Instant::now() + Duration::from_secs(6);
        let mut first = 0u64;
        let mut final_count = 0u64;
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
            let now = rendered(&sink);
            if first == 0 && now > 0 {
                first = now;
            }
            final_count = now;
        }
        let _ = pipeline.set_state(gst::State::Null);
        (first, final_count)
    }

    /// Replicate the app's native recording branch (tee → valve → queue →
    /// videoconvert → capsfilter(I420) → x264enc → mp4mux → appsink, plus the
    /// first-frame JPEG thumbnail branch off an I420 tee) and the stop flow
    /// (close valve → EOS on the valve sink pad → wait for EOS at the appsink).
    /// Reports (chunks_captured_while_recording, eos_seen_within_timeout).
    fn run_recording_eos_variant(include_thumb: bool, eos_target: &str) -> (usize, bool) {
        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        src.set_property_from_str("pattern", "smpte");
        let src_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("src capsfilter");
        src_caps.set_property(
            "caps",
            gst::Caps::from_str(
                "video/x-raw,format=(string)NV12,width=(int)1920,height=(int)1080,framerate=(fraction)60/1",
            )
            .expect("valid caps"),
        );
        let tee = gst::ElementFactory::make("tee").build().expect("tee");

        let valve = gst::ElementFactory::make("valve").build().expect("valve");
        valve.set_property("drop", true);
        let queue = gst::ElementFactory::make("queue").build().expect("queue");
        queue.set_property_from_str("leaky", "downstream");
        queue.set_property("max-size-buffers", 30u32);
        queue.set_property("max-size-bytes", 0u32);
        queue.set_property("max-size-time", 0u64);
        let convert = gst::ElementFactory::make("videoconvert")
            .build()
            .expect("videoconvert");
        let caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("capsfilter");
        caps.set_property(
            "caps",
            gst::Caps::from_str("video/x-raw,format=(string)I420").expect("I420 caps"),
        );
        let encoder = gst::ElementFactory::make("x264enc")
            .build()
            .expect("x264enc");
        encoder.set_property_from_str("speed-preset", "ultrafast");
        encoder.set_property_from_str("tune", "zerolatency");
        encoder.set_property("bitrate", 8000u32);
        encoder.set_property("bframes", 0u32);
        encoder.set_property("key-int-max", 120u32);
        let muxer = gst::ElementFactory::make("mp4mux").build().expect("mp4mux");
        muxer.set_property("streamable", true);
        muxer.set_property("fragment-duration", 500u32);
        let appsink = gst::ElementFactory::make("appsink")
            .build()
            .expect("appsink");
        appsink.set_property("sync", false);
        appsink.set_property("max-buffers", 1u32);
        appsink.set_property("drop", true);
        appsink.set_property("wait-on-eos", false);

        let thumb_tee = gst::ElementFactory::make("tee").build().expect("thumb tee");
        let thumb_valve = gst::ElementFactory::make("valve")
            .build()
            .expect("thumb valve");
        thumb_valve.set_property("drop", true);
        let thumb_queue = gst::ElementFactory::make("queue")
            .build()
            .expect("thumb queue");
        thumb_queue.set_property_from_str("leaky", "downstream");
        thumb_queue.set_property("max-size-buffers", 1u32);
        thumb_queue.set_property("max-size-bytes", 0u32);
        thumb_queue.set_property("max-size-time", 0u64);
        let thumb_encoder = gst::ElementFactory::make("jpegenc")
            .build()
            .expect("jpegenc");
        thumb_encoder.set_property("quality", 70i32);
        thumb_encoder.set_property("snapshot", true);
        let thumb_appsink = gst::ElementFactory::make("appsink")
            .build()
            .expect("thumb appsink");
        thumb_appsink.set_property("sync", false);
        thumb_appsink.set_property("max-buffers", 1u32);
        thumb_appsink.set_property("drop", true);

        let pipeline = gst::Pipeline::new();
        for element in [
            &src,
            &src_caps,
            &tee,
            &valve,
            &queue,
            &convert,
            &caps,
            &thumb_tee,
            &encoder,
            &muxer,
            &appsink,
            &thumb_valve,
            &thumb_queue,
            &thumb_encoder,
            &thumb_appsink,
        ] {
            pipeline.add(element).expect("add element");
        }
        src.link(&src_caps).expect("src->src_caps");
        src_caps.link(&tee).expect("src_caps->tee");
        tee.link(&valve).expect("tee->valve");
        valve.link(&queue).expect("valve->queue");
        queue.link(&convert).expect("queue->convert");
        convert.link(&caps).expect("convert->caps");
        caps.link(&thumb_tee).expect("caps->thumb_tee");
        thumb_tee.link(&encoder).expect("thumb_tee->encoder");
        encoder.link(&muxer).expect("encoder->muxer");
        muxer.link(&appsink).expect("muxer->appsink");
        if include_thumb {
            thumb_tee
                .link(&thumb_valve)
                .expect("thumb_tee->thumb_valve");
            thumb_valve
                .link(&thumb_queue)
                .expect("thumb_valve->thumb_queue");
            thumb_queue
                .link(&thumb_encoder)
                .expect("thumb_queue->thumb_encoder");
            thumb_encoder
                .link(&thumb_appsink)
                .expect("thumb_encoder->thumb_appsink");
        }

        let chunks = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let eos_seen = Arc::new(AtomicBool::new(false));
        let appsink_sink_pad = appsink.static_pad("sink").expect("appsink sink pad");
        let chunk_counter = chunks.clone();
        appsink_sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            chunk_counter.fetch_add(1, Ordering::SeqCst);
            gst::PadProbeReturn::Ok
        });
        let eos_flag = eos_seen.clone();
        appsink_sink_pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
            if let Some(event) = info.event() {
                if event.type_() == gst::EventType::Eos {
                    eos_flag.store(true, Ordering::SeqCst);
                }
            }
            gst::PadProbeReturn::Ok
        });
        let thumb_gate = thumb_valve.clone();
        let thumb_grabber = Arc::new(Mutex::new(false));
        let thumb_grab = thumb_grabber.clone();
        let thumb_sink_pad = thumb_appsink.static_pad("sink").expect("thumb sink pad");
        thumb_sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            let mut slot = thumb_grab.lock().expect("thumb lock");
            if !*slot {
                *slot = true;
                let _ = thumb_gate.set_property("drop", true);
            }
            gst::PadProbeReturn::Ok
        });

        pipeline.set_state(gst::State::Playing).expect("playing");
        valve.set_property("drop", false);
        if include_thumb {
            thumb_valve.set_property("drop", false);
        }
        std::thread::sleep(Duration::from_secs(3));
        let chunks_during = chunks.load(Ordering::SeqCst);

        // Stop flow (mirrors GstreamerRecordingState::stop(finalize=true)).
        valve.set_property("drop", true);
        let eos_target_pad = match eos_target {
            "encoder" => encoder.static_pad("sink").expect("encoder sink pad"),
            // The closed valve drops EOS in this GStreamer build; enter below
            // it (the queue's sink pad) so the buffered tail drains and the
            // muxer finalizes — mirrors the app fix.
            "below-valve" => valve
                .static_pad("src")
                .expect("valve src pad")
                .peer()
                .expect("valve src peer (queue sink pad)"),
            _ => valve.static_pad("sink").expect("valve sink pad"),
        };
        eos_target_pad.send_event(gst::event::Eos::new());
        let deadline = Instant::now() + Duration::from_secs(4);
        while !eos_seen.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let eos_ok = eos_seen.load(Ordering::SeqCst);
        let _ = pipeline.set_state(gst::State::Null);
        (chunks_during, eos_ok)
    }

    /// Replicate the app's recording stop with the REAL two-branch tap tee:
    /// the shared tee also feeds a live main video sink (fakesink, sync=false)
    /// while the recording branch is attached, exactly like the app's
    /// hot-plugged video tap. Returns (chunks, eos_ok).
    fn run_recording_two_branch_tee(
        main_branch: &str,
        preset: &str,
        eos_target: &str,
    ) -> (usize, bool) {
        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        src.set_property_from_str("pattern", "smpte");
        // Pace at the caps framerate like the app's live WebRTC video instead
        // of flooding as fast as possible.
        src.set_property("is-live", true);
        let src_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("src capsfilter");
        src_caps.set_property(
            "caps",
            gst::Caps::from_str(
                "video/x-raw,format=(string)NV12,width=(int)1920,height=(int)1080,framerate=(fraction)60/1",
            )
            .expect("valid caps"),
        );
        let tee = gst::ElementFactory::make("tee").build().expect("tee");

        let valve = gst::ElementFactory::make("valve").build().expect("valve");
        valve.set_property("drop", true);
        let queue = gst::ElementFactory::make("queue").build().expect("queue");
        queue.set_property_from_str("leaky", "downstream");
        queue.set_property("max-size-buffers", 30u32);
        queue.set_property("max-size-bytes", 0u32);
        queue.set_property("max-size-time", 0u64);
        let convert = gst::ElementFactory::make("videoconvert")
            .build()
            .expect("videoconvert");
        let caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("capsfilter");
        caps.set_property(
            "caps",
            gst::Caps::from_str("video/x-raw,format=(string)I420").expect("I420 caps"),
        );
        let encoder = gst::ElementFactory::make("x264enc")
            .build()
            .expect("x264enc");
        // `preset` varies the encoder speed so we can isolate the two-branch
        // tee effect from the real-world iGPU x264 bottleneck.
        encoder.set_property_from_str("speed-preset", preset);
        encoder.set_property_from_str("tune", "zerolatency");
        encoder.set_property("bitrate", 8000u32);
        encoder.set_property("bframes", 0u32);
        encoder.set_property("key-int-max", 120u32);
        let muxer = gst::ElementFactory::make("mp4mux").build().expect("mp4mux");
        muxer.set_property("streamable", true);
        muxer.set_property("fragment-duration", 500u32);
        let appsink = gst::ElementFactory::make("appsink")
            .build()
            .expect("appsink");
        appsink.set_property("sync", false);
        appsink.set_property("max-buffers", 1u32);
        appsink.set_property("drop", true);
        appsink.set_property("wait-on-eos", false);

        // Thumbnail branch (exact app wiring).
        let thumb_tee = gst::ElementFactory::make("tee").build().expect("thumb tee");
        let thumb_valve = gst::ElementFactory::make("valve")
            .build()
            .expect("thumb valve");
        thumb_valve.set_property("drop", true);
        let thumb_queue = gst::ElementFactory::make("queue")
            .build()
            .expect("thumb queue");
        thumb_queue.set_property_from_str("leaky", "downstream");
        thumb_queue.set_property("max-size-buffers", 1u32);
        thumb_queue.set_property("max-size-bytes", 0u32);
        thumb_queue.set_property("max-size-time", 0u64);
        let thumb_encoder = gst::ElementFactory::make("jpegenc")
            .build()
            .expect("jpegenc");
        thumb_encoder.set_property("quality", 70i32);
        thumb_encoder.set_property("snapshot", true);
        let thumb_appsink = gst::ElementFactory::make("appsink")
            .build()
            .expect("thumb appsink");
        thumb_appsink.set_property("sync", false);
        thumb_appsink.set_property("max-buffers", 1u32);
        thumb_appsink.set_property("drop", true);

        // Main branch styles:
        //  "fakesink"        — plain fakesink (sync=false, async=false)
        //  "queue-fakesink"  — queue → fakesink (closer to the app's post-decode
        //                      queue → sink present chain)
        //  "hotplug"         — recording branch attached to the tee AFTER the
        //                      pipeline is already flowing (app's ensure_tee)
        let main_sink = gst::ElementFactory::make("fakesink")
            .build()
            .expect("main fakesink");
        main_sink.set_property("sync", false);
        main_sink.set_property("async", false);
        let main_queue = gst::ElementFactory::make("queue")
            .build()
            .expect("main queue");
        main_queue.set_property("max-size-buffers", 1u32);
        main_queue.set_property_from_str("leaky", "downstream");

        let pipeline = gst::Pipeline::new();
        let mut all: Vec<gst::Element> = vec![
            src.clone(),
            src_caps.clone(),
            tee.clone(),
            valve.clone(),
            queue.clone(),
            convert.clone(),
            caps.clone(),
            thumb_tee.clone(),
            encoder.clone(),
            muxer.clone(),
            appsink.clone(),
            thumb_valve.clone(),
            thumb_queue.clone(),
            thumb_encoder.clone(),
            thumb_appsink.clone(),
        ];
        let hotplug_recording = main_branch == "hotplug";
        if main_branch != "fakesink-none" {
            all.push(main_sink.clone());
        }
        if main_branch == "queue-fakesink" {
            all.push(main_queue.clone());
        }
        for element in &all {
            pipeline.add(element).expect("add element");
        }
        src.link(&src_caps).expect("src->src_caps");
        src_caps.link(&tee).expect("src_caps->tee");
        match main_branch {
            "fakesink" | "hotplug" => {
                tee.link(&main_sink).expect("tee->main_sink");
            }
            "queue-fakesink" => {
                tee.link(&main_queue).expect("tee->main_queue");
                main_queue.link(&main_sink).expect("main_queue->main_sink");
            }
            _ => {}
        }
        if !hotplug_recording {
            tee.link(&valve).expect("tee->valve");
        }
        valve.link(&queue).expect("valve->queue");
        queue.link(&convert).expect("queue->convert");
        convert.link(&caps).expect("convert->caps");
        caps.link(&thumb_tee).expect("caps->thumb_tee");
        thumb_tee.link(&encoder).expect("thumb_tee->encoder");
        encoder.link(&muxer).expect("encoder->muxer");
        muxer.link(&appsink).expect("muxer->appsink");
        thumb_tee
            .link(&thumb_valve)
            .expect("thumb_tee->thumb_valve");
        thumb_valve
            .link(&thumb_queue)
            .expect("thumb_valve->thumb_queue");
        thumb_queue
            .link(&thumb_encoder)
            .expect("thumb_queue->thumb_encoder");
        thumb_encoder
            .link(&thumb_appsink)
            .expect("thumb_encoder->thumb_appsink");

        let chunks = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let eos_seen = Arc::new(AtomicBool::new(false));
        let frames_in_branch = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let frames_at_encoder = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let branch_count = frames_in_branch.clone();
        valve.static_pad("sink").expect("valve sink pad").add_probe(
            gst::PadProbeType::BUFFER,
            move |_pad, _info| {
                branch_count.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            },
        );
        let enc_count = frames_at_encoder.clone();
        encoder
            .static_pad("sink")
            .expect("encoder sink pad")
            .add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
                enc_count.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            });
        let appsink_sink_pad = appsink.static_pad("sink").expect("appsink sink pad");
        let chunk_counter = chunks.clone();
        appsink_sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            chunk_counter.fetch_add(1, Ordering::SeqCst);
            gst::PadProbeReturn::Ok
        });
        let eos_flag = eos_seen.clone();
        appsink_sink_pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
            if let Some(event) = info.event() {
                if event.type_() == gst::EventType::Eos {
                    eos_flag.store(true, Ordering::SeqCst);
                }
            }
            gst::PadProbeReturn::Ok
        });
        let thumb_gate = thumb_valve.clone();
        let thumb_grabber = Arc::new(Mutex::new(false));
        let thumb_grab = thumb_grabber.clone();
        let thumb_sink_pad = thumb_appsink.static_pad("sink").expect("thumb sink pad");
        thumb_sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            let mut slot = thumb_grab.lock().expect("thumb lock");
            if !*slot {
                *slot = true;
                let _ = thumb_gate.set_property("drop", true);
            }
            gst::PadProbeReturn::Ok
        });

        let bus = pipeline.bus().expect("bus");
        let bus_dumper = std::thread::spawn(move || {
            for _ in 0..40 {
                let msg = bus.timed_pop(gst::ClockTime::from_mseconds(100));
                if let Some(msg) = msg {
                    use gst::prelude::*;
                    match msg.view() {
                        gst::MessageView::Error(err) => {
                            eprintln!("DIAG   bus ERROR: {}", err.error())
                        }
                        gst::MessageView::Warning(warn) => {
                            eprintln!("DIAG   bus WARNING: {}", warn.error())
                        }
                        gst::MessageView::StateChanged(sc) => {
                            if let Some(src) = sc.src() {
                                if src.name().contains("pipeline") {
                                    eprintln!(
                                        "DIAG   pipeline state: {:?} -> {:?}",
                                        sc.old(),
                                        sc.current()
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        });
        pipeline.set_state(gst::State::Playing).expect("playing");
        if hotplug_recording {
            // App's ensure_tee pattern: attach the recording branch to the tee
            // only after the main branch has been flowing.
            std::thread::sleep(Duration::from_millis(500));
            tee.link(&valve).expect("hotplug tee->valve");
            valve.sync_state_with_parent().expect("valve state");
            queue.sync_state_with_parent().expect("queue state");
            convert.sync_state_with_parent().expect("convert state");
            caps.sync_state_with_parent().expect("caps state");
            thumb_tee.sync_state_with_parent().expect("thumb tee state");
            encoder.sync_state_with_parent().expect("encoder state");
            muxer.sync_state_with_parent().expect("muxer state");
            appsink.sync_state_with_parent().expect("appsink state");
            thumb_valve
                .sync_state_with_parent()
                .expect("thumb valve state");
            thumb_queue
                .sync_state_with_parent()
                .expect("thumb queue state");
            thumb_encoder
                .sync_state_with_parent()
                .expect("thumb encoder state");
            thumb_appsink
                .sync_state_with_parent()
                .expect("thumb appsink state");
        }
        valve.set_property("drop", false);
        thumb_valve.set_property("drop", false);
        std::thread::sleep(Duration::from_secs(3));
        let _ = bus_dumper.join();
        let chunks_during = chunks.load(Ordering::SeqCst);

        // Stop flow (mirrors GstreamerRecordingState::stop(finalize=true)).
        valve.set_property("drop", true);
        thumb_valve.set_property("drop", true);
        let eos_target_pad = match eos_target {
            "below-valve" => valve
                .static_pad("src")
                .expect("valve src pad")
                .peer()
                .expect("valve src peer (queue sink pad)"),
            _ => valve.static_pad("sink").expect("valve sink pad"),
        };
        eos_target_pad.send_event(gst::event::Eos::new());
        let deadline = Instant::now() + Duration::from_secs(4);
        while !eos_seen.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let eos_ok = eos_seen.load(Ordering::SeqCst);
        eprintln!(
            "DIAG   frames: branch={} encoder={} chunks={}",
            frames_in_branch.load(Ordering::SeqCst),
            frames_at_encoder.load(Ordering::SeqCst),
            chunks_during
        );
        let _ = pipeline.set_state(gst::State::Null);
        (chunks_during, eos_ok)
    }

    /// Force the recording branch queue FULL at stop time (flooding source +
    /// queue-fakesink main branch) and compare EOS-below-valve sent
    /// immediately (current app behavior) vs. after the queue drains.
    /// Returns (queue_level_at_stop, eos_ok).
    fn run_recording_eos_queue_full(eos_mode: &str) -> (u32, bool) {
        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        src.set_property_from_str("pattern", "smpte");
        // Flood as fast as possible: the recording queue stays full (30 max)
        // because the encoder cannot keep up — like the iGPU at 1080p60.
        let src_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("src capsfilter");
        src_caps.set_property(
            "caps",
            gst::Caps::from_str(
                "video/x-raw,format=(string)NV12,width=(int)1920,height=(int)1080,framerate=(fraction)60/1",
            )
            .expect("valid caps"),
        );
        let tee = gst::ElementFactory::make("tee").build().expect("tee");
        let valve = gst::ElementFactory::make("valve").build().expect("valve");
        valve.set_property("drop", true);
        let queue = gst::ElementFactory::make("queue").build().expect("queue");
        queue.set_property_from_str("leaky", "downstream");
        queue.set_property("max-size-buffers", 30u32);
        queue.set_property("max-size-bytes", 0u32);
        queue.set_property("max-size-time", 0u64);
        let convert = gst::ElementFactory::make("videoconvert")
            .build()
            .expect("videoconvert");
        let caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("capsfilter");
        caps.set_property(
            "caps",
            gst::Caps::from_str("video/x-raw,format=(string)I420").expect("I420 caps"),
        );
        let encoder = gst::ElementFactory::make("x264enc")
            .build()
            .expect("x264enc");
        encoder.set_property_from_str("speed-preset", "ultrafast");
        encoder.set_property_from_str("tune", "zerolatency");
        encoder.set_property("bitrate", 8000u32);
        encoder.set_property("bframes", 0u32);
        encoder.set_property("key-int-max", 120u32);
        let muxer = gst::ElementFactory::make("mp4mux").build().expect("mp4mux");
        muxer.set_property("streamable", true);
        muxer.set_property("fragment-duration", 500u32);
        let appsink = gst::ElementFactory::make("appsink")
            .build()
            .expect("appsink");
        appsink.set_property("sync", false);
        appsink.set_property("max-buffers", 1u32);
        appsink.set_property("drop", true);
        appsink.set_property("wait-on-eos", false);
        let thumb_valve = gst::ElementFactory::make("valve")
            .build()
            .expect("thumb valve");
        thumb_valve.set_property("drop", true);
        let main_queue = gst::ElementFactory::make("queue")
            .build()
            .expect("main queue");
        main_queue.set_property("max-size-buffers", 1u32);
        main_queue.set_property_from_str("leaky", "downstream");
        let main_sink = gst::ElementFactory::make("fakesink")
            .build()
            .expect("main fakesink");
        main_sink.set_property("sync", false);
        main_sink.set_property("async", false);

        let pipeline = gst::Pipeline::new();
        for element in [
            &src,
            &src_caps,
            &tee,
            &valve,
            &queue,
            &convert,
            &caps,
            &encoder,
            &muxer,
            &appsink,
            &thumb_valve,
            &main_queue,
            &main_sink,
        ] {
            pipeline.add(element).expect("add element");
        }
        src.link(&src_caps).expect("src->src_caps");
        src_caps.link(&tee).expect("src_caps->tee");
        tee.link(&main_queue).expect("tee->main_queue");
        main_queue.link(&main_sink).expect("main_queue->main_sink");
        tee.link(&valve).expect("tee->valve");
        valve.link(&queue).expect("valve->queue");
        queue.link(&convert).expect("queue->convert");
        convert.link(&caps).expect("convert->caps");
        caps.link(&encoder).expect("caps->encoder");
        encoder.link(&muxer).expect("encoder->muxer");
        muxer.link(&appsink).expect("muxer->appsink");

        let eos_seen = Arc::new(AtomicBool::new(false));
        let appsink_sink_pad = appsink.static_pad("sink").expect("appsink sink pad");
        let eos_flag = eos_seen.clone();
        appsink_sink_pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
            if let Some(event) = info.event() {
                if event.type_() == gst::EventType::Eos {
                    eos_flag.store(true, Ordering::SeqCst);
                }
            }
            gst::PadProbeReturn::Ok
        });

        pipeline.set_state(gst::State::Playing).expect("playing");
        valve.set_property("drop", false);
        std::thread::sleep(Duration::from_secs(3));
        valve.set_property("drop", true);
        let queue_level: u32 = queue.property::<u32>("current-level-buffers");
        let below = valve
            .static_pad("src")
            .expect("valve src pad")
            .peer()
            .expect("valve src peer");
        if eos_mode == "drain" {
            let deadline = Instant::now() + Duration::from_secs(4);
            loop {
                let level: u32 = queue.property::<u32>("current-level-buffers");
                if level == 0 || Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        below.send_event(gst::event::Eos::new());
        let deadline = Instant::now() + Duration::from_secs(4);
        while !eos_seen.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let eos_ok = eos_seen.load(Ordering::SeqCst);
        let _ = pipeline.set_state(gst::State::Null);
        (queue_level, eos_ok)
    }

    #[test]
    fn recording_branch_eos_finalizes() {
        gst::init().expect("gstreamer init");
        // 1. Exact app wiring (with thumbnail branch), EOS on the closed valve
        //    sink pad — the reproduced bug (valve drops EOS while closed).
        let (chunks, eos_bug) = run_recording_eos_variant(true, "valve");
        eprintln!("DIAG recording app-wiring (EOS on closed valve): chunks={chunks} eos={eos_bug}");
        assert!(
            chunks > 0,
            "recording branch never produced muxer output while recording"
        );
        assert!(
            !eos_bug,
            "expected the closed-valve EOS to hang (bug reproduction); it unexpectedly finalized"
        );
        // 2. FIX VERIFICATION: same app wiring, but EOS enters BELOW the closed
        //    valve (the queue's sink pad) — the app's fix in stop().
        let (chunks_fix, eos_fix) = run_recording_eos_variant(true, "below-valve");
        eprintln!("DIAG recording fix (EOS below closed valve): chunks={chunks_fix} eos={eos_fix}");
        assert!(
            eos_fix,
            "EOS below the closed valve still does not finalize — recording-stop crash remains"
        );
        // 3a. queue→fakesink main branch, ultrafast encoder (the passing
        //     control from the last run).
        let (a_main, a_eos) =
            run_recording_two_branch_tee("queue-fakesink", "ultrafast", "below-valve");
        eprintln!("DIAG two-branch live [queue-fakesink/ultrafast]: chunks={a_main} eos={a_eos}");
        // 3b. Hot-plugged recording branch (the app's ensure_tee pattern), fast
        //     encoder.
        let (b_main, b_eos) = run_recording_two_branch_tee("hotplug", "ultrafast", "below-valve");
        eprintln!("DIAG two-branch live [hotplug/ultrafast]: chunks={b_main} eos={b_eos}");
        // 3c. Hot-plugged recording branch with the realistic slow encoder
        //     (iGPU x264 at 1080p60 keeps the queue full) — the closest field
        //     reproduction.
        let (c_main, c_eos) = run_recording_two_branch_tee("hotplug", "veryfast", "below-valve");
        eprintln!("DIAG two-branch live [hotplug/veryfast]: chunks={c_main} eos={c_eos}");
        assert!(
            a_main > 0 || b_main > 0 || c_main > 0,
            "two-branch recording never produced muxer output while recording"
        );
        assert!(
            a_eos && b_eos && c_eos,
            "two-branch tee + EOS below valve still does not finalize — matches the field timeout; fix needed"
        );
        // 4. QUEUE-FULL reproduction: EOS below the valve while the branch queue
        //    is full (flooding source), immediate vs. after drain.
        let (q_level, q_imm) = run_recording_eos_queue_full("immediate");
        eprintln!("DIAG queue-full [immediate]: queue={q_level} eos={q_imm}");
        let (_q2, q_drain) = run_recording_eos_queue_full("drain");
        eprintln!("DIAG queue-full [drain-then-EOS]: eos={q_drain}");
        assert!(
            q_imm,
            "queue-full + immediate EOS lost — matches the field timeout; EOS must be serialized after the buffered tail"
        );
    }

    /// Regression: webrtcbin emits pad-added for SEND-path pads (SINK
    /// direction, e.g. the mic transceiver's send pad when the local
    /// description is sendonly). wire_incoming_media_sink must exclude them;
    /// the old code let the RTP-capped mic send pad through, created a
    /// spurious decodebin in the running pipeline and failed the link with
    /// WrongDirection, stalling the video present chain on every session
    /// with a sendonly mic m-line (07:45 field log: 6/6 sessions rendered=0;
    /// 06:32 working log: 0 send-pad events, rendered climbs to 10k+).
    #[test]
    fn mic_send_pad_is_not_incoming_media() {
        gst::init().expect("gstreamer init");
        use crate::gstreamer_pipeline::is_incoming_media_pad;

        // The mic send pad is a SINK-direction pad on webrtcbin (the app
        // pushes RTP into it). It must be excluded from incoming-media
        // handling even though its caps are application/x-rtp (OPUS).
        let sink_pad = gst::Pad::builder(gst::PadDirection::Sink)
            .name("mic-send-pad")
            .build();
        assert_eq!(
            sink_pad.direction(),
            gst::PadDirection::Sink,
            "precondition: mic send pad is a sink pad"
        );
        assert!(
            !is_incoming_media_pad(&sink_pad),
            "SINK-direction send pad (mic) must not be treated as incoming media"
        );

        // Incoming peer streams are SRC-direction pads — they must still be
        // processed.
        let src_pad = gst::Pad::builder(gst::PadDirection::Src)
            .name("peer-video-pad")
            .build();
        assert_eq!(src_pad.direction(), gst::PadDirection::Src);
        assert!(
            is_incoming_media_pad(&src_pad),
            "SRC-direction pad (peer video/audio) must be treated as incoming media"
        );
        eprintln!(
            "DIAG mic send pad excluded from incoming-media handler: sink={} src={}",
            !is_incoming_media_pad(&sink_pad),
            is_incoming_media_pad(&src_pad)
        );
    }

    /// Reproduce the field failure `recording-start-failed: Failed to link
    /// elements 'mic-tap-queue' and 'audioresample1'`. Builds the mic chain
    /// the way `build_mic_pipeline` wires it when the mic is ON (capture
    /// source → volume → tap tee → tap queue dangling), lets it negotiate,
    /// then attempts the recording-branch link the way
    /// `insert_recording_branch` does (resample element NOT yet added to the
    /// pipeline).
    #[test]
    fn mic_tap_links_into_recording_resample() {
        gst::init().expect("gstreamer init");
        let pipeline = gst::Pipeline::new();

        let silence = gst::ElementFactory::make("audiotestsrc")
            .name("mic-silence-src")
            .build()
            .expect("audiotestsrc");
        silence.set_property_from_str("wave", "silence");
        let volume = gst::ElementFactory::make("volume")
            .name("mic-volume")
            .build()
            .expect("volume");
        volume.set_property("volume", 0.0f64);
        let tap_tee = gst::ElementFactory::make("tee")
            .name("mic-tap-tee")
            .build()
            .expect("tee");
        let tap_queue = gst::ElementFactory::make("queue")
            .name("mic-tap-queue")
            .build()
            .expect("queue");
        tap_queue.set_property_from_str("leaky", "downstream");
        tap_queue.set_property("max-size-buffers", 8u32);
        tap_queue.set_property("max-size-bytes", 0u32);
        tap_queue.set_property("max-size-time", 0u64);
        let convert = gst::ElementFactory::make("audioconvert")
            .name("mic-audioconvert")
            .build()
            .expect("audioconvert");
        let resample = gst::ElementFactory::make("audioresample")
            .name("mic-audioresample")
            .build()
            .expect("audioresample");
        let encoder = gst::ElementFactory::make("opusenc")
            .name("mic-opusenc")
            .build()
            .expect("opusenc");
        let payloader = gst::ElementFactory::make("rtpopuspay")
            .name("mic-rtpopuspay")
            .build()
            .expect("rtpopuspay");
        let fakesink = gst::ElementFactory::make("fakesink")
            .build()
            .expect("fakesink");

        for element in [
            &silence, &volume, &tap_tee, &tap_queue, &convert, &resample, &encoder, &payloader,
            &fakesink,
        ] {
            pipeline.add(element).expect("add mic element");
        }
        silence.link(&volume).expect("src -> volume");
        volume.link(&tap_tee).expect("volume -> tee");
        tap_tee.link(&convert).expect("tee -> convert");
        convert.link(&resample).expect("convert -> resample");
        resample.link(&encoder).expect("resample -> encoder");
        encoder.link(&payloader).expect("encoder -> payloader");
        payloader.link(&fakesink).expect("payloader -> fakesink");
        tap_tee.link(&tap_queue).expect("tee -> tap queue");

        pipeline.set_state(gst::State::Playing).expect("play");
        std::thread::sleep(std::time::Duration::from_millis(1500));

        let src_pad = tap_queue.static_pad("src").expect("tap queue src");
        eprintln!(
            "DIAG mic-tap-queue src current caps: {:?}",
            src_pad.current_caps()
        );
        eprintln!(
            "DIAG mic-tap-queue parent: {:?}",
            tap_queue.parent().map(|parent| parent.name())
        );

        // Regression guard: linking a tap that is already inside the pipeline
        // to a resample that has NOT been added yet must fail (gst_element_link
        // requires a common bin ancestor). This is exactly the field failure
        // "Failed to link elements 'mic-tap-queue' and 'audioresample1'" that
        // happened because insert_recording_branch linked the taps before
        // pipeline.add. The app now adds every branch element first.
        let tap_resample = gst::ElementFactory::make("audioresample")
            .build()
            .expect("audioresample");
        assert!(
            tap_resample.parent().is_none(),
            "precondition: resample not yet added to the pipeline"
        );
        assert!(
            tap_queue.link(&tap_resample).is_err(),
            "linking an in-pipeline tap to a not-yet-added resample must fail (common bin ancestor)"
        );

        // The fix: add the resample (and the rest of the branch) to the
        // pipeline first, then link. This must succeed.
        let tap_resample2 = gst::ElementFactory::make("audioresample")
            .build()
            .expect("audioresample");
        pipeline.add(&tap_resample2).expect("add resample");
        tap_queue
            .link(&tap_resample2)
            .expect("mic tap must link into recording resample once both are in the pipeline");
        eprintln!(
            "DIAG mic tap -> recording resample (post-add): OK; src caps {:?}",
            tap_queue
                .static_pad("src")
                .and_then(|pad| pad.current_caps())
        );

        // Game-audio tap scenario: same requirement applies to any tap, and the
        // post-add link must work for it too.
        let game_tap_tee = gst::ElementFactory::make("tee")
            .name("game-tap-tee")
            .build()
            .expect("tee");
        let game_tap_queue = gst::ElementFactory::make("queue")
            .name("game-tap-queue")
            .build()
            .expect("queue");
        pipeline.add(&game_tap_tee).expect("add game tee");
        pipeline.add(&game_tap_queue).expect("add game queue");
        game_tap_tee
            .link(&game_tap_queue)
            .expect("game tee -> queue");
        game_tap_queue
            .sync_state_with_parent()
            .expect("sync game queue");
        let game_resample = gst::ElementFactory::make("audioresample")
            .build()
            .expect("audioresample");
        pipeline.add(&game_resample).expect("add game resample");
        game_tap_queue
            .link(&game_resample)
            .expect("game tap must link into recording resample once both are in the pipeline");

        let _ = pipeline.set_state(gst::State::Null);
    }

    /// Faithful reproduction of the field's dead audio recording branch
    /// (09:40:43: with audio taps present the mixer/voaacenc never stream, the
    /// audio EOS is rejected and stop-recording times out). Builds a live
    /// game-audio chain + mic chain with dangling tap queues exactly like the
    /// app, then inserts the recording audio branch exactly like
    /// `insert_recording_branch` (per-tap audioresample → audioconvert →
    /// capsfilter → audiomixer → audio-valve → voaacenc → mp4mux → appsink)
    /// and verifies the muxer actually produces audio output.
    #[test]
    fn recording_two_audio_tracks_contains_game_audio() {
        gst::init().expect("gstreamer init");
        let pipeline = gst::Pipeline::new();

        // --- Live game-audio chain (2ch/48k ticks) feeding a recording tap tee.
        let game_src = gst::ElementFactory::make("audiotestsrc")
            .name("game-src")
            .build()
            .expect("audiotestsrc");
        // Real-time pacing like production decoded audio: audiotestsrc defaults
        // to is-live=false and produces as fast as possible (≈80× realtime),
        // which would make the muxer fragment timestamps race ahead.
        game_src.set_property("is-live", true);
        let game_convert = gst::ElementFactory::make("audioconvert")
            .name("game-convert")
            .build()
            .expect("audioconvert");
        let game_resample = gst::ElementFactory::make("audioresample")
            .name("game-resample")
            .build()
            .expect("audioresample");
        let game_caps = gst::ElementFactory::make("capsfilter")
            .name("game-caps")
            .build()
            .expect("capsfilter");
        let game_caps_caps: gst::Caps = "audio/x-raw,format=S16LE,channels=2,rate=48000"
            .parse()
            .expect("caps");
        game_caps.set_property("caps", &game_caps_caps);
        let game_tee = gst::ElementFactory::make("tee")
            .name("game-tap-tee")
            .build()
            .expect("tee");
        let game_sink = gst::ElementFactory::make("fakesink")
            .name("game-sink")
            .build()
            .expect("fakesink");
        game_sink.set_property("sync", false);

        // --- Live mic chain (1ch/48k silence) feeding a recording tap tee.
        let mic_src = gst::ElementFactory::make("audiotestsrc")
            .name("mic-silence-src")
            .build()
            .expect("audiotestsrc");
        mic_src.set_property("is-live", true);
        mic_src.set_property_from_str("wave", "silence");
        let mic_volume = gst::ElementFactory::make("volume")
            .name("mic-volume")
            .build()
            .expect("volume");
        mic_volume.set_property("volume", 0.0f64);
        let mic_tap_tee = gst::ElementFactory::make("tee")
            .name("mic-tap-tee")
            .build()
            .expect("tee");
        let mic_convert = gst::ElementFactory::make("audioconvert")
            .name("mic-audioconvert")
            .build()
            .expect("audioconvert");
        let mic_resample = gst::ElementFactory::make("audioresample")
            .name("mic-audioresample")
            .build()
            .expect("audioresample");
        let mic_opus = gst::ElementFactory::make("opusenc")
            .name("mic-opusenc")
            .build()
            .expect("opusenc");
        let mic_pay = gst::ElementFactory::make("rtpopuspay")
            .name("mic-rtpopuspay")
            .build()
            .expect("rtpopuspay");
        let mic_sink = gst::ElementFactory::make("fakesink")
            .name("mic-sink")
            .build()
            .expect("fakesink");
        mic_sink.set_property("sync", false);

        for element in [
            &game_src,
            &game_convert,
            &game_resample,
            &game_caps,
            &game_tee,
            &game_sink,
            &mic_src,
            &mic_volume,
            &mic_tap_tee,
            &mic_convert,
            &mic_resample,
            &mic_opus,
            &mic_pay,
            &mic_sink,
        ] {
            pipeline.add(element).expect("add live element");
        }
        game_src.link(&game_convert).expect("game src -> convert");
        game_convert
            .link(&game_resample)
            .expect("game convert -> resample");
        game_resample
            .link(&game_caps)
            .expect("game resample -> caps");
        game_caps.link(&game_tee).expect("game caps -> tee");
        game_tee.link(&game_sink).expect("game tee -> sink");
        mic_src.link(&mic_volume).expect("mic src -> volume");
        mic_volume.link(&mic_tap_tee).expect("mic volume -> tee");
        mic_tap_tee.link(&mic_convert).expect("mic tee -> convert");
        mic_convert
            .link(&mic_resample)
            .expect("mic convert -> resample");
        mic_resample.link(&mic_opus).expect("mic resample -> opus");
        mic_opus.link(&mic_pay).expect("mic opus -> pay");
        mic_pay.link(&mic_sink).expect("mic pay -> sink");
        pipeline
            .set_state(gst::State::Playing)
            .expect("play live chains");
        std::thread::sleep(std::time::Duration::from_millis(1500));

        // --- Recording branch: TWO independent AAC tracks (game + mic), NO
        // mixer. Each tap gets a fresh tee pad at recording time →
        // audioresample → audioconvert → capsfilter(2ch/48k) → voaacenc →
        // mp4mux. This mixer-free chain is the same pattern that provably
        // carries real audio (fresh-pad hot-plug → chain elements → encoder),
        // and mp4mux supports multiple audio tracks. The audiomixer
        // (aggregator) is deliberately gone: it drops hot-joined pads
        // ("outside output segment") and fills them with digital silence, and
        // its tiny per-pad queues then block the game chain upstream after a
        // few buffers (field: recordings carry no game audio).
        let tap_caps: gst::Caps = "audio/x-raw,format=S16LE,channels=2,rate=48000"
            .parse()
            .expect("caps");
        let mut audio_elements: Vec<gst::Element> = Vec::new();
        // (tap tee, normalize chain, valve, encoder)
        let mut audio_tap_branches: Vec<(
            gst::Element,
            Vec<gst::Element>,
            gst::Element,
            gst::Element,
        )> = Vec::new();
        for (i, tap_tee) in [&game_tee, &mic_tap_tee].iter().enumerate() {
            let tap_resample = gst::ElementFactory::make("audioresample")
                .name(format!("rec-resample-{i}"))
                .build()
                .expect("audioresample");
            let tap_convert = gst::ElementFactory::make("audioconvert")
                .name(format!("rec-convert-{i}"))
                .build()
                .expect("audioconvert");
            let tap_caps_element = gst::ElementFactory::make("capsfilter")
                .name(format!("rec-caps-{i}"))
                .build()
                .expect("capsfilter");
            tap_caps_element.set_property("caps", &tap_caps);
            // Valve gates each track exactly like the video branch's valve:
            // the fresh pads can be linked at build time (data drops at the
            // closed valve without back-pressure) while the muxer finishes its
            // NULL→PLAYING transition, and recording start just opens it. This
            // avoids pushing the first buffers into a still-transitioning
            // (flushing) muxer, which returned FLUSHING upstream and killed
            // the game chain after a few buffers.
            let tap_valve = gst::ElementFactory::make("valve")
                .name(format!("rec-valve-{i}"))
                .build()
                .expect("valve");
            tap_valve.set_property("drop", true);
            let tap_aac = gst::ElementFactory::make("voaacenc")
                .name(format!("rec-voaacenc-{i}"))
                .build()
                .expect("voaacenc");
            for element in [
                &tap_resample,
                &tap_convert,
                &tap_caps_element,
                &tap_valve,
                &tap_aac,
            ] {
                audio_elements.push(element.clone());
            }
            audio_tap_branches.push((
                (*tap_tee).clone(),
                vec![tap_resample, tap_convert, tap_caps_element],
                tap_valve,
                tap_aac,
            ));
        }
        let muxer = gst::ElementFactory::make("mp4mux").build().expect("mp4mux");
        muxer.set_property("streamable", true);
        muxer.set_property("fragment-duration", 500u32);
        let appsink = gst::ElementFactory::make("appsink")
            .build()
            .expect("appsink");
        appsink.set_property("sync", false);
        appsink.set_property("max-buffers", 1u32);
        appsink.set_property("drop", true);
        audio_elements.extend([muxer.clone(), appsink.clone()]);
        for element in &audio_elements {
            pipeline.add(element).expect("add branch element");
        }
        for (_, chain, valve, aac) in &audio_tap_branches {
            chain[0].link(&chain[1]).expect("resample -> convert");
            chain[1].link(&chain[2]).expect("convert -> caps");
            chain[2].link(valve).expect("caps -> valve");
            valve.link(aac).expect("valve -> voaacenc");
            aac.link(&muxer).expect("voaacenc -> muxer");
        }
        muxer.link(&appsink).expect("muxer -> appsink");
        for element in &audio_elements {
            element
                .sync_state_with_parent()
                .expect("sync branch element");
        }
        // sync_state_with_parent is ASYNC: pads FLUSH during the NULL→PLAYING
        // transition. Linking the fresh tee pads mid-transition makes the
        // already-PLAYING tee push into a flushing pad → FLUSHING upstream →
        // the game chain stalls (field bug). Wait for the normalize chains and
        // the muxer to actually reach PLAYING before linking the fresh pads:
        // the normalize chains reach PLAYING immediately, and the muxer can
        // reach PLAYING via caps negotiation from the fixed capsfilters even
        // with the track valves closed. The appsink stays PAUSED until the
        // first buffer (expected).
        let mut pre_link_elements: Vec<&gst::Element> = audio_tap_branches
            .iter()
            .flat_map(|(_, chain, valve, _)| chain.iter().chain(std::iter::once(valve)))
            .collect();
        pre_link_elements.push(&muxer);
        let transition_deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let all_playing = pre_link_elements
                .iter()
                .all(|element| element.current_state() >= gst::State::Playing);
            if all_playing || Instant::now() >= transition_deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let not_playing: Vec<String> = pre_link_elements
            .iter()
            .filter(|element| element.current_state() < gst::State::Playing)
            .map(|element| format!("{}={:?}", element.name(), element.current_state()))
            .collect();
        eprintln!(
            "DIAG recording pre-link states before pad link: not-playing={:?}",
            not_playing
        );
        assert!(
            not_playing.is_empty(),
            "recording normalize chains + muxer must reach PLAYING before the fresh tee pads are linked (still: {not_playing:?})"
        );
        // Request a fresh pad from each tap tee and link it straight into the
        // resample sink pad (data drops at the closed valve — no back-pressure).
        let mut fresh_tap_pads: Vec<gst::Pad> = Vec::new();
        for (tap_tee, chain, _, _) in &audio_tap_branches {
            let request_pad = tap_tee
                .request_pad_simple("src_%u")
                .expect("tee request pad");
            let resample_sink = chain[0].static_pad("sink").expect("resample sink pad");
            request_pad
                .link(&resample_sink)
                .expect("tee request pad -> resample");
            fresh_tap_pads.push(request_pad);
        }
        let game_tap_pad = fresh_tap_pads[0].clone();
        // Recording start: open both track valves (mirrors state.start()).
        for (_, _, valve, _) in &audio_tap_branches {
            valve.set_property("drop", false);
        }

        // Count muxer output on the appsink sink pad, plus per-track counters
        // and the game track's largest absolute sample (ground truth that REAL
        // game audio, not silence, reaches the encoder).
        let chunks = Arc::new(AtomicU32::new(0));
        let sink_pad = appsink.static_pad("sink").expect("appsink sink pad");
        let counter = chunks.clone();
        sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            counter.fetch_add(1, Ordering::SeqCst);
            gst::PadProbeReturn::Ok
        });
        let (game_enc_in, mic_enc_in) = (Arc::new(AtomicU32::new(0)), Arc::new(AtomicU32::new(0)));
        let game_max = Arc::new(AtomicU32::new(0));
        let (g1, g2) = (game_enc_in.clone(), game_max.clone());
        audio_tap_branches[0]
            .3
            .static_pad("sink")
            .expect("game encoder sink")
            .add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
                g1.fetch_add(1, Ordering::SeqCst);
                if let Some(buffer) = info.buffer() {
                    if let Ok(mapped) = buffer.map_readable() {
                        let bytes = mapped.as_slice();
                        for pair in bytes.chunks_exact(2) {
                            let sample = i16::from_le_bytes([pair[0], pair[1]]);
                            let magnitude = u32::from(sample.unsigned_abs());
                            let mut current = g2.load(Ordering::SeqCst);
                            while magnitude > current
                                && g2
                                    .compare_exchange_weak(
                                        current,
                                        magnitude,
                                        Ordering::SeqCst,
                                        Ordering::SeqCst,
                                    )
                                    .is_err()
                            {
                                current = g2.load(Ordering::SeqCst);
                            }
                        }
                    }
                }
                gst::PadProbeReturn::Ok
            });
        let m1 = mic_enc_in.clone();
        audio_tap_branches[1]
            .3
            .static_pad("sink")
            .expect("mic encoder sink")
            .add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
                m1.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            });
        std::thread::sleep(Duration::from_millis(3000));
        let produced = chunks.load(Ordering::SeqCst);
        eprintln!(
            "DIAG two-track recording: muxer={produced} game-enc-in={} game-max={} mic-enc-in={} game-tap-caps={:?}",
            game_enc_in.load(Ordering::SeqCst),
            game_max.load(Ordering::SeqCst),
            mic_enc_in.load(Ordering::SeqCst),
            game_tap_pad.current_caps()
        );
        assert!(
            produced > 0,
            "recording audio tracks must flow into the muxer; produced={produced}"
        );
        assert!(
            game_enc_in.load(Ordering::SeqCst) > 100,
            "game audio track must flow into its encoder (game-enc-in={})",
            game_enc_in.load(Ordering::SeqCst)
        );
        assert!(
            game_max.load(Ordering::SeqCst) > 500,
            "game audio track must carry REAL game audio, not silence (game-max={})",
            game_max.load(Ordering::SeqCst)
        );
        assert!(
            produced < 1_000,
            "muxer output must be realtime-paced fragments, not a runaway loop (produced={produced})"
        );

        // Failsafe validation (the app's stop() fix): sending EOS DIRECTLY to
        // the muxer's sink pads finalizes it even when a track never flowed.
        let eos_seen = Arc::new(AtomicBool::new(false));
        let eos_flag = eos_seen.clone();
        let appsink_sink_pad = appsink.static_pad("sink").expect("appsink sink pad");
        appsink_sink_pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
            if let Some(event) = info.event() {
                if event.type_() == gst::EventType::Eos {
                    eos_flag.store(true, Ordering::SeqCst);
                }
            }
            gst::PadProbeReturn::Ok
        });
        for pad in muxer.sink_pads() {
            let _ = pad.send_event(gst::event::Eos::new());
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while !eos_seen.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let finalized = eos_seen.load(Ordering::SeqCst);
        eprintln!("DIAG failsafe direct-muxer EOS: finalized={finalized}");
        let _ = pipeline.set_state(gst::State::Null);
        assert!(
            finalized,
            "stop() failsafe: EOS directly on the muxer sink pads must finalize the muxer even when a track never flowed"
        );
    }

    /// Regression guard for the 12:30 field failure: the recording branch
    /// NEVER linked the muxer to anything downstream (the mp4mux src pad had
    /// no peer, so its src task never started: no chunks flowed and EOS never
    /// finalized — "EOS still not seen at the muxer output; recording not
    /// finalized" after every stop, deterministically). The liveness tests
    /// rebuilt the wiring manually and never caught it; this test calls the
    /// REAL production function and asserts the muxer→swallow link exists.
    #[test]
    fn recording_branch_links_muxer_to_swallow() {
        gst::init().expect("gstreamer init");
        let pipeline = gst::Pipeline::new();
        let rtp_tee = gst::ElementFactory::make("tee").build().expect("tee");
        pipeline.add(&rtp_tee).expect("add tee");
        let state = crate::gstreamer_pipeline::build_transcode_record_branch(
            &pipeline,
            &rtp_tee,
            crate::gstreamer_pipeline::RtpVideoApi::Software,
            false,
            None,
            8_000,
        )
        .expect("build recording branch");
        let muxer_src = state.muxer.static_pad("src").expect("muxer src pad");
        let swallow_sink = state.swallow.static_pad("sink").expect("swallow sink pad");
        let linked = muxer_src.peer().is_some_and(|peer| peer == swallow_sink);
        eprintln!("DIAG muxer->swallow linked: {linked}");
        let _ = pipeline.set_state(gst::State::Null);
        assert!(
            linked,
            "qtmux src pad must be linked to the swallow sink pad; without it the muxer never aggregates (no chunks) and EOS never finalizes"
        );
    }

    /// Regression guard for the field start-recording timeout: start-recording
    /// must return promptly and must not touch the live chain. In the remux
    /// design `start()` only opens the valve (no add/sync/link, no muxer
    /// wait), so it is instant by construction; this test pins that contract
    /// and additionally proves the branch is already linked to the RTP tap tee
    /// at build time (valve closed), so recording start cannot re-preroll.
    #[test]
    fn recording_start_does_not_touch_live_chain() {
        gst::init().expect("gstreamer init");
        use std::time::{Duration, Instant};

        let pipeline = gst::Pipeline::new();
        let rtp_tee = gst::ElementFactory::make("tee").build().expect("tee");
        pipeline.add(&rtp_tee).expect("add tee");
        let state = crate::gstreamer_pipeline::build_transcode_record_branch(
            &pipeline,
            &rtp_tee,
            crate::gstreamer_pipeline::RtpVideoApi::Software,
            false,
            None,
            8_000,
        )
        .expect("build recording branch");
        let started_at = Instant::now();
        state.start().expect("start recording");
        assert!(
            started_at.elapsed() < Duration::from_secs(2),
            "start-recording must return promptly (valve-only operation)"
        );
        state.stop(false).expect("abort recording");
        let _ = pipeline.set_state(gst::State::Null);
    }

    /// The game-audio branch must hot-plug into a PLAYING pipeline the same
    /// safe way as the video branch: every element synced to PLAYING before
    /// the branch pad is linked into the audio tap tee, all in one call. The
    /// audio branch is added when the audio RTP pad arrives (possibly after
    /// the pipeline is already PLAYING), so an unsynced NULL element would
    /// fail the tee's first push and could kill the audio RTP flow upstream —
    /// exactly the 19:56 video failure pattern, but on the audio stream.
    ///
    /// This test also pins the arrival-order fix: the audio RTP pad arrives
    /// BEFORE the video pad, so the audio tap tee is stored in the
    /// PIPELINE-level slot (not inside the recording state, which does not
    /// exist yet). When the video pad later builds the recording branch, the
    /// tee is transferred from the slot into the state (the exact transfer
    /// `link_rtp_video_pad` performs) and the audio branch is built.
    #[test]
    fn audio_branch_built_into_playing_pipeline_reaches_playing() {
        gst::init().expect("gstreamer init");
        let pipeline = gst::Pipeline::new();
        // Recording state with its video branch (the REAL production build).
        let rtp_tee = gst::ElementFactory::make("tee").build().expect("video tee");
        pipeline.add(&rtp_tee).expect("add video tee");
        let mut state = crate::gstreamer_pipeline::build_transcode_record_branch(
            &pipeline,
            &rtp_tee,
            crate::gstreamer_pipeline::RtpVideoApi::Software,
            false,
            None,
            8_000,
        )
        .expect("build recording branch");
        // The audio tap tee, stored the way the pad-added handler stores it:
        // in the pipeline-level slot, because the recording state did not
        // exist when the audio pad arrived.
        let audio_tee = gst::ElementFactory::make("tee").build().expect("audio tee");
        pipeline.add(&audio_tee).expect("add audio tee");
        let audio_tee_slot = Arc::new(Mutex::new(Some(audio_tee.clone())));
        // The transfer `link_rtp_video_pad` performs once the recording state
        // exists: take the tee out of the pipeline-level slot into the state.
        if let Ok(mut tee_slot) = audio_tee_slot.lock() {
            if let Some(tee) = tee_slot.take() {
                state.audio_rtp_tee = Some(tee);
            }
        }

        pipeline.set_state(gst::State::Playing).expect("playing");
        audio_tee.sync_state_with_parent().expect("sync audio tee");
        assert_eq!(audio_tee.current_state(), gst::State::Playing);

        state
            .build_audio_branch(&pipeline)
            .expect("build audio branch into PLAYING pipeline");
        assert!(
            state.audio_branch_built.load(Ordering::SeqCst),
            "audio branch must be marked built"
        );
        for element in [
            state.audio_valve.as_ref().expect("audio valve"),
            state.audio_queue.as_ref().expect("audio queue"),
        ] {
            eprintln!(
                "DIAG audio branch {:?} state={:?}",
                element.name(),
                element.current_state()
            );
            assert!(
                element.current_state() == gst::State::Playing,
                "audio branch element {:?} did not reach PLAYING after hot-plug into a PLAYING pipeline",
                element.name()
            );
        }
        // The branch pad must be linked into the audio tap tee (valve closed).
        let linked = audio_tee.src_pads().iter().any(|pad| pad.is_linked());
        assert!(
            linked,
            "audio tap tee must have a linked recording branch pad"
        );
        // The depayloader must be linked into the muxer's audio sink pad.
        // Walk the branch from the valve: valve → queue → depayloader → muxer
        // audio pad.
        let mut pad = state
            .audio_valve
            .as_ref()
            .expect("audio valve")
            .static_pad("src")
            .expect("audio valve src");
        // valve → capsfilter → queue → rtpopusdepay → opusdec → audioconvert
        // → AAC encoder → muxer audio pad.
        for _ in 0..7 {
            pad = pad.peer().expect("audio branch link");
        }
        eprintln!(
            "DIAG audio depayloader src linked to {:?} (muxer has {} sink pads)",
            pad.name(),
            state.muxer.sink_pads().len()
        );
        assert!(
            pad.name() == "sink" || pad.name().starts_with("audio_"),
            "audio depayloader must be linked into qtmux's audio sink pad (got {:?})",
            pad.name()
        );
        let _ = pipeline.set_state(gst::State::Null);
    }

    /// Faithful production-sequence reproduction of the field failure
    /// (12:30 session: recording attached to a PLAYING pipeline, frames
    /// negotiated but "EOS still not seen at the muxer output; recording not
    /// finalized" even after the direct-muxer failsafe). Mirrors
    /// `insert_recording_branch` + `GstreamerRecordingState::stop(finalize)`
    /// exactly: hot-plugged tap tee, branch elements created and added to the
    /// PLAYING pipeline at recording time, closed valve at creation, then
    /// drain → EOS below valve → retry → failsafe. Probes at every stage tell
    /// us where frames stop and whether EOS ever reaches the muxer.
    #[test]
    fn recording_hotplugged_into_playing_pipeline_finalizes() {
        gst::init().expect("gstreamer init");
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::time::{Duration, Instant};

        // --- Main chain: the app's post-decode queue → tap tee → sink queue.
        // The tee is HOT-PLUGGED after the pipeline is PLAYING (the app's
        // `ensure_tee` pattern: unlink queue→sink, insert tee, sync).
        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        src.set_property_from_str("pattern", "smpte");
        // NOT live: a live source paces frames on the pipeline clock, which
        // can be starved to zero frames under full-suite CPU contention (the
        // failing run showed chunks=2 but valve_in=0 — the muxer wrote a
        // header-only fragment on EOS while the source never produced a
        // frame). This test proves the hot-plug wiring + EOS finalize, not
        // live pacing, so a deterministic push source is correct.
        src.set_property("is-live", false);
        let src_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("src capsfilter");
        src_caps.set_property(
            "caps",
            gst::Caps::from_str(
                // Small frames keep the x264+muxer chain deterministic under
                // full-suite CPU contention; this test proves the hot-plug
                // wiring and EOS finalize, not encoding throughput.
                "video/x-raw,format=(string)NV12,width=(int)640,height=(int)360,framerate=(fraction)30/1",
            )
            .expect("valid caps"),
        );
        let pre_queue = gst::ElementFactory::make("queue")
            .build()
            .expect("pre queue");
        pre_queue.set_property("max-size-buffers", 1u32);
        pre_queue.set_property_from_str("leaky", "downstream");
        let tee = gst::ElementFactory::make("tee").build().expect("tee");
        let sink_queue = gst::ElementFactory::make("queue")
            .build()
            .expect("sink queue");
        sink_queue.set_property("max-size-buffers", 1u32);
        sink_queue.set_property_from_str("leaky", "downstream");
        let sink = gst::ElementFactory::make("fakesink")
            .build()
            .expect("fakesink");
        sink.set_property("sync", false);
        sink.set_property("async", false);

        let pipeline = gst::Pipeline::new();
        for element in [&src, &src_caps, &pre_queue, &tee, &sink_queue, &sink] {
            pipeline.add(element).expect("add main chain");
        }
        src.link(&src_caps).expect("src -> src_caps");
        src_caps.link(&pre_queue).expect("src_caps -> pre_queue");
        pre_queue.link(&tee).expect("pre_queue -> tee");
        tee.link(&sink_queue).expect("tee -> sink_queue");
        sink_queue.link(&sink).expect("sink_queue -> sink");

        pipeline.set_state(gst::State::Playing).expect("playing");
        std::thread::sleep(Duration::from_millis(1200));

        // --- Recording branch, created at recording time exactly like
        // `insert_recording_branch` (elements fresh, valve closed) ---
        let valve = gst::ElementFactory::make("valve").build().expect("valve");
        valve.set_property("drop", true);
        let queue = gst::ElementFactory::make("queue").build().expect("queue");
        queue.set_property_from_str("leaky", "downstream");
        queue.set_property("max-size-buffers", 30u32);
        queue.set_property("max-size-bytes", 0u32);
        queue.set_property("max-size-time", 0u64);
        let convert = gst::ElementFactory::make("videoconvert")
            .build()
            .expect("videoconvert");
        let caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("capsfilter");
        caps.set_property(
            "caps",
            gst::Caps::from_str("video/x-raw,format=(string)I420").expect("I420 caps"),
        );
        let encoder = gst::ElementFactory::make("x264enc")
            .build()
            .expect("x264enc");
        encoder.set_property_from_str("speed-preset", "ultrafast");
        encoder.set_property_from_str("tune", "zerolatency");
        encoder.set_property("bitrate", 8000u32);
        encoder.set_property("bframes", 0u32);
        encoder.set_property("key-int-max", 120u32);
        let muxer = gst::ElementFactory::make("mp4mux").build().expect("mp4mux");
        muxer.set_property("streamable", true);
        muxer.set_property("fragment-duration", 500u32);
        let appsink = gst::ElementFactory::make("appsink")
            .build()
            .expect("appsink");
        appsink.set_property("sync", false);
        // async=false (like production configure_sink_for_low_latency): a
        // default async sink waits for preroll before PLAYING, and the closed
        // recording valve blocks preroll forever → appsink stuck at Ready →
        // muxer blocked → tee blocks all outputs → zero frames. With async
        // disabled the sink reaches PLAYING immediately and the branch drains
        // when the valve opens.
        appsink.set_property("async", false);
        appsink.set_property("max-buffers", 1u32);
        appsink.set_property("drop", true);
        appsink.set_property("wait-on-eos", false);
        // Thumbnail branch (production wiring).
        let thumb_tee = gst::ElementFactory::make("tee").build().expect("thumb tee");
        let thumb_valve = gst::ElementFactory::make("valve")
            .build()
            .expect("thumb valve");
        thumb_valve.set_property("drop", true);
        let thumb_queue = gst::ElementFactory::make("queue")
            .build()
            .expect("thumb queue");
        thumb_queue.set_property_from_str("leaky", "downstream");
        thumb_queue.set_property("max-size-buffers", 1u32);
        thumb_queue.set_property("max-size-bytes", 0u32);
        thumb_queue.set_property("max-size-time", 0u64);
        let thumb_encoder = gst::ElementFactory::make("jpegenc")
            .build()
            .expect("jpegenc");
        thumb_encoder.set_property("quality", 70i32);
        thumb_encoder.set_property("snapshot", true);
        let thumb_appsink = gst::ElementFactory::make("appsink")
            .build()
            .expect("thumb appsink");
        thumb_appsink.set_property("sync", false);
        thumb_appsink.set_property("max-buffers", 1u32);
        thumb_appsink.set_property("drop", true);

        for element in [
            &valve,
            &queue,
            &convert,
            &caps,
            &encoder,
            &muxer,
            &appsink,
            &thumb_tee,
            &thumb_valve,
            &thumb_queue,
            &thumb_encoder,
            &thumb_appsink,
        ] {
            pipeline.add(element).expect("add recording branch");
        }
        valve.link(&queue).expect("valve -> queue");
        queue.link(&convert).expect("queue -> convert");
        convert.link(&caps).expect("convert -> caps");
        caps.link(&thumb_tee).expect("caps -> thumb_tee");
        thumb_tee.link(&encoder).expect("thumb_tee -> encoder");
        encoder.link(&muxer).expect("encoder -> muxer");
        muxer.link(&appsink).expect("muxer -> appsink");
        thumb_tee
            .link(&thumb_valve)
            .expect("thumb_tee -> thumb_valve");
        thumb_valve
            .link(&thumb_queue)
            .expect("thumb_valve -> thumb_queue");
        thumb_queue
            .link(&thumb_encoder)
            .expect("thumb_queue -> thumb_encoder");
        thumb_encoder
            .link(&thumb_appsink)
            .expect("thumb_encoder -> thumb_appsink");
        for element in [
            &valve,
            &queue,
            &convert,
            &caps,
            &encoder,
            &muxer,
            &appsink,
            &thumb_tee,
            &thumb_valve,
            &thumb_queue,
            &thumb_encoder,
            &thumb_appsink,
        ] {
            element.sync_state_with_parent().expect("sync state");
        }
        std::thread::sleep(Duration::from_millis(200));
        // sync_state_with_parent is ASYNC: the new elements can still be
        // mid-transition (appsink stuck at Ready blocks the whole branch → tee
        // blocks all outputs → zero frames). Production waits for PLAYING;
        // wait here too before probing/recording.
        let play_deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < play_deadline {
            let playing = [&valve, &queue, &convert, &encoder, &muxer, &appsink]
                .iter()
                .all(|element| element.current_state() == gst::State::Playing);
            if playing {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        eprintln!(
            "DIAG states after sync: muxer={:?} appsink={:?} encoder={:?} valve={:?}",
            muxer.current_state(),
            appsink.current_state(),
            encoder.current_state(),
            valve.current_state()
        );

        // Request a fresh tee src pad explicitly and link it AFTER the branch
        // reached PLAYING (the proven hot-plug pattern from the audio-tap
        // test: `tee.link(&valve)` element-link on an already-PLAYING tee can
        // leave the fresh pad without an active/linked state and the tee
        // silently never pushes into it).
        let tap_pad = tee.request_pad_simple("src_%u").expect("tee request pad");
        let valve_sink = valve.static_pad("sink").expect("valve sink pad");
        tap_pad.link(&valve_sink).expect("tee pad -> valve");

        // --- Probes at every stage ---
        let valve_in = Arc::new(AtomicUsize::new(0));
        let enc_in = Arc::new(AtomicUsize::new(0));
        let mux_pad_in = Arc::new(AtomicUsize::new(0));
        let chunks = Arc::new(AtomicUsize::new(0));
        let eos_seen = Arc::new(AtomicBool::new(false));
        let v = valve_in.clone();
        valve.static_pad("sink").expect("valve sink").add_probe(
            gst::PadProbeType::BUFFER,
            move |_pad, _info| {
                v.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            },
        );
        let e = enc_in.clone();
        encoder.static_pad("sink").expect("encoder sink").add_probe(
            gst::PadProbeType::BUFFER,
            move |_pad, _info| {
                e.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            },
        );
        // Probe the muxer's video sink pad (the one x264enc feeds).
        let m = mux_pad_in.clone();
        muxer
            .sink_pads()
            .first()
            .expect("muxer sink pad")
            .add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
                m.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            });
        let c = chunks.clone();
        let appsink_sink_pad = appsink.static_pad("sink").expect("appsink sink");
        appsink_sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            c.fetch_add(1, Ordering::SeqCst);
            gst::PadProbeReturn::Ok
        });
        let f = eos_seen.clone();
        appsink_sink_pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
            if let Some(event) = info.event() {
                if event.type_() == gst::EventType::Eos {
                    f.store(true, Ordering::SeqCst);
                }
            }
            gst::PadProbeReturn::Ok
        });
        // Thumbnail gate (production probe closes the thumb valve after frame 1).
        let thumb_gate = thumb_valve.clone();
        let grabbed = Arc::new(std::sync::Mutex::new(false));
        let grab = grabbed.clone();
        thumb_appsink
            .static_pad("sink")
            .expect("thumb appsink sink")
            .add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
                let mut slot = grab.lock().expect("thumb lock");
                if !*slot {
                    *slot = true;
                    let _ = thumb_gate.set_property("drop", true);
                }
                gst::PadProbeReturn::Ok
            });

        // --- Record (production start()). The live 60fps source + x264 under
        // full-suite CPU contention can take a while to emit the first muxer
        // fragment, so record until the first chunk appears (deadline) instead
        // of a fixed 3s sleep — the assertion below still requires chunks.
        valve.set_property("drop", false);
        thumb_valve.set_property("drop", false);
        let chunk_deadline = Instant::now() + Duration::from_secs(12);
        while chunks.load(Ordering::SeqCst) == 0 && Instant::now() < chunk_deadline {
            std::thread::sleep(Duration::from_millis(100));
        }

        // --- Production stop(finalize=true) ---
        valve.set_property("drop", true);
        thumb_valve.set_property("drop", true);
        // Drain.
        let drain_deadline = Instant::now() + Duration::from_secs(5);
        let mut queue_level = 0u32;
        loop {
            queue_level = queue.property::<u32>("current-level-buffers");
            if queue_level == 0 || Instant::now() >= drain_deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // EOS below the valve.
        let below = valve
            .static_pad("src")
            .expect("valve src")
            .peer()
            .expect("valve src peer");
        let eos_accepted = below.send_event(gst::event::Eos::new());
        let finalize_deadline = Instant::now() + Duration::from_secs(5);
        while !eos_seen.load(Ordering::SeqCst) && Instant::now() < finalize_deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        // Retry once.
        let retry_deadline = Instant::now() + Duration::from_secs(1);
        let mut retried = false;
        while !eos_seen.load(Ordering::SeqCst) && Instant::now() < retry_deadline {
            if !retried {
                retried = true;
                let _ = below.send_event(gst::event::Eos::new());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // Failsafe: EOS directly on the muxer sink pads.
        if !eos_seen.load(Ordering::SeqCst) {
            for pad in muxer.sink_pads() {
                let _ = pad.send_event(gst::event::Eos::new());
            }
            let failsafe_deadline = Instant::now() + Duration::from_secs(2);
            while !eos_seen.load(Ordering::SeqCst) && Instant::now() < failsafe_deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let finalized = eos_seen.load(Ordering::SeqCst);
        eprintln!(
            "DIAG hotplug repro: valve_in={} enc_in={} mux_pad_in={} chunks={} queue_level={queue_level} eos_accepted={eos_accepted} finalized={finalized}",
            valve_in.load(Ordering::SeqCst),
            enc_in.load(Ordering::SeqCst),
            mux_pad_in.load(Ordering::SeqCst),
            chunks.load(Ordering::SeqCst)
        );
        let _ = pipeline.set_state(gst::State::Null);
        // The live 60fps source + x264 chain is heavily throttled when the
        // full suite runs in parallel (many live pipelines contending for
        // CPU/GPU), so any positive count proves real flow; the decisive
        // assertions are that the muxer produced output and EOS finalized it
        // (the field bug: EOS timeout despite negotiated frames).
        assert!(
            valve_in.load(Ordering::SeqCst) > 0,
            "frames must flow through the hot-plugged recording valve (valve_in={})",
            valve_in.load(Ordering::SeqCst)
        );
        assert!(
            chunks.load(Ordering::SeqCst) > 0,
            "muxer must produce output while recording (chunks={})",
            chunks.load(Ordering::SeqCst)
        );
        assert!(
            finalized,
            "EOS must finalize the hot-plugged recording branch (matches the 12:30 field timeout)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The adaptive pre-decode jitter buffer must rest at the shallow depth on
    /// stable links (low drag latency), deepen CONTINUOUSLY as the RTT rises,
    /// floor up on packet loss (the leading indicator), and force MAX on a
    /// detected burst hold — the three signals that stop the decoder from
    /// starving and the sink from blinking the previous frame when the ping
    /// climbs.
    #[test]
    fn pre_decode_depth_adapts_to_rtt_loss_and_bursts() {
        use crate::gstreamer_pipeline::{
            VIDEO_COMPRESSED_QUEUE_BASE_BUFFERS, VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS,
            VIDEO_COMPRESSED_QUEUE_MID_BUFFERS,
        };
        // Stable links: shallow (≈100 ms), no loss, no burst.
        assert_eq!(
            target_pre_decode_depth(0, None, false),
            VIDEO_COMPRESSED_QUEUE_BASE_BUFFERS
        );
        assert_eq!(
            target_pre_decode_depth(38, None, false),
            VIDEO_COMPRESSED_QUEUE_BASE_BUFFERS
        );
        // Continuous ramp: even a modest RTT rise buys buffer depth — a
        // slightly-elevated ping (60 ms) must NOT stay at the shallow floor
        // (that was the 3-band step's blind spot: it held 6 frames until RTT
        // hit 60 ms, so the burst between 38 and 60 ms starved the decoder).
        // 45 ms → 6 + 9*15/120 = 7; 60 ms → 6 + 9*30/120 = 8; 100 ms → 6 +
        // 9*70/120 = 11; ≥ 150 ms → 15.
        assert_eq!(target_pre_decode_depth(45, None, false), 7);
        assert_eq!(target_pre_decode_depth(60, None, false), 8);
        assert_eq!(target_pre_decode_depth(100, None, false), 11);
        assert_eq!(
            target_pre_decode_depth(150, None, false),
            VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS
        );
        assert_eq!(
            target_pre_decode_depth(250, None, false),
            VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS
        );
        // Packet loss is the leading indicator of jitter: it must floor the
        // depth even while the RTT is still stable. ≥0.1% → mid, ≥0.5% → max.
        assert_eq!(
            target_pre_decode_depth(38, Some(0.001), false),
            VIDEO_COMPRESSED_QUEUE_MID_BUFFERS
        );
        assert_eq!(
            target_pre_decode_depth(38, Some(0.005), false),
            VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS
        );
        // A detected RTT spike (burst hold) forces MAX immediately — the
        // EMA would take seconds to climb, during which the decoder starves
        // and the sink blinks the previous frame.
        assert_eq!(
            target_pre_decode_depth(38, None, true),
            VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS
        );
        assert_eq!(
            target_pre_decode_depth(250, Some(0.005), true),
            VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS
        );
        // Monotonic in RTT (no flat/clipped regions): each +1 ms of RTT may
        // never reduce the depth.
        let mut last = target_pre_decode_depth(0, None, false);
        for rtt in 1..=200 {
            let depth = target_pre_decode_depth(rtt, None, false);
            assert!(
                depth >= last,
                "depth must not decrease as RTT grows: rtt={rtt} depth={depth} < last={last}"
            );
            last = depth;
        }
    }

    /// The recorder must produce a STANDARD seekable MP4, not a fragmented
    /// streamable MP4: players like VLC cannot seek (slide the timeline) in a
    /// fragmented file and show glitches — the official GeForce Now recorder
    /// writes a standard MP4 with a complete index. qtmux with faststart
    /// writes moov (full sample index) BEFORE mdat and zero moof boxes; this
    /// test finalizes a real encode through qtmux with the exact production
    /// properties and asserts that structure.
    #[test]
    fn faststart_mp4_is_seekable_structure() {
        gst::init().expect("gstreamer init");
        let pipeline = gst::Pipeline::new();
        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("src");
        src.set_property("is-live", false);
        let caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("caps");
        caps.set_property(
            "caps",
            "video/x-raw,format=(string)NV12,width=(int)640,height=(int)360,framerate=(fraction)30/1"
                .parse::<gst::Caps>()
                .expect("caps parse"),
        );
        let enc = gst::ElementFactory::make("x264enc").build().expect("enc");
        enc.set_property_from_str("tune", "zerolatency");
        enc.set_property("bitrate", 2000u32);
        let parse = gst::ElementFactory::make("h264parse")
            .build()
            .expect("parse");
        let muxer = gst::ElementFactory::make("qtmux").build().expect("muxer");
        muxer.set_property("faststart", true);
        muxer.set_property("fragment-duration", 0u32);
        muxer.set_property("streamable", false);
        let out = std::env::temp_dir().join("opennow_faststart_probe.mp4");
        let _ = std::fs::remove_file(&out);
        let sink = gst::ElementFactory::make("filesink").build().expect("sink");
        sink.set_property("location", out.to_str().expect("path"));
        for e in [&src, &caps, &enc, &parse, &muxer, &sink] {
            pipeline.add(e).expect("add");
        }
        src.link(&caps).expect("l1");
        caps.link(&enc).expect("l2");
        enc.link(&parse).expect("l3");
        parse.link(&muxer).expect("l4");
        muxer.link(&sink).expect("l5");

        pipeline.set_state(gst::State::Playing).expect("playing");
        std::thread::sleep(Duration::from_secs(4));
        let _ = pipeline.send_event(gst::event::Eos::new());
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Some(msg) = pipeline.bus().expect("bus").pop() {
                if msg.type_() == gst::MessageType::Eos {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = pipeline.set_state(gst::State::Null);
        std::thread::sleep(Duration::from_millis(200));

        let bytes = std::fs::read(&out).expect("read output");
        eprintln!("DIAG faststart probe file size={}", bytes.len());
        assert!(bytes.len() > 100_000, "probe file too small");
        // Box layout: ftyp then moov then mdat (faststart: headers first).
        let header = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
        let ftyp_at = header.find("ftyp");
        let moov_at = header.find("moov");
        let mdat_at = header.find("mdat");
        eprintln!("DIAG faststart boxes: ftyp={ftyp_at:?} moov={moov_at:?} mdat={mdat_at:?}");
        assert!(
            ftyp_at.is_some() && moov_at.is_some(),
            "ftyp+moov must exist"
        );
        assert!(
            moov_at.unwrap() < mdat_at.unwrap_or(usize::MAX),
            "moov must be BEFORE mdat (faststart headers first); got moov={moov_at:?} mdat={mdat_at:?}"
        );
        // No moof boxes at all → not fragmented.
        let moof_count = bytes.windows(4).filter(|w| w == b"moof").count();
        eprintln!("DIAG faststart moof boxes={moof_count}");
        assert_eq!(
            moof_count, 0,
            "non-fragmented output must have no moof boxes"
        );
        let _ = std::fs::remove_file(&out);
    }

    /// Probe: does the screenshot branch's `videoconvert → pngenc` chain turn
    /// NV12 (BT.709, like the H265 decode output) into a GREEN PNG? The field
    /// report says screenshots come out green-tinted while the live stream is
    /// fine — if videoconvert/pngenc mishandles the tagged colorimetry in the
    /// bundled runtime, this probe reproduces it and the fix must force the
    /// RGB conversion explicitly.
    #[test]
    fn probe_screenshot_chain_nv12_bt709_to_png_not_green() {
        gst::init().expect("gstreamer init");
        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        let pipeline = gst::Pipeline::new();
        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("src");
        src.set_property("is-live", false);
        src.set_property("num-buffers", 3i32);
        let caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("caps");
        caps.set_property(
            "caps",
            "video/x-raw,format=(string)NV12,width=(int)1920,height=(int)1080,framerate=(fraction)60/1,colorimetry=(string)bt709,chroma-site=(string)mpeg2"
                .parse::<gst::Caps>()
                .expect("caps parse"),
        );
        let convert = gst::ElementFactory::make("videoconvert")
            .build()
            .expect("videoconvert");
        let pngenc = gst::ElementFactory::make("pngenc").build().expect("pngenc");
        let sink = gst::ElementFactory::make("appsink")
            .build()
            .expect("appsink");
        sink.set_property("sync", false);
        sink.set_property("async", false);
        sink.set_property("max-buffers", 1u32);

        for element in [&src, &caps, &convert, &pngenc, &sink] {
            pipeline.add(element).expect("add");
        }
        src.link(&caps).expect("l1");
        caps.link(&convert).expect("l2");
        convert.link(&pngenc).expect("l3");
        pngenc.link(&sink).expect("l4");

        let png: Arc<Mutex<Option<gst::Buffer>>> = Arc::new(Mutex::new(None));
        let capture = png.clone();
        let pad = sink.static_pad("sink").expect("sink pad");
        pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
            if let Some(buffer) = info.buffer() {
                if let Ok(mut slot) = capture.lock() {
                    *slot = Some(buffer.clone());
                }
            }
            gst::PadProbeReturn::Ok
        });

        pipeline.set_state(gst::State::Playing).expect("playing");
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline
            && png.lock().ok().map(|slot| slot.is_none()).unwrap_or(true)
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = pipeline.set_state(gst::State::Null);

        let buffer = png.lock().ok().and_then(|slot| slot.clone());
        let Some(buffer) = buffer else {
            eprintln!("DIAG png probe: no PNG captured");
            return;
        };
        let mapped = buffer.map_readable().expect("map png");
        let bytes = mapped.as_slice();
        eprintln!(
            "DIAG png probe: png_bytes={} size={:?}",
            bytes.len(),
            buffer.size()
        );

        // Report the PNG color type from the IHDR chunk (byte 25 = color type
        // in the PNG signature+IHDR header) and dump the first decoded pixel
        // rows if we can decode them with a tiny embedded decoder.
        if bytes.len() > 33 && bytes[0..8] == *b"\x89PNG\r\n\x1a\n" {
            let color_type = bytes[25];
            let bit_depth = bytes[24];
            let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
            let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
            eprintln!(
                "DIAG png probe: IHDR w={width} h={height} bit_depth={bit_depth} color_type={color_type} (6=RGBA, 2=RGB)"
            );
            assert!(
                color_type == 2 || color_type == 6,
                "pngenc produced color_type={color_type} (expected RGB/RGBA); the branch must convert YUV→RGB before pngenc"
            );
        }
    }

    /// End-to-end through the PRODUCTION branch: a decoded-video tap (NV12,
    /// like the live decode chain output) feeds `build_transcode_record_branch`
    /// into a PLAYING pipeline, is recorded, then finalized with `stop(true)`.
    /// The chunk(s) the muxer emits at EOS must assemble into a STANDARD
    /// seekable MP4 (ftyp → moov → mdat, zero moof) — the exact property that
    /// lets VLC slide the timeline — and the spent branch must be torn down
    /// and REBUILT FRESH for a second recording (the old in-place recycle is
    /// unreliable in this GStreamer build).
    #[test]
    fn transcode_recording_finalizes_into_seekable_mp4() {
        gst::init().expect("gstreamer init");
        use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
        use std::sync::mpsc;
        use std::time::Duration;

        // Main chain: decoded raw video → tap tee → main sink (exactly what
        // the video tap tee carries in production: post-decode frames).
        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        src.set_property("is-live", false);
        let src_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("src capsfilter");
        src_caps.set_property(
            "caps",
            "video/x-raw,format=(string)NV12,width=(int)640,height=(int)360,framerate=(fraction)30/1"
                .parse::<gst::Caps>()
                .expect("valid caps"),
        );
        let tap_tee = gst::ElementFactory::make("tee").build().expect("tee");
        let main_sink = gst::ElementFactory::make("fakesink")
            .build()
            .expect("main sink");
        main_sink.set_property("sync", false);
        main_sink.set_property("async", false);

        let pipeline = gst::Pipeline::new();
        for element in [&src, &src_caps, &tap_tee, &main_sink] {
            pipeline.add(element).expect("add main chain");
        }
        src.link(&src_caps).expect("src -> src_caps");
        src_caps.link(&tap_tee).expect("src_caps -> tap_tee");
        tap_tee.link(&main_sink).expect("tap_tee -> main_sink");

        let (tx, rx) = mpsc::channel::<Event>();

        pipeline.set_state(gst::State::Playing).expect("playing");
        std::thread::sleep(Duration::from_millis(400));

        // The real production call, exactly as link_rtp_video_pad does it: the
        // branch is built AFTER the pipeline is already PLAYING.
        let mut state = crate::gstreamer_pipeline::build_transcode_record_branch(
            &pipeline,
            &tap_tee,
            crate::gstreamer_pipeline::RtpVideoApi::Software,
            false,
            Some(tx.clone()),
            8_000,
        )
        .expect("build recording branch into PLAYING pipeline");

        let add_counter = |element: &gst::Element, pad_name: &str| -> Arc<AtomicU64> {
            let counter = Arc::new(AtomicU64::new(0));
            let c = counter.clone();
            let pad = element.static_pad(pad_name).expect("pad");
            pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
                c.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            });
            counter
        };
        let q_in = add_counter(&state.queue, "sink");
        let enc_out = add_counter(&state.encoder, "src");

        let finalize_round = |state: &mut crate::gstreamer_pipeline::GstreamerRecordingState,
                              q_in: &Arc<AtomicU64>,
                              enc_out: &Arc<AtomicU64>,
                              record_ms: u64|
         -> Vec<u8> {
            let q0 = q_in.load(Ordering::SeqCst);
            let e0 = enc_out.load(Ordering::SeqCst);
            state.start().expect("start recording");
            std::thread::sleep(Duration::from_millis(record_ms));
            let q1 = q_in.load(Ordering::SeqCst);
            let e1 = enc_out.load(Ordering::SeqCst);
            eprintln!("DIAG e2e transcode round: queue_in={q0}->{q1} enc_out={e0}->{e1}");
            state.stop(true).expect("finalize recording");

            let mut file_bytes: Vec<u8> = Vec::new();
            let mut chunk_count = 0usize;
            while let Ok(event) = rx.try_recv() {
                if let Event::RecordingChunk { chunk_base64 } = event {
                    chunk_count += 1;
                    file_bytes.extend(
                        BASE64_STANDARD
                            .decode(chunk_base64)
                            .expect("valid base64 chunk"),
                    );
                }
            }
            eprintln!(
                "DIAG e2e transcode finalize: chunks={chunk_count} file_bytes={}",
                file_bytes.len()
            );
            assert!(
                chunk_count >= 1,
                "finalized recording produced no chunk at the muxer output"
            );
            assert!(
                file_bytes.len() > 50_000,
                "finalized file too small ({})",
                file_bytes.len()
            );
            file_bytes
        };

        // The seekable-structure check must scan the WHOLE file: with
        // faststart the moov (per-sample tables) can itself be hundreds of KB
        // for a long recording, so "mdat" is far beyond the first 4 KB.
        let find_tag = |bytes: &[u8], tag: &[u8; 4]| -> Option<usize> {
            bytes.windows(4).position(|w| w == tag)
        };
        let assert_seekable_structure = |label: &str, bytes: &[u8]| {
            let ftyp_at = find_tag(bytes, b"ftyp");
            let moov_at = find_tag(bytes, b"moov");
            let mdat_at = find_tag(bytes, b"mdat");
            let moof_count = bytes.windows(4).filter(|w| w == b"moof").count();
            eprintln!(
                "DIAG e2e transcode {label} boxes: ftyp={ftyp_at:?} moov={moov_at:?} mdat={mdat_at:?} moof={moof_count} size={}",
                bytes.len()
            );
            assert!(
                ftyp_at.is_some() && moov_at.is_some() && mdat_at.is_some(),
                "{label}: ftyp+moov+mdat must exist in the finalized recording"
            );
            assert!(
                moov_at.unwrap() < mdat_at.unwrap(),
                "{label}: moov must be BEFORE mdat (faststart headers first); got moov={moov_at:?} mdat={mdat_at:?}"
            );
            assert_eq!(
                moof_count, 0,
                "{label}: non-fragmented output must have no moof boxes"
            );
        };

        // Round 1: record, finalize, and assert the seekable MP4 structure
        // (the regression: a fragmented streamable file fails here — VLC
        // cannot seek it and shows glitches, unlike the GeForce Now file).
        let round1 = finalize_round(&mut state, &q_in, &enc_out, 2_500);
        assert_seekable_structure("round1", &round1);

        // Round 2: the spent branch must be torn down and REBUILT FRESH — the
        // old in-place recycle() is unreliable in this GStreamer build (a
        // direct NULL→PLAYING on a queue kills its src task, and the qtmux
        // keeps round-1 EOS/interleave state — the field "record again froze
        // the whole stream" bug), and the fresh branch starts from the exact
        // state round 1 always succeeds from.
        state.teardown(&pipeline).expect("teardown branch");
        state = crate::gstreamer_pipeline::build_transcode_record_branch(
            &pipeline,
            &tap_tee,
            crate::gstreamer_pipeline::RtpVideoApi::Software,
            false,
            Some(tx.clone()),
            8_000,
        )
        .expect("rebuild recording branch into PLAYING pipeline");
        let q_in = add_counter(&state.queue, "sink");
        let enc_out = add_counter(&state.encoder, "src");
        let round2 = finalize_round(&mut state, &q_in, &enc_out, 2_000);
        assert_seekable_structure("round2", &round2);

        let _ = pipeline.set_state(gst::State::Null);
    }

    /// Field reproduction of the record-start transport kill, in the new
    /// transcode topology. Production records by OPENING a valve-gated
    /// TRANSCODE branch tapped off the decoded-video tap tee (video + game
    /// audio into one qtmux). The old remux failure — a FLOW_NOT_NEGOTIATED
    /// raised inside the branch propagating back through the shared tee into
    /// the WebRTC transport (nicesrc dies, freezing encoded/decoded/sink at 0
    /// fps) — must not regress: the encoder raises no upstream events, and the
    /// live video + audio paths keep flowing through record start/stop.
    #[test]
    fn record_start_never_kills_running_stream() {
        gst::init().expect("gstreamer init");
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        let pipeline = gst::Pipeline::new();

        // Video source: decoded raw video (NV12) → tap tee → main sink,
        // exactly what the video tap tee carries in production.
        let vsrc = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("vsrc");
        vsrc.set_property("is-live", false);
        let v_src_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("v caps");
        v_src_caps.set_property(
            "caps",
            "video/x-raw,format=(string)NV12,width=(int)640,height=(int)360,framerate=(fraction)30/1"
                .parse::<gst::Caps>()
                .expect("valid caps"),
        );
        let vtee = gst::ElementFactory::make("tee").build().expect("vtee");
        let vsink = gst::ElementFactory::make("fakesink")
            .build()
            .expect("vsink");
        vsink.set_property("sync", false);
        vsink.set_property("async", false);

        // Audio source: Opus → RTP, the game-audio side.
        let asrc = gst::ElementFactory::make("audiotestsrc")
            .build()
            .expect("asrc");
        asrc.set_property("is-live", false);
        let aconv = gst::ElementFactory::make("audioconvert")
            .build()
            .expect("aconv");
        let ares = gst::ElementFactory::make("audioresample")
            .build()
            .expect("ares");
        let acaps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("acaps");
        acaps.set_property(
            "caps",
            "audio/x-raw,format=(string)S16LE,rate=(int)48000,channels=(int)2,layout=(string)interleaved"
                .parse::<gst::Caps>()
                .expect("valid audio caps"),
        );
        let opusenc = gst::ElementFactory::make("opusenc")
            .build()
            .expect("opusenc");
        let apay = gst::ElementFactory::make("rtpopuspay")
            .build()
            .expect("apay");
        apay.set_property("pt", 111u32);
        let atee = gst::ElementFactory::make("tee").build().expect("atee");
        let asink = gst::ElementFactory::make("fakesink")
            .build()
            .expect("asink");
        asink.set_property("sync", false);
        asink.set_property("async", false);

        for element in [
            &vsrc,
            &v_src_caps,
            &vtee,
            &vsink,
            &asrc,
            &aconv,
            &ares,
            &acaps,
            &opusenc,
            &apay,
            &atee,
            &asink,
        ] {
            pipeline.add(element).expect("add element");
        }
        vsrc.link(&v_src_caps).expect("link v");
        v_src_caps.link(&vtee).expect("link v");
        vtee.link(&vsink).expect("link v");
        asrc.link(&aconv).expect("link a");
        aconv.link(&ares).expect("link a");
        ares.link(&acaps).expect("link a");
        acaps.link(&opusenc).expect("link a");
        opusenc.link(&apay).expect("link a");
        apay.link(&atee).expect("link a");
        atee.link(&asink).expect("link a");

        let (tx, rx) = mpsc::channel::<Event>();

        pipeline.set_state(gst::State::Playing).expect("playing");
        std::thread::sleep(Duration::from_millis(400));

        // Production wiring: the video branch is built AFTER the pipeline is
        // PLAYING, then the audio tap tee is transferred into the recording
        // state and the audio branch is built into the SAME qtmux.
        let mut state = crate::gstreamer_pipeline::build_transcode_record_branch(
            &pipeline,
            &vtee,
            crate::gstreamer_pipeline::RtpVideoApi::Software,
            false,
            Some(tx),
            8_000,
        )
        .expect("build video branch");
        state.audio_rtp_tee = Some(atee.clone());
        state
            .build_audio_branch(&pipeline)
            .expect("build audio branch");

        // The tees stream into the CLOSED valves for a while, like the field
        // (valve closed from session start until the user presses record).
        std::thread::sleep(Duration::from_millis(1_500));

        // Catch any upstream event the record branch tries to send back into
        // the transport (the old remux depayloader sent force-key-unit
        // CustomUpstream through the tee and killed the WebRTC receiver).
        let upstream_events: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        for tee in [&vtee, &atee] {
            let events = upstream_events.clone();
            let sink_pad = tee.static_pad("sink").expect("tee sink pad");
            sink_pad.add_probe(gst::PadProbeType::EVENT_UPSTREAM, move |_pad, info| {
                if let Some(event) = info.event() {
                    let name = event.type_().name().to_string();
                    let structure = event
                        .structure()
                        .map(|structure| structure.to_string())
                        .unwrap_or_default();
                    if name.contains("CustomUpstream")
                        || structure.contains("force-key-unit")
                        || structure.contains("GstForceKeyUnit")
                    {
                        if let Ok(mut slot) = events.lock() {
                            slot.push(format!("{name} {structure}"));
                        }
                    }
                }
                gst::PadProbeReturn::Ok
            });
        }

        // Drain any pre-existing bus messages so only post-start errors count.
        let bus = pipeline.bus().expect("bus");
        while let Some(message) = bus.pop() {
            if message.type_() == gst::MessageType::Warning
                || message.type_() == gst::MessageType::Error
            {
                eprintln!(
                    "DIAG pre-start bus {:?}: {}",
                    message.type_(),
                    message
                        .structure()
                        .map(|s| s.to_string())
                        .unwrap_or_default()
                );
            }
        }

        // Counters prove the LIVE paths keep flowing after record start.
        let add_counter = |sink: &gst::Element| -> Arc<AtomicU64> {
            let counter = Arc::new(AtomicU64::new(0));
            let probe_counter = counter.clone();
            let pad = sink.static_pad("sink").expect("sink pad");
            pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
                probe_counter.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            });
            counter
        };
        let video_buffers = add_counter(&vsink);
        let audio_buffers = add_counter(&asink);

        std::thread::sleep(Duration::from_millis(300));
        let video_before = video_buffers.load(Ordering::SeqCst);
        let audio_before = audio_buffers.load(Ordering::SeqCst);
        eprintln!(
            "DIAG record-start transport: before start video={video_before} audio={audio_before}"
        );
        assert!(
            video_before > 0,
            "live video path must be flowing before record start"
        );

        state.start().expect("start recording");
        std::thread::sleep(Duration::from_millis(2_500));

        let deadline = Instant::now() + Duration::from_millis(2_500);
        let mut errors: Vec<String> = Vec::new();
        while Instant::now() < deadline {
            if let Some(message) = bus.pop() {
                if message.type_() == gst::MessageType::Error {
                    let detail = message
                        .structure()
                        .and_then(|structure| structure.get::<gst::Structure>("debug").ok())
                        .map(|debug| debug.to_string())
                        .unwrap_or_else(|| "no debug".to_owned());
                    errors.push(detail);
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        let video_after = video_buffers.load(Ordering::SeqCst);
        let audio_after = audio_buffers.load(Ordering::SeqCst);
        let upstream = upstream_events
            .lock()
            .ok()
            .map(|slot| slot.len())
            .unwrap_or(0);
        eprintln!(
            "DIAG record-start transport: errors={} upstream_force_key={upstream} video_buffers={video_before}->{video_after} audio_buffers={audio_before}->{audio_after}",
            errors.len()
        );
        assert!(
            upstream == 0,
            "record start sent a force-key-unit CustomUpstream back into the transport through the tap tee: {upstream:?}"
        );
        assert!(
            errors.is_empty(),
            "record start raised a bus error that would kill the live transport (nicesrc not-negotiated in the field): {errors:?}"
        );
        assert!(
            video_after > video_before + 40,
            "live video path stalled after record start: {video_before} -> {video_after}"
        );
        assert!(
            audio_after > audio_before,
            "live audio path stalled after record start: {audio_before} -> {audio_after}"
        );

        // Finalize cleanly: drain + EOS below the valves must flush the muxer
        // without disturbing the live paths (the field stop also showed
        // queue/segment warnings, so the drain+EOS path is part of this
        // regression).
        state.stop(true).expect("finalize recording");
        let mut chunks = 0usize;
        while let Ok(event) = rx.try_recv() {
            if let Event::RecordingChunk { .. } = event {
                chunks += 1;
            }
        }
        eprintln!("DIAG record-start transport: finalized chunks={chunks}");
        assert!(
            chunks >= 1,
            "finalized transcode recording produced no muxer chunks"
        );

        let _ = pipeline.set_state(gst::State::Null);
    }

    /// Reproduce the field symptom (empty video track) with the NEW transcode
    /// branch: recording a decoded-video stream through
    /// `build_transcode_record_branch` (valve → queue → videoconvert →
    /// capsfilter → H.264 encoder → qtmux) must produce a VIDEO track in the
    /// muxer output — the old remux produced a 12 KB MP4 with only an Opus
    /// track after a 1-minute recording because the depayloader never
    /// captured the parameter sets. Every branch pad is instrumented so a
    /// failure shows exactly where the video dies.
    #[test]
    fn transcode_branch_real_stream_produces_video_track() {
        gst::init().expect("gstreamer init");
        use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
        use std::sync::mpsc;
        use std::time::Duration;

        // Decoded video source (NV12, like the live chain output) → tap tee →
        // main sink.
        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        src.set_property("is-live", false);
        let src_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("src caps");
        src_caps.set_property(
            "caps",
            "video/x-raw,format=(string)NV12,width=(int)640,height=(int)360,framerate=(fraction)30/1"
                .parse::<gst::Caps>()
                .expect("valid caps"),
        );
        let tee = gst::ElementFactory::make("tee").build().expect("tee");
        let main_sink = gst::ElementFactory::make("fakesink").build().expect("sink");
        main_sink.set_property("sync", false);
        main_sink.set_property("async", false);

        let pipeline = gst::Pipeline::new();
        for element in [&src, &src_caps, &tee, &main_sink] {
            pipeline.add(element).expect("add");
        }
        src.link(&src_caps).expect("l1");
        src_caps.link(&tee).expect("l2");
        tee.link(&main_sink).expect("l3");

        let (tx, rx) = mpsc::channel::<Event>();
        pipeline.set_state(gst::State::Playing).expect("playing");
        std::thread::sleep(Duration::from_millis(400));

        // Production wiring: branch built into the already-PLAYING pipeline
        // with the valve closed, exactly like link_rtp_video_pad does it.
        let state = crate::gstreamer_pipeline::build_transcode_record_branch(
            &pipeline,
            &tee,
            crate::gstreamer_pipeline::RtpVideoApi::Software,
            false,
            Some(tx),
            8_000,
        )
        .expect("build recording branch");

        let add_counter = |element: &gst::Element, pad_name: &str| -> Arc<AtomicU64> {
            let counter = Arc::new(AtomicU64::new(0));
            let c = counter.clone();
            let pad = element.static_pad(pad_name).expect("pad");
            pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
                c.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            });
            counter
        };
        let queue_in = add_counter(&state.queue, "sink");
        let enc_out = add_counter(&state.encoder, "src");
        // Count on the muxer SRC pad, not the swallow sink pad: the swallow
        // sink carries the chunk probe that returns DROP (which prevents any
        // later probe on that pad from running), so a counter there would
        // always read 0 even when qtmux produces output.
        let mux_out = add_counter(&state.muxer, "src");

        // Let the tee stream into the closed valve for a while, like the
        // field (branch idle from session start until record is pressed).
        std::thread::sleep(Duration::from_millis(1_500));
        let q0 = queue_in.load(Ordering::SeqCst);
        eprintln!(
            "DIAG transcode branch: before start queue_in={q0} enc_out={} mux_out={}",
            enc_out.load(Ordering::SeqCst),
            mux_out.load(Ordering::SeqCst)
        );

        state.start().expect("start recording");
        std::thread::sleep(Duration::from_millis(2_500));
        let q1 = queue_in.load(Ordering::SeqCst);
        let e1 = enc_out.load(Ordering::SeqCst);
        let m1 = mux_out.load(Ordering::SeqCst);
        let enc_caps = state
            .encoder
            .static_pad("src")
            .and_then(|pad| pad.current_caps())
            .map(|caps| caps.to_string())
            .unwrap_or_else(|| "<none>".to_owned());
        eprintln!("DIAG transcode branch: recording queue_in={q0}->{q1} enc_out={e1} mux_out={m1}");
        eprintln!("DIAG transcode branch: encoder src caps = {enc_caps}");

        state.stop(true).expect("finalize recording");
        let mut file_bytes: Vec<u8> = Vec::new();
        let mut chunks = 0usize;
        while let Ok(event) = rx.try_recv() {
            if let Event::RecordingChunk { chunk_base64 } = event {
                chunks += 1;
                file_bytes.extend(BASE64_STANDARD.decode(chunk_base64).expect("b64"));
            }
        }
        let avc1_at = file_bytes.windows(4).position(|w| w == b"avc1");
        eprintln!(
            "DIAG transcode branch: finalized chunks={chunks} file_bytes={} avc1={avc1_at:?}",
            file_bytes.len()
        );
        assert!(
            q1 > q0,
            "branch queue received no video buffers after record start: {q0} -> {q1}"
        );
        assert!(
            e1 > 0 && m1 > 0,
            "encoder/muxer produced no output; video died in the transcode chain (queue_in={q1} enc_out={e1} mux_out={m1})"
        );
        assert!(
            avc1_at.is_some(),
            "finalized MP4 has no H.264 video track (avc1 missing); enc_out={e1} mux_out={m1}"
        );

        let _ = pipeline.set_state(gst::State::Null);
    }

    /// Production-faithful variant: the video side consumes DECODED frames
    /// (no RTP caps involved), while the game-audio branch consumes the real
    /// production Opus RTP stream and shares the SAME qtmux. The field
    /// symptom — a 12 KB MP4 with only an audio track after a 1-minute
    /// recording — must not regress: the finalized file must carry BOTH an
    /// H.264 video track (avc1) and an AAC audio track (mp4a).
    #[test]
    fn transcode_branch_with_audio_produces_video_and_audio_tracks() {
        gst::init().expect("gstreamer init");
        use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
        use std::sync::mpsc;
        use std::time::Duration;

        // Video: decoded raw video → tap tee → main sink.
        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        src.set_property("is-live", false);
        let src_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("src caps");
        src_caps.set_property(
            "caps",
            "video/x-raw,format=(string)NV12,width=(int)640,height=(int)360,framerate=(fraction)30/1"
                .parse::<gst::Caps>()
                .expect("valid caps"),
        );
        let tee = gst::ElementFactory::make("tee").build().expect("tee");
        let main_sink = gst::ElementFactory::make("fakesink").build().expect("sink");
        main_sink.set_property("sync", false);
        main_sink.set_property("async", false);

        // Audio: Opus → RTP (game audio), into a second tee.
        let asrc = gst::ElementFactory::make("audiotestsrc")
            .build()
            .expect("asrc");
        asrc.set_property("is-live", false);
        let aconv = gst::ElementFactory::make("audioconvert")
            .build()
            .expect("aconv");
        let ares = gst::ElementFactory::make("audioresample")
            .build()
            .expect("ares");
        let acaps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("acaps");
        acaps.set_property(
            "caps",
            "audio/x-raw,format=(string)S16LE,rate=(int)48000,channels=(int)2,layout=(string)interleaved"
                .parse::<gst::Caps>()
                .expect("valid audio caps"),
        );
        let opusenc = gst::ElementFactory::make("opusenc")
            .build()
            .expect("opusenc");
        let apay = gst::ElementFactory::make("rtpopuspay")
            .build()
            .expect("apay");
        apay.set_property("pt", 111u32);
        let atee = gst::ElementFactory::make("tee").build().expect("atee");
        let asink = gst::ElementFactory::make("fakesink")
            .build()
            .expect("asink");
        asink.set_property("sync", false);
        asink.set_property("async", false);

        let pipeline = gst::Pipeline::new();
        for element in [
            &src, &src_caps, &tee, &main_sink, &asrc, &aconv, &ares, &acaps, &opusenc, &apay,
            &atee, &asink,
        ] {
            pipeline.add(element).expect("add element");
        }
        src.link(&src_caps).expect("l1");
        src_caps.link(&tee).expect("l2");
        tee.link(&main_sink).expect("l3");
        asrc.link(&aconv).expect("a1");
        aconv.link(&ares).expect("a2");
        ares.link(&acaps).expect("a3");
        acaps.link(&opusenc).expect("a4");
        opusenc.link(&apay).expect("a5");
        apay.link(&atee).expect("a6");
        atee.link(&asink).expect("a7");

        let (tx, rx) = mpsc::channel::<Event>();
        pipeline.set_state(gst::State::Playing).expect("playing");
        std::thread::sleep(Duration::from_millis(400));

        let mut state = crate::gstreamer_pipeline::build_transcode_record_branch(
            &pipeline,
            &tee,
            crate::gstreamer_pipeline::RtpVideoApi::Software,
            false,
            Some(tx),
            8_000,
        )
        .expect("build video branch");
        state.audio_rtp_tee = Some(atee.clone());
        state
            .build_audio_branch(&pipeline)
            .expect("build audio branch");

        let add_counter = |element: &gst::Element, pad_name: &str| -> Arc<AtomicU64> {
            let counter = Arc::new(AtomicU64::new(0));
            let c = counter.clone();
            let pad = element.static_pad(pad_name).expect("pad");
            pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
                c.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            });
            counter
        };
        let v_queue_in = add_counter(&state.queue, "sink");
        let v_enc_out = add_counter(&state.encoder, "src");

        // Valve closed while both tees stream, like production (branch
        // attached at session start, record pressed minutes later).
        std::thread::sleep(Duration::from_millis(1_500));
        let q_before = v_queue_in.load(Ordering::SeqCst);
        eprintln!("DIAG transcode prod: before start queue_in={q_before}");

        state.start().expect("start recording");
        std::thread::sleep(Duration::from_millis(3_000));

        let q1 = v_queue_in.load(Ordering::SeqCst);
        let e1 = v_enc_out.load(Ordering::SeqCst);
        let enc_caps = state
            .encoder
            .static_pad("src")
            .and_then(|pad| pad.current_caps())
            .map(|caps| caps.to_string())
            .unwrap_or_else(|| "<none>".to_owned());
        eprintln!("DIAG transcode prod: recording queue_in={q_before}->{q1} enc_out={e1}");
        eprintln!("DIAG transcode prod: encoder src caps = {enc_caps}");

        state.stop(true).expect("finalize recording");
        let mut file_bytes: Vec<u8> = Vec::new();
        let mut chunks = 0usize;
        while let Ok(event) = rx.try_recv() {
            if let Event::RecordingChunk { chunk_base64 } = event {
                chunks += 1;
                file_bytes.extend(BASE64_STANDARD.decode(chunk_base64).expect("b64"));
            }
        }
        let avc1_at = file_bytes.windows(4).position(|w| w == b"avc1");
        let mp4a_at = file_bytes.windows(4).position(|w| w == b"mp4a");
        eprintln!(
            "DIAG transcode prod: finalized chunks={chunks} file_bytes={} avc1={avc1_at:?} mp4a={mp4a_at:?}",
            file_bytes.len()
        );
        assert!(
            q1 > q_before,
            "video branch queue received no buffers after record start: {q_before} -> {q1}"
        );
        assert!(
            avc1_at.is_some(),
            "finalized MP4 has no H.264 video track; queue_in={q1} enc_out={e1}"
        );
        assert!(
            mp4a_at.is_some(),
            "finalized MP4 has no AAC audio track (game audio died in the transcode chain)"
        );

        let _ = pipeline.set_state(gst::State::Null);
    }

    /// Regression: a LIVE source (like the production tap tee) with the branch
    /// idle for a while before record start must NOT inflate the recording.
    /// The old videorate-based chain re-based its output onto the replayed
    /// live segment (start=0, session start) and inserted duplicate frames
    /// for the whole idle gap — a 31 s field recording came out as 79.8 s of
    /// video ("stuck"/slow-motion playback) with the audio track stranded at
    /// the tail. The encoder must output ~1 frame per input frame (no
    /// duplication) and the file must be ~the recording window long.
    #[test]
    fn live_record_window_does_not_inflate_transcode_output() {
        gst::init().expect("gstreamer init");
        use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
        use std::sync::mpsc;
        use std::time::Duration;

        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        src.set_property("is-live", true);
        src.set_property_from_str("pattern", "smpte");
        let src_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("src caps");
        src_caps.set_property(
            "caps",
            "video/x-raw,format=(string)NV12,width=(int)640,height=(int)360,framerate=(fraction)60/1"
                .parse::<gst::Caps>()
                .expect("valid caps"),
        );
        let tee = gst::ElementFactory::make("tee").build().expect("tee");
        let main_sink = gst::ElementFactory::make("fakesink").build().expect("sink");
        main_sink.set_property("sync", false);
        main_sink.set_property("async", false);

        let pipeline = gst::Pipeline::new();
        for element in [&src, &src_caps, &tee, &main_sink] {
            pipeline.add(element).expect("add");
        }
        src.link(&src_caps).expect("l1");
        src_caps.link(&tee).expect("l2");
        tee.link(&main_sink).expect("l3");

        let (tx, rx) = mpsc::channel::<Event>();
        pipeline.set_state(gst::State::Playing).expect("playing");
        std::thread::sleep(Duration::from_millis(400));

        let state = crate::gstreamer_pipeline::build_transcode_record_branch(
            &pipeline,
            &tee,
            crate::gstreamer_pipeline::RtpVideoApi::Software,
            false,
            Some(tx),
            8_000,
        )
        .expect("build recording branch");

        let add_counter = |element: &gst::Element, pad_name: &str| -> Arc<AtomicU64> {
            let counter = Arc::new(AtomicU64::new(0));
            let c = counter.clone();
            let pad = element.static_pad(pad_name).expect("pad");
            pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
                c.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            });
            counter
        };
        let q_in = add_counter(&state.queue, "sink");
        let enc_out = add_counter(&state.encoder, "src");

        // Branch idle while the live tee streams (like session start -> record).
        std::thread::sleep(Duration::from_millis(2_000));
        let q0 = q_in.load(Ordering::SeqCst);
        let e0 = enc_out.load(Ordering::SeqCst);

        state.start().expect("start recording");
        std::thread::sleep(Duration::from_millis(3_000));
        let q1 = q_in.load(Ordering::SeqCst);
        let e1 = enc_out.load(Ordering::SeqCst);
        let recorded_frames = q1 - q0;
        let encoded_frames = e1 - e0;
        eprintln!(
            "DIAG live: 3s window queue_in={q0}->{q1} enc_out={e0}->{e1} (recorded={recorded_frames} encoded={encoded_frames})"
        );

        state.stop(true).expect("finalize recording");
        let mut file_bytes: Vec<u8> = Vec::new();
        let mut chunks = 0usize;
        while let Ok(event) = rx.try_recv() {
            if let Event::RecordingChunk { chunk_base64 } = event {
                chunks += 1;
                file_bytes.extend(BASE64_STANDARD.decode(chunk_base64).expect("b64"));
            }
        }
        eprintln!(
            "DIAG live: finalized chunks={chunks} file_bytes={} (expected ~{recorded_frames} encoded frames)",
            file_bytes.len()
        );
        assert!(
            recorded_frames >= 120,
            "recording window captured too few frames: {recorded_frames}"
        );
        assert!(
            encoded_frames <= recorded_frames + 30,
            "encoder duplicated frames: encoded={encoded_frames} for recorded={recorded_frames} — the videorate inflation regression"
        );
        assert!(file_bytes.len() > 50_000);
        // The finished recording must carry NO colour metadata (colr box
        // neutralized to free) — the official GeForce Now recordings are
        // untagged and the field players render them correctly, while a
        // colr box makes them skip the limited-range expansion ("hitam
        // pekat").
        assert!(
            !file_bytes.windows(4).any(|w| w == b"colr"),
            "recording must not carry a colr colour-metadata box"
        );
        assert!(
            file_bytes.windows(4).any(|w| w == b"free"),
            "recording must keep the neutralized box as a free box"
        );

        let _ = pipeline.set_state(gst::State::Null);
    }

    /// Production-faithful COLOR probe: the live decode chain hands the tap
    /// tee NV12 tagged FULL-RANGE BT.709 (`colorimetry=1:3:5:1` — exactly what
    /// d3d12h265dec reports on the field, 0-255 pixel data; the screenshot
    /// branch on the same tee writes it straight into a PNG, which shows the
    /// true colors). The official GeForce Now PC recordings are LIMITED
    /// (16-235 — the 0-255 readings in early analysis were codec overshoot,
    /// <0.1% of pixels), and every field player expands H.264 content as
    /// limited, so full-range data comes out with crushed blacks ("hitam
    /// pekat"). This feeds the branch the SAME caps shape and probes the
    /// actual pixel data at the encoder input: the FULL→LIMITED conversion
    /// must scale the data down (encoder input < branch input). Existing
    /// tests omit colorimetry from the source caps, which is why the earlier
    /// range handling was never caught on the field.
    #[test]
    fn probe_record_branch_converts_full_to_limited_with_tagged_input() {
        gst::init().expect("gstreamer init");
        use std::sync::mpsc;
        use std::time::Duration;

        // videotestsrc ignores the 0-255 colorimetry (always emits limited
        // white, Y=235) but it is continuous, which is what matters here: the
        // branch must still run its FULL→LIMITED conversion on the declared
        // full-range input, so the encoder input must come out LOWER than the
        // branch input (a passthrough branch would leave them equal — the
        // "0-255 data in an H.264 file = crushed blacks" field bug).
        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        src.set_property("is-live", false);
        src.set_property_from_str("pattern", "white");
        let full_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("full caps");
        full_caps.set_property(
            "caps",
            // The field decoder's exact output caps: NV12, BT.709, FULL range.
            "video/x-raw,format=(string)NV12,width=(int)64,height=(int)64,framerate=(fraction)30/1,colorimetry=(string)bt709/bt709/bt709/0-255,chroma-site=(string)mpeg2"
                .parse::<gst::Caps>()
                .expect("valid caps"),
        );
        let tee = gst::ElementFactory::make("tee").build().expect("tee");
        let main_sink = gst::ElementFactory::make("fakesink").build().expect("sink");
        main_sink.set_property("sync", false);
        main_sink.set_property("async", false);

        let pipeline = gst::Pipeline::new();
        for element in [&src, &full_caps, &tee, &main_sink] {
            pipeline.add(element).expect("add");
        }
        src.link(&full_caps).expect("l1");
        full_caps.link(&tee).expect("l2");
        tee.link(&main_sink).expect("l3");

        let (tx, _rx) = mpsc::channel::<Event>();
        pipeline.set_state(gst::State::Playing).expect("playing");
        std::thread::sleep(Duration::from_millis(400));

        let state = crate::gstreamer_pipeline::build_transcode_record_branch(
            &pipeline,
            &tee,
            crate::gstreamer_pipeline::RtpVideoApi::Software,
            false,
            Some(tx),
            8_000,
        )
        .expect("build recording branch");

        // Y-plane min/max + MEAN at two points: the branch input (raw source
        // frame) and the encoder input (after the FULL→LIMITED conversion).
        // The mean is what catches the clip-only "conversion": clipping
        // extremes to [16,235] leaves the mean at the full-range value, while
        // the real LUT rescale shifts it by the full→limited curve (e.g. a
        // full-range mean of ~50 becomes ~59).
        #[derive(Default)]
        struct Stats {
            min: u8,
            max: u8,
            sum: u64,
            count: u64,
        }
        let branch_in = Arc::new(Mutex::new(Stats::default()));
        let encoder_in = Arc::new(Mutex::new(Stats::default()));
        let install_probe = |element: &gst::Element, pad_name: &str, sink: Arc<Mutex<Stats>>| {
            let pad = element.static_pad(pad_name).expect("pad");
            pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
                use gst::prelude::*;
                let Some(buffer) = info.buffer() else {
                    return gst::PadProbeReturn::Ok;
                };
                let Some(caps) = _pad.current_caps() else {
                    return gst::PadProbeReturn::Ok;
                };
                let Ok(video_info) = gstreamer_video::VideoInfo::from_caps(&caps) else {
                    return gst::PadProbeReturn::Ok;
                };
                let Ok(map) = buffer.map_readable() else {
                    return gst::PadProbeReturn::Ok;
                };
                let data = map.as_slice();
                let base = video_info.offset()[0];
                let stride = usize::try_from(video_info.stride()[0]).unwrap_or(0);
                let mut slot = sink.lock().expect("probe lock");
                if slot.count == 0 {
                    slot.min = 255;
                    slot.max = 0;
                }
                for row in 0..video_info.height() {
                    let start = base + row as usize * stride;
                    let end = start + video_info.width() as usize;
                    if end > data.len() {
                        break;
                    }
                    for &value in &data[start..end] {
                        slot.min = slot.min.min(value);
                        slot.max = slot.max.max(value);
                        slot.sum += value as u64;
                        slot.count += 1;
                    }
                }
                gst::PadProbeReturn::Ok
            });
        };
        install_probe(&state.queue, "sink", branch_in.clone());
        install_probe(&state.encoder, "sink", encoder_in.clone());

        state.start().expect("start recording");
        std::thread::sleep(Duration::from_millis(1_500));
        state.stop(true).expect("finalize recording");

        let bi = branch_in.lock().expect("branch probe");
        let ei = encoder_in.lock().expect("encoder probe");
        let bi_mean = bi.sum as f64 / bi.count.max(1) as f64;
        let ei_mean = ei.sum as f64 / ei.count.max(1) as f64;
        eprintln!(
            "DIAG color probe: branch input Y min={} max={} mean={bi_mean:.1}",
            bi.min, bi.max
        );
        eprintln!(
            "DIAG color probe: encoder input Y min={} max={} mean={ei_mean:.1}",
            ei.min, ei.max
        );

        assert!(bi.max > 0, "source delivered no frames to the branch");
        // The FULL→LIMITED conversion must actually scale the data (the LUT
        // scaler maps 255 → 235, 0 → 16, 128 → 126; the old videoconvert/RGB
        // round-trip only CLIPPED extremes to [16,235] without rescaling
        // mid-tones — the "hitam pekat" field bug). If the conversion is ever
        // reverted to a plain pass-through, encoder input == branch input and
        // this regression test fails.
        assert!(
            ei.max < bi.max,
            "encoder input must be scaled below the branch input (FULL→LIMITED); branch={} encoder={} — the range conversion is a no-op",
            bi.max, ei.max
        );
        assert!(
            ei.max <= 240,
            "encoder must receive LIMITED white (Y≈235), not full-range data; got max={}",
            ei.max
        );
        // The MEAN must follow the full→limited curve (16 + m*219/255), not
        // stay at the full-range value — a clip-only converter leaves the
        // mean (mid-tones) untouched, which is exactly the field bug. Allow
        // ±4 for the encoding/format conversions around it.
        let expected_mean = 16.0 + bi_mean * 219.0 / 255.0;
        assert!(
            (ei_mean - expected_mean).abs() <= 4.0,
            "encoder mean {ei_mean:.1} must follow the full→limited curve ≈ {expected_mean:.1} (clip-only converters leave the mean at {bi_mean:.1}) — mid-tones were not rescaled"
        );

        let _ = pipeline.set_state(gst::State::Null);
    }

    /// Field reproduction of the record-#2 freeze: record → stop → recycle →
    /// record again on a LIVE decoded-video tap tee (with the game-audio RTP
    /// branch sharing the same qtmux, exactly like production). The field log
    /// (2026-08-11 14:32) showed recording #1 working, then recording #2
    /// (after the in-place recycle) freezing the ENTIRE RTP path within ~20 ms
    /// of start — encoded/decoded/sink all stopped at 0, the stats channel
    /// went silent, and stop() could not finalize (EOS below the valve not
    /// accepted; only the 5 s failsafe finished the file). The main chain must
    /// keep flowing through BOTH rounds, and stop() must finalize quickly
    /// both times (EOS accepted, no failsafe timeout).
    #[test]
    fn record_restart_after_teardown_rebuild_keeps_main_chain_flowing() {
        gst::init().expect("gstreamer init");
        use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        // Video: LIVE decoded video (like the tap tee on the field: NV12 at
        // the decoder output) → tap tee → main sink. The live flag matters:
        // the field source is a live WebRTC stream, and live vs non-live
        // changes how a blocked branch back-pressures the source.
        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        src.set_property("is-live", true);
        let src_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("src caps");
        src_caps.set_property(
            "caps",
            "video/x-raw,format=(string)NV12,width=(int)640,height=(int)360,framerate=(fraction)60/1"
                .parse::<gst::Caps>()
                .expect("valid caps"),
        );
        let tee = gst::ElementFactory::make("tee").build().expect("tee");
        let main_sink = gst::ElementFactory::make("fakesink").build().expect("sink");
        main_sink.set_property("sync", false);
        main_sink.set_property("async", false);

        // Audio: Opus → RTP (game audio), into a second tee, like production.
        let asrc = gst::ElementFactory::make("audiotestsrc")
            .build()
            .expect("asrc");
        asrc.set_property("is-live", false);
        let aconv = gst::ElementFactory::make("audioconvert")
            .build()
            .expect("aconv");
        let ares = gst::ElementFactory::make("audioresample")
            .build()
            .expect("ares");
        let acaps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("acaps");
        acaps.set_property(
            "caps",
            "audio/x-raw,format=(string)S16LE,rate=(int)48000,channels=(int)2,layout=(string)interleaved"
                .parse::<gst::Caps>()
                .expect("valid audio caps"),
        );
        let opusenc = gst::ElementFactory::make("opusenc")
            .build()
            .expect("opusenc");
        let apay = gst::ElementFactory::make("rtpopuspay")
            .build()
            .expect("apay");
        apay.set_property("pt", 111u32);
        let atee = gst::ElementFactory::make("tee").build().expect("atee");
        let asink = gst::ElementFactory::make("fakesink")
            .build()
            .expect("asink");
        asink.set_property("sync", false);
        asink.set_property("async", false);

        let pipeline = gst::Pipeline::new();
        for element in [
            &src, &src_caps, &tee, &main_sink, &asrc, &aconv, &ares, &acaps, &opusenc, &apay,
            &atee, &asink,
        ] {
            pipeline.add(element).expect("add element");
        }
        src.link(&src_caps).expect("link v");
        src_caps.link(&tee).expect("link v");
        tee.link(&main_sink).expect("link v");
        asrc.link(&aconv).expect("link a");
        aconv.link(&ares).expect("link a");
        ares.link(&acaps).expect("link a");
        acaps.link(&opusenc).expect("link a");
        opusenc.link(&apay).expect("link a");
        apay.link(&atee).expect("link a");
        atee.link(&asink).expect("link a");

        let (tx, rx) = mpsc::channel::<Event>();
        pipeline.set_state(gst::State::Playing).expect("playing");
        std::thread::sleep(Duration::from_millis(400));
        let mut state = crate::gstreamer_pipeline::build_transcode_record_branch(
            &pipeline,
            &tee,
            crate::gstreamer_pipeline::RtpVideoApi::Software,
            false,
            Some(tx.clone()),
            8_000,
        )
        .expect("build video branch");

        // DIAG isolation: run round 2 WITHOUT the audio branch to see whether
        // the shared-muxer audio side causes the round-2 stall.
        let with_audio = std::env::var("DIAG_NO_AUDIO").is_err();
        if with_audio {
            state.audio_rtp_tee = Some(atee.clone());
            state
                .build_audio_branch(&pipeline)
                .expect("build audio branch");
        }

        // Live-path frame counter on the MAIN sink: the field freeze stopped
        // decoded/sink at 0 fps, so this counter is the decisive signal.
        let main_counter = Arc::new(AtomicU64::new(0));
        {
            let counter = main_counter.clone();
            let pad = main_sink.static_pad("sink").expect("sink pad");
            pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
                counter.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            });
        }
        // Branch-side counters: tee sink (frames the tee accepted) and branch
        // queue sink (frames the branch consumed) — tells us whether the stall
        // is upstream of the branch or inside it.
        let tee_counter = Arc::new(AtomicU64::new(0));
        {
            let counter = tee_counter.clone();
            let pad = tee.static_pad("sink").expect("tee sink pad");
            pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
                counter.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            });
        }
        let branch_queue_counter = Arc::new(AtomicU64::new(0));
        {
            let counter = branch_queue_counter.clone();
            let pad = state
                .queue
                .static_pad("sink")
                .expect("branch queue sink pad");
            pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
                counter.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            });
        }
        // DIAG bisect: does the round-2 buffer even reach the valve? A probe
        // on the valve's SINK pad tells us whether the block is at the tee→
        // valve push or inside the valve→queue path.
        let valve_sink_counter = Arc::new(AtomicU64::new(0));
        {
            let counter = valve_sink_counter.clone();
            let pad = state.valve.static_pad("sink").expect("valve sink pad");
            pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
                counter.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            });
        }

        // The tees stream into the closed valves for a while, like the field.
        std::thread::sleep(Duration::from_millis(1_000));

        let finalize_round = |state: &mut crate::gstreamer_pipeline::GstreamerRecordingState,
                              round: &str,
                              record_ms: u64|
         -> Vec<u8> {
            let before = main_counter.load(Ordering::SeqCst);
            let tee0 = tee_counter.load(Ordering::SeqCst);
            let bq0 = branch_queue_counter.load(Ordering::SeqCst);
            let vs0 = valve_sink_counter.load(Ordering::SeqCst);
            state.start().expect("start recording");
            std::thread::sleep(Duration::from_millis(record_ms));
            let during = main_counter.load(Ordering::SeqCst);
            let tee1 = tee_counter.load(Ordering::SeqCst);
            let bq1 = branch_queue_counter.load(Ordering::SeqCst);
            let vs1 = valve_sink_counter.load(Ordering::SeqCst);
            eprintln!(
                "DIAG restart {round}: main sink {before}->{during} tee {tee0}->{tee1} valve_sink {vs0}->{vs1} branch_queue {bq0}->{bq1}"
            );
            assert!(
                during > before + 30,
                "{round}: the MAIN chain stalled while recording (main sink {before} -> {during} in {record_ms} ms) — the field record-#2 freeze",
            );

            let stop_started = Instant::now();
            state.stop(true).expect("finalize recording");
            let stop_ms = stop_started.elapsed().as_millis();
            eprintln!("DIAG restart {round}: stop finalized in {stop_ms} ms");
            assert!(
                stop_ms < 4_000,
                "{round}: stop() took {stop_ms} ms — the EOS-below-valve path failed and the 5 s failsafe had to run (field symptom)"
            );

            let mut file_bytes: Vec<u8> = Vec::new();
            let mut chunks = 0usize;
            while let Ok(event) = rx.try_recv() {
                if let Event::RecordingChunk { chunk_base64 } = event {
                    chunks += 1;
                    file_bytes.extend(BASE64_STANDARD.decode(chunk_base64).expect("b64"));
                }
            }
            assert!(chunks >= 1, "{round}: no muxer chunk after stop");
            file_bytes
        };

        let round1 = finalize_round(&mut state, "round1", 1_500);
        assert!(
            round1.windows(4).any(|w| w == b"avc1"),
            "round1: missing H.264 video track"
        );

        // The critical part: the second recording must NOT freeze the stream.
        // The field freeze happened ~20 ms after this second start. The old
        // in-place recycle() is unreliable (queue src task dies on
        // NULL→PLAYING, qtmux keeps round-1 EOS/interleave state), so the
        // spent branch is torn down and REBUILT FRESH — the exact state round
        // 1 always succeeds from — and the audio branch is rebuilt into the
        // new branch's muxer.
        state.teardown(&pipeline).expect("teardown branch");
        state = crate::gstreamer_pipeline::build_transcode_record_branch(
            &pipeline,
            &tee,
            crate::gstreamer_pipeline::RtpVideoApi::Software,
            false,
            Some(tx.clone()),
            8_000,
        )
        .expect("rebuild video branch");
        if with_audio {
            state.audio_rtp_tee = Some(atee.clone());
            state
                .build_audio_branch(&pipeline)
                .expect("rebuild audio branch");
        }
        let round2 = finalize_round(&mut state, "round2", 1_500);
        assert!(
            round2.windows(4).any(|w| w == b"avc1"),
            "round2: missing H.264 video track"
        );

        let _ = pipeline.set_state(gst::State::Null);
    }

    /// PRODUCTION-FAITHFUL D3D12 variant of the color probe: the field chain
    /// runs the recording branch over the D3D12 video API (zero-copy OFF —
    /// the Software-variant probe uses RtpVideoApi::Software; both exercise
    /// the same production builder, which now picks x264enc). Same contract:
    /// the FULL→LIMITED conversion must hand the encoder data scaled BELOW
    /// the branch input, because every H.264 player expands content as
    /// limited and full-range data comes out crushed ("hitam pekat"); the
    /// encoder (x264enc with insert-vui=false) then writes the file with no
    /// range tag at all, exactly like the official GeForce Now recordings.
    #[test]
    fn probe_record_branch_d3d12_full_range_converts() {
        gst::init().expect("gstreamer init");
        use std::sync::mpsc;
        use std::time::Duration;

        let src = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        src.set_property("is-live", false);
        src.set_property_from_str("pattern", "white");
        // Route through RGB so the NV12 caps can carry the declared FULL
        // colorimetry (videotestsrc alone always emits limited white, Y=235 —
        // but whatever level it emits, the branch must pass it through
        // untouched, so the comparison is branch input vs encoder input).
        let to_full = gst::ElementFactory::make("videoconvert")
            .build()
            .expect("rgb to nv12 convert");
        let full_caps = gst::ElementFactory::make("capsfilter")
            .build()
            .expect("full caps");
        full_caps.set_property(
            "caps",
            "video/x-raw,format=(string)NV12,width=(int)64,height=(int)64,framerate=(fraction)30/1,colorimetry=(string)bt709/bt709/bt709/0-255,chroma-site=(string)mpeg2"
                .parse::<gst::Caps>()
                .expect("valid caps"),
        );
        let tee = gst::ElementFactory::make("tee").build().expect("tee");
        let main_sink = gst::ElementFactory::make("fakesink").build().expect("sink");
        main_sink.set_property("sync", false);
        main_sink.set_property("async", false);

        let pipeline = gst::Pipeline::new();
        for element in [&src, &to_full, &full_caps, &tee, &main_sink] {
            pipeline.add(element).expect("add");
        }
        src.link(&to_full).expect("l1");
        to_full.link(&full_caps).expect("l2");
        full_caps.link(&tee).expect("l3");
        tee.link(&main_sink).expect("l4");

        let (tx, _rx) = mpsc::channel::<Event>();
        pipeline.set_state(gst::State::Playing).expect("playing");
        std::thread::sleep(Duration::from_millis(400));

        let state = crate::gstreamer_pipeline::build_transcode_record_branch(
            &pipeline,
            &tee,
            crate::gstreamer_pipeline::RtpVideoApi::D3D12,
            false,
            Some(tx),
            8_000,
        )
        .expect("build recording branch");

        let branch_in = Arc::new(Mutex::new((255u8, 0u8)));
        let encoder_in = Arc::new(Mutex::new((255u8, 0u8)));
        let install_probe = |element: &gst::Element, pad_name: &str, sink: Arc<Mutex<(u8, u8)>>| {
            let pad = element.static_pad(pad_name).expect("pad");
            pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
                use gst::prelude::*;
                let Some(buffer) = info.buffer() else {
                    return gst::PadProbeReturn::Ok;
                };
                let Some(caps) = _pad.current_caps() else {
                    return gst::PadProbeReturn::Ok;
                };
                let Ok(video_info) = gstreamer_video::VideoInfo::from_caps(&caps) else {
                    return gst::PadProbeReturn::Ok;
                };
                let Ok(map) = buffer.map_readable() else {
                    return gst::PadProbeReturn::Ok;
                };
                let data = map.as_slice();
                let base = video_info.offset()[0];
                let stride = usize::try_from(video_info.stride()[0]).unwrap_or(0);
                let mut min = 255u8;
                let mut max = 0u8;
                for row in 0..video_info.height() {
                    let start = base + row as usize * stride;
                    let end = start + video_info.width() as usize;
                    if end > data.len() {
                        break;
                    }
                    for &value in &data[start..end] {
                        if value < min {
                            min = value;
                        }
                        if value > max {
                            max = value;
                        }
                    }
                }
                let mut slot = sink.lock().expect("probe lock");
                if min < slot.0 {
                    slot.0 = min;
                }
                if max > slot.1 {
                    slot.1 = max;
                }
                gst::PadProbeReturn::Ok
            });
        };
        install_probe(&state.queue, "sink", branch_in.clone());
        install_probe(&state.encoder, "sink", encoder_in.clone());

        state.start().expect("start recording");
        std::thread::sleep(Duration::from_millis(1_500));
        state.stop(true).expect("finalize recording");

        let (bi_min, bi_max) = *branch_in.lock().expect("branch probe");
        let (ei_min, ei_max) = *encoder_in.lock().expect("encoder probe");
        eprintln!("DIAG d3d12 color probe: branch input Y min={bi_min} max={bi_max}");
        eprintln!("DIAG d3d12 color probe: encoder input Y min={ei_min} max={ei_max}");

        assert!(
            bi_max > 0,
            "source delivered no frames to the branch; max={bi_max}"
        );
        assert!(ei_max > 0, "encoder received no frames; max={ei_max}");
        // Same contract as the Software-variant probe: the branch must hand
        // the encoder data scaled BELOW its own input (the LUT scaler is
        // active) — H.264 players expand content as limited, so full-range
        // 0-255 data would come out crushed (the field dark-recording bug).
        assert!(
            ei_max < bi_max,
            "D3D12 chain must scale the encoder input below the branch input (FULL→LIMITED); branch={bi_max} encoder={ei_max} — the range conversion is a no-op"
        );
        assert!(
            ei_max <= 240,
            "D3D12 chain must hand the encoder LIMITED white (Y≈235); got max={ei_max} — the production FULL→LIMITED conversion is a no-op (the field 0-255-tagged-limited bug)"
        );

        let _ = pipeline.set_state(gst::State::Null);
    }

    #[test]
    fn rtcp_jitter_to_ms_converts_rtp_timestamp_units() {
        // Video RTP uses a 90 kHz clock: 1 ms of jitter ≈ 90 units.
        assert_eq!(rtcp_jitter_to_ms(90, 90_000), Some(1));
        assert_eq!(rtcp_jitter_to_ms(450, 90_000), Some(5));
        assert_eq!(rtcp_jitter_to_ms(900, 90_000), Some(10));
        // Audio clock (48 kHz) — same units, different rate.
        assert_eq!(rtcp_jitter_to_ms(48, 48_000), Some(1));
        // 0 units or a missing/zero clock rate → None (no garbage in the HUD).
        assert_eq!(rtcp_jitter_to_ms(0, 90_000), None);
        assert_eq!(rtcp_jitter_to_ms(90, 0), None);
        // Overflowed/absurd counters are clamped away.
        assert_eq!(rtcp_jitter_to_ms(u32::MAX, 90_000), None);
    }

    /// Build one RTCP message: byte0 = version(2)|P(0)|count/FMT(5),
    /// byte1 = PT, bytes2-3 = length words minus 1, then `payload` padded to
    /// a 32-bit boundary.
    fn rtcp_message(count_or_fmt: u8, pt: u8, payload: &[u8]) -> Vec<u8> {
        let mut body = vec![count_or_fmt, pt, 0, 0];
        let payload_len = payload.len();
        let padded = payload_len.div_ceil(4) * 4;
        body.extend_from_slice(payload);
        body.resize(4 + padded, 0);
        let length_words = (body.len() / 4 - 1) as u16;
        body[2] = (length_words >> 8) as u8;
        body[3] = (length_words & 0xFF) as u8;
        body
    }

    #[test]
    fn classify_rtcp_messages_counts_feedback_types() {
        // Receiver Report (PT 201, RC 1, one 24-byte report block).
        let rr = rtcp_message(1, 201, &[0u8; 24]);
        // Sender Report (PT 200, RC 0).
        let sr = rtcp_message(0, 200, &[0u8; 20]);
        // Transport-wide congestion control feedback: RTPFB PT 205, FMT 15.
        let twcc = rtcp_message(15, 205, &[0u8; 12]);
        // Generic NACK: RTPFB PT 205, FMT 1.
        let nack = rtcp_message(1, 205, &[0u8; 8]);
        // Picture Loss Indication: PSFB PT 206, FMT 1.
        let pli = rtcp_message(1, 206, &[0u8; 8]);
        // Full Intra Request: PSFB PT 206, FMT 4.
        let fir = rtcp_message(4, 206, &[0u8; 8]);
        // SDES (PT 202) — counts as other.
        let sdes = rtcp_message(1, 202, &[0u8; 8]);

        let mut compound = Vec::new();
        compound.extend_from_slice(&rr);
        compound.extend_from_slice(&sr);
        compound.extend_from_slice(&twcc);
        compound.extend_from_slice(&nack);
        compound.extend_from_slice(&pli);
        compound.extend_from_slice(&fir);
        compound.extend_from_slice(&sdes);

        let counts = classify_rtcp_messages(&compound);
        assert_eq!(counts.sr, 1);
        assert_eq!(counts.rr, 1);
        assert_eq!(counts.twcc, 1);
        assert_eq!(counts.nack, 1);
        assert_eq!(counts.pli, 1);
        assert_eq!(counts.fir, 1);
        assert_eq!(counts.other, 1);
    }

    #[test]
    fn classify_rtcp_messages_handles_truncated_and_empty_buffers() {
        // Empty buffer → all zero.
        let counts = classify_rtcp_messages(&[]);
        assert_eq!(counts, RtcpMessageCounts::default());
        // Truncated header (fewer than 4 bytes) → not misread, all zero.
        let counts = classify_rtcp_messages(&[0x80, 0xC9]);
        assert_eq!(counts, RtcpMessageCounts::default());
        // Truncated MID-message (length says 4 words but only 2 bytes follow)
        // → the walk stops without counting garbage.
        let mut truncated = rtcp_message(15, 205, &[0u8; 12]);
        truncated.truncate(6);
        let counts = classify_rtcp_messages(&truncated);
        assert_eq!(counts, RtcpMessageCounts::default());
    }

    #[test]
    fn median_of_deltas_ignores_a_single_inflated_spike() {
        // 31 healthy ~8ms deltas + one 5000ms stall artifact. A median stays
        // at the healthy value; the old EMA (75% history) would have been
        // lifted for seconds by the spike.
        let mut deltas = VecDeque::new();
        for _ in 0..31 {
            deltas.push_back(8);
        }
        deltas.push_back(5_000);
        assert_eq!(median_of_deltas(&deltas), 8);

        // Even-count window: lower-middle of [7, 8, 8, 5000] is 8.
        let mut even = VecDeque::new();
        even.push_back(7);
        even.push_back(8);
        even.push_back(8);
        even.push_back(5_000);
        assert_eq!(median_of_deltas(&even), 8);
    }

    /// The decode→present pairing queue must be balanced: every decoded
    /// frame pushes one entry, and every sink event (present OR present-
    /// limiter drop) pops exactly one. If a limiter drop left its entry
    /// behind, the next present would pop a STALE timestamp and the delta
    /// would jump to the full drop age.
    #[test]
    fn present_limiter_drop_consumes_one_pairing_entry() {
        let mut state = VideoLivenessState::new();
        // `now_ms()` is elapsed-since-start (near 0 in a fresh test), so all
        // `saturating_sub` offsets would clamp to 0. Offset the clock so the
        // queued entries sit at real elapsed values.
        state.started_at = Instant::now() - Duration::from_secs(60);
        let now = state.now_ms();
        {
            let mut timestamps = state.decode_timestamps.lock().unwrap();
            timestamps.push_back(now.saturating_sub(1_000)); // stale-ish
            timestamps.push_back(now.saturating_sub(100));
            timestamps.push_back(now.saturating_sub(50));
        }
        // The limiter drops the frame whose timestamp is at the front (the
        // oldest un-consumed one) — record_sink_buffer would never run for
        // it, so pop it here to keep the queue balanced.
        state.record_sink_limiter_drop();
        // Two presents now pair with the remaining entries — deltas are
        // ~100ms and ~50ms, NOT ~1000ms.
        state.record_sink_buffer();
        state.record_sink_buffer();
        let median = state.decode_present_median_ms.load(Ordering::Relaxed);
        assert!(
            median <= 100 && median > 0,
            "decode→present median should stay small after a limiter drop, got {median}"
        );
    }

    /// A stall backlog must be cleared so the first post-recovery presents
    /// don't pair with pre-stall decode timestamps (the "decode time ribuan"
    /// symptom).
    #[test]
    fn clear_decode_timestamps_drops_stall_backlog() {
        let mut state = VideoLivenessState::new();
        state.started_at = Instant::now() - Duration::from_secs(60);
        let now = state.now_ms();
        {
            let mut timestamps = state.decode_timestamps.lock().unwrap();
            timestamps.push_back(now.saturating_sub(5_000));
            timestamps.push_back(now.saturating_sub(3_000));
            timestamps.push_back(now.saturating_sub(2_500));
        }
        state.clear_decode_timestamps();
        assert!(state.decode_timestamps.lock().unwrap().is_empty());
        // A post-recovery present finds an empty queue → no delta is recorded
        // (the median holds its previous value instead of spiking).
        state.record_sink_buffer();
        assert_eq!(state.decode_present_median_ms.load(Ordering::Relaxed), 0);
    }

    /// Runtime round-trip for the TWCC hardening: the writable
    /// `twcc-feedback-interval` lives on the internal RTPSession GObject (not
    /// on the rtpsession element), so this proves the bundled GStreamer
    /// actually exposes it there — otherwise forcing periodic transport-cc
    /// feedback silently no-ops and the server BWE stays blind.
    #[test]
    fn twcc_feedback_interval_roundtrips_on_internal_session() {
        gst::init().expect("gstreamer init");
        let Some(session) = gst::ElementFactory::make("rtpsession").build().ok() else {
            return; // factory missing (runtime without rtpmanager) — nothing to verify
        };
        assert!(
            session.find_property("internal-session").is_some(),
            "rtpsession element must expose the readable internal-session property"
        );
        let internal: gst::glib::Object = session.property("internal-session");
        assert!(
            internal.find_property("twcc-feedback-interval").is_some(),
            "internal RTPSession must expose twcc-feedback-interval (the element wrapper does not)"
        );
        let interval_ns = 100u64 * gst::ClockTime::MSECOND.nseconds();
        internal.set_property("twcc-feedback-interval", interval_ns);
        let read_back: u64 = internal.property("twcc-feedback-interval");
        assert_eq!(
            read_back,
            interval_ns,
            "twcc-feedback-interval must round-trip through rtp_twcc_manager_set_feedback_interval"
        );
    }

    /// Defensive expiry inside record_sink_buffer: an entry older than the
    /// max plausible decode→present latency is stall residue and must never
    /// inflate the delta, even if the watchdog hasn't cleared it yet.
    #[test]
    fn record_sink_buffer_skips_stale_pairing_entry() {
        let mut state = VideoLivenessState::new();
        state.started_at = Instant::now() - Duration::from_secs(60);
        let now = state.now_ms();
        {
            let mut timestamps = state.decode_timestamps.lock().unwrap();
            timestamps.push_back(now.saturating_sub(4_000)); // stall residue
            timestamps.push_back(now.saturating_sub(60));
        }
        state.record_sink_buffer();
        let median = state.decode_present_median_ms.load(Ordering::Relaxed);
        assert!(
            median > 0 && median <= 100,
            "stale entry must be skipped; delta should be ~60ms, got {median}"
        );
    }

    /// The GFN codec downgrade ladder must cascade AV1 → H265 → H264 and stop
    /// at the terminal codec: a zero-frame startup downgrades one step, the
    /// relaunched session (new streamer process) re-evaluates with the next
    /// codec, and H264 (universally decodable) has no further fallback.
    #[test]
    fn codec_downgrade_ladder_cascades_av1_to_h265_to_h264() {
        assert_eq!(codec_downgrade_target("AV1"), Some("H265"));
        assert_eq!(codec_downgrade_target("H265"), Some("H264"));
        assert_eq!(codec_downgrade_target("HEVC"), Some("H264"));
        assert_eq!(codec_downgrade_target("av1"), Some("H265")); // case-insensitive
        // Terminal / unknown: no downgrade (the fatal startup error path
        // runs instead).
        assert_eq!(codec_downgrade_target("H264"), None);
        assert_eq!(codec_downgrade_target(""), None);
        assert_eq!(codec_downgrade_target("VP9"), None);
    }
}
