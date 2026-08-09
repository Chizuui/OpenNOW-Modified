use crate::gstreamer_backend::send_log;
use crate::gstreamer_config::{use_external_renderer_window, use_stacked_renderer};
use crate::gstreamer_input::{
    stats_channel_bitrate_kbps, stats_channel_game_fps, stats_channel_packet_loss_fraction,
    stats_channel_rtt_ms,
};
use crate::gstreamer_pipeline::{configure_queue, set_property_if_supported};
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
    decoded_total: AtomicU64,
    sink_total: AtomicU64,
    /// Decode finish timestamps, popped one per sink present to measure the
    /// decode→present pipeline latency (filled into the HUD "Decode time").
    decode_timestamps: Mutex<VecDeque<u64>>,
    /// EMA of decode→present latency in ms.
    avg_decode_present_ms: AtomicU32,
    zero_copy_d3d11: AtomicBool,
    zero_copy_d3d12: AtomicBool,
    rtp_video_src_pad: Mutex<Option<gst::Pad>>,
    requested_fps: AtomicU32,
    framerate_mismatch_warned: AtomicBool,
    transition_flush_escalation_enabled: AtomicBool,
    first_encoded_logged: AtomicBool,
    startup_keyframe_requested: AtomicBool,
    startup_resync_requested: AtomicBool,
    startup_fatal_reported: AtomicBool,
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
            decoded_total: AtomicU64::new(0),
            sink_total: AtomicU64::new(0),
            decode_timestamps: Mutex::new(VecDeque::new()),
            avg_decode_present_ms: AtomicU32::new(0),
            zero_copy_d3d11: AtomicBool::new(false),
            zero_copy_d3d12: AtomicBool::new(false),
            rtp_video_src_pad: Mutex::new(None),
            requested_fps: AtomicU32::new(0),
            framerate_mismatch_warned: AtomicBool::new(false),
            transition_flush_escalation_enabled: AtomicBool::new(true),
            first_encoded_logged: AtomicBool::new(false),
            startup_keyframe_requested: AtomicBool::new(false),
            startup_resync_requested: AtomicBool::new(false),
            startup_fatal_reported: AtomicBool::new(false),
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
    }

    pub(crate) fn update_hardware_acceleration(&self, value: impl Into<String>) {
        if let Ok(mut hardware_acceleration) = self.hardware_acceleration.lock() {
            *hardware_acceleration = value.into();
        }
    }

    pub(crate) fn record_encoded_buffer(&self, size: usize) {
        self.last_encoded_ms.store(self.now_ms(), Ordering::Relaxed);
        self.encoded_bytes_total
            .fetch_add(size as u64, Ordering::Relaxed);
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

    pub(crate) fn record_sink_buffer(&self) {
        let now_ms = self.now_ms();
        self.last_sink_ms.store(now_ms, Ordering::Relaxed);
        self.sink_total.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut timestamps) = self.decode_timestamps.lock() {
            if let Some(decoded_at_ms) = timestamps.pop_front() {
                let delta = now_ms.saturating_sub(decoded_at_ms) as u32;
                let current = self.avg_decode_present_ms.load(Ordering::Relaxed);
                let next = if current == 0 {
                    delta
                } else {
                    // EMA (75% history, 25% latest) — smooths frame-to-frame jitter.
                    (current * 3 + delta) / 4
                };
                self.avg_decode_present_ms.store(next, Ordering::Relaxed);
            }
        }
    }

    /// Average decode→present latency in ms (None until the first present).
    fn avg_decode_present_ms(&self) -> Option<u32> {
        let value = self.avg_decode_present_ms.load(Ordering::Relaxed);
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

    pub(crate) fn set_rtp_video_src_pad(&self, pad: &gst::Pad) {
        if let Ok(mut current) = self.rtp_video_src_pad.lock() {
            *current = Some(pad.clone());
        }
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

    fn rtp_video_src_pad(&self) -> Option<gst::Pad> {
        self.rtp_video_src_pad
            .lock()
            .ok()
            .and_then(|current| current.clone())
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
}

impl Default for VideoLivenessMonitor {
    fn default() -> Self {
        Self {
            state: Arc::new(VideoLivenessState::new()),
            stop: Arc::new(AtomicBool::new(false)),
            started: Arc::new(AtomicBool::new(false)),
            thread: Arc::new(Mutex::new(None)),
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

    pub(crate) fn update_hardware_acceleration(&self, value: impl Into<String>) {
        self.state.update_hardware_acceleration(value);
    }

    pub(crate) fn record_encoded_buffer(&self, size: usize) {
        self.state.record_encoded_buffer(size);
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

    pub(crate) fn update_caps(&self, caps: &str) {
        self.state.update_caps(caps);
    }

    pub(crate) fn set_rtp_video_src_pad(&self, pad: &gst::Pad) {
        self.state.set_rtp_video_src_pad(pad);
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
        let state = self.state.clone();
        let stop = self.stop.clone();
        let thread = thread::spawn(move || {
            run_video_liveness_watchdog(state, stop, pipeline, sink, event_sender);
        });
        if let Ok(mut slot) = self.thread.lock() {
            *slot = Some(thread);
        }
    }

    pub(crate) fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.started.store(false, Ordering::SeqCst);
        let handle = self.thread.lock().ok().and_then(|mut slot| slot.take());
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

fn run_video_liveness_watchdog(
    state: Arc<VideoLivenessState>,
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

    while !stop.load(Ordering::SeqCst) {
        thread::sleep(VIDEO_LIVENESS_POLL_INTERVAL);

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
            let local_rtcp_rtt_ms = query_rtcp_rtt_ms(&pipeline);
            update_native_stats_overlay(
                &sink,
                &state,
                rates.encoded_kbps.round() as u32,
                rates,
                decoded_total,
                sink_total,
                local_rtcp_rtt_ms,
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
                        "[NetworkHealth] server rtt={}ms loss={:.4}% rtcp={} sinkDrop={:.2}% sink={:.1}fps bitrate={:.1}Mbps",
                        server_rtt,
                        loss_percent,
                        local_rtcp_rtt_ms
                            .map(|rtt| format!("{rtt}ms"))
                            .unwrap_or_else(|| "n/a (receiver-only)".to_owned()),
                        drop_percent,
                        rates.sink_fps,
                        rates.encoded_kbps / 1000.0,
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
            maybe_recover_video_startup(&state, &pipeline, &event_sender);
            continue;
        }

        let now_ms = state.now_ms();
        let encoded_age_ms = age_since_ms(now_ms, state.last_encoded_ms.load(Ordering::Relaxed));
        let decoded_age_ms = age_since_ms(now_ms, state.last_decoded_ms.load(Ordering::Relaxed));
        let sink_age_ms = age_since_ms(now_ms, last_sink_ms);
        let likely_stage = classify_video_stall(encoded_age_ms, decoded_age_ms, sink_age_ms);
        let transition_stall = likely_stage == "decode-chain-stalled"
            && encoded_age_ms.is_some_and(|age| age <= 1_000);

        match tracker.evaluate(now_ms, last_sink_ms) {
            VideoStallAction::None => {}
            VideoStallAction::RequestKeyframe { attempt, stall_ms } => {
                request_upstream_key_unit(&state, &event_sender);
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
            VideoStallAction::Resync { attempt, stall_ms } => {
                request_upstream_key_unit(&state, &event_sender);
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
                    request_upstream_key_unit(&state, &event_sender);
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
                    request_upstream_key_unit(&state, &event_sender);
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
    pipeline: &gst::Pipeline,
    event_sender: &Option<Sender<Event>>,
) {
    let now_ms = state.now_ms();
    let last_audio_ms = state.last_audio_ms.load(Ordering::Relaxed);
    let first_audio_ms = state.first_startup_audio_ms.load(Ordering::Relaxed);
    let last_encoded_ms = state.last_encoded_ms.load(Ordering::Relaxed);
    if first_audio_ms == 0
        || last_audio_ms == 0
        || now_ms.saturating_sub(last_audio_ms) > VIDEO_STARTUP_KEYFRAME_MS
    {
        return;
    }
    let audio_active_ms = now_ms.saturating_sub(first_audio_ms);

    let decoded_total = state.decoded_total.load(Ordering::Relaxed);
    let sink_total = state.sink_total.load(Ordering::Relaxed);
    let encoded_age = if last_encoded_ms == 0 {
        "never".to_owned()
    } else {
        format!("{}ms", now_ms.saturating_sub(last_encoded_ms))
    };

    if audio_active_ms >= VIDEO_STARTUP_KEYFRAME_MS
        && !state
            .startup_keyframe_requested
            .swap(true, Ordering::Relaxed)
    {
        send_log(
            event_sender,
            "warn",
            format!(
                "Native video startup has no rendered frame after {audio_active_ms}ms of active audio; startupAge={now_ms}ms encodedAge={encoded_age} decoded={decoded_total} sink={sink_total}. Requesting keyframe."
            ),
        );
        request_upstream_key_unit(state, event_sender);
    }

    if audio_active_ms >= VIDEO_STARTUP_RESYNC_MS
        && !state.startup_resync_requested.swap(true, Ordering::Relaxed)
    {
        send_log(
            event_sender,
            "warn",
            format!(
                "Native video startup still has no rendered frame after {audio_active_ms}ms of active audio; startupAge={now_ms}ms encodedAge={encoded_age} decoded={decoded_total} sink={sink_total}. Requesting keyframe and GStreamer latency resync."
            ),
        );
        request_upstream_key_unit(state, event_sender);
        if let Err(error) = pipeline.recalculate_latency() {
            send_log(
                event_sender,
                "warn",
                format!("Failed to resync GStreamer latency during native video startup recovery: {error}."),
            );
        }
    }

    if audio_active_ms >= VIDEO_STARTUP_FATAL_MS
        && !state.startup_fatal_reported.swap(true, Ordering::Relaxed)
    {
        send_log(
            event_sender,
            "error",
            format!(
                "Native video startup still has no rendered frame after {audio_active_ms}ms of active audio; startupAge={now_ms}ms encodedAge={encoded_age} decoded={decoded_total} sink={sink_total}. Treating startup as failed instead of restarting the WebRTC pipeline."
            ),
        );
        request_upstream_key_unit(state, event_sender);
        if let Some(event_sender) = event_sender {
            let _ = event_sender.send(Event::Error {
                code: "native-video-startup-timeout".to_owned(),
                message: "Native video startup timed out before the first rendered frame."
                    .to_owned(),
            });
        }
    }
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

fn request_upstream_key_unit(state: &VideoLivenessState, event_sender: &Option<Sender<Event>>) {
    let Some(src_pad) = state.rtp_video_src_pad() else {
        send_log(
            event_sender,
            "warn",
            "Unable to request upstream video key unit: no RTP video source pad registered."
                .to_owned(),
        );
        return;
    };

    let event = gst::event::CustomUpstream::builder(
        gst::Structure::builder("GstForceKeyUnit")
            .field("all-headers", true)
            .build(),
    )
    .build();

    if src_pad.send_event(event) {
        send_log(
            event_sender,
            "debug",
            "Requested upstream video key unit via RTP source pad.".to_owned(),
        );
    } else {
        send_log(
            event_sender,
            "warn",
            "Upstream video key-unit request was not accepted by the RTP source pad.".to_owned(),
        );
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
/// outgoing RTP. The native pipeline carries it on the mic m-line: real
/// capture when the user's mic is on, and a muted generated-silence
/// keepalive when it is off (see build_mic_pipeline). With
/// OPENNOW_NATIVE_MIC=0 there is no outgoing RTP and this returns None, and
/// the HUD falls back to the server-reported stats_channel field.
fn query_rtcp_rtt_ms(pipeline: &gst::Pipeline) -> Option<u32> {
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
                if let Ok(rtt_fixed) = source.get::<u32>("rb-round-trip") {
                    // 16.16 fixed point: value / 65536 seconds → ms.
                    let rtt_ms = (f64::from(rtt_fixed) / 65536.0 * 1000.0).round();
                    if rtt_ms > 0.0 && rtt_ms <= 2000.0 {
                        return Some(rtt_ms as u32);
                    }
                }
            }
        }
    }
    None
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
    let game_fps = stats_channel_game_fps();
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
            local_rtcp_rtt_ms,
            network_packet_loss_percent: stats_channel_packet_loss_fraction()
                .map(|loss| loss * 100.0),
            network_bitrate_kbps: stats_channel_bitrate_kbps(),
            decode_time_ms: state.avg_decode_present_ms(),
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
    // whenever it is available; "-" until any source reports.
    let ping_ms = local_rtcp_rtt_ms.or((rtt_ms > 0).then_some(rtt_ms));
    // Server-reported session bitrate (stats_channel counter, confidence-gated
    // in the native streamer); "-" until enough consistent samples confirm the
    // counter is cumulative bytes.
    let server_bitrate = stats_channel_bitrate_kbps()
        .map(|kbps| format!("{:.1}", f64::from(kbps) / 1000.0))
        .unwrap_or_else(|| "-".to_owned());
    let text = format!(
        "{} {}  {:.1}/{:.1} Mbps  Bit {:.0}%  Ping {}ms  Srv {} Mbps\nGame {:.0}fps  Stream {:.0}fps  Decode {:.0}fps  Drop {:.2}%  {}",
        codec,
        resolution,
        bitrate_mbps,
        target_mbps,
        bitrate_performance_percent,
        ping_ms.map(|ms| ms.to_string()).unwrap_or_else(|| "-".to_owned()),
        server_bitrate,
        // Server-reported game render FPS (stats_channel) — the real game rate,
        // which can exceed the negotiated stream FPS; 0 until the first frame.
        f64::from(stats_channel_game_fps()),
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
        if now < state.next_present_at {
            state.dropped = state.dropped.saturating_add(1);
            return gst::PadProbeReturn::Drop;
        }

        state.passed = state.passed.saturating_add(1);
        while state.next_present_at <= now {
            state.next_present_at += frame_interval;
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
        rendered: stats.get::<u64>("rendered").ok(),
        dropped: stats.get::<u64>("dropped").ok(),
        average_rate: stats.get::<f64>("average-rate").ok(),
    }
}

/// Number of frames the sink has presented so far (None when the sink has no
/// `stats` property). Used by the deferred video-tap attach to wait for the
/// D3D sink to finish warming up before hot-plugging the tap tee.
pub(crate) fn sink_rendered_frame_count(sink: &gst::Element) -> Option<u64> {
    read_sink_stats(sink).rendered
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
}
