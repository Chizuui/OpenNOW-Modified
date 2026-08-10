use crate::gstreamer_backend::send_log;
use crate::gstreamer_config::{
    automatic_present_max_fps, av1_decoder_preference, h265_decoder_preference,
    requested_video_backend, use_external_renderer_window, use_internal_renderer,
    use_stacked_renderer, vrr_present_max_fps, zero_copy_requested, CodecDecoderPreference,
    EXTERNAL_RENDERER_ENV, NATIVE_D3D_FULLSCREEN_ENV, NATIVE_PRESENT_MAX_FPS_ENV,
    NATIVE_VIDEO_API_ENV, NATIVE_VIDEO_BACKEND_ENV, PRESENT_LIMITER_AUTO_SENTINEL,
    PRESENT_LIMITER_VRR_SENTINEL,
};
#[cfg(target_os = "windows")]
use crate::gstreamer_input::NativeWindowInputBridge;
use crate::gstreamer_input::{
    create_input_data_channels, wire_remote_data_channels, GstreamerInputChannels,
    GstreamerInputState,
};
use crate::gstreamer_liveness::{
    install_present_limiter, sink_rendered_frame_count, watch_audio_activity,
    watch_first_sink_buffer, watch_rtp_video_bitrate, watch_video_caps_transitions,
    watch_video_decoded_rate, watch_video_sink_caps_transitions, watch_video_sink_rate,
    VideoLivenessMonitor,
};
#[cfg(target_os = "windows")]
use crate::gstreamer_platform::arm_internal_child_input;
use crate::gstreamer_platform::{
    apply_stacked_renderer_surface, arm_stacked_sink_input_capture, primary_display_refresh_hz,
    release_native_input_capture, start_external_renderer_window_guard,
    start_stacked_renderer_window_guard, stop_stacked_renderer_window_guard,
    update_external_renderer_surface,
};
use crate::gstreamer_transitions::DEFAULT_VIDEO_QUEUE_DEPTH;
use crate::internal_renderer::InternalRenderer;
use crate::nvst_video::{annexb_appsrc_caps, spawn_nvst_udp_receive, NvstVideoReceiveHandle};
use crate::protocol::{
    Event, IceCandidatePayload, IceServer, NativeRenderSurface, NativeScreenshotEvent,
    NativeStreamerSessionContext, NativeVideoBackendCapability, NativeVideoCodecCapability,
    NvstVideoSession,
};
use crate::sdp::IceCredentials;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use gst::glib;
use gst::prelude::*;
use gstreamer as gst;
use gstreamer_sdp as gst_sdp;
use gstreamer_webrtc as gst_webrtc;
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

const WEBRTC_LATENCY_MS: u32 = 2;
const DEFAULT_GFN_STUN_SERVER: &str = "stun://stun2.l.google.com:19302";
const VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS: u32 = 6;
/// How long to wait for a fresh frame to arrive at the screenshot grab branch
/// after opening its valve before giving up (the stream may be paused).
const SCREENSHOT_CAPTURE_TIMEOUT_MS: u64 = 2_000;
/// How long to wait for the recording encoder/muxer to flush after EOS before
/// falling back to the muxer-direct EOS failsafe (and then giving up).
/// Generous on purpose: on weak CPUs / iGPUs the branch queue (up to 30
/// frames) drains through x264enc at a few tens of fps, and the muxer must
/// also flush its final fragment. Bounded so stop(finalize=true) always
/// completes well inside the Electron request timeout even when the audio
/// branch never flowed (the audio EOS is rejected below the dead audio
/// valve, the muxer would otherwise wait forever, and the failsafe below
/// finalizes it directly).
const RECORDING_FINALIZE_TIMEOUT_MS: u64 = 4_000;
/// How long to wait for the recording branch queue to drain after the capture
/// valve closes, before injecting EOS. Once the queue is empty the EOS is
/// serialized after the last buffered frame instead of racing ahead of it
/// (an EOS that overtakes a full queue can be lost inside the encoder/muxer
/// on slow machines, leaving the recording un-finalized).
const RECORDING_DRAIN_TIMEOUT_MS: u64 = 4_000;
/// How long to wait after the muxer-direct EOS failsafe before giving up on
/// finalizing the recording.
const RECORDING_FAILSAFE_TIMEOUT_MS: u64 = 2_000;
/// Default recording bitrate (kbps) for the native H.264 encoder.
// Must stay unsigned: x264enc's `bitrate` is a guint, and gstreamer-rs
// `set_property` panics (process exit 101) when the Rust integer width/
// signedness mismatches the GObject property type.
const RECORDING_BITRATE_KBPS: u32 = 8_000;
pub(crate) const VIDEO_QUEUE_MAX_BUFFERS: u32 = DEFAULT_VIDEO_QUEUE_DEPTH;
const AUDIO_QUEUE_MAX_BUFFERS: u32 = 2;

// gstreamer-rs exposes the generic ICE transport but not the NICE stream that
// owns remote credentials. GFN uses UUID ICE passwords, so we need the actual
// NICE stream after GStreamer's SDP parser validates a sanitized copy.
#[repr(C)]
struct GstWebRTCNiceTransportCompat {
    parent: gst_webrtc::ffi::GstWebRTCICETransport,
    stream: *mut gst_webrtc::ffi::GstWebRTCICEStream,
    _priv: glib::ffi::gpointer,
}

#[derive(Debug, Clone, Copy)]
struct ActualNiceIceStream {
    ptr: *mut gst_webrtc::ffi::GstWebRTCICEStream,
    stream_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DecodedMediaKind {
    Audio,
    Video,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RtpVideoChainRole {
    ReceiveCapsFilter,
    Depayloader,
    Parser,
    PreDecodeQueue,
    Decoder,
    PostDecodeRateSetter,
    PostDecodeConverter,
    PostDecodeCapsFilter,
    StatsOverlay,
    PostDecodeQueue,
    Sink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RtpVideoApi {
    D3D11,
    D3D12,
    VideoToolbox,
    Nvdec,
    Vaapi,
    V4L2,
    Vulkan,
    Software,
}

impl RtpVideoApi {
    fn label(self) -> &'static str {
        match self {
            Self::D3D11 => "D3D11",
            Self::D3D12 => "D3D12",
            Self::VideoToolbox => "VideoToolbox",
            Self::Nvdec => "NVIDIA NVDEC",
            Self::Vaapi => "VAAPI",
            Self::V4L2 => "V4L2",
            Self::Vulkan => "Vulkan",
            Self::Software => "software",
        }
    }

    fn capability_id(self) -> &'static str {
        match self {
            Self::D3D11 => "d3d11",
            Self::D3D12 => "d3d12",
            Self::VideoToolbox => "videotoolbox",
            Self::Nvdec => "nvdec",
            Self::Vaapi => "vaapi",
            Self::V4L2 => "v4l2",
            Self::Vulkan => "vulkan",
            Self::Software => "software",
        }
    }

    fn platform(self) -> &'static str {
        match self {
            Self::D3D11 | Self::D3D12 => "windows",
            Self::VideoToolbox => "macos",
            Self::Nvdec | Self::Vaapi | Self::V4L2 => "linux",
            Self::Vulkan if current_platform_label() == "windows" => "windows",
            Self::Vulkan => "linux",
            Self::Software => "cross-platform",
        }
    }

    fn memory_caps(self) -> Option<&'static str> {
        match self {
            // D3D decoders and sinks can negotiate GPU memory directly. Keep
            // the capsfilter opt-in so startup does not fail when a live RTP
            // stream's raw caps are still settling.
            Self::D3D11 => zero_copy_requested().then_some("video/x-raw(memory:D3D11Memory)"),
            Self::D3D12 => zero_copy_requested().then_some("video/x-raw(memory:D3D12Memory)"),
            Self::VideoToolbox => zero_copy_requested().then_some("video/x-raw(memory:GLMemory)"),
            Self::Vaapi => zero_copy_requested().then_some("video/x-raw(memory:VAMemory)"),
            // Linux: keep Vulkan images in-GPU. Windows uses a DXVA→upload hybrid
            // (vulkanh264dec currently SIGSEGVs on NVIDIA Windows), so skip a hard
            // VulkanImage capsfilter on that path.
            Self::Vulkan if cfg!(target_os = "windows") => None,
            Self::Vulkan => Some("video/x-raw(memory:VulkanImage)"),
            _ => None,
        }
    }

    fn post_decode_converter_factory(self) -> Option<&'static str> {
        match self {
            Self::D3D11 | Self::D3D12 => None,
            // Windows Vulkan present chain inserts download/convert/upload explicitly.
            Self::Vulkan if cfg!(target_os = "windows") => None,
            Self::Vulkan => Some("vulkancolorconvert"),
            Self::VideoToolbox | Self::Vaapi if zero_copy_requested() => None,
            // Non-D3D hardware decoders are not guaranteed to negotiate directly with every
            // platform sink. Keep these paths reliable with an explicit raw-video conversion stage.
            Self::VideoToolbox | Self::Nvdec | Self::Vaapi | Self::Software => Some("videoconvert"),
            // V4L2 stateless decoders expose DMABuf on devices such as Raspberry Pi.
            // Let glimagesink import it directly instead of forcing a CPU copy.
            Self::V4L2 => None,
        }
    }

    fn stats_overlay_factory(self) -> Option<&'static str> {
        match self {
            Self::D3D11 | Self::D3D12 => Some("dwritetextoverlay"),
            _ => None,
        }
    }

    fn sink_factory(self) -> &'static str {
        match self {
            Self::D3D11 => "d3d11videosink",
            Self::D3D12 => "d3d12videosink",
            Self::VideoToolbox => "glimagesink",
            Self::Nvdec => "glimagesink",
            Self::Vaapi => "glimagesink",
            Self::V4L2 => "glimagesink",
            Self::Vulkan => "vulkansink",
            Self::Software => "autovideosink",
        }
    }

    fn decoder_factory(self, codec: &str) -> Option<&'static str> {
        match (self, codec) {
            (Self::D3D11, "H265" | "HEVC") => Some("d3d11h265dec"),
            (Self::D3D11, "H264") => Some("d3d11h264dec"),
            (Self::D3D11, "AV1") => Some("d3d11av1dec"),
            (Self::D3D12, "H265" | "HEVC") => Some("d3d12h265dec"),
            (Self::D3D12, "H264") => Some("d3d12h264dec"),
            (Self::D3D12, "AV1") => Some("d3d12av1dec"),
            (Self::VideoToolbox, "H265" | "HEVC" | "H264") => Some("vtdec_hw"),
            (Self::Nvdec, "H265" | "HEVC") => Some("nvh265dec"),
            (Self::Nvdec, "H264") => Some("nvh264dec"),
            (Self::Nvdec, "AV1") => Some("nvav1dec"),
            (Self::Vaapi, "H265" | "HEVC") => Some("vah265dec"),
            (Self::Vaapi, "H264") => Some("vah264dec"),
            (Self::Vaapi, "AV1") => Some("vaav1dec"),
            (Self::V4L2, "H265" | "HEVC") => Some("v4l2slh265dec"),
            (Self::V4L2, "H264") => Some("v4l2slh264dec"),
            (Self::V4L2, "AV1") => Some("v4l2slav1dec"),
            // vulkanh264dec/vulkanh265dec SIGSEGV on current NVIDIA Windows drivers;
            // use DXVA decode (prefer D3D12) and either D3D present (Internal) or
            // upload into Vulkan (External).
            (Self::Vulkan, "H265" | "HEVC") if cfg!(target_os = "windows") => Some("d3d12h265dec"),
            (Self::Vulkan, "H264") if cfg!(target_os = "windows") => Some("d3d12h264dec"),
            (Self::Vulkan, "AV1") if cfg!(target_os = "windows") => Some("d3d12av1dec"),
            (Self::Vulkan, "H265" | "HEVC") => Some("vulkanh265dec"),
            (Self::Vulkan, "H264") => Some("vulkanh264dec"),
            (Self::Vulkan, "AV1") => Some("vulkanav1dec"),
            (Self::Software, "H265" | "HEVC") => Some("avdec_h265"),
            (Self::Software, "H264") => Some("avdec_h264"),
            (Self::Software, "AV1") => Some("avdec_av1"),
            _ => None,
        }
    }

    fn fallback_decoder_factories(self, codec: &str) -> &'static [&'static str] {
        match (self, codec) {
            (Self::Vaapi, "H265" | "HEVC") => &["vaapih265dec"],
            (Self::Vaapi, "H264") => &["vaapih264dec"],
            (Self::Vaapi, "AV1") => &["vaapiav1dec"],
            (Self::V4L2, "H265" | "HEVC") => &["v4l2h265dec"],
            (Self::V4L2, "H264") => &["v4l2h264dec"],
            (Self::V4L2, "AV1") => &["v4l2av1dec"],
            (Self::VideoToolbox, "H265" | "HEVC" | "H264") => &["vtdec"],
            (Self::Vulkan, "H265" | "HEVC") if cfg!(target_os = "windows") => {
                &["d3d11h265dec", "nvh265dec"]
            }
            (Self::Vulkan, "H264") if cfg!(target_os = "windows") => &["d3d11h264dec", "nvh264dec"],
            (Self::Vulkan, "AV1") if cfg!(target_os = "windows") => &["d3d11av1dec", "nvav1dec"],
            (Self::Software, "AV1") => &["dav1ddec", "av1dec"],
            _ => &[],
        }
    }

    fn sink_fallback_factories(self) -> &'static [&'static str] {
        match self {
            Self::VideoToolbox => &["osxvideosink", "autovideosink"],
            // Prefer X11-capable sinks first: Internal Linux embeds via GstVideoOverlay
            // into an X11 child. waylandsink cannot paint into that handle.
            Self::Nvdec | Self::Vaapi | Self::V4L2 => &[
                "ximagesink",
                "xvimagesink",
                "glimagesink",
                "waylandsink",
                "autovideosink",
            ],
            Self::Software => &["ximagesink", "xvimagesink", "glimagesink", "waylandsink"],
            _ => &[],
        }
    }

    /// Sinks that can bind to the Internal X11 child via GstVideoOverlay.
    fn internal_x11_sink_candidates(self) -> &'static [&'static str] {
        match self {
            Self::Nvdec | Self::Vaapi | Self::V4L2 | Self::Software => {
                &["glimagesink", "ximagesink", "xvimagesink"]
            }
            // vulkansink implements GstVideoOverlay on Linux, so it can bind
            // directly to the X11 child while retaining VulkanImage memory.
            Self::Vulkan => &["vulkansink"],
            _ => &[],
        }
    }

    fn is_gpu_path(self) -> bool {
        !matches!(self, Self::Software)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RtpVideoChainSpec {
    pub(crate) factory: &'static str,
    pub(crate) role: RtpVideoChainRole,
    pub(crate) caps: Option<String>,
}

impl RtpVideoChainSpec {
    fn new(factory: &'static str, role: RtpVideoChainRole) -> Self {
        Self {
            factory,
            role,
            caps: None,
        }
    }

    fn with_caps(factory: &'static str, role: RtpVideoChainRole, caps: impl Into<String>) -> Self {
        Self {
            factory,
            role,
            caps: Some(caps.into()),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GstreamerRenderState {
    surface: Arc<Mutex<Option<NativeRenderSurface>>>,
    video_sink: Arc<Mutex<Option<gst::Element>>>,
    internal_renderer: Arc<InternalRenderer>,
    external_renderer_logged: Arc<AtomicBool>,
    internal_renderer_logged: Arc<AtomicBool>,
    external_window_guard_started: Arc<AtomicBool>,
    external_window_guard_stop: Arc<AtomicBool>,
}

impl Default for GstreamerRenderState {
    fn default() -> Self {
        Self {
            surface: Arc::new(Mutex::new(None)),
            video_sink: Arc::new(Mutex::new(None)),
            internal_renderer: Arc::new(InternalRenderer::new()),
            external_renderer_logged: Arc::new(AtomicBool::new(false)),
            internal_renderer_logged: Arc::new(AtomicBool::new(false)),
            external_window_guard_started: Arc::new(AtomicBool::new(false)),
            external_window_guard_stop: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl GstreamerRenderState {
    fn set_surface(&self, surface: NativeRenderSurface, event_sender: &Option<Sender<Event>>) {
        if let Ok(mut current) = self.surface.lock() {
            *current = Some(surface);
        }
        self.apply(event_sender);
    }

    fn set_video_sink(&self, sink: gst::Element, event_sender: &Option<Sender<Event>>) {
        if let Ok(mut current) = self.video_sink.lock() {
            *current = Some(sink.clone());
        }
        if use_internal_renderer() {
            if let Err(message) = self.internal_renderer.set_video_sink(sink) {
                send_log(event_sender, "warn", message);
            }
        }
        self.apply(event_sender);
    }

    fn apply(&self, event_sender: &Option<Sender<Event>>) {
        let sink_ready = self.video_sink.lock().ok().and_then(|sink| sink.clone());
        let Some(_sink) = sink_ready else {
            // Wait for the video sink to exist before deciding how to render.
            return;
        };

        if use_stacked_renderer() {
            if let Some(surface) = self.surface.lock().ok().and_then(|surface| surface.clone()) {
                // gstreamer-video 0.25 has no public getter for the overlay's
                // window handle, so pass 0 and let the platform layer find the
                // streamer's own top-level window via EnumWindows (the sink is
                // the only window this process owns in stacked mode).
                apply_stacked_renderer_surface(&surface, 0);
            }
            if !self
                .external_window_guard_started
                .swap(true, Ordering::SeqCst)
            {
                self.external_window_guard_stop
                    .store(false, Ordering::SeqCst);
                start_stacked_renderer_window_guard(
                    event_sender.clone(),
                    self.external_window_guard_stop.clone(),
                );
            }
            if !self.internal_renderer_logged.swap(true, Ordering::SeqCst) {
                send_log(
                    event_sender,
                    "info",
                    format!(
                        "Using stacked native GStreamer renderer window (video behind transparent Electron shell); set {EXTERNAL_RENDERER_ENV}=0 for the internal child-surface renderer."
                    ),
                );
            }
            return;
        }

        if use_external_renderer_window() {
            if let Some(surface) = self.surface.lock().ok().and_then(|surface| surface.clone()) {
                update_external_renderer_surface(&surface);
            }
            if !self
                .external_window_guard_started
                .swap(true, Ordering::SeqCst)
            {
                self.external_window_guard_stop
                    .store(false, Ordering::SeqCst);
                start_external_renderer_window_guard(
                    event_sender.clone(),
                    self.external_window_guard_stop.clone(),
                );
            }
            if !self.external_renderer_logged.swap(true, Ordering::SeqCst) {
                send_log(
                    event_sender,
                    "info",
                    format!(
                        "Using external native GStreamer renderer window; set {EXTERNAL_RENDERER_ENV}=0 for the internal child-surface renderer."
                    ),
                );
            }
            return;
        }

        let surface = self.surface.lock().ok().and_then(|surface| surface.clone());
        let Some(surface) = surface else {
            return;
        };

        if !self.internal_renderer_logged.swap(true, Ordering::SeqCst) {
            send_log(
                event_sender,
                "info",
                format!(
                    "Using internal native child-surface renderer; set {EXTERNAL_RENDERER_ENV}=1 for the floating GStreamer window."
                ),
            );
        }

        if let Err(message) = self.internal_renderer.apply_surface(&surface) {
            send_log(event_sender, "warn", message);
        }

        // Keep ClipCursor / capture rect aligned with the StreamView hole, and
        // (re)arm RawInput if the child HWND was recreated on parent change.
        #[cfg(target_os = "windows")]
        {
            update_external_renderer_surface(&surface);
            let hwnd = self.internal_renderer.child_handle();
            if hwnd != 0 {
                let _ = arm_internal_child_input(hwnd);
            }
        }
    }

    fn stop_external_renderer_window_guard(&self) {
        self.external_window_guard_stop
            .store(true, Ordering::SeqCst);
        self.external_window_guard_started
            .store(false, Ordering::SeqCst);
    }

    fn destroy_internal_renderer(&self) {
        self.internal_renderer.destroy();
        self.internal_renderer_logged.store(false, Ordering::SeqCst);
    }
}

/// The video-chain tap shared by screenshots and recording: the tee inserted
/// between the last pre-sink element and the sink. Screenshots and recording
/// open extra tee src pads on demand. The tee is NOT inserted while the video
/// chain is attached — it is hot-plugged lazily on first use (see
/// `ensure_tee`): on GStreamer 1.28.x a d3d12videosink whose present chain is
/// still warming up stalls permanently (rendered stays 0) when a tee with a
/// second — even inert and valve-gated — branch is present while the first
/// frames flow. The same tee hot-plugged after the sink has presented a few
/// frames is safe.
#[derive(Debug, Clone)]
pub(crate) struct GstreamerVideoTap {
    /// Lazily-created tap tee (None until the first screenshot/recording use).
    pub(crate) tee: Option<gst::Element>,
    /// The element the tee is inserted after (the post-decode queue).
    pub(crate) before_sink: gst::Element,
    /// The video sink the tee feeds.
    pub(crate) sink: gst::Element,
    pub(crate) video_api: RtpVideoApi,
    /// Whether the chain actually negotiated D3D memory. This is derived from
    /// the chain caps, not the global zero-copy preference: H264-D3D12 now
    /// deliberately downloads to system memory even when the preference is on.
    pub(crate) zero_copy: bool,
}

impl GstreamerVideoTap {
    /// Hot-plug the tap tee between `before_sink` and `sink`, waiting for the
    /// sink to start presenting first (the d3d12 present-chain stall is a
    /// warm-up race). Idempotent: returns the existing tee once created.
    pub(crate) fn ensure_tee(&mut self, pipeline: &gst::Pipeline) -> Result<gst::Element, String> {
        if let Some(tee) = self.tee.as_ref() {
            return Ok(tee.clone());
        }
        if self.sink.find_property("stats").is_some() {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                if sink_rendered_frame_count(&self.sink).unwrap_or(0) >= 8 {
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    return Err(
                        "Video tap attach timed out waiting for the sink to present frames."
                            .to_owned(),
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        } else {
            // Sinks without a rendered counter (vulkan/software): give the
            // present chain a short head start before hot-plugging.
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        let tee = make_element("tee")?;
        pipeline
            .add(&tee)
            .map_err(|error| format!("Failed to add video tap tee: {error}"))?;
        self.before_sink.unlink(&self.sink);
        self.before_sink
            .link(&tee)
            .map_err(|error| format!("Failed to link video chain into tap tee: {error:?}"))?;
        tee.link(&self.sink)
            .map_err(|error| format!("Failed to link tap tee to video sink: {error:?}"))?;
        tee.sync_state_with_parent()
            .map_err(|error| format!("Failed to sync video tap tee state: {error}"))?;
        self.tee = Some(tee.clone());
        Ok(tee)
    }
}

/// Native recording branch: tee → valve → queue → (download) → videoconvert →
/// capsfilter(I420) → x264enc → mp4mux (fragmented, streamable) → appsink.
/// Muxer output buffers are captured by a pad probe and streamed to the
/// Electron main process as `recording-chunk` events (base64), which are
/// appended to the recording file in order. `stop(finalize=true)` sends EOS
/// down the branch, waits for the flush, and the pipeline emits a
/// `recording-finished` event strictly after the last chunk so the main
/// process can close the file safely.
#[derive(Debug, Clone)]
pub(crate) struct GstreamerRecordingState {
    /// Video branch valve (between the video tap tee and the encoder chain).
    valve: gst::Element,
    /// Per-track audio valves (one per audio source: game audio, mic). Each
    /// track is an independent chain tap → audioresample → audioconvert →
    /// capsfilter(2ch/48k) → valve → voaacenc → mp4mux, so no audio mixer
    /// (aggregator) is involved — a mixer hot-plugged into a PLAYING pipeline
    /// drops the joined pads ("outside output segment") and fills them with
    /// digital silence, then its tiny per-pad queues block the game chain
    /// upstream (field: recordings carry no game audio). Empty for video-only
    /// recordings without any audio source.
    audio_valves: Vec<gst::Element>,
    /// Thumbnail valve (between the I420 tee and the JPEG grabber): open only
    /// while capturing the first frame, then closed again (idle ~0 cost).
    thumb_valve: gst::Element,
    /// Base64 JPEG of the first encoded recording frame; `None` until a frame
    /// has been captured (or the recording had no frames).
    thumbnail: Arc<Mutex<Option<String>>>,
    pub(crate) appsink: gst::Element,
    /// The mp4mux element. `stop(finalize=true)` falls back to sending EOS
    /// directly on its sink pads when the normal below-valve EOS path cannot
    /// complete (a recording audio branch that never flowed rejects the audio
    /// EOS, so the muxer would otherwise wait forever and stop() would time
    /// out); sending EOS on the muxer's sink pads finalizes it regardless.
    pub(crate) muxer: gst::Element,
    elements: Vec<gst::Element>,
    tee: gst::Element,
    /// (tap tee, requested fresh pad) for each audio track currently linked
    /// into the recording branch. On teardown the fresh pad is unlinked and
    /// released back to its tee so the next recording can request a new one
    /// (fresh pads never have the parked-src-task problem of dangling queues).
    audio_taps: Vec<(gst::Element, gst::Pad)>,
    /// The video-branch queue (valve → queue → convert → …). `stop()` drains
    /// it before injecting EOS so the EOS is serialized after the buffered
    /// tail instead of racing ahead of it.
    queue: gst::Element,
    eos_seen: Arc<AtomicBool>,
    active: Arc<AtomicBool>,
    /// True after a finalized (EOS) recording: the branch must be rebuilt
    /// before the next recording because the muxer does not accept new data
    /// after EOS.
    spent: Arc<AtomicBool>,
    event_sender: Option<Sender<Event>>,
}

impl GstreamerRecordingState {
    pub(crate) fn start(&self) -> Result<(), String> {
        self.eos_seen.store(false, Ordering::SeqCst);
        // Set active before opening the valve so the chunk probe never drops
        // the first muxer output (ftyp + moov).
        self.active.store(true, Ordering::SeqCst);
        self.valve.set_property("drop", false);
        for audio_valve in &self.audio_valves {
            audio_valve.set_property("drop", false);
        }
        // Reset + open the thumbnail grabber; its probe closes the valve right
        // after the first frame is captured.
        if let Ok(mut slot) = self.thumbnail.lock() {
            *slot = None;
        }
        self.thumb_valve.set_property("drop", false);
        let audio_note = if self.audio_valves.is_empty() {
            " (video only — no audio source available)"
        } else if self.audio_valves.len() == 1 {
            " with audio"
        } else {
            " with game + mic audio tracks"
        };
        send_log(
            &self.event_sender,
            "info",
            format!("Native recording started (H.264 fragmented MP4{audio_note})."),
        );
        Ok(())
    }

    pub(crate) fn stop(&self, finalize: bool) -> Result<(), String> {
        // Stop new frames entering the branch first; buffers already inside
        // (queue → encoder → muxer) keep flowing.
        self.valve.set_property("drop", true);
        for audio_valve in &self.audio_valves {
            audio_valve.set_property("drop", true);
        }
        self.thumb_valve.set_property("drop", true);
        if !finalize {
            self.active.store(false, Ordering::SeqCst);
            send_log(
                &self.event_sender,
                "info",
                "Native recording aborted; capture valves closed.".to_owned(),
            );
            return Ok(());
        }

        // EOS both branches (video + audio); mp4mux only emits EOS once every
        // linked pad has seen EOS, so both events must be sent.
        //
        // IMPORTANT: the valve is closed (drop=true) BEFORE this point, and in
        // the bundled GStreamer the valve drops EOS events while closed —
        // sending EOS into the valve's sink pad never reaches the encoder/
        // muxer (so the recording never finalizes and stop(finalize=true)
        // times out: recording-stop crash), AND as an upstream event it also
        // propagates back through the shared tap tee into the main video
        // chain — every recording stop in the field logs froze the whole
        // stream (video-output-stalled at the same moment the stop command
        // arrived). Enter EOS BELOW the valve instead (the next element's sink
        // pad): frames already buffered in the queue drain first, then EOS, so
        // the muxer finalizes normally and the main video path is untouched.
        //
        // Draining first matters: the branch queue (leaky, up to 30 frames)
        // forwards EOS ahead of its buffered tail on slow encoders, and an EOS
        // that overtakes a full queue can be dropped inside the encoder/muxer
        // (buffers-after-EOS) — the field logs show the recording never
        // finalizing. Wait for `current-level-buffers` to hit 0 so the EOS is
        // serialized after the last frame, then inject it.
        let drain_start = std::time::Instant::now();
        let drain_deadline =
            drain_start + std::time::Duration::from_millis(RECORDING_DRAIN_TIMEOUT_MS);
        let mut queue_level = 0u32;
        loop {
            queue_level = self.queue.property::<u32>("current-level-buffers");
            if queue_level == 0 || std::time::Instant::now() >= drain_deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let drain_ms = drain_start.elapsed().as_millis();
        if queue_level != 0 {
            send_log(
                &self.event_sender,
                "warn",
                format!(
                    "Recording branch queue did not drain before EOS (still {queue_level} buffers); injecting EOS anyway."
                ),
            );
        }

        for element in std::iter::once(&self.valve).chain(self.audio_valves.iter()) {
            let src_pad = element
                .static_pad("src")
                .ok_or_else(|| "Recording valve has no src pad.".to_owned())?;
            let below = src_pad
                .peer()
                .ok_or_else(|| "Recording valve is not linked to the encoder chain.".to_owned())?;
            let accepted = below.send_event(gst::event::Eos::new());
            send_log(
                &self.event_sender,
                "info",
                format!(
                    "Native recording stop: sent EOS below valve (accepted={accepted}) after draining {queue_level} queued buffer(s)."
                ),
            );
        }
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(RECORDING_FINALIZE_TIMEOUT_MS);
        while !self.eos_seen.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if !self.eos_seen.load(Ordering::SeqCst) {
            // The first EOS may have been lost racing the last in-flight
            // frame; a second EOS after the drain is harmless and often
            // completes the flush.
            let retry_deadline =
                std::time::Instant::now() + std::time::Duration::from_millis(1_000);
            let mut retried = false;
            while !self.eos_seen.load(Ordering::SeqCst)
                && std::time::Instant::now() < retry_deadline
            {
                if !retried {
                    retried = true;
                    for element in std::iter::once(&self.valve).chain(self.audio_valves.iter()) {
                        if let Some(src_pad) = element.static_pad("src") {
                            if let Some(below) = src_pad.peer() {
                                let _ = below.send_event(gst::event::Eos::new());
                            }
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }

        // FAILSAFE: mp4mux only emits EOS once EVERY sink pad has seen it.
        // If an audio track never flowed (e.g. the tap tee had no data, or a
        // fresh pad link failed and the track was skipped) it rejects the
        // audio EOS below its valve — the muxer then waits forever for that
        // audio pad's EOS and stop(finalize=true) times out (field: "Native
        // recording stop: sent EOS below valve (accepted=false)", muxer EOS
        // never seen, Electron 5s timeout). Sending EOS directly on the
        // muxer's sink pads bypasses the dead audio upstream: the muxer
        // finalizes (probe sees EOS at the appsink)
        // even when no audio buffer ever flowed.
        if !self.eos_seen.load(Ordering::SeqCst) {
            send_log(
                &self.event_sender,
                "warn",
                "Native recording stop: normal EOS path did not finalize the muxer; sending EOS directly on the muxer sink pads (dead audio branch).".to_owned(),
            );
            for pad in self.muxer.sink_pads() {
                let _ = pad.send_event(gst::event::Eos::new());
            }
            let failsafe_deadline = std::time::Instant::now()
                + std::time::Duration::from_millis(RECORDING_FAILSAFE_TIMEOUT_MS);
            while !self.eos_seen.load(Ordering::SeqCst)
                && std::time::Instant::now() < failsafe_deadline
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        let finalized = self.eos_seen.load(Ordering::SeqCst);
        if !finalized {
            // Keep the branch usable: without EOS the muxer was not spent.
            self.active.store(false, Ordering::SeqCst);
            send_log(
                &self.event_sender,
                "warn",
                "Native recording stop: EOS still not seen at the muxer output; recording not finalized.".to_owned(),
            );
            return Err("Timed out waiting for the recording encoder to flush.".to_owned());
        }
        send_log(
            &self.event_sender,
            "info",
            format!("Native recording stop: muxer EOS seen (queue drained in {drain_ms} ms)."),
        );
        self.active.store(false, Ordering::SeqCst);
        self.spent.store(true, Ordering::SeqCst);
        let thumb_note = if self
            .thumbnail
            .lock()
            .ok()
            .is_some_and(|slot| slot.is_some())
        {
            " with thumbnail"
        } else {
            " without thumbnail"
        };
        send_log(
            &self.event_sender,
            "info",
            format!("Native recording finalized; H.264/MP4 chunks flushed{thumb_note}."),
        );
        Ok(())
    }

    /// Base64 JPEG of the first captured recording frame (gallery thumbnail).
    fn thumbnail(&self) -> Option<String> {
        self.thumbnail.lock().ok().and_then(|slot| slot.clone())
    }
}

/// Screenshot frame-grab branch tapped off the video chain right before the
/// sink: tee → valve → queue → (download) → videoconvert → pngenc → appsink.
/// The valve is closed (drop=true) whenever no capture is in flight, so the
/// branch costs ~zero when idle; on `capture()` the valve opens for one frame
/// interval and a pad probe on the appsink sink pad stores the newest encoded
/// PNG buffer, then the valve closes again before it is read out.
#[derive(Debug, Clone)]
pub(crate) struct GstreamerScreenshotGrab {
    valve: gst::Element,
    appsink: gst::Element,
    last_buffer: Arc<Mutex<Option<gst::Buffer>>>,
    event_sender: Option<Sender<Event>>,
}

impl GstreamerScreenshotGrab {
    fn capture(&self) -> Result<NativeScreenshotEvent, String> {
        // Drop any stale sample from a previous capture before opening the
        // valve so the probe only reports a freshly presented frame.
        if let Ok(mut slot) = self.last_buffer.lock() {
            *slot = None;
        }
        self.valve.set_property("drop", false);
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(SCREENSHOT_CAPTURE_TIMEOUT_MS);
        let buffer = loop {
            let captured = self.last_buffer.lock().ok().and_then(|slot| slot.clone());
            if captured.is_some() {
                break captured;
            }
            if std::time::Instant::now() >= deadline {
                break None;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        // Close the valve immediately once a frame is in hand (or the wait
        // expired) so the grab branch goes back to idle.
        self.valve.set_property("drop", true);
        let buffer = buffer.ok_or_else(|| {
            "Timed out waiting for a video frame to screenshot; the stream may be paused."
                .to_owned()
        })?;

        // The probe stores only the buffer; caps come from the appsink pad at
        // read-out time (they do not change between frames within a session).
        let caps = self
            .appsink
            .static_pad("sink")
            .and_then(|pad| pad.current_caps());
        let (width, height) = caps
            .as_ref()
            .and_then(|caps| caps.structure(0))
            .map(|structure| {
                (
                    structure.get::<i32>("width").unwrap_or(0).max(0) as u32,
                    structure.get::<i32>("height").unwrap_or(0).max(0) as u32,
                )
            })
            .unwrap_or((0, 0));

        let mapped = buffer
            .map_readable()
            .map_err(|error| format!("Failed to map captured frame: {error}"))?;
        let png_bytes = mapped.as_slice();
        if png_bytes.is_empty() {
            return Err("Captured frame is empty.".to_owned());
        }
        let png_base64 = BASE64_STANDARD.encode(png_bytes);
        send_log(
            &self.event_sender,
            "info",
            format!(
                "Captured native screenshot: {width}x{height}, {} PNG bytes.",
                png_bytes.len()
            ),
        );
        Ok(NativeScreenshotEvent {
            png_base64,
            width,
            height,
        })
    }
}

/// Native microphone send path: platform capture (WASAPI/osxaudiosrc/pulse) → Opus → RTP into the
/// negotiated mic transceiver. Disabling the mic pauses the whole chain so
/// NO outgoing RTP is emitted while it is off — volume-0 alone would still
/// encode and send silence packets, and that continuous RTP stream was the
/// only structural delta behind the periodic video stalls (see the
/// negotiate_answer regression note). The sendonly m-line stays negotiated
/// (re-negotiation would be disruptive).
#[derive(Debug)]
pub(crate) struct GstreamerMicPipeline {
    volume: gst::Element,
    elements: Vec<gst::Element>,
}

impl GstreamerMicPipeline {
    fn set_enabled(&self, enabled: bool) {
        // Disable = run the WHOLE chain back to NULL (reversible, keeps the
        // negotiated m-line and the payloader→webrtcbin pad link): PAUSED is
        // not a hard stop — chain elements pass data through in PAUSED and
        // audiotestsrc-class sources keep pushing — so only NULL guarantees
        // that no buffers (→ no Opus → no RTP packets) leave the chain while
        // the mic is off. Re-enabling returns every element to PLAYING; the
        // capture device is re-opened and the RTP stream resumes.
        let target = if enabled {
            gst::State::Playing
        } else {
            gst::State::Null
        };
        for element in &self.elements {
            let _ = element.set_state(target);
        }
        self.volume
            .set_property("volume", if enabled { 1.0f64 } else { 0.0f64 });
    }
}

/// Find the Opus payload type negotiated for the mic m-line in the local
/// answer. The mic m-line is the media section with `a=mid:3` (or a `m=mic`
/// media type); the game-audio m-line also carries Opus, so scoping to the
/// mic section matters.
fn negotiated_mic_opus_payload(answer_sdp: &str) -> Option<u32> {
    let mut section_media: Option<&str> = None;
    let mut section_mid: Option<&str> = None;
    for raw_line in answer_sdp.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("m=") {
            section_media = rest.split_whitespace().next();
            section_mid = None;
            continue;
        }
        if let Some(mid) = line.strip_prefix("a=mid:") {
            section_mid = Some(mid.trim());
            continue;
        }
        let is_mic_section = section_media == Some("mic")
            || section_mid == Some("3")
            || (section_media == Some("audio") && section_mid.is_some_and(|mid| mid != "0"));
        if !is_mic_section {
            continue;
        }
        if let Some(rtpmap) = line.strip_prefix("a=rtpmap:") {
            if rtpmap.to_ascii_uppercase().contains("OPUS") {
                if let Some((pt, _)) = rtpmap.split_once(' ') {
                    if let Ok(pt) = pt.parse::<u32>() {
                        return Some(pt);
                    }
                }
            }
        }
    }
    None
}

#[derive(Debug)]
pub(crate) struct GstreamerPipeline {
    pub(crate) pipeline: gst::Pipeline,
    pub(crate) webrtc: gst::Element,
    input_state: GstreamerInputState,
    input_channels: Option<GstreamerInputChannels>,
    mic: Arc<Mutex<Option<GstreamerMicPipeline>>>,
    #[cfg(target_os = "windows")]
    native_window_input_bridge: Option<NativeWindowInputBridge>,
    render_state: GstreamerRenderState,
    present_max_fps: Arc<AtomicU32>,
    d3d_fullscreen_sink: Arc<AtomicBool>,
    /// When true, WebRTC RTP video pads are ignored (classic NVST UDP owns video).
    skip_webrtc_video: Arc<AtomicBool>,
    nvst_receive: Option<NvstVideoReceiveHandle>,
    video_liveness: VideoLivenessMonitor,
    screenshot_grab: Arc<Mutex<Option<GstreamerScreenshotGrab>>>,
    video_tap: Arc<Mutex<Option<GstreamerVideoTap>>>,
    recording: Arc<Mutex<Option<GstreamerRecordingState>>>,
    /// Recording tap tee on the decoded game-audio chain: built when the audio
    /// chain appears. The recording branch requests a fresh src pad per
    /// recording (fresh pads never carry the parked-src-task problem of the
    /// old dangling queues).
    game_audio_tap: Arc<Mutex<Option<gst::Element>>>,
    /// Recording tap tee on the mic chain after the mute volume, so muting the
    /// mic also silences it in recordings. Same fresh-pad-per-recording
    /// contract as the game-audio tap.
    mic_audio_tap: Arc<Mutex<Option<gst::Element>>>,
    /// The mic transceiver + negotiated Opus payload type from the last
    /// answer, kept so `set_microphone_enabled(true)` can attach the mic send
    /// path ON DEMAND when the mic was OFF at session start (the quick-menu /
    /// shortcut toggle mid-session, when no mic pipeline was ever built). The
    /// mic m-line is always negotiated sendonly in the answer; only the
    /// actual capture/encode pipeline is deferred until the user enables it.
    mic_transceiver: Option<gst_webrtc::WebRTCRTPTransceiver>,
    mic_payload_type: Option<u32>,
    event_sender: Option<Sender<Event>>,
    pub(crate) original_remote_ice_credentials: Option<IceCredentials>,
}

impl GstreamerPipeline {
    pub(crate) fn build(
        event_sender: Option<Sender<Event>>,
        ice_servers: &[IceServer],
    ) -> Result<Self, String> {
        init_gstreamer()?;

        let pipeline = gst::Pipeline::new();
        let webrtc = gst::ElementFactory::make("webrtcbin")
            .name("opennow-webrtcbin")
            .property_from_str("bundle-policy", "max-bundle")
            .build()
            .map_err(|error| format!("Failed to create webrtcbin: {error}"))?;
        configure_webrtc_low_latency(&webrtc);
        let stun_server = resolve_gstreamer_stun_server(ice_servers);
        webrtc.set_property("stun-server", &stun_server);
        send_log(
            &event_sender,
            "info",
            format!("Configured GStreamer ICE with STUN server {stun_server}."),
        );

        let input_state = GstreamerInputState::default();
        let render_state = GstreamerRenderState::default();
        let video_liveness = VideoLivenessMonitor::default();
        wire_local_ice_events(&webrtc, event_sender.clone())?;
        wire_webrtc_state_events(&webrtc, event_sender.clone());
        wire_remote_data_channels(&webrtc, event_sender.clone());
        start_gstreamer_bus_diagnostics(
            &pipeline,
            event_sender.clone(),
            video_liveness.stop_flag(),
            video_liveness.clone(),
        );
        let present_max_fps = Arc::new(AtomicU32::new(0));
        let d3d_fullscreen_sink = Arc::new(AtomicBool::new(false));
        let skip_webrtc_video = Arc::new(AtomicBool::new(false));
        let screenshot_grab = Arc::new(Mutex::new(None));
        let video_tap = Arc::new(Mutex::new(None));
        let game_audio_tap = Arc::new(Mutex::new(None));
        wire_incoming_media_sink(
            &pipeline,
            &webrtc,
            event_sender.clone(),
            render_state.clone(),
            present_max_fps.clone(),
            d3d_fullscreen_sink.clone(),
            skip_webrtc_video.clone(),
            video_liveness.clone(),
            screenshot_grab.clone(),
            video_tap.clone(),
            game_audio_tap.clone(),
        );

        pipeline
            .add(&webrtc)
            .map_err(|error| format!("Failed to add webrtcbin to pipeline: {error}"))?;
        pipeline
            .set_state(gst::State::Ready)
            .map_err(|error| format!("Failed to set GStreamer pipeline to Ready: {error:?}"))?;

        Ok(Self {
            pipeline,
            webrtc,
            input_state,
            input_channels: None,
            mic: Arc::new(Mutex::new(None)),
            #[cfg(target_os = "windows")]
            native_window_input_bridge: None,
            render_state,
            present_max_fps,
            d3d_fullscreen_sink,
            skip_webrtc_video,
            nvst_receive: None,
            video_liveness,
            screenshot_grab,
            video_tap,
            recording: Arc::new(Mutex::new(None)),
            // The SAME Arc that `wire_incoming_media_sink` populated: the game
            // audio chain stores its tap tee in this slot, and start_recording
            // reads it back. A fresh empty Arc here (the old bug) made every
            // recording report "no audio source (game audio or mic tap)" even
            // though the game-audio tap tee was attached and flowing. The mic
            // tap slot is the struct field itself (the deferred mic attach
            // thread writes into it via `self.mic_audio_tap`), so it stays a
            // fresh Arc here.
            game_audio_tap,
            mic_audio_tap: Arc::new(Mutex::new(None)),
            mic_transceiver: None,
            mic_payload_type: None,
            event_sender,
            original_remote_ice_credentials: None,
        })
    }

    pub(crate) fn parse_offer_sdp(sdp: &str) -> Result<gst_sdp::SDPMessage, String> {
        init_gstreamer()?;
        gst_sdp::SDPMessage::parse_buffer(sdp.as_bytes())
            .map_err(|error| format!("GStreamer rejected the remote SDP offer: {error:?}"))
    }

    pub(crate) fn webrtc_name(&self) -> String {
        self.webrtc.name().to_string()
    }

    pub(crate) fn set_present_max_fps(&self, fps: u32) {
        self.present_max_fps.store(fps, Ordering::SeqCst);
    }

    pub(crate) fn set_d3d_fullscreen_sink(&self, enabled: bool) {
        self.d3d_fullscreen_sink.store(enabled, Ordering::SeqCst);
    }

    pub(crate) fn configure_stats(
        &self,
        context: &NativeStreamerSessionContext,
        target_bitrate_kbps: u32,
    ) {
        self.video_liveness.configure(context, target_bitrate_kbps);
    }

    /// Attach classic NVST UDP video: appsrc → parse → decoder → sink, plus UDP recv thread.
    /// Keeps webrtcbin for SCTP input; ignores WebRTC RTP video pads.
    pub(crate) fn attach_nvst_video(
        &mut self,
        session: NvstVideoSession,
        fallback_codec: &str,
        requested_fps: Option<u32>,
        d3d_fullscreen_sink: bool,
    ) -> Result<(), String> {
        if self.nvst_receive.is_some() {
            return Ok(());
        }

        let codec = session
            .codec
            .as_deref()
            .filter(|c| !c.is_empty())
            .unwrap_or(fallback_codec);
        let codec_upper = codec.to_ascii_uppercase();
        let encoding = match codec_upper.as_str() {
            "H264" => "H264",
            "H265" | "HEVC" => "H265",
            other => {
                return Err(format!(
                    "NVST classic UDP video scaffold supports H264/H265, got {other}"
                ));
            }
        };

        self.skip_webrtc_video.store(true, Ordering::SeqCst);

        let (video_api, mut specs) = rtp_video_chain_specs(encoding, requested_fps).ok_or_else(|| {
            format!(
                "NVST Annex-B decode chain unavailable for {encoding}; install GStreamer plugins or set {NATIVE_VIDEO_BACKEND_ENV}=software."
            )
        })?;
        let zero_copy = specs.iter().any(|spec| {
            spec.caps
                .as_deref()
                .is_some_and(|caps| caps.contains("memory:D3D"))
        });
        // Drop RTP depayloader — appsrc feeds assembled Annex-B AUs.
        specs.retain(|spec| spec.role != RtpVideoChainRole::Depayloader);
        if specs
            .first()
            .is_none_or(|spec| spec.role != RtpVideoChainRole::Parser)
        {
            return Err(format!(
                "NVST video chain for {encoding} is missing a parser after depayloader removal."
            ));
        }

        let caps_str = annexb_appsrc_caps(encoding);
        let appsrc = gst::ElementFactory::make("appsrc")
            .name("nvst-annexb")
            .build()
            .map_err(|error| format!("Failed to create nvst-annexb appsrc: {error}"))?;
        let caps = caps_str
            .parse::<gst::Caps>()
            .map_err(|error| format!("Invalid NVST appsrc caps: {error}"))?;
        appsrc.set_property("caps", &caps);
        set_property_if_supported(&appsrc, "is-live", true);
        set_property_from_str_if_supported(&appsrc, "format", "time");
        set_property_if_supported(&appsrc, "block", false);
        set_property_if_supported(&appsrc, "max-bytes", 0u64);
        set_property_from_str_if_supported(&appsrc, "stream-type", "stream");

        let streaming_reported = Arc::new(AtomicBool::new(false));
        let mut elements: Vec<gst::Element> = Vec::with_capacity(specs.len() + 1);

        let result = (|| -> Result<(), String> {
            send_log(
                &self.event_sender,
                "info",
                format!(
                    "Attaching NVST classic UDP video ({encoding}) via appsrc Annex-B → {}; {}",
                    video_api.label(),
                    format_video_chain_selection(encoding, video_api, &specs)
                ),
            );

            let configured_present_max_fps = self.present_max_fps.load(Ordering::SeqCst);
            let effective = effective_present_max_fps(
                configured_present_max_fps,
                requested_fps,
                video_api,
                primary_display_refresh_hz(),
            );
            self.present_max_fps.store(effective, Ordering::SeqCst);

            self.pipeline
                .add(&appsrc)
                .map_err(|error| format!("Failed to add NVST appsrc: {error}"))?;
            elements.push(appsrc.clone());

            for spec in &specs {
                let element = make_element(spec.factory)?;
                configure_rtp_video_chain_element(
                    &element,
                    spec.clone(),
                    video_api,
                    d3d_fullscreen_sink,
                );
                if spec.role == RtpVideoChainRole::StatsOverlay {
                    self.video_liveness.set_stats_overlay(Some(element.clone()));
                }
                self.pipeline.add(&element).map_err(|error| {
                    format!(
                        "Failed to add {} for NVST {encoding} video chain: {error}",
                        spec.factory
                    )
                })?;
                elements.push(element);
            }

            for pair in elements.windows(2) {
                pair[0].link(&pair[1]).map_err(|error| {
                    format!(
                        "Failed to link {} -> {} for NVST {encoding}: {error:?}",
                        element_factory_name(&pair[0]),
                        element_factory_name(&pair[1])
                    )
                })?;
            }

            // Screenshot/recording tap is deferred: the tee is hot-plugged on
            // first use (see GstreamerVideoTap::ensure_tee) because attaching a
            // second branch while the D3D sink is warming up stalls its present
            // chain on some GStreamer releases.
            if elements.len() >= 2 {
                let before_sink = elements[elements.len() - 2].clone();
                let sink_element = elements[elements.len() - 1].clone();
                if let Ok(mut slot) = self.video_tap.lock() {
                    *slot = Some(GstreamerVideoTap {
                        tee: None,
                        before_sink,
                        sink: sink_element,
                        video_api,
                        zero_copy,
                    });
                }
                send_log(
                    &self.event_sender,
                    "info",
                    format!(
                        "Native video tap deferred for NVST {encoding}: tee hot-plugged on first screenshot/recording use."
                    ),
                );
            }

            let sink = elements
                .last()
                .ok_or_else(|| format!("NVST {encoding} video chain has no sink."))?;
            if let Some(post_decode_queue) =
                specs
                    .iter()
                    .zip(elements.iter().skip(1))
                    .find_map(|(spec, element)| {
                        (spec.role == RtpVideoChainRole::PostDecodeQueue).then_some(element)
                    })
            {
                self.video_liveness
                    .set_post_decode_queue(post_decode_queue.clone());
                watch_video_decoded_rate(
                    post_decode_queue,
                    &self.event_sender,
                    Some(self.video_liveness.clone()),
                );
            }
            if let Some(pre_decode_queue) =
                specs
                    .iter()
                    .zip(elements.iter().skip(1))
                    .find_map(|(spec, element)| {
                        (spec.role == RtpVideoChainRole::PreDecodeQueue).then_some(element)
                    })
            {
                self.video_liveness
                    .set_pre_decode_queue(pre_decode_queue.clone());
            }
            if let Some(parser) =
                specs
                    .iter()
                    .zip(elements.iter().skip(1))
                    .find_map(|(spec, element)| {
                        (spec.role == RtpVideoChainRole::Parser).then_some(element)
                    })
            {
                watch_video_caps_transitions(
                    parser,
                    "parser",
                    &self.event_sender,
                    self.video_liveness.clone(),
                );
            }
            if let Some(decoder) =
                specs
                    .iter()
                    .zip(elements.iter().skip(1))
                    .find_map(|(spec, element)| {
                        (spec.role == RtpVideoChainRole::Decoder).then_some(element)
                    })
            {
                self.video_liveness.set_decoder(decoder.clone());
                watch_video_caps_transitions(
                    decoder,
                    "decoder",
                    &self.event_sender,
                    self.video_liveness.clone(),
                );
            }

            self.render_state
                .set_video_sink(sink.clone(), &self.event_sender);
            install_present_limiter(
                sink,
                self.present_max_fps.clone(),
                &self.event_sender,
                Some(self.video_liveness.clone()),
            );
            watch_video_sink_caps_transitions(
                sink,
                &self.event_sender,
                Some(self.video_liveness.clone()),
            );
            watch_first_sink_buffer(sink, "video", &self.event_sender, &streaming_reported);
            watch_video_sink_rate(sink, &self.event_sender, Some(self.video_liveness.clone()));

            for element in &elements {
                element.sync_state_with_parent().map_err(|error| {
                    format!("Failed to sync NVST {encoding} video-chain element state: {error}")
                })?;
            }

            self.video_liveness.update_hardware_acceleration(format!(
                "GStreamer {} (NVST UDP)",
                video_api.label()
            ));
            self.video_liveness.start(
                self.pipeline.clone(),
                sink.clone(),
                self.event_sender.clone(),
            );

            let handle = spawn_nvst_udp_receive(session, appsrc, self.event_sender.clone())?;
            self.nvst_receive = Some(handle);

            // Ensure pipeline can run the appsrc branch even before WebRTC offer.
            let _ = self.pipeline.set_state(gst::State::Playing);

            Ok(())
        })();

        if result.is_err() {
            self.skip_webrtc_video.store(false, Ordering::SeqCst);
            for element in &elements {
                let _ = element.set_state(gst::State::Null);
                let _ = self.pipeline.remove(element);
            }
        }

        result
    }

    fn ensure_input_data_channels(
        &mut self,
        partial_reliable_threshold_ms: u32,
    ) -> Result<(), String> {
        if self.input_channels.is_some() {
            return Ok(());
        }

        self.input_state.reset();
        let channels = create_input_data_channels(
            &self.webrtc,
            self.input_state.clone(),
            self.event_sender.clone(),
            partial_reliable_threshold_ms,
        )?;
        let _ = channels.labels();
        self.input_channels = Some(channels);
        self.ensure_native_window_input_bridge();
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn ensure_native_window_input_bridge(&mut self) {
        // Win32 RawInput: floating external window OR internal child HWND.
        // Electron click-through across a topmost D3D sibling is unreliable.
        if self.native_window_input_bridge.is_some() {
            return;
        }
        let Some(input_channels) = self.input_channels.clone() else {
            return;
        };

        self.native_window_input_bridge = Some(NativeWindowInputBridge::start(
            self.input_state.clone(),
            input_channels,
            self.event_sender.clone(),
        ));
        if use_internal_renderer() {
            let hwnd = self.render_state.internal_renderer.child_handle();
            if hwnd != 0 && arm_internal_child_input(hwnd) {
                send_log(
                    &self.event_sender,
                    "info",
                    "Armed RawInput capture on the internal child HWND.".to_owned(),
                );
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn ensure_native_window_input_bridge(&mut self) {
        if use_internal_renderer() {
            return;
        }
        send_log(
            &self.event_sender,
            "warn",
            format!(
                "Native OS-level input capture is not implemented for {}; Electron input forwarding remains active.",
                std::env::consts::OS
            ),
        );
    }

    pub(crate) fn negotiate_answer(
        &mut self,
        offer_sdp: gst_sdp::SDPMessage,
        original_remote_credentials: Option<&IceCredentials>,
        partial_reliable_threshold_ms: u32,
        microphone_enabled: bool,
    ) -> Result<String, String> {
        let offer =
            gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Offer, offer_sdp);
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|error| {
                format!("Failed to set GStreamer pipeline to Playing before negotiation: {error:?}")
            })?;
        self.set_description("set-remote-description", &offer)?;
        if let Some(credentials) = original_remote_credentials {
            self.original_remote_ice_credentials = Some(credentials.clone());
            self.try_restore_original_remote_ice_credentials("after remote description")?;
        }
        self.ensure_input_data_channels(partial_reliable_threshold_ms)?;

        // Microphone: before creating the answer, force the offer's mic
        // m-line transceiver to sendonly with Opus codec preferences.
        // webrtcbin answers unassigned media m-lines recvonly, so without
        // this the mic m-line is never negotiated as sendonly and the server
        // never receives mic audio.
        //
        // REGRESSION NOTE (2026-08-09): this block used to run unconditionally
        // ("always-on" mic, official-client parity). That build stalled EVERY
        // video backend on this machine: d3d12/d3d11/vulkan all presented 0
        // frames after the first buffer and entered the stall-recovery loop,
        // while the same binary with the mic off rendered a stable 60fps
        // across multiple sessions (A/B: 02:34 log working, 09:56+ log
        // broken). ROOT CAUSE FOUND (2026-08-09): negotiating the mic m-line
        // sendonly makes webrtcbin create its SEND-path pad (SINK direction)
        // during set-local-description, and the pad-added handler in
        // wire_incoming_media_sink treated that send pad as incoming media:
        // is_rtp_pad() passed (application/x-rtp OPUS caps), the audio branch
        // created a spurious decodebin, added + synced it into the running
        // pipeline (state churn: Paused -> Paused pending Paused) and failed
        // the link with WrongDirection — stalling the video present chain in
        // every session (06:32 working log: 0 send-pad prep / 0 WrongDirection
        // / rendered climbs to 10k+; 07:45 broken log: 6 send-pad preps /
        // 6 WrongDirection / rendered=0 for H265+H264+AV1 on d3d12+d3d11,
        // with the deferred mic attach never even firing). The fix filters
        // SINK-direction pads out of the incoming-media handler (same check
        // that data channels — also sendonly but non-RTP — already passed
        // through untouched). The mic send path is therefore ON by default
        // again; the attach itself stays deferred (spawn_deferred_mic_attach)
        // and carries outgoing RTP ONLY when the user's mic is actually on
        // (real platform capture). There is deliberately no
        // generated-silence keepalive: a muted keepalive kept continuous
        // outgoing RTP alive, and that continuous stream was the only
        // structural delta between the stable 60fps era and the periodic
        // video stalls. When the mic is off (or capture is unavailable) no
        // RTP is sent at all, the server sends no Receiver Reports, and the
        // HUD ping falls back to the (often 0) server-reported
        // stats_channel field. OPENNOW_NATIVE_MIC=0 remains as a kill-switch
        // for any machine where the mic send path still stalls the video
        // present chain.
        let mic_attach_allowed = Self::native_mic_attach_allowed();
        if !mic_attach_allowed {
            send_log(
                &self.event_sender,
                "warn",
                "Native mic send path is force-disabled via OPENNOW_NATIVE_MIC=0; the mic m-line is not negotiated sendonly and no local RTCP round-trip is measured (HUD ping falls back to the server field)."
                    .to_owned(),
            );
        }
        let mic_transceiver = if mic_attach_allowed {
            self.prepare_mic_transceiver()
        } else {
            None
        };

        let answer = self.create_answer()?;
        let answer_sdp = answer
            .sdp()
            .as_text()
            .map_err(|error| format!("Failed to serialize GStreamer answer SDP: {error}"))?;

        // Only attach once a concrete Opus payload type was negotiated for
        // the mic m-line (mirrors the web client, which sends Opus mic).
        let mic_payload = mic_transceiver
            .as_ref()
            .and_then(|_| negotiated_mic_opus_payload(&answer_sdp));

        self.set_description("set-local-description", &answer)?;
        self.try_restore_original_remote_ice_credentials("after local description")?;

        if let Some(transceiver) = mic_transceiver {
            if let Some(payload_type) = mic_payload {
                // Keep the transceiver + payload so a mid-session mic toggle
                // (set_microphone_enabled with no pipeline built yet) can
                // attach the send path on demand.
                self.mic_transceiver = Some(transceiver.clone());
                self.mic_payload_type = Some(payload_type);
                // Attach asynchronously, only after the video sink has started
                // presenting (regression note above: attaching during video
                // warm-up stalls every backend's present chain). The mic
                // m-line is already sendonly in this answer; RTP starts once
                // the elements are linked.
                self.spawn_deferred_mic_attach(transceiver, payload_type, microphone_enabled);
            } else {
                send_log(
                    &self.event_sender,
                    "warn",
                    "The answer negotiated no Opus payload for the mic m-line; mic audio will not be sent."
                        .to_owned(),
                );
            }
        }

        Ok(answer_sdp)
    }

    pub(crate) fn try_restore_original_remote_ice_credentials(
        &mut self,
        stage: &str,
    ) -> Result<bool, String> {
        let Some(credentials) = self.original_remote_ice_credentials.clone() else {
            return Ok(false);
        };

        if credentials.ufrag.is_empty() || credentials.pwd.is_empty() {
            return Err(
                "Cannot restore original remote ICE credentials: offer credentials are empty."
                    .to_owned(),
            );
        }

        let Some(ice_agent) = self
            .webrtc
            .property::<Option<gst_webrtc::WebRTCICE>>("ice-agent")
        else {
            return Err(
                "Cannot restore original remote ICE credentials: webrtcbin has no ICE agent."
                    .to_owned(),
            );
        };
        let ice_agent_ptr = ice_agent.as_ptr() as *mut gst_webrtc::ffi::GstWebRTCICE;
        let ufrag = CString::new(credentials.ufrag.as_str())
            .map_err(|_| "Cannot restore original remote ICE credentials: ufrag contains NUL.")?;
        let pwd = CString::new(credentials.pwd.as_str())
            .map_err(|_| "Cannot restore original remote ICE credentials: pwd contains NUL.")?;

        let streams = self.negotiated_nice_streams();
        if streams.is_empty() {
            send_log(
                &self.event_sender,
                "warn",
                format!(
                    "GStreamer has not exposed actual NICE ICE streams {stage}; deferring GFN remote ICE credential restoration."
                ),
            );
            return Ok(false);
        }

        let mut restored = 0usize;
        let stream_ids = streams
            .iter()
            .map(|stream| stream.stream_id)
            .collect::<Vec<_>>();
        for stream in &streams {
            let accepted = unsafe {
                gst_webrtc::ffi::gst_webrtc_ice_set_remote_credentials(
                    ice_agent_ptr,
                    stream.ptr,
                    ufrag.as_ptr(),
                    pwd.as_ptr(),
                ) != glib::ffi::GFALSE
            };
            if accepted {
                restored += 1;
            } else {
                send_log(
                    &self.event_sender,
                    "warn",
                    format!(
                        "GStreamer ICE agent rejected original remote credentials for actual stream {}.",
                        stream.stream_id
                    ),
                );
            }
        }

        if restored == 0 {
            send_log(
                &self.event_sender,
                "warn",
                format!(
                    "GStreamer rejected original GFN remote ICE credentials on all actual streams {stage}; ICE may fail."
                ),
            );
            return Ok(false);
        }

        send_log(
            &self.event_sender,
            "info",
            format!(
                "Restored original GFN remote ICE credentials on {restored}/{} actual GStreamer NICE ICE stream(s) {stage}; streamIds={stream_ids:?}.",
                streams.len()
            ),
        );
        Ok(true)
    }

    fn negotiated_nice_streams(&self) -> Vec<ActualNiceIceStream> {
        let mut streams = Vec::new();
        let mut seen_stream_pointers = HashSet::new();
        let mut seen_transport_summaries = Vec::new();
        for index in 0..8 {
            let transceiver = self
                .webrtc
                .emit_by_name::<Option<gst_webrtc::WebRTCRTPTransceiver>>(
                    "get-transceiver",
                    &[&(index as i32)],
                );
            let Some(transceiver) = transceiver else {
                continue;
            };

            if let Some(receiver) = transceiver.receiver() {
                if let Some(transport) = receiver.transport() {
                    self.collect_nice_stream_from_dtls_transport(
                        &transport,
                        index,
                        "receiver",
                        &mut streams,
                        &mut seen_stream_pointers,
                        &mut seen_transport_summaries,
                    );
                }
            }
            if let Some(sender) = transceiver.sender() {
                if let Some(transport) = sender.transport() {
                    self.collect_nice_stream_from_dtls_transport(
                        &transport,
                        index,
                        "sender",
                        &mut streams,
                        &mut seen_stream_pointers,
                        &mut seen_transport_summaries,
                    );
                }
            }
        }

        if !seen_transport_summaries.is_empty() {
            send_log(
                &self.event_sender,
                "debug",
                format!(
                    "GStreamer negotiated ICE transports: {}.",
                    seen_transport_summaries.join(", ")
                ),
            );
        }
        streams
    }

    fn collect_nice_stream_from_dtls_transport(
        &self,
        dtls_transport: &gst_webrtc::WebRTCDTLSTransport,
        transceiver_index: u32,
        direction: &str,
        streams: &mut Vec<ActualNiceIceStream>,
        seen_stream_pointers: &mut HashSet<usize>,
        seen_transport_summaries: &mut Vec<String>,
    ) {
        let session_id = dtls_transport.session_id();
        let Some(ice_transport) = dtls_transport.transport() else {
            seen_transport_summaries.push(format!(
                "transceiver {transceiver_index} {direction} dtlsSession={session_id} iceTransport=none"
            ));
            return;
        };

        let transport_type = ice_transport.type_().name().to_owned();
        let component = ice_transport.component();
        let state = ice_transport.state();
        let Some(stream) = nice_stream_from_ice_transport(&ice_transport) else {
            seen_transport_summaries.push(format!(
                "transceiver {transceiver_index} {direction} dtlsSession={session_id} iceTransportType={transport_type} component={component:?} state={state:?} stream=none"
            ));
            return;
        };

        seen_transport_summaries.push(format!(
            "transceiver {transceiver_index} {direction} dtlsSession={session_id} iceTransportType={transport_type} component={component:?} state={state:?} streamId={}",
            stream.stream_id
        ));

        let stream_pointer = stream.ptr as usize;
        if seen_stream_pointers.insert(stream_pointer) {
            streams.push(stream);
        }
    }

    pub(crate) fn set_description(
        &self,
        signal_name: &'static str,
        description: &gst_webrtc::WebRTCSessionDescription,
    ) -> Result<(), String> {
        let promise = gst::Promise::new();
        self.webrtc
            .emit_by_name::<()>(signal_name, &[description, &promise]);
        wait_for_promise(&promise, signal_name)
    }

    fn create_answer(&self) -> Result<gst_webrtc::WebRTCSessionDescription, String> {
        let promise = gst::Promise::new();
        self.webrtc
            .emit_by_name::<()>("create-answer", &[&None::<gst::Structure>, &promise]);
        wait_for_promise(&promise, "create-answer")?;
        let reply = promise
            .get_reply()
            .ok_or_else(|| "GStreamer create-answer resolved without a reply.".to_owned())?;
        reply
            .get::<gst_webrtc::WebRTCSessionDescription>("answer")
            .map_err(|error| {
                format!(
                    "GStreamer create-answer reply did not contain an answer: {error}; reply={}",
                    describe_structure(reply)
                )
            })
    }

    pub(crate) fn add_remote_ice(&mut self, candidate: &IceCandidatePayload) -> Result<(), String> {
        if candidate.candidate.trim().is_empty() {
            return Err("Remote ICE candidate is empty.".to_owned());
        }
        self.try_restore_original_remote_ice_credentials("before adding remote ICE candidate")?;
        let sdp_m_line_index = candidate.sdp_m_line_index.unwrap_or(0);
        self.webrtc.emit_by_name::<()>(
            "add-ice-candidate",
            &[&sdp_m_line_index, &candidate.candidate],
        );
        Ok(())
    }

    pub(crate) fn set_microphone_enabled(&self, enabled: bool) {
        // Toggle an existing mic pipeline (mute/unmute the send path).
        let mut already_attached = false;
        if let Ok(slot) = self.mic.lock() {
            if let Some(mic) = slot.as_ref() {
                mic.set_enabled(enabled);
                already_attached = true;
            }
        }
        if already_attached || !enabled {
            return;
        }
        // No mic pipeline yet (the mic was OFF at session start): attach the
        // send path on demand using the negotiated transceiver/payload. The
        // attach is deferred until the video sink is presenting (same warm-up
        // guard as the session-start attach).
        let Some(transceiver) = self.mic_transceiver.clone() else {
            send_log(
                &self.event_sender,
                "warn",
                "Cannot enable the mic mid-session: no mic transceiver was negotiated (mic send path may be force-disabled via OPENNOW_NATIVE_MIC=0)."
                    .to_owned(),
            );
            return;
        };
        let Some(payload_type) = self.mic_payload_type else {
            send_log(
                &self.event_sender,
                "warn",
                "Cannot enable the mic mid-session: no Opus payload type was negotiated for the mic m-line."
                    .to_owned(),
            );
            return;
        };
        send_log(
            &self.event_sender,
            "info",
            "Mic toggled ON mid-session: attaching the mic send path (deferred until the video sink is presenting)."
                .to_owned(),
        );
        self.spawn_deferred_mic_attach(transceiver, payload_type, true);
    }

    pub(crate) fn mic_attached(&self) -> bool {
        self.mic.lock().map(|slot| slot.is_some()).unwrap_or(false)
    }

    /// Capture the last presented video frame as a PNG (base64) by briefly
    /// opening the valve-gated grab branch and pulling the newest encoded
    /// sample from the appsink. The grab branch (and the tap tee it hangs off)
    /// is built lazily on first use — after the sink is already presenting,
    /// which avoids the d3d12 present-chain stall a warm-up tee insertion
    /// triggers on some GStreamer releases.
    pub(crate) fn capture_screenshot(&self) -> Result<NativeScreenshotEvent, String> {
        let grab = {
            let mut slot = self
                .screenshot_grab
                .lock()
                .map_err(|_| "Screenshot grab lock poisoned.".to_owned())?;
            if slot.is_none() {
                *slot = Some(self.build_screenshot_grab()?);
            }
            slot.as_ref()
                .cloned()
                .ok_or_else(|| "Screenshot grab missing after build.".to_owned())?
        };
        grab.capture()
    }

    /// Lazily build the screenshot grab branch, hot-plugging the video tap tee
    /// first if it does not exist yet.
    fn build_screenshot_grab(&self) -> Result<GstreamerScreenshotGrab, String> {
        let (tee, video_api, zero_copy) = {
            let mut tap_slot = self
                .video_tap
                .lock()
                .map_err(|_| "Video tap lock poisoned.".to_owned())?;
            let tap = tap_slot.as_mut().ok_or_else(|| {
                "Screenshot capture is not ready: the native video chain has no video tap (waiting for game video)."
                    .to_owned()
            })?;
            (
                tap.ensure_tee(&self.pipeline)?,
                tap.video_api,
                tap.zero_copy,
            )
        };
        insert_screenshot_grab_branch(
            &self.pipeline,
            &tee,
            video_api,
            zero_copy,
            &self.event_sender,
        )
    }

    /// Start a native recording: open (or build, if missing/spent) the
    /// H.264/MP4 recording branch on the shared video tap.
    pub(crate) fn start_recording(&self) -> Result<(), String> {
        let (tee, video_api, zero_copy) = {
            let mut tap_slot = self
                .video_tap
                .lock()
                .map_err(|_| "Video tap lock poisoned.".to_owned())?;
            let tap = tap_slot.as_mut().ok_or_else(|| {
                "Recording is not ready: the native video chain has no video tap (waiting for game video)."
                    .to_owned()
            })?;
            // Hot-plug the tap tee lazily (after the sink is presenting) on
            // first use; subsequent recordings reuse the same tee.
            (
                tap.ensure_tee(&self.pipeline)?,
                tap.video_api,
                tap.zero_copy,
            )
        };
        let pipeline = self.pipeline.clone();

        let mut slot = self
            .recording
            .lock()
            .map_err(|_| "Recording state lock poisoned.".to_owned())?;
        if let Some(state) = slot.as_ref() {
            if state.active.load(Ordering::SeqCst) {
                return Ok(());
            }
            if state.spent.load(Ordering::SeqCst) {
                // The muxer is spent after an EOS-finalized recording; rebuild
                // a fresh branch so a second recording works in the session.
                teardown_recording_branch(&pipeline, state);
                *slot = None;
            }
        }
        if slot.is_none() {
            let state = insert_recording_branch(
                &pipeline,
                &tee,
                video_api,
                zero_copy,
                &self.game_audio_tap,
                &self.mic_audio_tap,
                self.event_sender.clone(),
            )?;
            *slot = Some(state);
        }
        let state = slot
            .as_ref()
            .ok_or_else(|| "Recording branch missing after start.".to_owned())?;
        state.start()
    }

    /// Stop the active recording. `finalize=true` flushes the encoder/muxer
    /// with EOS and emits `recording-finished` (via the event channel, strictly
    /// after every chunk); `finalize=false` aborts (valve closed, branch kept).
    pub(crate) fn stop_recording(&self, finalize: bool) -> Result<(), String> {
        let state = self.recording.lock().ok().and_then(|slot| slot.clone());
        let Some(state) = state else {
            return Ok(());
        };
        if !state.active.load(Ordering::SeqCst) {
            return Ok(());
        }
        state.stop(finalize)?;
        if finalize {
            if let Some(event_sender) = &self.event_sender {
                let _ = event_sender.send(Event::RecordingFinished {
                    thumbnail_base64: state.thumbnail(),
                });
            }
        }
        Ok(())
    }

    /// Locate the offer's mic m-line transceiver and force it sendonly with
    /// Opus codec preferences. Without this, webrtcbin answers unassigned
    /// audio m-lines with recvonly (or rejects them) because no track is
    /// attached, and the server never receives mic audio.
    ///
    /// Only reachable when native_mic_attach_allowed() is true (see the
    /// negotiate() regression note; default on, OPENNOW_NATIVE_MIC=0 to
    /// force-disable).
    fn prepare_mic_transceiver(&self) -> Option<gst_webrtc::WebRTCRTPTransceiver> {
        let Some(transceiver) = self.find_mic_transceiver() else {
            send_log(
                &self.event_sender,
                "warn",
                "Microphone is enabled but the server offer has no mic m-line; mic audio will not be sent."
                    .to_owned(),
            );
            return None;
        };

        let opus_caps: gst::Caps = "application/x-rtp, media=(string)audio, encoding-name=(string)OPUS, payload=(int)[0,127]"
            .parse()
            .expect("valid Opus RTP caps");
        transceiver.set_codec_preferences(Some(&opus_caps));
        transceiver.set_direction(gst_webrtc::WebRTCRTPTransceiverDirection::Sendonly);
        send_log(
            &self.event_sender,
            "info",
            format!(
                "Prepared mic transceiver (mid={:?}, mline={}) for sendonly Opus.",
                transceiver.mid(),
                transceiver.mlineindex()
            ),
        );
        Some(transceiver)
    }

    /// The offer's mic m-line is the audio transceiver on mid "3" (GFN layout:
    /// audio=0, video=1, application=2, mic=3). Falls back to the last audio
    /// transceiver that is not the recvonly game-audio line.
    fn find_mic_transceiver(&self) -> Option<gst_webrtc::WebRTCRTPTransceiver> {
        let mut audio = Vec::new();
        for index in 0..8i32 {
            let transceiver = self
                .webrtc
                .emit_by_name::<Option<gst_webrtc::WebRTCRTPTransceiver>>(
                    "get-transceiver",
                    &[&index],
                );
            let Some(transceiver) = transceiver else {
                break;
            };
            if transceiver.kind() == gst_webrtc::WebRTCKind::Audio {
                audio.push(transceiver);
            }
        }
        if audio.is_empty() {
            return None;
        }
        if let Some(found) = audio
            .iter()
            .find(|transceiver| transceiver.mid().as_deref() == Some("3"))
        {
            return Some(found.clone());
        }
        if let Some(found) = audio.iter().rev().find(|transceiver| {
            transceiver.direction() != gst_webrtc::WebRTCRTPTransceiverDirection::Recvonly
        }) {
            return Some(found.clone());
        }
        audio.first().cloned()
    }

    /// Attach the mic send path asynchronously, deferred until the video sink
    /// has started presenting frames. The 09:53 regression showed that
    /// attaching the mic pipeline while the video present chain is still
    /// warming up stalls EVERY backend (rendered stays 0 after the first
    /// buffer); the same warm-sink guard as the tap-tee hot-plug avoids that
    /// race. The mic m-line is already negotiated sendonly in the answer, so
    /// RTP simply starts flowing once the elements are linked here.
    fn spawn_deferred_mic_attach(
        &self,
        transceiver: gst_webrtc::WebRTCRTPTransceiver,
        payload_type: u32,
        enabled: bool,
    ) {
        let pipeline = self.pipeline.clone();
        let webrtc = self.webrtc.clone();
        let event_sender = self.event_sender.clone();
        let video_tap = self.video_tap.clone();
        let mic = self.mic.clone();
        let mic_audio_tap = self.mic_audio_tap.clone();
        std::thread::spawn(move || {
            // Mic off → attach NOTHING. There is deliberately no muted
            // generated-silence keepalive: continuous outgoing RTP (even
            // silence) was the only structural delta behind the periodic
            // video stalls, so the mic send path must be completely dead when
            // the mic is off. With no outgoing RTP the server sends no
            // Receiver Reports and the HUD ping falls back to the
            // server-reported stats_channel field.
            if !enabled {
                send_log(
                    &event_sender,
                    "info",
                    "Mic is off — no mic RTP path is attached (no silence keepalive); the HUD ping falls back to the server-reported stats field."
                        .to_owned(),
                );
                return;
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            let mut no_stats_head_start: Option<std::time::Instant> = None;
            let warm = loop {
                let state = video_tap.lock().ok().and_then(|slot| {
                    slot.as_ref().map(|tap| {
                        (
                            tap.sink.clone(),
                            sink_rendered_frame_count(&tap.sink).unwrap_or(0),
                        )
                    })
                });
                match state {
                    Some((sink, rendered)) if sink.find_property("stats").is_some() => {
                        if rendered >= 8 {
                            break true;
                        }
                    }
                    Some((_, _)) => {
                        // Sink without a rendered counter (vulkan/software):
                        // allow a short head start once the sink exists.
                        let started =
                            *no_stats_head_start.get_or_insert_with(std::time::Instant::now);
                        if started.elapsed() >= std::time::Duration::from_millis(500) {
                            break true;
                        }
                    }
                    None => no_stats_head_start = None,
                }
                if std::time::Instant::now() >= deadline {
                    break false;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            };
            if !warm {
                send_log(
                    &event_sender,
                    "warn",
                    "Deferred mic attach timed out waiting for the video sink to present frames; mic audio will not be sent."
                        .to_owned(),
                );
                return;
            }
            match Self::build_mic_pipeline(
                &pipeline,
                &webrtc,
                &event_sender,
                &transceiver,
                payload_type,
                enabled,
            ) {
                Ok((mic_pipeline, mic_tap)) => {
                    if let Ok(mut slot) = mic.lock() {
                        *slot = Some(mic_pipeline);
                    }
                    if let Ok(mut slot) = mic_audio_tap.lock() {
                        *slot = mic_tap;
                    }
                    send_log(
                        &event_sender,
                        "info",
                        "Native mic RTP path attached after video warm-up.".to_owned(),
                    );
                }
                Err(message) => send_log(
                    &event_sender,
                    "warn",
                    format!("Deferred microphone attach failed: {message}"),
                ),
            }
        });
    }

    /// Whether the native mic send path may be negotiated/attached at all.
    /// Default ON: the stall was traced to the pad-added handler treating the
    /// mic send pad (SINK direction) as incoming media (spurious decodebin +
    /// WrongDirection + pipeline state churn during set-local-description),
    /// which is now fixed — see the regression note in negotiate_answer and
    /// the direction filter at the top of wire_incoming_media_sink's
    /// pad-added handler. The attach is still deferred until the video sink
    /// has presented a few frames (spawn_deferred_mic_attach).
    /// OPENNOW_NATIVE_MIC=0 force-disables it (kill-switch) for any machine
    /// where the mic send path still stalls the video present chain.
    fn native_mic_attach_allowed() -> bool {
        match std::env::var_os("OPENNOW_NATIVE_MIC") {
            Some(value) => value != "0",
            None => true,
        }
    }

    /// Create the platform's default microphone capture source. GStreamer's
    /// audio source elements are platform-specific: `wasapi2src` (Windows),
    /// `osxaudiosrc` (macOS), and `pulsesrc`/`pipewiresrc` (Linux, tried in
    /// order since PipeWire ships a PulseAudio shim). `autoaudiosrc` is the
    /// final cross-platform fallback (ALSA on Linux, CoreAudio on macOS).
    fn create_mic_source_element() -> Result<(gst::Element, &'static str), String> {
        #[cfg(target_os = "windows")]
        let candidates: &[&str] = &["wasapi2src", "autoaudiosrc"];
        #[cfg(target_os = "macos")]
        let candidates: &[&str] = &["osxaudiosrc", "autoaudiosrc"];
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let candidates: &[&str] = &["pulsesrc", "pipewiresrc", "autoaudiosrc"];

        let mut last_error: Option<String> = None;
        for factory in candidates {
            let element = gst::ElementFactory::make(factory)
                .name("mic-audiosrc")
                .build();
            match element {
                Ok(element) => return Ok((element, factory)),
                Err(error) => last_error = Some(error.to_string()),
            }
        }
        Err(format!(
            "No microphone capture source available (tried {candidates:?}): {}",
            last_error.unwrap_or_else(|| "unknown".to_owned())
        ))
    }

    /// Find the webrtcbin sink pad (`sink_%u`) that belongs to the mic
    /// transceiver. It is created when the local description (sendonly mic
    /// m-line) is applied, so this must run after `set-local-description`.
    fn find_transceiver_sink_pad(
        webrtc: &gst::Element,
        transceiver: &gst_webrtc::WebRTCRTPTransceiver,
    ) -> Option<gst::Pad> {
        for pad in webrtc.pads() {
            if pad.direction() != gst::PadDirection::Sink {
                continue;
            }
            let pad_transceiver: Option<gst_webrtc::WebRTCRTPTransceiver> =
                pad.property("transceiver");
            if pad_transceiver
                .as_ref()
                .is_some_and(|other| other.as_ptr() == transceiver.as_ptr())
            {
                return Some(pad);
            }
        }
        None
    }

    /// Build the platform capture → Opus → RTP send path and link it into the
    /// mic transceiver's sink pad. The payload type comes from the negotiated
    /// answer so the RTP we send matches the SDP the server accepted.
    /// `enabled` must be true (callers gate on it): when the mic is off there
    /// is deliberately NO generated-silence keepalive — continuous outgoing
    /// RTP (even muted silence) was the only structural delta behind the
    /// periodic video stalls — so no mic pipeline exists at all and the HUD
    /// ping falls back to the server stats field. Capture unavailability is
    /// likewise NOT masked with silence: it aborts the attach (Err).
    /// Returns the mic pipeline plus the mic-tap queue (recording audio tap).
    fn build_mic_pipeline(
        pipeline: &gst::Pipeline,
        webrtc: &gst::Element,
        event_sender: &Option<Sender<Event>>,
        transceiver: &gst_webrtc::WebRTCRTPTransceiver,
        payload_type: u32,
        enabled: bool,
    ) -> Result<(GstreamerMicPipeline, Option<gst::Element>), String> {
        let Some(sink_pad) = Self::find_transceiver_sink_pad(webrtc, transceiver) else {
            return Err(
                "webrtcbin created no send pad for the mic transceiver after set-local-description."
                    .to_owned(),
            );
        };

        let result = (|| -> Result<Vec<gst::Element>, String> {
            let mut elements = Vec::with_capacity(6);
            // Mic ON → real capture ONLY. There is deliberately no
            // generated-silence keepalive fallback: a continuous outgoing RTP
            // stream (even muted silence) was the only structural delta
            // between the stable 60fps era and the periodic video stalls, so
            // the mic send path must be completely dead whenever there is no
            // real capture — whether the mic is off at session start or the
            // capture device is unavailable. With no outgoing RTP the server
            // sends no Receiver Reports and the HUD ping falls back to the
            // server-reported stats_channel field.
            if !enabled {
                return Err(
                    "mic disabled — no mic RTP path is built (no silence keepalive).".to_owned(),
                );
            }
            let (source, source_factory) = match GstreamerPipeline::create_mic_source_element() {
                Ok((source, factory)) => {
                    set_property_if_supported(&source, "low-latency", true);
                    (source, factory)
                }
                Err(error) => {
                    return Err(format!(
                            "Microphone capture unavailable ({error}); no mic RTP is sent (no generated-silence keepalive) — the HUD ping falls back to the server stats field."
                        ));
                }
            };

            let volume = gst::ElementFactory::make("volume")
                .name("mic-volume")
                .build()
                .map_err(|error| format!("Failed to create mic volume: {error}"))?;
            volume.set_property("volume", 1.0f64);

            let convert = gst::ElementFactory::make("audioconvert")
                .name("mic-audioconvert")
                .build()
                .map_err(|error| format!("Failed to create mic audioconvert: {error}"))?;
            let resample = gst::ElementFactory::make("audioresample")
                .name("mic-audioresample")
                .build()
                .map_err(|error| format!("Failed to create mic audioresample: {error}"))?;
            let encoder = gst::ElementFactory::make("opusenc")
                .name("mic-opusenc")
                .build()
                .map_err(|error| format!("Failed to create mic opusenc: {error}"))?;
            let payloader = gst::ElementFactory::make("rtpopuspay")
                .name("mic-rtpopuspay")
                .build()
                .map_err(|error| format!("Failed to create mic rtpopuspay: {error}"))?;
            payloader.set_property("pt", payload_type);

            for element in [&source, &volume, &convert, &resample, &encoder, &payloader] {
                pipeline.add(element).map_err(|error| {
                    format!("Failed to add {} to the pipeline: {error}", element.name())
                })?;
                elements.push(element.clone());
            }
            send_log(
                event_sender,
                "info",
                format!("Native microphone source: {source_factory} (platform capture)."),
            );

            // Recording tap after the mute volume: the tee itself is stored
            // (no dangling queue — a queue built while the mic chain was idle
            // parked its src task on FLOW_UNLINKED and never restarted when
            // relinked at recording time). The recording branch requests a
            // fresh src pad per recording and releases it on teardown, exactly
            // like the game-audio and video taps. Muting the mic (volume 0)
            // also silences it in recordings because the tap sits after the
            // mute volume.
            let tap_tee = gst::ElementFactory::make("tee")
                .name("mic-tap-tee")
                .build()
                .map_err(|error| format!("Failed to create mic tap tee: {error}"))?;
            pipeline.add(&tap_tee).map_err(|error| {
                format!("Failed to add {} to the pipeline: {error}", tap_tee.name())
            })?;
            elements.push(tap_tee.clone());

            for pair in [
                (&source, &volume),
                (&volume, &tap_tee),
                (&tap_tee, &convert),
                (&convert, &resample),
                (&resample, &encoder),
                (&encoder, &payloader),
            ] {
                pair.0.link(pair.1).map_err(|error| {
                    format!(
                        "Failed to link {} -> {}: {error:?}",
                        pair.0.name(),
                        pair.1.name()
                    )
                })?;
            }
            let payloader_src = payloader
                .static_pad("src")
                .ok_or_else(|| "mic rtpopuspay has no src pad.".to_owned())?;
            payloader_src.link(&sink_pad).map_err(|error| {
                format!("Failed to link mic rtpopuspay -> webrtcbin sink pad: {error:?}")
            })?;

            for element in &elements {
                element.sync_state_with_parent().map_err(|error| {
                    format!(
                        "Failed to sync mic element {} state: {error}",
                        element.name()
                    )
                })?;
            }

            send_log(
                event_sender,
                "info",
                format!(
                    "Native mic RTP path attached: {source_factory} (capture) → Opus → RTP payload {payload_type} on mid {:?}.",
                    transceiver.mid()
                ),
            );
            Ok(elements)
        })();

        match result {
            Ok(elements) => {
                let mic_tap = elements
                    .iter()
                    .find(|element| element.name() == "mic-tap-tee")
                    .cloned();
                let mic_pipeline = GstreamerMicPipeline {
                    volume: elements
                        .iter()
                        .find(|element| element.name() == "mic-volume")
                        .cloned()
                        .ok_or_else(|| "mic-volume element missing after attach.".to_owned())?,
                    elements,
                };
                Ok((mic_pipeline, mic_tap))
            }
            Err(message) => Err(message),
        }
    }

    pub(crate) fn send_input_packet(&self, payload: &[u8], partially_reliable: bool) -> bool {
        if !self.input_state.ready.load(Ordering::SeqCst)
            || self.input_state.paused.load(Ordering::SeqCst)
        {
            return false;
        }

        let Some(input_channels) = &self.input_channels else {
            return false;
        };

        input_channels.send_packet(payload, partially_reliable)
    }

    pub(crate) fn set_input_paused(&self, paused: bool) {
        self.input_state.paused.store(paused, Ordering::SeqCst);
        // Shared with the platform so the stacked guard tick cannot re-arm the
        // sink RawInput capture (re-hiding the cursor) while an overlay is open.
        crate::gstreamer_input::set_input_paused_flag(paused);
        if paused {
            release_native_input_capture();
        } else {
            // Stacked mode: (re)arm the sink-window RawInput mouse + keyboard
            // capture so input bypasses the Electron bridge entirely (low
            // latency). No-op in other render modes / when not ready.
            arm_stacked_sink_input_capture();
        }
    }

    pub(crate) fn update_render_surface(&self, surface: NativeRenderSurface) {
        self.video_liveness
            .set_stats_overlay_visible(surface.visible && surface.show_stats);
        self.render_state.set_surface(surface, &self.event_sender);
    }

    pub(crate) fn stop(mut self) -> Result<(), String> {
        if let Some(handle) = self.nvst_receive.take() {
            handle.stop();
        }
        if let Ok(mut slot) = self.mic.lock() {
            if let Some(mic) = slot.take() {
                for element in mic.elements {
                    let _ = element.set_state(gst::State::Null);
                    let _ = self.pipeline.remove(&element);
                }
            }
        }
        self.skip_webrtc_video.store(false, Ordering::SeqCst);
        self.video_liveness.set_stats_overlay_visible(false);
        self.render_state.stop_external_renderer_window_guard();
        stop_stacked_renderer_window_guard();
        self.render_state.destroy_internal_renderer();
        #[cfg(target_os = "windows")]
        if let Some(mut bridge) = self.native_window_input_bridge.take() {
            bridge.stop();
        }
        self.input_state.stop_heartbeat();
        self.video_liveness.stop();
        self.pipeline
            .set_state(gst::State::Null)
            .map(|_| ())
            .map_err(|error| format!("Failed to stop GStreamer pipeline: {error:?}"))
    }
}

pub(crate) fn resolve_gstreamer_stun_server(ice_servers: &[IceServer]) -> String {
    ice_servers
        .iter()
        .flat_map(|server| server.urls.iter())
        .find_map(|url| {
            let url = url.trim();
            if url.starts_with("stun://") {
                Some(url.to_owned())
            } else {
                url.strip_prefix("stun:")
                    .map(|endpoint| format!("stun://{endpoint}"))
            }
        })
        .unwrap_or_else(|| DEFAULT_GFN_STUN_SERVER.to_owned())
}

fn nice_stream_from_ice_transport(
    transport: &gst_webrtc::WebRTCICETransport,
) -> Option<ActualNiceIceStream> {
    if transport.type_().name() != "GstWebRTCNiceTransport" {
        return None;
    }

    unsafe {
        let transport_ptr = transport.as_ptr() as *mut GstWebRTCNiceTransportCompat;
        if transport_ptr.is_null() {
            return None;
        }

        let stream_ptr = (*transport_ptr).stream;
        if stream_ptr.is_null() {
            return None;
        }

        Some(ActualNiceIceStream {
            ptr: stream_ptr,
            stream_id: (*stream_ptr).stream_id,
        })
    }
}

pub(crate) fn init_gstreamer() -> Result<(), String> {
    gst::init().map_err(|error| format!("Failed to initialize GStreamer: {error}"))?;
    #[cfg(target_os = "linux")]
    {
        static RTP_PLUGIN_REGISTRATION: OnceLock<Result<(), String>> = OnceLock::new();
        RTP_PLUGIN_REGISTRATION
            .get_or_init(|| {
                if gst::ElementFactory::find("rtpav1depay").is_some() {
                    return Ok(());
                }
                gstrsrtp::plugin_register_static().map_err(|error| {
                    format!("Failed to register the bundled AV1 RTP plugin: {error}")
                })
            })
            .clone()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(())
    }
}

pub(crate) fn set_property_if_supported<T: Into<glib::Value>>(
    element: &gst::Element,
    name: &str,
    value: T,
) {
    if let Some(property) = element.find_property(name) {
        if !property.flags().contains(glib::ParamFlags::WRITABLE) {
            return;
        }

        let value = value.into();
        let value_type = value.type_();
        let property_type = property.value_type();
        if value_type == property_type || value_type.is_a(property_type) {
            element.set_property_from_value(name, &value);
        }
    }
}

pub(crate) fn set_property_from_str_if_supported(element: &gst::Element, name: &str, value: &str) {
    if element.find_property(name).is_some() {
        element.set_property_from_str(name, value);
    }
}

pub(crate) fn configure_webrtc_low_latency(webrtc: &gst::Element) {
    set_property_if_supported(webrtc, "latency", WEBRTC_LATENCY_MS);
}

pub(crate) fn configure_queue_for_low_latency(element: &gst::Element, media_label: &str) {
    let max_buffers = if media_label == "video" {
        VIDEO_QUEUE_MAX_BUFFERS
    } else {
        AUDIO_QUEUE_MAX_BUFFERS
    };

    configure_queue(element, max_buffers, true);
}

pub(crate) fn configure_queue(element: &gst::Element, max_buffers: u32, leaky_downstream: bool) {
    set_property_if_supported(element, "max-size-buffers", max_buffers);
    set_property_if_supported(element, "max-size-bytes", 0u32);
    set_property_if_supported(element, "max-size-time", 0u64);
    if leaky_downstream {
        set_property_from_str_if_supported(element, "leaky", "downstream");
    } else {
        set_property_from_str_if_supported(element, "leaky", "no");
    }
}

pub(crate) fn configure_sink_for_low_latency(element: &gst::Element) {
    // GFN-aligned present: never clock-sync or QoS-throttle the sink. Latency
    // comes from decode + a depth-1 leaky post-decode queue + optional present
    // limiter, not from GstBaseSink pacing.
    set_property_if_supported(element, "sync", false);
    set_property_if_supported(element, "async", false);
    set_property_if_supported(element, "qos", false);
    set_property_if_supported(element, "max-lateness", -1i64);
    set_property_if_supported(element, "processing-deadline", 0u64);
    set_property_if_supported(element, "render-delay", 0u64);
    set_property_if_supported(element, "throttle-time", 0u64);
    set_property_if_supported(element, "enable-last-sample", false);
    set_property_if_supported(element, "show-preroll-frame", false);
    set_property_if_supported(element, "redraw-on-update", true);
    set_property_if_supported(element, "force-aspect-ratio", true);
}

/// Configure d3d11/d3d12videosink for low-latency Internal/External present.
///
/// GStreamer docs: `fullscreen` is ignored unless `fullscreen-toggle-mode`
/// includes `property`. Internal always keeps exclusive fullscreen off (caller
/// passes `d3d_fullscreen_sink=false`); External + Cloud G-Sync may enable it.
pub(crate) fn configure_d3d_video_sink(element: &gst::Element, d3d_fullscreen_sink: bool) {
    configure_sink_for_low_latency(element);
    // d3d12 only: attaching the swapchain directly to an external HWND can turn
    // a present stall into upstream decode backpressure on the child-surface path.
    set_property_if_supported(element, "direct-swapchain", false);
    set_property_if_supported(element, "error-on-closed", false);
    // RawInput owns mouse/keyboard; do not let the sink emit GstNavigation events.
    set_property_if_supported(element, "enable-navigation-events", false);
    set_property_if_supported(element, "fullscreen-on-alt-enter", false);
    if d3d_fullscreen_sink {
        set_property_from_str_if_supported(element, "fullscreen-toggle-mode", "property");
        set_property_if_supported(element, "fullscreen", true);
    } else {
        set_property_from_str_if_supported(element, "fullscreen-toggle-mode", "none");
        set_property_if_supported(element, "fullscreen", false);
    }
}

pub(crate) fn configure_stats_overlay_element(element: &gst::Element) {
    set_property_if_supported(element, "visible", false);
    set_property_if_supported(element, "text", "");
    set_property_if_supported(element, "auto-resize", true);
    set_property_if_supported(element, "layout-x", 0.018f64);
    set_property_if_supported(element, "layout-y", 0.018f64);
    set_property_if_supported(element, "layout-width", 0.55f64);
    set_property_if_supported(element, "layout-height", 0.18f64);
    set_property_if_supported(element, "font-family", "Cascadia Mono");
    set_property_if_supported(element, "font-size", 18f32);
    set_property_from_str_if_supported(element, "text-alignment", "leading");
    set_property_from_str_if_supported(element, "paragraph-alignment", "near");
    set_property_if_supported(element, "foreground-color", 0xF2FF_FFFFu32);
    set_property_if_supported(element, "outline-color", 0xD000_0000u32);
}

pub(crate) fn wait_for_promise(promise: &gst::Promise, operation: &str) -> Result<(), String> {
    match promise.wait() {
        gst::PromiseResult::Replied => {
            if let Some(reply) = promise.get_reply() {
                if reply.has_field("error") {
                    return Err(format!(
                        "GStreamer promise returned an error during {operation}: {}",
                        describe_structure(reply)
                    ));
                }
            }
            Ok(())
        }
        gst::PromiseResult::Interrupted => {
            Err(format!("GStreamer promise interrupted during {operation}."))
        }
        gst::PromiseResult::Expired => {
            Err(format!("GStreamer promise expired during {operation}."))
        }
        gst::PromiseResult::Pending => Err(format!(
            "GStreamer promise still pending during {operation}."
        )),
        other => Err(format!(
            "GStreamer promise failed during {operation}: {other:?}"
        )),
    }
}

pub(crate) fn describe_structure(structure: &gst::StructureRef) -> String {
    let fields = structure
        .iter()
        .map(|(name, value)| {
            let rendered = value
                .get::<&glib::Error>()
                .map(|error| format!("{error:?}"))
                .unwrap_or_else(|_| format!("{value:?}"));
            format!("{}={rendered}", name.as_str())
        })
        .collect::<Vec<_>>();

    format!("{} {{{}}}", structure.name().as_str(), fields.join(", "))
}

fn wire_local_ice_events(
    webrtc: &gst::Element,
    event_sender: Option<Sender<Event>>,
) -> Result<(), String> {
    let Some(event_sender) = event_sender else {
        return Ok(());
    };

    webrtc.connect("on-ice-candidate", false, move |values| {
        let sdp_m_line_index = values.get(1).and_then(glib_value_to_u32).unwrap_or(0);
        let candidate = values
            .get(2)
            .and_then(|value| value.get::<String>().ok())
            .unwrap_or_default();

        if !candidate.trim().is_empty() {
            let _ = event_sender.send(Event::LocalIce {
                candidate: IceCandidatePayload {
                    candidate,
                    sdp_mid: Some(sdp_m_line_index.to_string()),
                    sdp_m_line_index: Some(sdp_m_line_index),
                    username_fragment: None,
                },
            });
        }

        None
    });
    Ok(())
}

fn glib_value_to_u32(value: &glib::Value) -> Option<u32> {
    let value_type = value.type_();
    if value_type == u32::static_type() {
        return value.get::<u32>().ok();
    }
    if value_type == i32::static_type() {
        return value
            .get::<i32>()
            .ok()
            .and_then(|value| u32::try_from(value).ok());
    }
    if value_type == u64::static_type() {
        return value
            .get::<u64>()
            .ok()
            .and_then(|value| u32::try_from(value).ok());
    }
    if value_type == i64::static_type() {
        return value
            .get::<i64>()
            .ok()
            .and_then(|value| u32::try_from(value).ok());
    }
    None
}

fn wire_webrtc_state_events(webrtc: &gst::Element, event_sender: Option<Sender<Event>>) {
    wire_webrtc_property_event(
        webrtc,
        event_sender.clone(),
        "ice-connection-state",
        "ICE connection state",
    );
    wire_webrtc_property_event(
        webrtc,
        event_sender.clone(),
        "ice-gathering-state",
        "ICE gathering state",
    );
    wire_webrtc_property_event(
        webrtc,
        event_sender,
        "connection-state",
        "peer connection state",
    );
}

fn wire_webrtc_property_event(
    webrtc: &gst::Element,
    event_sender: Option<Sender<Event>>,
    property_name: &'static str,
    label: &'static str,
) {
    if event_sender.is_none() || webrtc.find_property(property_name).is_none() {
        return;
    }

    webrtc.connect_notify(Some(property_name), move |element, _| {
        let value = element.property_value(property_name);
        send_log(
            &event_sender,
            "debug",
            format!("GStreamer WebRTC {label}: {value:?}."),
        );
    });
}

fn start_gstreamer_bus_diagnostics(
    pipeline: &gst::Pipeline,
    event_sender: Option<Sender<Event>>,
    stop: Arc<AtomicBool>,
    video_liveness: VideoLivenessMonitor,
) {
    let Some(bus) = pipeline.bus() else {
        send_log(
            &event_sender,
            "warn",
            "GStreamer pipeline has no bus; native diagnostics will be limited.".to_owned(),
        );
        return;
    };

    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            let Some(message) = bus.timed_pop_filtered(
                gst::ClockTime::from_mseconds(250),
                &[
                    gst::MessageType::Error,
                    gst::MessageType::Warning,
                    gst::MessageType::Qos,
                    gst::MessageType::Latency,
                    gst::MessageType::StateChanged,
                    gst::MessageType::Eos,
                ],
            ) else {
                continue;
            };

            match message.view() {
                gst::MessageView::Error(error) => send_log(
                    &event_sender,
                    "error",
                    format!(
                        "GStreamer bus error from {}: {}; debug={:?}.",
                        message_src_name(&message),
                        error.error(),
                        error.debug()
                    ),
                ),
                gst::MessageView::Warning(warning) => send_log(
                    &event_sender,
                    "warn",
                    format!(
                        "GStreamer bus warning from {}: {}; debug={:?}.",
                        message_src_name(&message),
                        warning.error(),
                        warning.debug()
                    ),
                ),
                gst::MessageView::Qos(_) => send_log(
                    &event_sender,
                    "debug",
                    format!(
                        "GStreamer bus QoS from {}: {}.",
                        message_src_name(&message),
                        message_structure_summary(&message)
                    ),
                ),
                gst::MessageView::Latency(_) => send_log(
                    &event_sender,
                    "debug",
                    format!(
                        "GStreamer bus latency update from {}.",
                        message_src_name(&message)
                    ),
                ),
                gst::MessageView::StateChanged(state) => {
                    if message
                        .src()
                        .and_then(|src| src.clone().downcast::<gst::Pipeline>().ok())
                        .is_some()
                    {
                        send_log(
                            &event_sender,
                            "debug",
                            format!(
                                "GStreamer pipeline state changed: {:?} -> {:?} pending {:?}.",
                                state.old(),
                                state.current(),
                                state.pending()
                            ),
                        );
                        video_liveness.record_transition(
                            "pipeline-state-change",
                            "pipeline",
                            Some(format!("{:?}", state.old())),
                            Some(format!("{:?}", state.current())),
                            None,
                            None,
                            None,
                            None,
                            &event_sender,
                        );
                    }
                }
                gst::MessageView::Eos(_) => send_log(
                    &event_sender,
                    "warn",
                    format!("GStreamer bus EOS from {}.", message_src_name(&message)),
                ),
                _ => {}
            }
        }
    });
}

fn message_src_name(message: &gst::Message) -> String {
    message
        .src()
        .map(|src| src.path_string().to_string())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn message_structure_summary(message: &gst::Message) -> String {
    message
        .structure()
        .map(|structure| structure.to_string())
        .unwrap_or_else(|| "no structure".to_owned())
}

fn wire_incoming_media_sink(
    pipeline: &gst::Pipeline,
    webrtc: &gst::Element,
    event_sender: Option<Sender<Event>>,
    render_state: GstreamerRenderState,
    present_max_fps: Arc<AtomicU32>,
    d3d_fullscreen_sink: Arc<AtomicBool>,
    skip_webrtc_video: Arc<AtomicBool>,
    video_liveness: VideoLivenessMonitor,
    _screenshot_grab: Arc<Mutex<Option<GstreamerScreenshotGrab>>>,
    video_tap: Arc<Mutex<Option<GstreamerVideoTap>>>,
    game_audio_tap: Arc<Mutex<Option<gst::Element>>>,
) {
    let pipeline = pipeline.downgrade();
    let streaming_reported = Arc::new(AtomicBool::new(false));
    webrtc.connect_pad_added(move |_webrtc, src_pad| {
        let Some(pipeline) = pipeline.upgrade() else {
            return;
        };
        let event_sender = event_sender.clone();

        // Only incoming (SRC-direction) pads carry media from the peer.
        // webrtcbin also emits pad-added for SEND-path pads (SINK
        // direction) — e.g. the mic transceiver's send pad once the local
        // description makes it sendonly. Those must never be treated as
        // incoming media: the old code let the mic send pad through
        // is_rtp_pad (its caps are application/x-rtp OPUS), created a
        // spurious decodebin, added + synced it into the running pipeline
        // (state churn: Paused -> Paused pending Paused) and then failed to
        // link with WrongDirection — stalling the video present chain on
        // every session where the mic m-line was negotiated sendonly
        // (regression note above; data channels are also sendonly but their
        // pads are not RTP, which is why they never tripped this).
        if !is_incoming_media_pad(src_pad) {
            send_log(
                &event_sender,
                "debug",
                format!(
                    "Ignoring WebRTC pad (incoming_media={}, direction={:?}, caps={:?}).",
                    is_incoming_media_pad(src_pad),
                    src_pad.direction(),
                    pad_caps_name(src_pad)
                ),
            );
            return;
        }

        if !is_rtp_pad(src_pad) {
            send_log(
                &event_sender,
                "debug",
                format!(
                    "Ignoring non-RTP WebRTC pad with caps {:?}.",
                    pad_caps_name(src_pad)
                ),
            );
            return;
        }

        if let Some(encoding) = rtp_video_encoding(src_pad) {
            if skip_webrtc_video.load(Ordering::SeqCst) {
                send_log(
                    &event_sender,
                    "info",
                    format!(
                        "Ignoring WebRTC RTP video pad ({encoding}); NVST classic UDP owns video."
                    ),
                );
                if let Err(error) =
                    link_decoded_media_to_fakesink(&pipeline, src_pad, "ignored webrtc video")
                {
                    send_log(&event_sender, "debug", error);
                }
                return;
            }
            match link_rtp_video_pad(
                &pipeline,
                src_pad,
                &encoding,
                &render_state,
                &event_sender,
                &streaming_reported,
                present_max_fps.clone(),
                d3d_fullscreen_sink.load(Ordering::SeqCst),
                video_liveness.clone(),
                &video_tap,
            ) {
                Ok(()) => return,
                Err(error) => send_log(
                    &event_sender,
                    "warn",
                    format!("{error}; falling back to decodebin."),
                ),
            }
        }

        let decodebin = match make_element("decodebin") {
            Ok(decodebin) => decodebin,
            Err(error) => {
                send_log(&event_sender, "warn", error);
                return;
            }
        };

        let decode_pipeline = pipeline.downgrade();
        let decode_sender = event_sender.clone();
        let decode_render_state = render_state.clone();
        let decode_streaming_reported = streaming_reported.clone();
        let decode_video_liveness = video_liveness.clone();
        let decode_game_audio_tap = game_audio_tap.clone();
        decodebin.connect_pad_added(move |_decodebin, decoded_pad| {
            let Some(pipeline) = decode_pipeline.upgrade() else {
                return;
            };
            let media_kind = decoded_media_kind(decoded_pad);
            if let Err(error) = link_decoded_media_pad(
                &pipeline,
                decoded_pad,
                &decode_render_state,
                &decode_sender,
                &decode_streaming_reported,
                &decode_video_liveness,
                &decode_game_audio_tap,
            ) {
                send_log(&decode_sender, "warn", error);
                if let Err(fallback_error) =
                    link_decoded_media_to_fakesink(&pipeline, decoded_pad, "decoded media fallback")
                {
                    send_log(&decode_sender, "warn", fallback_error);
                }
                return;
            }

            send_log(
                &decode_sender,
                "info",
                format!(
                    "Linked decoded {} stream to native sink chain.",
                    media_kind.label()
                ),
            );
        });

        if let Err(error) = pipeline.add(&decodebin) {
            send_log(
                &event_sender,
                "warn",
                format!("Failed to add decodebin: {error}"),
            );
            return;
        }
        if let Err(error) = decodebin.sync_state_with_parent() {
            send_log(
                &event_sender,
                "warn",
                format!("Failed to sync decodebin state: {error}"),
            );
            return;
        }

        let Some(sink_pad) = decodebin.static_pad("sink") else {
            send_log(
                &event_sender,
                "warn",
                "decodebin has no sink pad.".to_owned(),
            );
            return;
        };
        if let Err(error) = src_pad.link(&sink_pad) {
            send_log(
                &event_sender,
                "warn",
                format!("Failed to link WebRTC RTP pad to decodebin: {error:?}"),
            );
        } else if rtp_video_encoding(src_pad).is_some() {
            video_liveness.set_rtp_video_src_pad(src_pad);
        }
    });
}

impl DecodedMediaKind {
    fn label(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Unknown => "unknown",
        }
    }
}

/// Whether a webrtcbin pad represents media coming IN from the peer.
/// Only SRC-direction pads do. webrtcbin emits pad-added for its SEND-path
/// pads too (SINK direction — e.g. the mic transceiver's send pad once the
/// local description is sendonly); those must be excluded or the incoming-
/// media handler would treat the mic send pad as a peer stream, create a
/// spurious decodebin in the running pipeline and fail the link with
/// WrongDirection (stalls the video present chain — see negotiate_answer's
/// regression note).
pub(crate) fn is_incoming_media_pad(pad: &gst::Pad) -> bool {
    pad.direction() == gst::PadDirection::Src
}

fn is_rtp_pad(pad: &gst::Pad) -> bool {
    pad_caps_name(pad)
        .as_deref()
        .is_some_and(|name| name == "application/x-rtp")
}

fn pad_caps_name(pad: &gst::Pad) -> Option<String> {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    caps.structure(0)
        .map(|structure| structure.name().to_string())
}

fn decoded_media_kind(pad: &gst::Pad) -> DecodedMediaKind {
    match pad_caps_name(pad).as_deref() {
        Some(name) if name.starts_with("video/") => DecodedMediaKind::Video,
        Some(name) if name.starts_with("audio/") => DecodedMediaKind::Audio,
        _ => DecodedMediaKind::Unknown,
    }
}

fn rtp_video_encoding(pad: &gst::Pad) -> Option<String> {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    let structure = caps.structure(0)?;
    if structure.name() != "application/x-rtp" {
        return None;
    }

    let media = structure.get::<String>("media").ok()?;
    if media != "video" {
        return None;
    }

    structure
        .get::<String>("encoding-name")
        .ok()
        .map(|encoding| encoding.to_ascii_uppercase())
}

fn rtp_video_depayloader_factory(codec: &str) -> Option<&'static str> {
    match codec {
        "H265" | "HEVC" => Some("rtph265depay"),
        "H264" => Some("rtph264depay"),
        "AV1" => Some("rtpav1depay"),
        _ => None,
    }
}

fn rtp_video_parser_factory(codec: &str) -> Option<&'static str> {
    match codec {
        "H265" | "HEVC" => Some("h265parse"),
        "H264" => Some("h264parse"),
        "AV1" => Some("av1parse"),
        _ => None,
    }
}

/// Post-decode capsfilter caps for the D3D11/D3D12 memory path.
///
/// Returns the memory caps unchanged. 10-bit-capable codecs (AV1, H265/HEVC)
/// no longer force `format=NV12` here: the D3D DXVA decoders refuse to convert
/// 10-bit → 8-bit internally (`not-negotiated` when the output caps demand
/// NV12 for a 10-bit stream). The conversion is instead done by the explicit
/// download + videoconvert stage those codecs get in
/// `rtp_video_chain_definition` and the Vulkan-internal chain.
pub(crate) fn post_decode_caps_for(
    _video_api: RtpVideoApi,
    _codec: &str,
    memory_caps: &str,
) -> String {
    memory_caps.to_owned()
}

/// Capsfilter spec for H265/HEVC receive chains.
///
/// GStreamer's `rtph265depay` (observed in the bundled 1.29.x runtime)
/// rejects receive-pad caps that carry the H265 fmtp parameters (`profile-id`,
/// `level-id`, `tier-flag`) which GFN echoes from its offer into the
/// negotiated caps — the receive pad fails with `not-negotiated` right after
/// the first RTP packet arrives, killing the whole session. H264's equivalent
/// fmtp fields are accepted by `rtph264depay`, and AV1's offer carries no such
/// fields, so only H265/HEVC needs this filter. The depayloader reconstructs
/// profile/level from the in-band VPS/SPS/PPS, so stripping the fields costs
/// nothing.
fn h265_receive_caps_strip_filter(codec: &str) -> Option<RtpVideoChainSpec> {
    if matches!(codec, "H265" | "HEVC") {
        Some(RtpVideoChainSpec::with_caps(
            "capsfilter",
            RtpVideoChainRole::ReceiveCapsFilter,
            "application/x-rtp, encoding-name=H265",
        ))
    } else {
        None
    }
}

pub(crate) fn rtp_video_chain_definition(
    encoding: &str,
    video_api: RtpVideoApi,
) -> Option<Vec<RtpVideoChainSpec>> {
    let codec = encoding.to_ascii_uppercase();

    #[cfg(target_os = "windows")]
    if video_api == RtpVideoApi::Vulkan {
        return windows_vulkan_present_chain_definition(codec.as_str());
    }

    let mut specs = Vec::with_capacity(8);
    if let Some(filter) = h265_receive_caps_strip_filter(codec.as_str()) {
        specs.push(filter);
    }
    specs.push(RtpVideoChainSpec::new(
        rtp_video_depayloader_factory(codec.as_str())?,
        RtpVideoChainRole::Depayloader,
    ));
    specs.push(RtpVideoChainSpec::new(
        rtp_video_parser_factory(codec.as_str())?,
        RtpVideoChainRole::Parser,
    ));
    specs.push(RtpVideoChainSpec::new(
        "queue",
        RtpVideoChainRole::PreDecodeQueue,
    ));
    specs.push(RtpVideoChainSpec::new(
        video_api.decoder_factory(codec.as_str())?,
        RtpVideoChainRole::Decoder,
    ));

    let is_ten_bit_capable = matches!(codec.as_str(), "AV1" | "H265" | "HEVC");
    let is_d3d = matches!(video_api, RtpVideoApi::D3D11 | RtpVideoApi::D3D12);
    // Keep D3D11 H264 zero-copy available, but avoid the D3D12 H264 zero-copy
    // path. The field log showed d3d12h264dec producing D3D12Memory correctly
    // for the first frame, then stopping decode while RTP continued flowing
    // (decoded=0, sink=0, rendered=33). The download + system-memory path is
    // already used successfully for H265/AV1 and avoids that driver/sink
    // present deadlock. Ten-bit codecs still require the same path on both
    // D3D backends because d3d11/d3d12videosink cannot reliably present their
    // native D3D textures.
    let needs_safe_system_memory_present =
        is_d3d && (is_ten_bit_capable || video_api == RtpVideoApi::D3D12);
    if needs_safe_system_memory_present {
        // Download the D3D texture, convert to 8-bit NV12, and let the sink
        // upload system memory. This also makes the H264 D3D12 path resilient
        // to the mid-stream zero-copy stall observed in production.
        let download = match video_api {
            RtpVideoApi::D3D12 => "d3d12download",
            _ => "d3d11download",
        };
        specs.push(RtpVideoChainSpec::new(
            download,
            RtpVideoChainRole::PostDecodeConverter,
        ));
        specs.push(RtpVideoChainSpec::new(
            "videoconvert",
            RtpVideoChainRole::PostDecodeConverter,
        ));
        specs.push(RtpVideoChainSpec::with_caps(
            "capsfilter",
            RtpVideoChainRole::PostDecodeCapsFilter,
            "video/x-raw,format=NV12",
        ));
    } else if let Some(memory_caps) = video_api.memory_caps() {
        specs.push(RtpVideoChainSpec::with_caps(
            "capsfilter",
            RtpVideoChainRole::PostDecodeCapsFilter,
            post_decode_caps_for(video_api, codec.as_str(), memory_caps),
        ));
    }
    if let Some(converter) = video_api.post_decode_converter_factory() {
        specs.push(RtpVideoChainSpec::new(
            converter,
            RtpVideoChainRole::PostDecodeConverter,
        ));
    }
    if let Some(overlay) = video_api.stats_overlay_factory() {
        specs.push(RtpVideoChainSpec::new(
            overlay,
            RtpVideoChainRole::StatsOverlay,
        ));
    }
    specs.push(RtpVideoChainSpec::new(
        "queue",
        RtpVideoChainRole::PostDecodeQueue,
    ));
    specs.push(RtpVideoChainSpec::new(
        video_api.sink_factory(),
        RtpVideoChainRole::Sink,
    ));

    Some(specs)
}

/// Windows Vulkan path.
///
/// Electron Internal hole-punch only composites DXGI swapchains on the child HWND.
/// A Win32 Vulkan surface on that HWND (or a GSTVULKAN child of it) presents black
/// even though vulkansink reports rendered frames. So:
/// - Internal: DXVA decode + `d3d12videosink` (D3D11 fallback; visible in Electron)
/// - External: DXVA decode + convert/upload + `vulkansink` (true Vulkan present)
///
/// Native `vulkanh264dec` currently access-violates under NVIDIA Windows drivers.
#[cfg(target_os = "windows")]
fn windows_vulkan_present_chain_definition(codec: &str) -> Option<Vec<RtpVideoChainSpec>> {
    if use_internal_renderer() {
        windows_vulkan_internal_present_chain_definition(codec)
    } else {
        windows_vulkan_external_present_chain_definition(codec)
    }
}

/// Internal Electron path: DXVA + D3D12 present (D3D11 fallback; DXGI hole-punch).
#[cfg(target_os = "windows")]
fn windows_vulkan_internal_present_chain_definition(codec: &str) -> Option<Vec<RtpVideoChainSpec>> {
    let decoder = RtpVideoApi::Vulkan.decoder_factory(codec)?;
    let prefer_d3d12 = decoder.starts_with("d3d12");
    let sink = if prefer_d3d12 {
        "d3d12videosink"
    } else {
        "d3d11videosink"
    };
    let memory_api = if prefer_d3d12 {
        RtpVideoApi::D3D12
    } else {
        RtpVideoApi::D3D11
    };
    let mut specs = Vec::with_capacity(8);
    if let Some(filter) = h265_receive_caps_strip_filter(codec) {
        specs.push(filter);
    }
    specs.push(RtpVideoChainSpec::new(
        rtp_video_depayloader_factory(codec)?,
        RtpVideoChainRole::Depayloader,
    ));
    specs.push(RtpVideoChainSpec::new(
        rtp_video_parser_factory(codec)?,
        RtpVideoChainRole::Parser,
    ));
    specs.push(RtpVideoChainSpec::new(
        "queue",
        RtpVideoChainRole::PreDecodeQueue,
    ));
    specs.push(RtpVideoChainSpec::new(decoder, RtpVideoChainRole::Decoder));
    if matches!(codec, "AV1" | "H265" | "HEVC") {
        // Same safe present path as the main D3D chain: 10-bit-capable DXVA
        // textures present as gray/pink garbage through zero-copy D3DMemory.
        let download = if prefer_d3d12 {
            "d3d12download"
        } else {
            "d3d11download"
        };
        specs.push(RtpVideoChainSpec::new(
            download,
            RtpVideoChainRole::PostDecodeConverter,
        ));
        specs.push(RtpVideoChainSpec::new(
            "videoconvert",
            RtpVideoChainRole::PostDecodeConverter,
        ));
        specs.push(RtpVideoChainSpec::with_caps(
            "capsfilter",
            RtpVideoChainRole::PostDecodeCapsFilter,
            "video/x-raw,format=NV12",
        ));
    } else if let Some(memory_caps) = memory_api.memory_caps() {
        specs.push(RtpVideoChainSpec::with_caps(
            "capsfilter",
            RtpVideoChainRole::PostDecodeCapsFilter,
            post_decode_caps_for(memory_api, codec, memory_caps),
        ));
    }
    specs.push(RtpVideoChainSpec::new(
        "dwritetextoverlay",
        RtpVideoChainRole::StatsOverlay,
    ));
    specs.push(RtpVideoChainSpec::new(
        "queue",
        RtpVideoChainRole::PostDecodeQueue,
    ));
    specs.push(RtpVideoChainSpec::new(sink, RtpVideoChainRole::Sink));
    Some(specs)
}

/// External / capability path: DXVA + vulkanupload + vulkansink.
#[cfg(target_os = "windows")]
fn windows_vulkan_external_present_chain_definition(codec: &str) -> Option<Vec<RtpVideoChainSpec>> {
    let decoder = RtpVideoApi::Vulkan.decoder_factory(codec)?;
    let mut specs = Vec::with_capacity(10);
    if let Some(filter) = h265_receive_caps_strip_filter(codec) {
        specs.push(filter);
    }
    specs.push(RtpVideoChainSpec::new(
        rtp_video_depayloader_factory(codec)?,
        RtpVideoChainRole::Depayloader,
    ));
    specs.push(RtpVideoChainSpec::new(
        rtp_video_parser_factory(codec)?,
        RtpVideoChainRole::Parser,
    ));
    specs.push(RtpVideoChainSpec::new(
        "queue",
        RtpVideoChainRole::PreDecodeQueue,
    ));
    specs.push(RtpVideoChainSpec::new(decoder, RtpVideoChainRole::Decoder));
    // Composite diagnostics while frames are still in the DXVA/D3D path.
    // dwritetextoverlay cannot consume VulkanImage memory after upload.
    specs.push(RtpVideoChainSpec::new(
        "dwritetextoverlay",
        RtpVideoChainRole::StatsOverlay,
    ));
    specs.push(RtpVideoChainSpec::new(
        "d3d11download",
        RtpVideoChainRole::PostDecodeConverter,
    ));
    specs.push(RtpVideoChainSpec::new(
        "videoconvert",
        RtpVideoChainRole::PostDecodeConverter,
    ));
    specs.push(RtpVideoChainSpec::with_caps(
        "capsfilter",
        RtpVideoChainRole::PostDecodeCapsFilter,
        "video/x-raw,format=RGBA",
    ));
    specs.push(RtpVideoChainSpec::new(
        "vulkanupload",
        RtpVideoChainRole::PostDecodeConverter,
    ));
    specs.push(RtpVideoChainSpec::new(
        "queue",
        RtpVideoChainRole::PostDecodeQueue,
    ));
    specs.push(RtpVideoChainSpec::new(
        RtpVideoApi::Vulkan.sink_factory(),
        RtpVideoChainRole::Sink,
    ));
    Some(specs)
}

fn preferred_rtp_video_apis(requested_fps: Option<u32>) -> Vec<RtpVideoApi> {
    let requested = requested_video_backend();
    preferred_rtp_video_apis_for(requested.as_str(), requested_fps)
}

pub(crate) fn preferred_rtp_video_apis_for(
    requested: &str,
    requested_fps: Option<u32>,
) -> Vec<RtpVideoApi> {
    match requested {
        "d3d11" => vec![RtpVideoApi::D3D11],
        "d3d12" => vec![RtpVideoApi::D3D12],
        "videotoolbox" | "vt" => vec![RtpVideoApi::VideoToolbox],
        "nvdec" | "nvcodec" | "nvidia" => vec![RtpVideoApi::Nvdec],
        "vaapi" | "va" => vec![RtpVideoApi::Vaapi],
        "v4l2" | "v4l2stateless" => vec![RtpVideoApi::V4L2],
        "vulkan" | "vk" => vec![RtpVideoApi::Vulkan],
        "software" | "sw" => vec![RtpVideoApi::Software],
        _ => default_rtp_video_api_priority(requested_fps),
    }
}

pub(crate) fn effective_present_max_fps(
    configured_present_max_fps: u32,
    requested_fps: Option<u32>,
    video_api: RtpVideoApi,
    display_hz: Option<u32>,
) -> u32 {
    if configured_present_max_fps == PRESENT_LIMITER_VRR_SENTINEL {
        if !matches!(video_api, RtpVideoApi::D3D11 | RtpVideoApi::D3D12) {
            return 0;
        }
        return requested_fps
            .filter(|fps| *fps > 0)
            .map(|fps| vrr_present_max_fps(fps, display_hz))
            .unwrap_or(0);
    }

    if configured_present_max_fps != PRESENT_LIMITER_AUTO_SENTINEL {
        return configured_present_max_fps;
    }

    // D3D11/D3D12 present (and Internal Vulkan→D3D) need the auto limiter so
    // stream fps above display Hz does not stall the DXGI present path.
    if !matches!(video_api, RtpVideoApi::D3D11 | RtpVideoApi::D3D12)
        && !(cfg!(target_os = "windows")
            && video_api == RtpVideoApi::Vulkan
            && use_internal_renderer())
    {
        return 0;
    }

    requested_fps
        .filter(|fps| *fps > 0)
        .map(|fps| automatic_present_max_fps(fps, display_hz))
        .unwrap_or(0)
}

pub(crate) fn default_rtp_video_api_priority(requested_fps: Option<u32>) -> Vec<RtpVideoApi> {
    #[cfg(target_os = "windows")]
    {
        if should_prefer_d3d12_for_high_fps(requested_fps) {
            return vec![
                RtpVideoApi::D3D12,
                RtpVideoApi::D3D11,
                RtpVideoApi::Software,
            ];
        }
        vec![
            RtpVideoApi::D3D11,
            RtpVideoApi::D3D12,
            RtpVideoApi::Software,
        ]
    }
    #[cfg(target_os = "macos")]
    {
        let _ = requested_fps;
        vec![RtpVideoApi::VideoToolbox, RtpVideoApi::Software]
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        let _ = requested_fps;
        vec![
            RtpVideoApi::V4L2,
            RtpVideoApi::Nvdec,
            RtpVideoApi::Vaapi,
            RtpVideoApi::Vulkan,
            RtpVideoApi::Software,
        ]
    }
    #[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
    {
        let _ = requested_fps;
        vec![
            RtpVideoApi::Nvdec,
            RtpVideoApi::Vaapi,
            RtpVideoApi::Vulkan,
            RtpVideoApi::V4L2,
            RtpVideoApi::Software,
        ]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = requested_fps;
        vec![RtpVideoApi::Software]
    }
}

fn should_prefer_d3d12_for_high_fps(requested_fps: Option<u32>) -> bool {
    requested_fps.is_some_and(|fps| fps >= 200)
}

/// Whether the native streamer can build a complete RTP decode chain for this
/// codec on this device (depayloader + parser + decoder + sink all present).
/// Used by offer preparation to decide whether the requested codec can be
/// safely hard-filtered, or whether fallback codecs must be kept in the offer.
pub(crate) fn can_decode_rtp_codec(codec: &str) -> bool {
    rtp_video_chain_specs(codec, None).is_some()
}

fn rtp_video_chain_specs(
    encoding: &str,
    requested_fps: Option<u32>,
) -> Option<(RtpVideoApi, Vec<RtpVideoChainSpec>)> {
    preferred_rtp_video_apis(requested_fps)
        .into_iter()
        .find_map(|video_api| {
            rtp_video_chain_specs_for_api(encoding, video_api, requested_fps)
                .map(|specs| (video_api, specs))
        })
}

/// Chain specs for one specific API, or None when that API cannot build a
/// complete chain for this codec (missing decoder/sink/plugin elements).
/// Shared by initial selection and the decoder-fallback ladder, which must be
/// able to rebuild with a *specific* next candidate rather than re-running
/// the preference search.
fn rtp_video_chain_specs_for_api(
    encoding: &str,
    video_api: RtpVideoApi,
    requested_fps: Option<u32>,
) -> Option<Vec<RtpVideoChainSpec>> {
    let codec = encoding.to_ascii_uppercase();
    let decoder = select_decoder_factory(video_api, codec.as_str())?;
    let sink = select_sink_factory(video_api)?;
    let mut specs = rtp_video_chain_definition(encoding, video_api)?;
    for spec in &mut specs {
        if spec.role == RtpVideoChainRole::Decoder {
            spec.factory = decoder;
        } else if spec.role == RtpVideoChainRole::Sink {
            spec.factory = sink;
        }
    }
    align_windows_vulkan_download_factory(&mut specs, decoder);
    align_windows_vulkan_internal_present(&mut specs, decoder);
    insert_requested_fps_capssetter(&mut specs, requested_fps);
    specs.retain(|spec| {
        spec.role != RtpVideoChainRole::StatsOverlay
            || gst::ElementFactory::find(spec.factory).is_some()
    });
    required_video_chain_elements_available(&specs).then_some(specs)
}

/// Decoder-fallback ladder: every API that could decode this stream, ordered
/// most-preferred first, with the platform's natural priority merged in so a
/// forced backend (e.g. `OPENNOW_NATIVE_VIDEO_BACKEND=d3d12`) still falls
/// back to D3D11 / software instead of failing hard. The currently-selected
/// API is excluded by the caller.
pub(crate) fn rtp_video_chain_fallback_ladder(
    encoding: &str,
    current: RtpVideoApi,
    requested_fps: Option<u32>,
) -> Vec<RtpVideoApi> {
    let mut ladder: Vec<RtpVideoApi> = Vec::new();
    for api in preferred_rtp_video_apis(requested_fps)
        .into_iter()
        .chain(default_rtp_video_api_priority(requested_fps).into_iter())
    {
        if api != current && !ladder.contains(&api) {
            ladder.push(api);
        }
    }
    // Software decode is the guaranteed-last fallback everywhere; keep it at
    // the tail even when a platform priority list omitted it.
    if RtpVideoApi::Software != current && !ladder.contains(&RtpVideoApi::Software) {
        ladder.push(RtpVideoApi::Software);
    }
    ladder.retain(|api| rtp_video_chain_specs_for_api(encoding, *api, requested_fps).is_some());
    ladder
}

pub(crate) fn align_windows_vulkan_download_factory(
    specs: &mut Vec<RtpVideoChainSpec>,
    decoder: &str,
) {
    #[cfg(target_os = "windows")]
    {
        let download = if decoder.starts_with("d3d12") {
            Some("d3d12download")
        } else if decoder.starts_with("d3d11") {
            Some("d3d11download")
        } else {
            // NVDEC Windows outputs system memory in our bundle, and software
            // decoders (dav1ddec / avdec_*) always output system memory; a D3D
            // download stage cannot accept either. Drop the download element;
            // the existing videoconvert + NV12 capsfilter still normalize the
            // system-memory frames for the D3D sink.
            None
        };

        match download {
            Some(factory) => {
                if let Some(spec) = specs.iter_mut().find(|spec| {
                    spec.role == RtpVideoChainRole::PostDecodeConverter
                        && (spec.factory == "d3d11download" || spec.factory == "d3d12download")
                }) {
                    spec.factory = factory;
                }
            }
            None => {
                specs.retain(|spec| {
                    !(spec.role == RtpVideoChainRole::PostDecodeConverter
                        && (spec.factory == "d3d11download" || spec.factory == "d3d12download"))
                });
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (specs, decoder);
    }
}

/// Keep Internal Vulkan→D3D present matched to the selected DXVA decoder family.
#[cfg(target_os = "windows")]
fn align_windows_vulkan_internal_present(specs: &mut Vec<RtpVideoChainSpec>, decoder: &str) {
    if !use_internal_renderer() {
        return;
    }
    let has_d3d_present = specs.iter().any(|spec| {
        spec.role == RtpVideoChainRole::Sink
            && (spec.factory == "d3d11videosink" || spec.factory == "d3d12videosink")
    });
    if !has_d3d_present {
        return;
    }

    let (sink, memory_caps) = if decoder.starts_with("d3d12")
        && gst::ElementFactory::find("d3d12videosink").is_some()
    {
        ("d3d12videosink", RtpVideoApi::D3D12.memory_caps())
    } else if decoder.starts_with("d3d11") && gst::ElementFactory::find("d3d11videosink").is_some()
    {
        ("d3d11videosink", RtpVideoApi::D3D11.memory_caps())
    } else {
        return;
    };

    if let Some(spec) = specs
        .iter_mut()
        .find(|spec| spec.role == RtpVideoChainRole::Sink)
    {
        spec.factory = sink;
    }
    if let Some(spec) = specs
        .iter_mut()
        .find(|spec| spec.role == RtpVideoChainRole::PostDecodeCapsFilter)
    {
        if let Some(caps) = memory_caps {
            spec.caps = Some(caps.to_owned());
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn align_windows_vulkan_internal_present(_specs: &mut Vec<RtpVideoChainSpec>, _decoder: &str) {}

fn insert_requested_fps_capssetter(specs: &mut Vec<RtpVideoChainSpec>, requested_fps: Option<u32>) {
    let Some(fps) = requested_fps.filter(|fps| *fps > 0) else {
        return;
    };
    if gst::ElementFactory::find("capssetter").is_none() {
        return;
    }
    // Windows Vulkan hybrid / D3D present: forcing plain video/x-raw onto a D3D
    // memory pad breaks caps negotiation.
    if specs.iter().any(|spec| {
        matches!(
            spec.factory,
            "d3d11download"
                | "d3d12download"
                | "vulkanupload"
                | "d3d11videosink"
                | "d3d12videosink"
                | "d3d11h264dec"
                | "d3d11h265dec"
                | "d3d11av1dec"
                | "d3d12h264dec"
                | "d3d12h265dec"
                | "d3d12av1dec"
        )
    }) {
        return;
    }
    let Some(decoder_index) = specs
        .iter()
        .position(|spec| spec.role == RtpVideoChainRole::Decoder)
    else {
        return;
    };

    specs.insert(
        decoder_index + 1,
        RtpVideoChainSpec::with_caps(
            "capssetter",
            RtpVideoChainRole::PostDecodeRateSetter,
            format!("video/x-raw,framerate=(fraction){fps}/1"),
        ),
    );
}

fn select_decoder_factory(video_api: RtpVideoApi, codec: &str) -> Option<&'static str> {
    let primary = video_api.decoder_factory(codec)?;
    // Per-codec override first: OPENNOW_NATIVE_AV1_DECODER / H265. Some Windows
    // D3D DXVA decoders are broken on specific GPUs (e.g. Intel UHD AV1
    // hardware decode corrupts every frame), so users can force dav1ddec or
    // avdec_h265. A missing override factory is ignored, falling back to the
    // normal selection below.
    if let Some(forced) = forced_decoder_factory(codec) {
        if decoder_factory_usable(forced) {
            return Some(forced);
        }
    }
    std::iter::once(primary)
        .chain(video_api.fallback_decoder_factories(codec).iter().copied())
        .find(|factory| decoder_factory_usable(factory))
}

/// Honor the `OPENNOW_NATIVE_AV1_DECODER` / `OPENNOW_NATIVE_H265_DECODER`
/// overrides, mapping each preference to a concrete decoder factory.
fn forced_decoder_factory(codec: &str) -> Option<&'static str> {
    match codec.to_ascii_uppercase().as_str() {
        "AV1" => match av1_decoder_preference() {
            CodecDecoderPreference::Auto => None,
            CodecDecoderPreference::D3D12 => Some("d3d12av1dec"),
            CodecDecoderPreference::D3D11 => Some("d3d11av1dec"),
            CodecDecoderPreference::Software => Some("dav1ddec"),
        },
        "H265" | "HEVC" => match h265_decoder_preference() {
            CodecDecoderPreference::Auto => None,
            CodecDecoderPreference::D3D12 => Some("d3d12h265dec"),
            CodecDecoderPreference::D3D11 => Some("d3d11h265dec"),
            CodecDecoderPreference::Software => Some("avdec_h265"),
        },
        _ => None,
    }
}

fn decoder_factory_usable(factory: &'static str) -> bool {
    static DECODER_PROBES: OnceLock<Mutex<HashMap<&'static str, bool>>> = OnceLock::new();
    let probes = DECODER_PROBES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(probes) = probes.lock() {
        if let Some(usable) = probes.get(factory) {
            return *usable;
        }
    }

    // Fast probe: registry lookup only. Instantiating each decoder + transitioning
    // to State::Ready during capability enumeration adds tens of seconds to hello
    // and stalls the Settings tab. The actual pipeline builds + state transitions
    // still happen at session start.
    let usable = gst::ElementFactory::find(factory).is_some();
    if let Ok(mut probes) = probes.lock() {
        probes.insert(factory, usable);
    }
    usable
}

fn select_sink_factory(video_api: RtpVideoApi) -> Option<&'static str> {
    // Internal Linux: never pick waylandsink for the X11 child overlay path.
    #[cfg(target_os = "linux")]
    if use_internal_renderer() {
        let internal = video_api.internal_x11_sink_candidates();
        if let Some(factory) = internal
            .iter()
            .copied()
            .find(|factory| gst::ElementFactory::find(factory).is_some())
        {
            return Some(factory);
        }
    }

    // Internal Windows + Vulkan: Electron hole-punch cannot composite Win32 Vulkan
    // swapchains; present with D3D12 (D3D11 fallback) VideoOverlay instead.
    #[cfg(target_os = "windows")]
    if use_internal_renderer() && video_api == RtpVideoApi::Vulkan {
        return ["d3d12videosink", "d3d11videosink"]
            .into_iter()
            .find(|factory| gst::ElementFactory::find(factory).is_some());
    }

    select_capability_sink_factory(video_api)
}

/// Sink advertised in capabilities / used when not overriding for Internal present.
fn select_capability_sink_factory(video_api: RtpVideoApi) -> Option<&'static str> {
    std::iter::once(video_api.sink_factory())
        .chain(video_api.sink_fallback_factories().iter().copied())
        .find(|factory| gst::ElementFactory::find(factory).is_some())
}

fn required_video_chain_elements_available(specs: &[RtpVideoChainSpec]) -> bool {
    specs
        .iter()
        .all(|spec| gst::ElementFactory::find(spec.factory).is_some())
}

fn all_rtp_video_apis() -> &'static [RtpVideoApi] {
    &[
        RtpVideoApi::D3D12,
        RtpVideoApi::D3D11,
        RtpVideoApi::VideoToolbox,
        RtpVideoApi::Nvdec,
        RtpVideoApi::Vaapi,
        RtpVideoApi::V4L2,
        RtpVideoApi::Vulkan,
        RtpVideoApi::Software,
    ]
}

fn all_video_codec_labels() -> &'static [&'static str] {
    &["H264", "H265", "AV1"]
}

pub(crate) fn current_platform_label() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        "other"
    }
}

fn backend_runs_on_current_platform(video_api: RtpVideoApi) -> bool {
    backend_runs_on_platform(video_api, current_platform_label())
}

pub(crate) fn backend_runs_on_platform(video_api: RtpVideoApi, platform: &str) -> bool {
    match video_api {
        RtpVideoApi::D3D11 | RtpVideoApi::D3D12 => platform == "windows",
        RtpVideoApi::VideoToolbox => platform == "macos",
        RtpVideoApi::Nvdec | RtpVideoApi::Vaapi | RtpVideoApi::V4L2 => platform == "linux",
        RtpVideoApi::Vulkan => matches!(platform, "windows" | "linux"),
        RtpVideoApi::Software => true,
    }
}

pub(crate) fn native_video_backend_capabilities() -> Vec<NativeVideoBackendCapability> {
    all_rtp_video_apis()
        .iter()
        .copied()
        .map(native_video_backend_capability)
        .collect()
}

fn native_video_backend_capability(video_api: RtpVideoApi) -> NativeVideoBackendCapability {
    let platform_supported = backend_runs_on_current_platform(video_api);
    // Advertise the true API sink (vulkansink). Internal Windows Vulkan may present
    // via d3d12/d3d11videosink at session time for Electron hole-punch compatibility.
    let sink_factory = platform_supported
        .then(|| select_capability_sink_factory(video_api))
        .flatten();
    let codecs = all_video_codec_labels()
        .iter()
        .map(|codec| {
            native_video_codec_capability(video_api, codec, platform_supported, sink_factory)
        })
        .collect::<Vec<_>>();
    let available =
        platform_supported && sink_factory.is_some() && codecs.iter().any(|codec| codec.available);
    let reason = if !platform_supported {
        Some(format!(
            "{} is a {} backend and does not run on {}.",
            video_api.label(),
            video_api.platform(),
            current_platform_label()
        ))
    } else if sink_factory.is_none() {
        Some(format!(
            "{} sink is unavailable; install the platform GStreamer video sink plugins.",
            video_api.label()
        ))
    } else if !available {
        Some(format!(
            "{} decoders are unavailable for H.264, H.265, and AV1.",
            video_api.label()
        ))
    } else {
        None
    };

    NativeVideoBackendCapability {
        backend: video_api.capability_id().to_owned(),
        platform: video_api.platform().to_owned(),
        codecs,
        zero_copy_modes: zero_copy_modes_for_backend(video_api),
        sink: sink_factory.map(str::to_owned),
        available,
        reason,
    }
}

fn native_video_codec_capability(
    video_api: RtpVideoApi,
    codec: &str,
    platform_supported: bool,
    sink: Option<&'static str>,
) -> NativeVideoCodecCapability {
    let depayloader = rtp_video_depayloader_factory(codec);
    let parser = rtp_video_parser_factory(codec);
    let decoder = platform_supported
        .then(|| select_decoder_factory(video_api, codec))
        .flatten();
    // Capability checks the External Vulkan present chain so vulkansink/vulkanupload
    // must be present even when Internal sessions present via D3D11.
    let definition = {
        #[cfg(target_os = "windows")]
        {
            if video_api == RtpVideoApi::Vulkan {
                windows_vulkan_external_present_chain_definition(codec)
            } else {
                rtp_video_chain_definition(codec, video_api)
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            rtp_video_chain_definition(codec, video_api)
        }
    };
    let available = platform_supported
        && sink.is_some()
        && decoder.is_some()
        && depayloader.is_some_and(|factory| gst::ElementFactory::find(factory).is_some())
        && parser.is_some_and(|factory| gst::ElementFactory::find(factory).is_some())
        && definition.is_some_and(|mut specs| {
            for spec in &mut specs {
                if spec.role == RtpVideoChainRole::Decoder {
                    if let Some(decoder) = decoder {
                        spec.factory = decoder;
                    }
                } else if spec.role == RtpVideoChainRole::Sink {
                    if let Some(sink) = sink {
                        spec.factory = sink;
                    }
                }
            }
            specs.retain(|spec| {
                spec.role != RtpVideoChainRole::StatsOverlay
                    || gst::ElementFactory::find(spec.factory).is_some()
            });
            required_video_chain_elements_available(&specs)
        });

    let reason = if !platform_supported {
        Some("Backend is not available on this platform.".to_owned())
    } else if depayloader.is_none() || parser.is_none() {
        Some("RTP depayloader or parser is not mapped for this codec.".to_owned())
    } else if decoder.is_none() {
        Some(format!(
            "{} decoder for {codec} is not installed.",
            video_api.label()
        ))
    } else if sink.is_none() {
        Some(format!(
            "{} video sink is not installed.",
            video_api.label()
        ))
    } else if !available {
        Some("Required GStreamer elements are not all available.".to_owned())
    } else {
        None
    };

    NativeVideoCodecCapability {
        codec: codec.to_ascii_lowercase(),
        available,
        decoder: decoder.map(str::to_owned),
        parser: parser.map(str::to_owned),
        depayloader: depayloader.map(str::to_owned),
        reason,
    }
}

fn zero_copy_modes_for_backend(video_api: RtpVideoApi) -> Vec<String> {
    match video_api {
        RtpVideoApi::D3D11 => vec!["D3D11Memory".to_owned()],
        RtpVideoApi::D3D12 => vec!["D3D12Memory".to_owned()],
        RtpVideoApi::VideoToolbox => vec!["GLMemory".to_owned()],
        RtpVideoApi::Nvdec => Vec::new(),
        RtpVideoApi::Vaapi => vec!["VAMemory".to_owned()],
        // Linux keeps decoded frames as VulkanImage. Windows uses DXVA→upload,
        // so there is no end-to-end VulkanImage zero-copy path yet.
        RtpVideoApi::Vulkan if cfg!(target_os = "windows") => Vec::new(),
        RtpVideoApi::Vulkan => vec!["VulkanImage".to_owned()],
        RtpVideoApi::V4L2 => vec!["DMABuf".to_owned()],
        RtpVideoApi::Software => Vec::new(),
    }
}

fn configure_rtp_video_chain_element(
    element: &gst::Element,
    spec: RtpVideoChainSpec,
    video_api: RtpVideoApi,
    d3d_fullscreen_sink: bool,
) {
    match spec.role {
        RtpVideoChainRole::ReceiveCapsFilter => {
            if let Some(caps) = spec
                .caps
                .as_deref()
                .and_then(|caps| caps.parse::<gst::Caps>().ok())
            {
                element.set_property("caps", &caps);
            }
        }
        RtpVideoChainRole::Depayloader => {
            set_property_if_supported(element, "request-keyframe", true);
            // Hard-waiting after packet loss can freeze the visible frame while RTP is still flowing.
            set_property_if_supported(element, "wait-for-keyframe", false);
        }
        RtpVideoChainRole::Parser => {
            set_property_if_supported(element, "disable-passthrough", true);
            set_property_if_supported(element, "config-interval", -1i32);
        }
        RtpVideoChainRole::PreDecodeQueue => {
            configure_queue(element, VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS, false);
        }
        RtpVideoChainRole::Decoder => {
            set_property_if_supported(element, "automatic-request-sync-points", true);
            set_property_if_supported(element, "discard-corrupted-frames", true);
            set_property_if_supported(element, "min-force-key-unit-interval", 100_000_000u64);
            set_property_if_supported(element, "qos", false);
        }
        RtpVideoChainRole::PostDecodeRateSetter => {
            if let Some(caps) = spec
                .caps
                .as_deref()
                .and_then(|caps| caps.parse::<gst::Caps>().ok())
            {
                element.set_property("caps", &caps);
            }
            set_property_if_supported(element, "join", true);
            set_property_if_supported(element, "replace", false);
            set_property_if_supported(element, "qos", false);
        }
        RtpVideoChainRole::PostDecodeCapsFilter => {
            if let Some(caps) = spec
                .caps
                .as_deref()
                .and_then(|caps| caps.parse::<gst::Caps>().ok())
            {
                element.set_property("caps", &caps);
            }
        }
        RtpVideoChainRole::PostDecodeConverter => {
            set_property_if_supported(element, "qos", false);
        }
        RtpVideoChainRole::StatsOverlay => {
            configure_stats_overlay_element(element);
        }
        RtpVideoChainRole::PostDecodeQueue => {
            configure_queue_for_low_latency(element, "video");
        }
        RtpVideoChainRole::Sink => {
            if matches!(video_api, RtpVideoApi::D3D11 | RtpVideoApi::D3D12) {
                configure_d3d_video_sink(element, d3d_fullscreen_sink);
            } else {
                configure_sink_for_low_latency(element);
            }
        }
    }
}

fn link_rtp_video_pad(
    pipeline: &gst::Pipeline,
    src_pad: &gst::Pad,
    encoding: &str,
    render_state: &GstreamerRenderState,
    event_sender: &Option<Sender<Event>>,
    streaming_reported: &Arc<AtomicBool>,
    present_max_fps: Arc<AtomicU32>,
    d3d_fullscreen_sink: bool,
    video_liveness: VideoLivenessMonitor,
    video_tap: &Arc<Mutex<Option<GstreamerVideoTap>>>,
) -> Result<(), String> {
    if src_pad.is_linked() {
        return Ok(());
    }

    let requested_fps = video_liveness.requested_fps();
    let (video_api, specs) = rtp_video_chain_specs(encoding, requested_fps).ok_or_else(|| {
        format!(
            "Explicit low-latency decode chain is unavailable for RTP {encoding}; install the platform GStreamer plugin packages or set {NATIVE_VIDEO_BACKEND_ENV}=software to force software decode."
        )
    })?;
    let elements = build_rtp_video_chain(
        pipeline,
        src_pad,
        encoding,
        video_api,
        &specs,
        render_state,
        event_sender,
        streaming_reported,
        present_max_fps.clone(),
        d3d_fullscreen_sink,
        &video_liveness,
        video_tap,
    )?;
    // Arm the decoder-chain fallback: if this chain never renders (or dies
    // mid-stream), the liveness watchdog rebuilds it with the next candidate
    // decoder API (D3D12 → D3D11 → software) instead of giving up. Only armed
    // on the initial WebRTC RTP build; NVST classic UDP has its own path.
    register_video_chain_fallback(
        pipeline,
        src_pad,
        encoding,
        video_api,
        requested_fps,
        render_state,
        event_sender,
        streaming_reported,
        present_max_fps,
        d3d_fullscreen_sink,
        &video_liveness,
        video_tap,
        elements,
    );
    send_log(
        event_sender,
        "info",
        format!(
            "Linked RTP {encoding} video through explicit low-latency {} decode chain.",
            video_api.label()
        ),
    );
    Ok(())
}

/// Build, wire and start one complete RTP video decode chain for `video_api`.
/// Shared by the initial build (`link_rtp_video_pad`) and the decoder-fallback
/// rebuild, which needs the exact same element creation / probe wiring / state
/// sync / src-pad link sequence for the next candidate API.
fn build_rtp_video_chain(
    pipeline: &gst::Pipeline,
    src_pad: &gst::Pad,
    encoding: &str,
    video_api: RtpVideoApi,
    specs: &[RtpVideoChainSpec],
    render_state: &GstreamerRenderState,
    event_sender: &Option<Sender<Event>>,
    streaming_reported: &Arc<AtomicBool>,
    present_max_fps: Arc<AtomicU32>,
    d3d_fullscreen_sink: bool,
    video_liveness: &VideoLivenessMonitor,
    video_tap: &Arc<Mutex<Option<GstreamerVideoTap>>>,
) -> Result<Vec<gst::Element>, String> {
    let zero_copy = specs.iter().any(|spec| {
        spec.caps
            .as_deref()
            .is_some_and(|caps| caps.contains("memory:D3D"))
    });
    video_liveness.update_hardware_acceleration(format!("GStreamer {}", video_api.label()));
    video_liveness.set_stats_overlay(None);
    let mut elements = Vec::with_capacity(specs.len());

    let result = (|| -> Result<(), String> {
        send_log(
            event_sender,
            "info",
            format_video_chain_selection(encoding, video_api, specs),
        );
        if video_api == RtpVideoApi::D3D12 {
            send_log(
                event_sender,
                "info",
                format_d3d12_selection_summary(video_liveness.requested_fps()),
            );
        }
        let configured_present_max_fps = present_max_fps.load(Ordering::SeqCst);
        let effective_present_max_fps = effective_present_max_fps(
            configured_present_max_fps,
            video_liveness.requested_fps(),
            video_api,
            primary_display_refresh_hz(),
        );
        present_max_fps.store(effective_present_max_fps, Ordering::SeqCst);
        if effective_present_max_fps > 0 {
            let reason = if configured_present_max_fps == PRESENT_LIMITER_AUTO_SENTINEL {
                "auto-enabled for the D3D present path to prevent display-rate present backpressure"
                    .to_owned()
            } else if configured_present_max_fps == PRESENT_LIMITER_VRR_SENTINEL {
                "kept below the display refresh ceiling for VRR".to_owned()
            } else {
                format!("configured by {NATIVE_PRESENT_MAX_FPS_ENV}")
            };
            send_log(
                event_sender,
                "info",
                format!(
                    "Native present limiter enabled at {effective_present_max_fps} fps for {} video path; reason: {reason}.",
                    video_api.label()
                ),
            );
        }
        if d3d_fullscreen_sink {
            send_log(
                event_sender,
                "info",
                format!(
                    "Native D3D sink fullscreen presentation enabled for Cloud G-Sync/VRR; set {NATIVE_D3D_FULLSCREEN_ENV}=0 to disable."
                ),
            );
        }
        for spec in specs {
            let element = make_element(spec.factory)?;
            configure_rtp_video_chain_element(
                &element,
                spec.clone(),
                video_api,
                d3d_fullscreen_sink,
            );
            if spec.role == RtpVideoChainRole::StatsOverlay {
                video_liveness.set_stats_overlay(Some(element.clone()));
            }
            pipeline.add(&element).map_err(|error| {
                format!(
                    "Failed to add {} for RTP {encoding} video chain: {error}",
                    spec.factory
                )
            })?;
            elements.push(element);
        }

        for pair in elements.windows(2) {
            pair[0].link(&pair[1]).map_err(|error| {
                format!(
                    "Failed to link {} -> {} for RTP {encoding} video chain: {error:?}",
                    element_factory_name(&pair[0]),
                    element_factory_name(&pair[1])
                )
            })?;
        }

        // Tap the chain right before the sink so screenshots capture exactly
        // what was presented (including the stats overlay when visible). The
        // branch is valve-gated, so it costs nothing while idle. The tee is
        // NOT hot-plugged here: attaching a second branch while the D3D sink
        // is warming up stalls its present chain on some GStreamer releases,
        // so GstreamerVideoTap::ensure_tee defers the hot-plug to first use.
        if elements.len() >= 2 {
            let before_sink = elements[elements.len() - 2].clone();
            let sink_element = elements[elements.len() - 1].clone();
            if let Ok(mut slot) = video_tap.lock() {
                *slot = Some(GstreamerVideoTap {
                    tee: None,
                    before_sink,
                    sink: sink_element,
                    video_api,
                    zero_copy,
                });
            }
            send_log(
                event_sender,
                "info",
                format!(
                    "Native video tap deferred for RTP {encoding}: tee hot-plugged on first screenshot/recording use."
                ),
            );
        }

        let first = elements
            .first()
            .ok_or_else(|| format!("No elements created for RTP {encoding} video chain."))?;
        let Some(first_sink_pad) = first.static_pad("sink") else {
            return Err(format!(
                "First RTP {encoding} video-chain element has no sink pad."
            ));
        };
        let sink = elements
            .last()
            .ok_or_else(|| format!("RTP {encoding} video chain has no sink element."))?;
        if let Some(post_decode_queue) =
            specs
                .iter()
                .zip(elements.iter())
                .find_map(|(spec, element)| {
                    (spec.role == RtpVideoChainRole::PostDecodeQueue).then_some(element)
                })
        {
            video_liveness.set_post_decode_queue(post_decode_queue.clone());
            watch_video_decoded_rate(
                post_decode_queue,
                event_sender,
                Some(video_liveness.clone()),
            );
        }
        if let Some(pre_decode_queue) =
            specs
                .iter()
                .zip(elements.iter())
                .find_map(|(spec, element)| {
                    (spec.role == RtpVideoChainRole::PreDecodeQueue).then_some(element)
                })
        {
            video_liveness.set_pre_decode_queue(pre_decode_queue.clone());
        }
        if let Some(parser) = specs
            .iter()
            .zip(elements.iter())
            .find_map(|(spec, element)| (spec.role == RtpVideoChainRole::Parser).then_some(element))
        {
            watch_video_caps_transitions(parser, "parser", event_sender, video_liveness.clone());
        }
        if let Some(decoder) = specs
            .iter()
            .zip(elements.iter())
            .find_map(|(spec, element)| {
                (spec.role == RtpVideoChainRole::Decoder).then_some(element)
            })
        {
            video_liveness.set_decoder(decoder.clone());
            watch_video_caps_transitions(decoder, "decoder", event_sender, video_liveness.clone());
        }
        render_state.set_video_sink(sink.clone(), event_sender);
        video_liveness.state().set_current_sink(sink.clone());
        install_present_limiter(
            sink,
            present_max_fps,
            event_sender,
            Some(video_liveness.clone()),
        );
        watch_video_sink_caps_transitions(sink, event_sender, Some(video_liveness.clone()));
        watch_first_sink_buffer(sink, "video", event_sender, streaming_reported);
        watch_video_sink_rate(sink, event_sender, Some(video_liveness.clone()));

        for element in &elements {
            element.sync_state_with_parent().map_err(|error| {
                format!("Failed to sync RTP {encoding} video-chain element state: {error}")
            })?;
        }
        src_pad
            .link(&first_sink_pad)
            .map_err(|error| format!("Failed to link RTP {encoding} video pad: {error:?}"))?;
        video_liveness.set_rtp_video_src_pad(src_pad);
        watch_rtp_video_bitrate(src_pad, video_liveness.clone(), event_sender);
        video_liveness.start(pipeline.clone(), sink.clone(), event_sender.clone());

        Ok(())
    })();

    if result.is_err() {
        for element in &elements {
            let _ = element.set_state(gst::State::Null);
            let _ = pipeline.remove(element);
        }
    }

    result?;
    Ok(elements)
}

/// Decoder-chain fallback state, armed after the initial RTP video chain
/// build. The liveness watchdog calls `try_rebuild` when startup/stall
/// recovery is exhausted; it tears down the current chain and rebuilds it with
/// the next candidate API, so a hardware decoder that silently produces no
/// frames (e.g. d3d12h265dec on some Intel iGPUs) gets replaced instead of
/// killing the session.
pub(crate) struct VideoChainRebuildContext {
    pipeline: gst::Pipeline,
    src_pad: gst::Pad,
    encoding: String,
    render_state: GstreamerRenderState,
    event_sender: Option<Sender<Event>>,
    streaming_reported: Arc<AtomicBool>,
    present_max_fps: Arc<AtomicU32>,
    d3d_fullscreen_sink: bool,
    video_liveness: VideoLivenessMonitor,
    video_tap: Arc<Mutex<Option<GstreamerVideoTap>>>,
    /// Current chain elements (torn down before the rebuild).
    elements: Vec<gst::Element>,
    /// The API currently in use, for diagnostics.
    current_api: RtpVideoApi,
    /// Remaining candidate APIs, most preferred first.
    candidates: Vec<RtpVideoApi>,
}

// Manual `Debug` instead of `#[derive(Debug)]`: the context holds a
// `VideoLivenessMonitor`, which itself holds this context (via
// `chain_rebuild`), so a derived impl would require `Debug` on both sides
// cyclically. Only the lightweight diagnostic fields are printed.
impl std::fmt::Debug for VideoChainRebuildContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoChainRebuildContext")
            .field("encoding", &self.encoding)
            .field("current_api", &self.current_api)
            .field("candidates", &self.candidates)
            .field("element_count", &self.elements.len())
            .finish_non_exhaustive()
    }
}

impl VideoChainRebuildContext {
    /// Try the next decoder candidate. Returns true when a fallback chain is
    /// now live, false when no candidates remain. On success the old chain is
    /// removed from the pipeline and the new one is fully wired (probes,
    /// sink, src-pad link, liveness restart).
    pub(crate) fn try_rebuild(&mut self, event_sender: &Option<Sender<Event>>) -> bool {
        // Refuse to rebuild while a screenshot/recording branch is hot-plugged
        // into the video tap: the tee is linked between the old chain's
        // before_sink/sink, and tearing them out mid-capture would orphan the
        // branch. The tap tee only exists after first capture use, which
        // requires the sink to have rendered frames — so a never-started
        // chain never has one.
        if let Ok(slot) = self.video_tap.lock() {
            if slot.as_ref().is_some_and(|tap| tap.tee.is_some()) {
                send_log(
                    event_sender,
                    "warn",
                    "Skipping video decoder fallback: a recording/screenshot branch is attached to the video tap."
                        .to_owned(),
                );
                return false;
            }
        }

        // `candidates` is ordered most-preferred-first (Software always at the
        // tail), so take from the front to try the next-best API before
        // falling back to software decode. Keep trying until one rebuild
        // succeeds or every candidate is exhausted.
        while let Some(next_api) = self.candidates.first().copied() {
            self.candidates.remove(0);

            send_log(
                event_sender,
                "warn",
                format!(
                    "Native video decoder fallback: rebuilding RTP {} chain with {} (was {}).",
                    self.encoding,
                    next_api.label(),
                    self.current_api.label()
                ),
            );

            // Tear down the current chain: unlink the RTP src pad, Null +
            // remove every element. A failed rebuild leaves the old chain
            // already removed, so the next candidate starts from a clean slate.
            if let Some(first) = self.elements.first() {
                if let Some(first_sink_pad) = first.static_pad("sink") {
                    let _ = self.src_pad.unlink(&first_sink_pad);
                }
            }
            for element in self.elements.drain(..) {
                let _ = element.set_state(gst::State::Null);
                let _ = self.pipeline.remove(&element);
            }
            self.video_liveness.clear_chain_elements();
            if let Ok(mut slot) = self.video_tap.lock() {
                *slot = None;
            }

            let requested_fps = self.video_liveness.requested_fps();
            let Some(specs) =
                rtp_video_chain_specs_for_api(&self.encoding, next_api, requested_fps)
            else {
                send_log(
                    event_sender,
                    "warn",
                    format!(
                        "Video decoder fallback: {} chain is unavailable for RTP {}; skipping.",
                        next_api.label(),
                        self.encoding
                    ),
                );
                continue;
            };

            match build_rtp_video_chain(
                &self.pipeline,
                &self.src_pad,
                &self.encoding,
                next_api,
                &specs,
                &self.render_state,
                &self.event_sender,
                &self.streaming_reported,
                self.present_max_fps.clone(),
                self.d3d_fullscreen_sink,
                &self.video_liveness,
                &self.video_tap,
            ) {
                Ok(elements) => {
                    self.elements = elements;
                    self.current_api = next_api;
                    send_log(
                        event_sender,
                        "warn",
                        format!(
                            "Native video decoder fallback succeeded: RTP {} now on {} decode chain.",
                            self.encoding,
                            next_api.label()
                        ),
                    );
                    return true;
                }
                Err(error) => {
                    send_log(
                        event_sender,
                        "warn",
                        format!(
                            "Video decoder fallback to {} failed: {error}",
                            next_api.label()
                        ),
                    );
                    // Fall through to the next candidate.
                }
            }
        }

        send_log(
            event_sender,
            "warn",
            format!(
                "Native video decoder fallback exhausted: no more candidates after {} for RTP {}.",
                self.current_api.label(),
                self.encoding
            ),
        );
        false
    }
}

fn register_video_chain_fallback(
    pipeline: &gst::Pipeline,
    src_pad: &gst::Pad,
    encoding: &str,
    current_api: RtpVideoApi,
    requested_fps: Option<u32>,
    render_state: &GstreamerRenderState,
    event_sender: &Option<Sender<Event>>,
    streaming_reported: &Arc<AtomicBool>,
    present_max_fps: Arc<AtomicU32>,
    d3d_fullscreen_sink: bool,
    video_liveness: &VideoLivenessMonitor,
    video_tap: &Arc<Mutex<Option<GstreamerVideoTap>>>,
    elements: Vec<gst::Element>,
) {
    let candidates = rtp_video_chain_fallback_ladder(encoding, current_api, requested_fps);
    if candidates.is_empty() {
        return;
    }
    send_log(
        event_sender,
        "info",
        format!(
            "Native video decoder fallback armed for RTP {encoding}: candidates after {} = [{}].",
            current_api.label(),
            candidates
                .iter()
                .map(|api| api.label())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    video_liveness.set_chain_rebuild(Some(VideoChainRebuildContext {
        pipeline: pipeline.clone(),
        src_pad: src_pad.clone(),
        encoding: encoding.to_owned(),
        render_state: render_state.clone(),
        event_sender: event_sender.clone(),
        streaming_reported: streaming_reported.clone(),
        present_max_fps,
        d3d_fullscreen_sink,
        video_liveness: video_liveness.clone(),
        video_tap: video_tap.clone(),
        elements,
        current_api,
        candidates,
    }));
}

pub(crate) fn format_video_chain_selection(
    encoding: &str,
    video_api: RtpVideoApi,
    specs: &[RtpVideoChainSpec],
) -> String {
    let decoder = specs
        .iter()
        .find(|spec| spec.role == RtpVideoChainRole::Decoder)
        .map(|spec| spec.factory)
        .unwrap_or("unknown");
    let sink = specs
        .iter()
        .find(|spec| spec.role == RtpVideoChainRole::Sink)
        .map(|spec| spec.factory)
        .unwrap_or("unknown");
    let converter = specs
        .iter()
        .filter(|spec| spec.role == RtpVideoChainRole::PostDecodeConverter)
        .map(|spec| spec.factory)
        .collect::<Vec<_>>()
        .join("+");
    let converter = if converter.is_empty() {
        "none".to_owned()
    } else {
        converter
    };
    let memory = specs
        .iter()
        .find(|spec| spec.role == RtpVideoChainRole::PostDecodeCapsFilter)
        .and_then(|spec| spec.caps.as_deref())
        .unwrap_or(if video_api.is_gpu_path() {
            "auto-negotiated"
        } else {
            "system-memory"
        });
    let acceleration = if video_api.is_gpu_path() {
        "hardware"
    } else {
        "software"
    };
    let path_note = if cfg!(target_os = "windows") && video_api == RtpVideoApi::Vulkan {
        if sink == "d3d12videosink" {
            " (DXVA decode + D3D12 present; Electron cannot composite Win32 vulkansink — use External for true Vulkan present)"
        } else if sink == "d3d11videosink" {
            " (DXVA decode + D3D11 present; Electron cannot composite Win32 vulkansink — use External for true Vulkan present)"
        } else {
            " (DXVA decode + Vulkan present; native vulkanh264dec is unstable on Windows)"
        }
    } else {
        ""
    };
    format!(
        "Selected native {acceleration} video path for RTP {encoding}: backend={}, decoder={decoder}, converter={converter}, renderer={sink}, memory={memory}{path_note}.",
        video_api.label()
    )
}

fn format_d3d12_selection_summary(requested_fps: Option<u32>) -> String {
    let backend_env = std::env::var(NATIVE_VIDEO_BACKEND_ENV).ok();
    let api_env = std::env::var(NATIVE_VIDEO_API_ENV).ok();
    let reason = if backend_env
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("d3d12"))
    {
        format!("forced by {NATIVE_VIDEO_BACKEND_ENV}=d3d12")
    } else if api_env
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("d3d12"))
    {
        format!("forced by {NATIVE_VIDEO_API_ENV}=d3d12")
    } else if should_prefer_d3d12_for_high_fps(requested_fps) {
        format!(
            "auto-selected for {} fps stream to avoid D3D11 display-rate present backpressure",
            requested_fps
                .map(|fps| fps.to_string())
                .unwrap_or_else(|| "high-FPS".to_owned())
        )
    } else {
        "D3D11 was unavailable/probe failed".to_owned()
    };

    format!(
        "Native D3D12 video path selected; reason: {reason}. env {NATIVE_VIDEO_BACKEND_ENV}={backend_env:?}, {NATIVE_VIDEO_API_ENV}={api_env:?}. If D3D12 stalls on a specific driver, force {NATIVE_VIDEO_BACKEND_ENV}=d3d11."
    )
}

fn element_factory_name(element: &gst::Element) -> String {
    element
        .factory()
        .map(|factory| factory.name().to_string())
        .unwrap_or_else(|| element.name().to_string())
}

fn link_decoded_media_pad(
    pipeline: &gst::Pipeline,
    src_pad: &gst::Pad,
    render_state: &GstreamerRenderState,
    event_sender: &Option<Sender<Event>>,
    streaming_reported: &Arc<AtomicBool>,
    video_liveness: &VideoLivenessMonitor,
    game_audio_tap: &Arc<Mutex<Option<gst::Element>>>,
) -> Result<(), String> {
    if src_pad.is_linked() {
        return Ok(());
    }

    match decoded_media_kind(src_pad) {
        DecodedMediaKind::Video => link_media_chain(
            pipeline,
            src_pad,
            &video_sink_factories(),
            "video",
            Some(render_state),
            event_sender,
            streaming_reported,
            Some(video_liveness),
            None,
        ),
        DecodedMediaKind::Audio => link_media_chain(
            pipeline,
            src_pad,
            &[
                ("queue", None),
                ("audioconvert", None),
                ("audioresample", None),
                ("autoaudiosink", Some(false)),
            ],
            "audio",
            None,
            event_sender,
            streaming_reported,
            None,
            Some(game_audio_tap),
        ),
        DecodedMediaKind::Unknown => Err(format!(
            "Unsupported decoded media caps {:?}; routing to fallback sink.",
            pad_caps_name(src_pad)
        )),
    }
}

fn video_sink_factories() -> Vec<(&'static str, Option<bool>)> {
    #[cfg(target_os = "windows")]
    {
        let d3d_sink = ["d3d12videosink", "d3d11videosink"]
            .into_iter()
            .find(|factory| gst::ElementFactory::find(factory).is_some());
        if let Some(sink) = d3d_sink {
            let mut factories = vec![("queue", None)];
            if gst::ElementFactory::find("dwritetextoverlay").is_some() {
                factories.push(("dwritetextoverlay", None));
            }
            factories.push((sink, Some(false)));
            return factories;
        }
    }

    let mut factories = vec![("queue", None), ("videoconvert", None)];
    if gst::ElementFactory::find("dwritetextoverlay").is_some() {
        factories.push(("dwritetextoverlay", None));
    }
    factories.push(("autovideosink", Some(false)));
    factories
}

fn link_media_chain(
    pipeline: &gst::Pipeline,
    src_pad: &gst::Pad,
    factories: &[(&str, Option<bool>)],
    media_label: &str,
    render_state: Option<&GstreamerRenderState>,
    event_sender: &Option<Sender<Event>>,
    streaming_reported: &Arc<AtomicBool>,
    video_liveness: Option<&VideoLivenessMonitor>,
    game_audio_tap: Option<&Arc<Mutex<Option<gst::Element>>>>,
) -> Result<(), String> {
    if media_label == "video" {
        if let Some(video_liveness) = video_liveness {
            video_liveness.set_stats_overlay(None);
        }
    }

    let mut elements = Vec::with_capacity(factories.len());
    for (factory, sync_property) in factories {
        let factory = *factory;
        let element = make_element(factory)?;
        if factory == "queue" {
            configure_queue_for_low_latency(&element, media_label);
        }
        if factory == "dwritetextoverlay" {
            configure_stats_overlay_element(&element);
            if media_label == "video" {
                if let Some(video_liveness) = video_liveness {
                    video_liveness.set_stats_overlay(Some(element.clone()));
                }
            }
        }
        if sync_property.is_some() || factory.ends_with("sink") {
            if factory == "d3d11videosink" || factory == "d3d12videosink" {
                // Fallback decodebin path: never exclusive-fullscreen (Internal default).
                configure_d3d_video_sink(&element, false);
            } else {
                configure_sink_for_low_latency(&element);
            }
        }
        pipeline
            .add(&element)
            .map_err(|error| format!("Failed to add {factory} for {media_label}: {error}"))?;
        elements.push(element);
    }

    for pair in elements.windows(2) {
        pair[0].link(&pair[1]).map_err(|error| {
            format!(
                "Failed to link {} -> {} for {media_label}: {error:?}",
                pair[0]
                    .factory()
                    .map(|factory| factory.name())
                    .unwrap_or_default(),
                pair[1]
                    .factory()
                    .map(|factory| factory.name())
                    .unwrap_or_default()
            )
        })?;
    }

    // Recording tap on the game-audio chain: tee between the last pre-sink
    // element and the audio sink. The TEE itself is stored (no dangling
    // queue): a fresh src pad is requested from it per recording and released
    // on teardown, exactly like the video tap. A dangling queue built at
    // stream start received data immediately, parked its src task on
    // FLOW_UNLINKED, and never restarted when relinked at recording time —
    // the recording audio branch never flowed and stop-recording timed out.
    if media_label == "audio" {
        if let Some(tap_slot) = game_audio_tap {
            let tap_empty = tap_slot.lock().ok().is_none_or(|slot| slot.is_none());
            if tap_empty && elements.len() >= 2 {
                let before_sink = elements[elements.len() - 2].clone();
                let sink = elements[elements.len() - 1].clone();
                let tap_tee = make_element("tee")?;
                pipeline
                    .add(&tap_tee)
                    .map_err(|error| format!("Failed to add game-audio tap tee: {error}"))?;
                before_sink.unlink(&sink);
                before_sink.link(&tap_tee).map_err(|error| {
                    format!("Failed to link audio chain into tap tee: {error:?}")
                })?;
                tap_tee
                    .link(&sink)
                    .map_err(|error| format!("Failed to link tap tee to audio sink: {error:?}"))?;
                tap_tee
                    .sync_state_with_parent()
                    .map_err(|error| format!("Failed to sync game-audio tap tee state: {error}"))?;
                if let Ok(mut slot) = tap_slot.lock() {
                    *slot = Some(tap_tee.clone());
                }
                send_log(
                    event_sender,
                    "info",
                    "Attached native recording game-audio tap tee.".to_owned(),
                );
            }
        }
    }

    let first = elements
        .first()
        .ok_or_else(|| format!("No elements created for {media_label} sink chain."))?;
    let Some(first_sink_pad) = first.static_pad("sink") else {
        return Err(format!(
            "First {media_label} sink-chain element has no sink pad."
        ));
    };
    src_pad
        .link(&first_sink_pad)
        .map_err(|error| format!("Failed to link decoded {media_label} pad: {error:?}"))?;

    if let Some(sink) = elements.last() {
        if media_label == "video" {
            if let Some(render_state) = render_state {
                render_state.set_video_sink(sink.clone(), event_sender);
            }
        }
        watch_first_sink_buffer(sink, media_label, event_sender, streaming_reported);
        if media_label == "audio" {
            if let Some(video_liveness) = video_liveness {
                watch_audio_activity(sink, video_liveness);
            }
        }
        if media_label == "video" {
            if let Some(video_liveness) = video_liveness {
                watch_video_sink_rate(sink, event_sender, Some(video_liveness.clone()));
                video_liveness.start(pipeline.clone(), sink.clone(), event_sender.clone());
            }
        }
    }

    for element in &elements {
        element.sync_state_with_parent().map_err(|error| {
            format!("Failed to sync {media_label} sink-chain element state: {error}")
        })?;
    }

    Ok(())
}

fn link_decoded_media_to_fakesink(
    pipeline: &gst::Pipeline,
    src_pad: &gst::Pad,
    label: &str,
) -> Result<(), String> {
    if src_pad.is_linked() {
        return Ok(());
    }

    let sink = gst::ElementFactory::make("fakesink")
        .property("sync", false)
        .property("async", false)
        .build()
        .map_err(|error| format!("Failed to create {label}: {error}"))?;
    configure_sink_for_low_latency(&sink);
    pipeline
        .add(&sink)
        .map_err(|error| format!("Failed to add {label}: {error}"))?;
    sink.sync_state_with_parent()
        .map_err(|error| format!("Failed to sync {label} state: {error}"))?;

    let Some(sink_pad) = sink.static_pad("sink") else {
        return Err(format!("{label} has no sink pad."));
    };
    src_pad
        .link(&sink_pad)
        .map(|_| ())
        .map_err(|error| format!("Failed to link {label}: {error:?}"))
}

fn make_element(factory: &str) -> Result<gst::Element, String> {
    gst::ElementFactory::make(factory)
        .build()
        .map_err(|error| format!("Failed to create GStreamer element {factory}: {error}"))
}

/// Build the valve-gated screenshot grab branch off an existing video tap tee:
/// valve → queue → [download] → videoconvert → pngenc → appsink.
///
/// The tee itself is hot-plugged earlier by `GstreamerVideoTap::ensure_tee`
/// (deferred to first use so the D3D sink is already presenting). The valve
/// starts closed (drop=true), so the branch is idle (zero frame cost) until
/// `capture()` briefly opens it. The download element is only added when
/// zero-copy is requested (frames are D3D textures then); system memory flows
/// straight into videoconvert.
fn insert_screenshot_grab_branch(
    pipeline: &gst::Pipeline,
    tee: &gst::Element,
    video_api: RtpVideoApi,
    zero_copy: bool,
    event_sender: &Option<Sender<Event>>,
) -> Result<GstreamerScreenshotGrab, String> {
    let valve = make_element("valve")?;
    let queue = make_element("queue")?;
    let convert = make_element("videoconvert")?;
    let pngenc = make_element("pngenc")?;
    let appsink = make_element("appsink")?;

    valve.set_property("drop", true);
    // Never let the grab branch back-pressure the video path: drop new buffers
    // when the queue fills instead of blocking the tee.
    queue.set_property_from_str("leaky", "downstream");
    queue.set_property("max-size-buffers", 2u32);
    queue.set_property("max-size-bytes", 0u32);
    queue.set_property("max-size-time", 0u64);
    // sync=false lets the branch run as fast as frames arrive; the pad probe
    // below keeps only the newest encoded PNG buffer.
    appsink.set_property("sync", false);
    appsink.set_property("max-buffers", 1u32);
    appsink.set_property("drop", true);
    appsink.set_property("wait-on-eos", false);

    let last_buffer: Arc<Mutex<Option<gst::Buffer>>> = Arc::new(Mutex::new(None));
    let appsink_sink_pad = appsink
        .static_pad("sink")
        .ok_or_else(|| "Screenshot appsink has no sink pad.".to_owned())?;
    let probe_last_buffer = last_buffer.clone();
    appsink_sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
        if let Some(buffer) = info.buffer() {
            if let Ok(mut slot) = probe_last_buffer.lock() {
                *slot = Some(buffer.clone());
            }
        }
        gst::PadProbeReturn::Ok
    });

    // D3D paths with zero-copy produce texture-backed frames; download them to
    // system memory first (videoconvert/pngenc cannot import D3D memory).
    let download_factory = match (video_api, zero_copy) {
        (RtpVideoApi::D3D11, true) => Some("d3d11download"),
        (RtpVideoApi::D3D12, true) => Some("d3d12download"),
        _ => None,
    };
    let download = match download_factory {
        Some(factory) => Some(make_element(factory)?),
        None => None,
    };

    let mut new_elements: Vec<&gst::Element> = vec![&valve, &queue];
    if let Some(download) = download.as_ref() {
        new_elements.push(download);
    }
    new_elements.extend([&convert, &pngenc, &appsink]);
    for element in &new_elements {
        pipeline
            .add(*element)
            .map_err(|error| format!("Failed to add screenshot grab element: {error}"))?;
    }

    tee.link(&valve).map_err(|error| {
        format!("Failed to link screenshot grab branch to video tap tee: {error:?}")
    })?;
    for pair in new_elements.windows(2) {
        pair[0]
            .link(pair[1])
            .map_err(|error| format!("Failed to link screenshot grab branch: {error:?}"))?;
    }

    for element in &new_elements {
        element
            .sync_state_with_parent()
            .map_err(|error| format!("Failed to sync screenshot grab element state: {error}"))?;
    }

    let chain_desc = match download_factory {
        Some(factory) => {
            format!("tee → valve → queue → {factory} → videoconvert → pngenc → appsink")
        }
        None => "tee → valve → queue → videoconvert → pngenc → appsink".to_owned(),
    };
    send_log(
        event_sender,
        "info",
        format!("Attached native screenshot grab branch ({chain_desc})."),
    );

    Ok(GstreamerScreenshotGrab {
        valve,
        appsink,
        last_buffer,
        event_sender: event_sender.clone(),
    })
}

/// Build the native recording branch off the shared video tap:
/// tee → valve → queue → (download) → videoconvert → capsfilter(I420) →
/// x264enc → mp4mux (fragmented, streamable) → appsink.
///
/// Muxer output buffers are captured by a BUFFER probe on the appsink sink
/// pad (each becomes one `recording-chunk` event); an EVENT probe records EOS
/// so `stop(finalize=true)` knows when the branch flushed. The valve starts
/// closed, so the branch costs nothing until a recording starts.
pub(crate) fn insert_recording_branch(
    pipeline: &gst::Pipeline,
    tee: &gst::Element,
    video_api: RtpVideoApi,
    zero_copy: bool,
    game_audio_tap: &Arc<Mutex<Option<gst::Element>>>,
    mic_audio_tap: &Arc<Mutex<Option<gst::Element>>>,
    event_sender: Option<Sender<Event>>,
) -> Result<GstreamerRecordingState, String> {
    let valve = make_element("valve")?;
    let queue = make_element("queue")?;
    let convert = make_element("videoconvert")?;
    let caps = make_element("capsfilter")?;
    let encoder = make_element("x264enc")?;
    let muxer = make_element("mp4mux")?;
    let appsink = make_element("appsink")?;

    valve.set_property("drop", true);
    // Frames past the valve are recording frames, but the branch must never
    // back-pressure the live video path: if the encoder lags, drop the oldest
    // queued frames instead of stalling the tee.
    queue.set_property_from_str("leaky", "downstream");
    queue.set_property("max-size-buffers", 30u32);
    queue.set_property("max-size-bytes", 0u32);
    queue.set_property("max-size-time", 0u64);

    let i420_caps: gst::Caps = "video/x-raw,format=I420"
        .parse()
        .map_err(|error| format!("Invalid recording caps: {error}"))?;
    caps.set_property("caps", &i420_caps);

    encoder.set_property_from_str("speed-preset", "ultrafast");
    encoder.set_property_from_str("tune", "zerolatency");
    encoder.set_property("bitrate", RECORDING_BITRATE_KBPS);
    encoder.set_property("bframes", 0u32);
    encoder.set_property("key-int-max", 120u32);

    // Fragmented MP4 in streamable mode: the first muxer buffer carries
    // ftyp + moov, then one buffer per ~500ms fragment. Appending these in
    // order produces a playable file (same contract as MediaRecorder chunks).
    muxer.set_property("streamable", true);
    muxer.set_property("fragment-duration", 500u32);

    // sync=false: the branch runs as fast as frames arrive; the probes below
    // capture every muxer output before appsink's internal (max 1) buffer.
    appsink.set_property("sync", false);
    appsink.set_property("max-buffers", 1u32);
    appsink.set_property("drop", true);
    appsink.set_property("wait-on-eos", false);

    // D3D paths with zero-copy produce texture-backed frames; download them to
    // system memory first (videoconvert/x264enc cannot import D3D memory).
    let download_factory = match (video_api, zero_copy) {
        (RtpVideoApi::D3D11, true) => Some("d3d11download"),
        (RtpVideoApi::D3D12, true) => Some("d3d12download"),
        _ => None,
    };
    let download = match download_factory {
        Some(factory) => Some(make_element(factory)?),
        None => None,
    };

    // --- Video sub-branch ---
    // The I420 frames are split after the capsfilter: one branch feeds the
    // H.264 encoder (the recording), the other feeds a first-frame JPEG
    // grabber used as the recording thumbnail in the gallery.
    let thumb_tee = make_element("tee")?;
    let thumb_valve = make_element("valve")?;
    let thumb_queue = make_element("queue")?;
    let thumb_encoder = make_element("jpegenc")?;
    let thumb_appsink = make_element("appsink")?;
    thumb_valve.set_property("drop", true);
    // Gallery thumbnail: compressed at quality 70 so the recording-finished
    // event stays light, and snapshot mode sends EOS right after the first
    // frame so exactly one JPEG is ever encoded (the probe below closes the
    // valve as a belt-and-suspenders guard).
    thumb_encoder.set_property("quality", 70i32);
    thumb_encoder.set_property("snapshot", true);
    // Only the very first frame is needed; never back-pressure the recording
    // branch if jpegenc lags (leaky, max 1 buffer).
    thumb_queue.set_property_from_str("leaky", "downstream");
    thumb_queue.set_property("max-size-buffers", 1u32);
    thumb_queue.set_property("max-size-bytes", 0u32);
    thumb_queue.set_property("max-size-time", 0u64);
    thumb_appsink.set_property("sync", false);
    thumb_appsink.set_property("max-buffers", 1u32);
    thumb_appsink.set_property("drop", true);

    let mut video_elements: Vec<gst::Element> = vec![valve.clone(), queue.clone()];
    if let Some(download) = download {
        video_elements.push(download);
    }
    video_elements.extend([convert, caps, thumb_tee.clone(), encoder.clone()]);

    // --- Audio sub-branch: TWO independent AAC tracks (game + mic), NO mixer.
    // Each available audio tap tee gets its own track: fresh tee pad →
    // audioresample → audioconvert → capsfilter(S16LE/2ch/48k) → valve(closed)
    // → voaacenc → mp4mux. The per-track valve gates the track exactly like
    // the video branch's valve (data drops at the closed valve without
    // back-pressure while the muxer finishes its state transition), and the
    // fresh pads are only requested after every branch element reached PLAYING
    // (sink pads FLUSH mid-transition, which stalled the game chain). This
    // deliberately avoids audiomixer: an aggregator hot-plugged into a PLAYING
    // pipeline drops the joined pads ("outside output segment") and fills them
    // with digital silence, then its tiny per-pad queues block the game chain
    // upstream after a few buffers (field: recordings carried no game audio).
    // mp4mux supports multiple audio tracks, so game and mic each keep their
    // own AAC track. The tap slots hold the tap TEES (fresh pads are requested
    // per recording); the old eager dangling tap queues are gone — a queue
    // that received data at stream start parked its src task on FLOW_UNLINKED
    // and never restarted when relinked (stop-recording timed out).
    let mut audio_taps: Vec<gst::Element> = Vec::new();
    for slot in [game_audio_tap, mic_audio_tap] {
        if let Ok(guard) = slot.lock() {
            if let Some(tap_tee) = guard.clone() {
                audio_taps.push(tap_tee);
            }
        }
    }
    let mut audio_elements: Vec<gst::Element> = Vec::new();
    // Per-track chains: (tap tee, [queue, resample, convert, capsfilter], valve, encoder).
    // The queue is intentionally leaky: mp4mux can remain PAUSED until the
    // first buffers arrive, and an audio branch must never propagate that
    // temporary back-pressure through the live game-audio tee.
    let mut audio_track_chains: Vec<(gst::Element, Vec<gst::Element>, gst::Element, gst::Element)> =
        Vec::new();
    let audio_valves: Vec<gst::Element> = if audio_taps.is_empty() {
        send_log(
            &event_sender,
            "warn",
            "Native recording has no audio source (game audio or mic tap); recording video only."
                .to_owned(),
        );
        Vec::new()
    } else {
        let tap_caps: gst::Caps = "audio/x-raw,format=S16LE,channels=2,rate=48000"
            .parse()
            .map_err(|error| format!("Invalid recording audio caps: {error}"))?;
        for tap_tee in &audio_taps {
            let tap_queue = make_element("queue")?;
            tap_queue.set_property_from_str("leaky", "downstream");
            tap_queue.set_property("max-size-buffers", AUDIO_QUEUE_MAX_BUFFERS);
            tap_queue.set_property("max-size-bytes", 0u32);
            tap_queue.set_property("max-size-time", 0u64);
            let tap_resample = make_element("audioresample")?;
            let tap_convert = make_element("audioconvert")?;
            let tap_caps_element = make_element("capsfilter")?;
            tap_caps_element.set_property("caps", &tap_caps);
            let tap_valve = make_element("valve")?;
            tap_valve.set_property("drop", true);
            let tap_aac = make_element("voaacenc")?;
            for element in [
                &tap_queue,
                &tap_resample,
                &tap_convert,
                &tap_caps_element,
                &tap_valve,
                &tap_aac,
            ] {
                audio_elements.push(element.clone());
            }
            audio_track_chains.push((
                tap_tee.clone(),
                vec![tap_queue, tap_resample, tap_convert, tap_caps_element],
                tap_valve,
                tap_aac,
            ));
        }
        audio_track_chains
            .iter()
            .map(|(_, _, valve, _)| valve.clone())
            .collect()
    };

    let mut elements: Vec<gst::Element> =
        Vec::with_capacity(video_elements.len() + audio_elements.len() + 4 + 2);
    elements.extend(video_elements.iter().cloned());
    elements.extend(audio_elements.iter().cloned());
    elements.extend([
        thumb_valve.clone(),
        thumb_queue.clone(),
        thumb_encoder.clone(),
        thumb_appsink.clone(),
    ]);
    elements.extend([muxer.clone(), appsink.clone()]);

    for element in &elements {
        pipeline
            .add(element)
            .map_err(|error| format!("Failed to add recording branch element: {error}"))?;
    }
    tee.link(&valve)
        .map_err(|error| format!("Failed to link video tap into recording branch: {error:?}"))?;
    for pair in video_elements.windows(2) {
        pair[0]
            .link(&pair[1])
            .map_err(|error| format!("Failed to link recording video branch: {error:?}"))?;
    }
    video_elements
        .last()
        .ok_or_else(|| "Recording video branch is empty.".to_owned())?
        .link(&muxer)
        .map_err(|error| format!("Failed to link x264enc -> mp4mux: {error:?}"))?;
    // The muxer's output MUST be linked into the appsink: the chunk BUFFER
    // probe and the EOS EVENT probe live on the appsink's sink pad, and the
    // mp4mux src task only starts when its src pad has a peer. Without this
    // link the muxer never aggregates (no chunks) and never pushes EOS, so
    // stop(finalize=true) times out with "EOS still not seen at the muxer
    // output" even after the direct-muxer failsafe (field: every native
    // recording since the feature landed failed exactly this way).
    muxer
        .link(&appsink)
        .map_err(|error| format!("Failed to link mp4mux -> appsink: {error:?}"))?;

    // Thumbnail grabber off the I420 tee: valve → queue → jpegenc → appsink.
    thumb_tee
        .link(&thumb_valve)
        .map_err(|error| format!("Failed to link recording thumbnail tee -> valve: {error:?}"))?;
    thumb_valve
        .link(&thumb_queue)
        .map_err(|error| format!("Failed to link recording thumbnail valve -> queue: {error:?}"))?;
    thumb_queue.link(&thumb_encoder).map_err(|error| {
        format!("Failed to link recording thumbnail queue -> jpegenc: {error:?}")
    })?;
    thumb_encoder.link(&thumb_appsink).map_err(|error| {
        format!("Failed to link recording thumbnail jpegenc -> appsink: {error:?}")
    })?;

    // Per-track internal chains — linked now that every element is inside the
    // pipeline (common-bin-ancestor requirement). Each track links its own
    // encoder straight into the muxer (mp4mux requests a new sink pad per
    // track), all before any element goes PLAYING.
    for (_, chain, valve, aac) in &audio_track_chains {
        chain[0].link(&chain[1]).map_err(|error| {
            format!("Failed to link recording audio queue -> resample: {error:?}")
        })?;
        chain[1].link(&chain[2]).map_err(|error| {
            format!("Failed to link recording audio convert -> caps: {error:?}")
        })?;
        chain[2]
            .link(valve)
            .map_err(|error| format!("Failed to link recording audio caps -> valve: {error:?}"))?;
        valve.link(aac).map_err(|error| {
            format!("Failed to link recording audio valve -> voaacenc: {error:?}")
        })?;
        aac.link(&muxer)
            .map_err(|error| format!("Failed to link recording voaacenc -> mp4mux: {error:?}"))?;
    }

    for element in &elements {
        element
            .sync_state_with_parent()
            .map_err(|error| format!("Failed to sync recording branch element state: {error}"))?;
    }

    // sync_state_with_parent is ASYNC: sink pads FLUSH during the NULL→PLAYING
    // transition. Requesting fresh tap-tee pads mid-transition makes the
    // already-PLAYING tap tees push into flushing pads → FLUSHING upstream →
    // the game chain stalls after a few buffers (field: game-audio tap
    // contributed nothing to recordings). Wait only for the per-track input
    // chains, not mp4mux: an aggregator with a closed valve and no requested
    // audio pads is expected to remain PAUSED until its first buffers arrive.
    // Including mp4mux here used to block start-recording for 10 seconds,
    // causing the Electron request timeout. The leaky input queue makes it
    // safe to link fresh pads while the muxer is still waiting for preroll.
    if !audio_track_chains.is_empty() {
        let pre_link_elements: Vec<gst::Element> = audio_track_chains
            .iter()
            .flat_map(|(_, chain, valve, _)| chain.iter().chain(std::iter::once(valve)))
            .cloned()
            .collect();
        let transition_deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(1_000);
        loop {
            let all_playing = pre_link_elements
                .iter()
                .all(|element| element.current_state() >= gst::State::Playing);
            if all_playing || std::time::Instant::now() >= transition_deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let not_playing: Vec<String> = pre_link_elements
            .iter()
            .filter(|element| element.current_state() < gst::State::Playing)
            .map(|element| format!("{}={:?}", element.name(), element.current_state()))
            .collect();
        if !not_playing.is_empty() {
            send_log(
                &event_sender,
                "warn",
                format!(
                    "Recording audio input elements did not reach PLAYING before tap link; proceeding without blocking live media: {not_playing:?}."
                ),
            );
        }
    }

    // Request a fresh pad from each audio tap tee and link it into the track's
    // decoupling queue sink. A tap tee that fails to provide/link a pad is skipped
    // with a warning so one bad audio source can never abort the whole
    // recording (video still records).
    let mut audio_taps_linked: Vec<(gst::Element, gst::Pad)> = Vec::new();
    for (tap_tee, chain, _, _) in &audio_track_chains {
        let Some(request_pad) = tap_tee.request_pad_simple("src_%u") else {
            send_log(
                &event_sender,
                "warn",
                format!(
                    "Skipping recording audio tap {}: failed to request a fresh pad.",
                    tap_tee.name()
                ),
            );
            continue;
        };
        let queue_sink = chain[0]
            .static_pad("sink")
            .ok_or_else(|| "Recording audio queue has no sink pad.".to_owned())?;
        if let Err(error) = request_pad.link(&queue_sink) {
            send_log(
                &event_sender,
                "warn",
                format!(
                    "Skipping recording audio tap {}: fresh pad link failed: {error:?}",
                    tap_tee.name()
                ),
            );
            let _ = tap_tee.release_request_pad(&request_pad);
            continue;
        }
        audio_taps_linked.push((tap_tee.clone(), request_pad));
    }

    let eos_seen = Arc::new(AtomicBool::new(false));
    // Inactive until the first `start()`; the chunk probe is gated on this so
    // no muxer output is ever captured while the branch is idle.
    let active = Arc::new(AtomicBool::new(false));
    let appsink_sink_pad = appsink
        .static_pad("sink")
        .ok_or_else(|| "Recording appsink has no sink pad.".to_owned())?;

    let chunk_sender = event_sender.clone();
    let chunk_active = active.clone();
    appsink_sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
        if chunk_active.load(Ordering::SeqCst) {
            if let Some(buffer) = info.buffer() {
                if let Ok(mapped) = buffer.map_readable() {
                    let chunk_base64 = BASE64_STANDARD.encode(mapped.as_slice());
                    if let Some(sender) = &chunk_sender {
                        let _ = sender.send(Event::RecordingChunk { chunk_base64 });
                    }
                }
            }
        }
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

    // Thumbnail capture: the very first JPEG frame is stored (base64) and the
    // thumb valve closes immediately after, so jpegenc runs for exactly one
    // frame per recording (zero cost while idle — valve starts closed).
    let thumbnail = Arc::new(Mutex::new(None));
    let thumb_slot = thumbnail.clone();
    let thumb_gate = thumb_valve.clone();
    let thumb_active = active.clone();
    let thumb_sink_pad = thumb_appsink
        .static_pad("sink")
        .ok_or_else(|| "Thumbnail appsink has no sink pad.".to_owned())?;
    thumb_sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
        if !thumb_active.load(Ordering::SeqCst) {
            return gst::PadProbeReturn::Ok;
        }
        let mut slot = match thumb_slot.lock() {
            Ok(slot) => slot,
            Err(_) => return gst::PadProbeReturn::Ok,
        };
        if slot.is_none() {
            if let Some(buffer) = info.buffer() {
                if let Ok(mapped) = buffer.map_readable() {
                    *slot = Some(BASE64_STANDARD.encode(mapped.as_slice()));
                    // One frame is enough; close the grabber so jpegenc idles.
                    thumb_gate.set_property("drop", true);
                }
            }
        }
        gst::PadProbeReturn::Ok
    });

    let mut chain_desc = match download_factory {
        Some(factory) => {
            format!("tee → valve → queue → {factory} → videoconvert → x264enc → mp4mux → appsink")
        }
        None => "tee → valve → queue → videoconvert → x264enc → mp4mux → appsink".to_owned(),
    };
    if !audio_valves.is_empty() {
        chain_desc.push_str(&format!(
            " | {} audio track(s): tap → queue → resample → convert → caps → valve → voaacenc → mp4mux",
            audio_valves.len()
        ));
    }
    chain_desc.push_str(" | thumbnail: tee → valve → jpegenc → appsink");
    send_log(
        &event_sender,
        "info",
        format!("Attached native recording branch ({chain_desc})."),
    );

    Ok(GstreamerRecordingState {
        valve,
        audio_valves,
        thumb_valve,
        thumbnail,
        appsink,
        muxer,
        elements,
        tee: tee.clone(),
        audio_taps: audio_taps_linked,
        queue,
        eos_seen,
        active,
        spent: Arc::new(AtomicBool::new(false)),
        event_sender,
    })
}

/// Tear down a spent recording branch: release each audio track's fresh tee
/// pad back to its tap tee, unlink the video branch from the video tap tee,
/// release the tee request pad, and remove the branch elements. Only called
/// after a finalized (EOS) recording, when the branch is quiescent.
fn teardown_recording_branch(pipeline: &gst::Pipeline, recording: &GstreamerRecordingState) {
    // Unlink and release each audio track's fresh pad back to its tap tee so
    // the next recording can request a new one (reusing a spent pad that has
    // seen EOS/teardown events is what left dangling queues dead).
    for (tap_tee, fresh_pad) in &recording.audio_taps {
        if let Some(peer) = fresh_pad.peer() {
            let _ = fresh_pad.unlink(&peer);
        }
        tap_tee.release_request_pad(fresh_pad);
    }
    if let Some(valve_sink_pad) = recording.valve.static_pad("sink") {
        if let Some(tee_src_pad) = valve_sink_pad.peer() {
            let _ = tee_src_pad.unlink(&valve_sink_pad);
            recording.tee.release_request_pad(&tee_src_pad);
        }
    }
    for element in recording.elements.iter().rev() {
        let _ = element.set_state(gst::State::Null);
        let _ = pipeline.remove(element);
    }
    send_log(
        &recording.event_sender,
        "info",
        "Tore down finalized native recording branch (ready for a fresh branch).".to_owned(),
    );
}

#[cfg(test)]
mod mic_pipeline_tests {
    use super::*;

    /// The mic send path must be COMPLETELY dead while the mic is off: no
    /// buffers may flow (→ no Opus → no RTP), not even volume-0 silence. The
    /// old volume-only mute kept continuous outgoing RTP alive, and that
    /// continuous stream was the only structural delta behind the periodic
    /// video stalls. Disabling pauses the source; enabling resumes it.
    #[test]
    fn mic_pipeline_pauses_source_when_disabled() {
        gst::init().expect("gstreamer init");
        let pipeline = gst::Pipeline::new();
        let source = gst::ElementFactory::make("audiotestsrc")
            .name("mic-test-src")
            .build()
            .expect("audiotestsrc");
        let volume = gst::ElementFactory::make("volume")
            .name("mic-volume")
            .build()
            .expect("volume");
        let sink = gst::ElementFactory::make("fakesink")
            .name("mic-test-sink")
            .build()
            .expect("fakesink");
        sink.set_property("sync", false);
        pipeline.add(&source).expect("add source");
        pipeline.add(&volume).expect("add volume");
        pipeline.add(&sink).expect("add sink");
        source.link(&volume).expect("src -> volume");
        volume.link(&sink).expect("volume -> sink");

        let buffers = Arc::new(AtomicU32::new(0));
        let counter = buffers.clone();
        sink.static_pad("sink").expect("sink pad").add_probe(
            gst::PadProbeType::BUFFER,
            move |_pad, _info| {
                counter.fetch_add(1, Ordering::SeqCst);
                gst::PadProbeReturn::Ok
            },
        );

        let mic = GstreamerMicPipeline {
            volume: volume.clone(),
            elements: vec![source.clone(), volume.clone()],
        };
        pipeline
            .set_state(gst::State::Playing)
            .expect("play pipeline");
        // A pipeline's state change is asynchronous; wait until PLAYING is
        // reached before toggling (the in-flight transition would otherwise
        // override the source pause).
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while pipeline.current_state() != gst::State::Playing
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(pipeline.current_state(), gst::State::Playing);
        // Drain anything that flowed before the pause so the disabled-window
        // counter starts at zero.
        buffers.store(0, Ordering::SeqCst);

        // Disabled → chain run back to NULL → the flow must STOP (a tiny
        // in-flight burst during the async NULL transition is tolerated; what
        // matters is that the counter stops growing).
        mic.set_enabled(false);
        std::thread::sleep(std::time::Duration::from_millis(500));
        let after_settle = buffers.load(Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(500));
        let after_wait = buffers.load(Ordering::SeqCst);
        assert_eq!(
            after_wait, after_settle,
            "no buffers (→ no RTP) may flow while the mic is off (after settle={after_settle}, after wait={after_wait})"
        );

        // Enabled → chain resumes → buffers flow again. The full suite runs
        // many live pipelines in parallel, so the state transition + first
        // buffers can take far longer than a fixed sleep; poll with a
        // deadline instead of asserting against a fixed window.
        mic.set_enabled(true);
        let flow_deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        loop {
            let flowing = buffers.load(Ordering::SeqCst);
            if flowing > after_wait || std::time::Instant::now() >= flow_deadline {
                let while_enabled = flowing;
                assert!(
                    while_enabled > after_wait,
                    "buffers must flow once the mic is enabled (before={after_wait}, after={while_enabled})"
                );
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = pipeline.set_state(gst::State::Null);
    }
}
