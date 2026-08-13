use crate::gstreamer_backend::send_log;
use crate::gstreamer_config::{
    automatic_present_max_fps, av1_decoder_preference, h265_decoder_preference,
    requested_video_backend, use_external_renderer_window, use_internal_renderer,
    use_stacked_renderer, vrr_present_max_fps, zero_copy_requested, CodecDecoderPreference,
    EXTERNAL_RENDERER_ENV, NATIVE_D3D_FULLSCREEN_ENV, NATIVE_PRESENT_MAX_FPS_ENV,
    NATIVE_VIDEO_API_ENV, NATIVE_VIDEO_BACKEND_ENV, PRESENT_LIMITER_AUTO_SENTINEL,
    PRESENT_LIMITER_STREAM_SENTINEL, PRESENT_LIMITER_VRR_SENTINEL,
};
#[cfg(target_os = "windows")]
use crate::gstreamer_input::NativeWindowInputBridge;
use crate::gstreamer_input::{
    create_input_data_channels, wire_remote_data_channels, GstreamerInputChannels,
    GstreamerInputState,
};
use crate::gstreamer_ice::{log_remote_ice_candidate, wire_ice_observability};
use crate::gstreamer_liveness::{
    install_present_limiter, pad_has_media_sticky_events, read_queue_level,
    sink_rendered_frame_count, watch_audio_activity, watch_depay_output, watch_first_sink_buffer,
    watch_rtcp_send_stats, watch_rtp_payload_dump, watch_rtp_video_bitrate,
    watch_video_caps_transitions, watch_video_decoded_rate, watch_video_sink_caps_transitions,
    watch_video_sink_rate, VideoLivenessMonitor,
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
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

// Two milliseconds is below the jitter of a real WAN connection and makes
// RTP frame reordering visible as repeated/back-and-forward frames. Keep a
// small playout buffer instead; this is still far below the browser client's
// usual WebRTC buffer while allowing normal Wi-Fi/Internet jitter to settle.
const WEBRTC_LATENCY_MS: u32 = 100;
const DEFAULT_GFN_STUN_SERVER: &str = "stun://stun2.l.google.com:19302";

/// Display-path caps pinned to the FULL-RANGE BT.709-family colorimetry the
/// GFN stream actually carries (`1:3:5:1` = primaries BT.709, transfer BT.709,
/// matrix SMPTE240M, range 0-255). DXVA decoders on different GPUs/drivers
/// may emit no colorimetry at all or a limited-range tag; without a pin the
/// sink then converts full-range data with limited coefficients and the
/// picture comes out washed-out gray. The recording branch already declares
/// the same full-range input for the same reason. Pinning the numeric form
/// keeps already-correct machines byte-identical (the tag matches what their
/// decoder emits) while forcing the right range everywhere else.
pub(crate) const DISPLAY_NV12_FULL_RANGE_CAPS: &str =
    "video/x-raw,format=NV12,colorimetry=1:3:5:1";
/// Compressed-frame jitter buffer between the parser and the decoder (leaky=no,
/// so it blocks the RTP thread instead of dropping). This constant is the DEEP
/// ceiling (~250 ms at 60 fps) that absorbs the 100-300 ms WAN jitter bursts
/// the field logs show (sink fps collapsing to single digits with only
/// 0.03-0.09% packet loss). A fixed deep buffer also added ~150 ms of constant
/// latency, which made in-game drags feel "patah-patah" on stable links — the
/// liveness monitor now resizes this queue ADAPTIVELY and continuously (base 6
/// frames ≈ 100 ms on stable links, ramping with RTT up to this ceiling, with
/// packet-loss floors and a burst-hold at MAX after detected spikes — see
/// target_pre_decode_depth in gstreamer_liveness), polled every watchdog tick
/// so it reacts within ~250 ms instead of the seconds a stale EMA took, so
/// steady-state latency stays tight and the anti-flicker protection engages
/// while the network actually needs it.
pub(crate) const VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS: u32 = 15;
/// Shallow floor of the adaptive compressed-frame jitter buffer (~100 ms at
/// 60 fps). On stable links (RTT ≤ ~30 ms) the buffer rests here, keeping
/// drag/input feel tight.
pub(crate) const VIDEO_COMPRESSED_QUEUE_BASE_BUFFERS: u32 = 6;
/// Mid depth (~167 ms at 60 fps) used as the packet-loss floor (≥0.1%) and
/// around the middle of the RTT ramp.
pub(crate) const VIDEO_COMPRESSED_QUEUE_MID_BUFFERS: u32 = 10;
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
/// Default recording encoder bitrate (kbps) used until the backend supplies
/// the session's negotiated maximum (session start always overrides it).
const RECORD_BITRATE_DEFAULT_KBPS: u32 = 8_000;
/// How long to wait after the muxer-direct EOS failsafe before giving up on
/// finalizing the recording.
const RECORDING_FAILSAFE_TIMEOUT_MS: u64 = 2_000;
/// How long the OFFLINE pass-through remux (temp ES files → limited H.264
/// MP4 via decode + range conversion + re-encode, in a throwaway pipeline)
/// may run before giving up. The Electron `stop-recording` request and the
/// `recording-finished` wait must budget for it (they were raised to 60 s in
/// step with this). A short clip remuxes in a few seconds; a long one (tens
/// of seconds at 1080p60) can take 15-40 s through the software x264
/// fallback on a weak CPU, so 45 s leaves headroom for the drain + EOS waits.
const RECORDING_REMUX_TIMEOUT_MS: u64 = 45_000;
/// How long `start()` waits (after the sticky-event replay) for the rebuilt
/// branch queue(s) to show the mandatory sticky CAPS event before opening the
/// capture valve. On a rebuild the fresh branch is hot-plugged into a PLAYING
/// pipeline: the tap tee re-emits its retained sticky events onto the new
/// branch pad asynchronously (on the tee's streaming thread), and if the
/// valve opens before stream-start/caps/segment land, the first live buffer
/// hits the queue with no stream-start (the field warning `Got data flow
/// before stream-start` on the rebuilt queue) and the branch can stall so
/// badly that stop() times out. CAPS is sticky and arrives AFTER stream-start
/// in the replay order, so its presence proves the whole sequence landed.
/// Bounded — a genuinely broken branch degrades to a warning, never a hang.
const RECORDING_STICKY_VERIFY_TIMEOUT_MS: u64 = 250;
/// The finalized MP4 is handed to Electron as base64 `recording-chunk` events
/// over a line-delimited stdout protocol, and Electron parses/decodes each
/// line SYNCHRONOUSLY in its main process. A multi-hundred-MB recording must
/// therefore never be ONE base64 line: a 70 MB file is ~94 MB of base64, and
/// `JSON.parse` + `Buffer.from(..., "base64")` on that single line freezes
/// the Electron UI ("Electron is not responding") and can crash the app
/// before the tmp file is ever written (the field's 0-byte .tmp). Emit
/// small fixed-size chunks instead — the protocol already says chunks arrive
/// in file order and must be appended in arrival order, and Electron does
/// exactly that through its serialized recordingChunkQueue.
const RECORDING_CHUNK_BYTES: usize = 1_048_576;
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
    /// Tap tee kept in the main video path from chain construction onward.
    /// Keeping it there avoids a live queue→sink unlink/relink when the first
    /// screenshot is requested (that renegotiation was the source of the
    /// visible flicker and decoder stalls in the field log).
    pub(crate) tee: Option<gst::Element>,
    /// Whether the optional screenshot branch has been attached to the tee.
    /// Decoder fallback is safe while this is false because only the main
    /// tee branch needs to be rewired; once a screenshot branch is attached,
    /// leave the tap topology intact.
    pub(crate) branch_attached: bool,
    /// The element the tee is inserted after (the post-decode queue).
    pub(crate) before_sink: gst::Element,
    /// The video sink the tee feeds.
    pub(crate) sink: gst::Element,
    pub(crate) video_api: RtpVideoApi,
    /// Whether the chain actually negotiated D3D memory. This is derived from
    /// the chain caps, not the global zero-copy preference: H264-D3D12 now
    /// deliberately downloads to system memory even when the preference is on.
    pub(crate) zero_copy: bool,
    /// RTP-level tap tee, permanently embedded between the webrtcbin video
    /// src pad and the decode chain. The native remux recorder taps the RTP
    /// stream here (before decode), so record start/stop never touches the
    /// decode/present chain — and a decoder-fallback rebuild of the decode
    /// chain (try_rebuild) keeps this tee and the recording branch on it
    /// intact. NOTE: this pad is the internal rtpbin session output — the
    /// RTP is NACK/RTX/FEC-recovered and jitter-ordered — so the pass-through
    /// recorder inherits the live session's loss recovery for free. Set once
    /// at the initial chain build, preserved across rebuilds.
    pub(crate) rtp_tee: Option<gst::Element>,
}

impl GstreamerVideoTap {
    /// Return the tap tee that was installed with the main video chain.
    /// Screenshot capture must not restructure the live queue→sink path: doing
    /// a first-use unlink/relink causes caps renegotiation and a visible
    /// present reset on the bundled D3D12 runtime.
    pub(crate) fn ensure_tee(&mut self, _pipeline: &gst::Pipeline) -> Result<gst::Element, String> {
        self.tee.clone().ok_or_else(|| {
            "Video tap tee is not ready: the native video chain has not finished building."
                .to_owned()
        })
    }

    pub(crate) fn mark_branch_attached(&mut self) {
        self.branch_attached = true;
    }
}

/// How the native recording branch stores the video.
///
/// `Encoded` (the DEFAULT) taps the DECODED video frames (the same reliable
/// post-decode tee the transcode branch uses) and re-encodes to H.264 — so
/// the recording captures EXACTLY what the user sees, immune to WAN RTP
/// jitter/loss and GFN stream restarts that the raw-RTP tap cannot survive,
/// and the file plays everywhere (universal H.264; no HEVC/AV1 codec needed
/// on the playback device). The encoded ES is written to a temp file and
/// muxed OFFLINE at stop (`remux_encoded_recording`) — no live muxer, so the
/// branch can never wedge. The re-encode costs GPU/CPU while recording (the
/// encoder competes with the decoder on single-iGPU laptops).
///
/// `PassThrough` taps the RAW RTP stream at the source tee (before decode)
/// and remuxes the received bitstream straight into the MP4 (depay → parse →
/// qtmux). Zero decode, zero re-encode, zero GPU/CPU — a recording costs
/// nothing to the live stream, exactly like the official GeForce Now client
/// ("just write what we receive to disk"). The file is H265/H264/AV1
/// (whatever the server sent) at the exact received quality, tagged with the
/// stream's colour metadata (colr nclx BT.709 full-range). Opt-in via
/// OPENNOW_RECORDING_MODE=passthrough. Two field-proven caveats keep it
/// opt-in rather than default: (1) the tap sits at the webrtcbin video SRC
/// pad — the internal rtpbin session output — so NACK/RTX/FEC recovery and
/// jitter reordering already happened before the branch sees a packet (the
/// live decode chain, fed from the SAME tee without its own jitter buffer,
/// renders lossless in the field; the branch sees only the video SSRC). What
/// remains on a lossy link is a NACK-unrecoverable hole; the branch resyncs
/// from the next PLI keyframe (request-keyframe + wait-for-keyframe) so the
/// ES keeps clean GOPs instead of broken ones; (2) the file carries the
/// stream codec, so H265/AV1 recordings need a player with HEVC/AV1 support
/// (Windows' default player needs the HEVC extension).
///
/// `Transcode` taps the DECODED video (post-decode tee), converts to 8-bit
/// 4:2:0 and re-encodes with H.264 (d3d12h264enc GPU encoder by default)
/// into a LIVE qtmux (a live muxer wedged the recording branches in the
/// field, which is why `Encoded` muxes offline instead). Universal H.264
/// output, but the encode competes with the decoder for the same GPU — on
/// single-iGPU laptops this shows up as decode-time spikes and stream
/// stutter while recording (the field report). Opt-in via
/// OPENNOW_RECORDING_MODE=transcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecordingMode {
    PassThrough,
    Encoded,
    Transcode,
}

impl RecordingMode {
    pub(crate) fn from_env() -> Self {
        match std::env::var("OPENNOW_RECORDING_MODE").as_deref() {
            Ok("passthrough") => RecordingMode::PassThrough,
            Ok("transcode") => RecordingMode::Transcode,
            Ok("encoded") => RecordingMode::Encoded,
            // Default: re-encode to universal H.264 from the reliable
            // post-decode tap. PassThrough (zero cost, but raw-RTP holes on
            // jitter + H265/AV1 files that need codec support on the player)
            // stays available via OPENNOW_RECORDING_MODE=passthrough.
            _ => RecordingMode::Encoded,
        }
    }
}

/// Native recording branch — TRANSCODE to H.264. The branch taps the DECODED
/// video right before the sink (the permanent video tap tee in the live
/// chain, the same tee screenshots use), converts it to 8-bit 4:2:0,
/// re-encodes with H.264 (d3d12h264enc GPU encoder by default so recording
/// costs ~0 CPU, with x264enc software fallback when no D3D12 video-encode
/// adapter is available), and muxes with AAC game audio into a standard
/// seekable MP4 (faststart):
///
///   tap tee → valve → queue → [d3d12download] → videoconvert → capsfilter
///   (FULL bt709) → H.264 encoder → h264parse → qtmux → swallow
///
/// Re-encoding (instead of the old source-bitstream RTP remux) is what makes
/// the recording UNIVERSAL — the field complaint: recordings of GFN's H.265
/// stream play back glitchy on weak devices (HEVC 1080p60 is heavy to
/// decode, and the remux file had a mid-GOP orphan-frame start plus
/// untagged color range that players mis-render). Transcoding fixes all of
/// it at once:
///
/// - Universal: H.264 main profile, 8-bit, plays on any device/player.
/// - Colors: the GFN stream is FULL-RANGE PC video (decoder output 0-255),
///   but H.264 playback expects LIMITED range and every field player expands
///   H.264 content as limited, so the branch converts 0-255 → 16-235 (RGB
///   round-trip, exact at both ends) and tags nothing (x264 with
///   insert-vui=false) — the recording is then limited + untagged, exactly
///   like the official GeForce Now PC recordings, and renders with the same
///   colors as the live stream and the in-app screenshot.
/// - Glitch-free start: the encoder begins a fresh GOP (IDR) at the first
///   frame it sees, so the file NEVER starts mid-GOP with orphan P-frames
///   referencing pre-recording frames (the decode glitches at the head of
///   the old files). Recording also starts with zero delay: no waiting for
///   the next server keyframe.

/// The branch is built at session start off the video tap tee (after the
/// decode chain is up), and the tee pad stays linked for the session, so
/// record start/stop is ONLY a valve open/close — the decode/present chain
/// is never touched and the pipeline cannot re-preroll (the exact failure of
/// the old post-decode x264 branch with a sink; this branch has NO sink —
/// the swallow queue tail with DROP probes is the same hot-plug-safe shape
/// as the old remux branch). The encoder runs only while the valve is open,
/// so recording costs no CPU when idle.
///
/// PASS-THROUGH branch flow diagnostics: zero-cost buffer counters at every
/// hop of the live recording branch, so a field session where the video ES
/// stays 0 bytes while the live stream renders (2026-08-12T22:58 log) can be
/// pinned to the exact element that stops flowing — tee→valve (valve_in),
/// valve→queue (queue_in), queue→depay (depay_in), depay→parse (parse_in),
/// parse→filesink (file_in). The counters are read and logged as one line by
/// `log()`, called on every start()/stop() and on the branch's first buffer.
#[derive(Debug, Clone)]
pub(crate) struct PassThroughFlowDiag {
    pub(crate) label: String,
    pub(crate) valve_in: Arc<AtomicU64>,
    pub(crate) queue_in: Arc<AtomicU64>,
    pub(crate) depay_in: Arc<AtomicU64>,
    pub(crate) parse_in: Arc<AtomicU64>,
    /// Buffers pushed out of the parser's SRC pad (valid AUs after the
    /// parser's garbage rejection). Comparing parse_in vs parse_out tells a
    /// field wedge apart: parse_in > parse_out means the depayloader emitted
    /// AUs with holes (NACK-unrecoverable packet gaps on a lossy link) that
    /// the parser dropped — the field: 384/2197 on a lossy session vs 64/2555
    /// on a clean one; parse_out > file_in would mean the ES capsfilter or
    /// filesink stalled.
    pub(crate) parse_out: Arc<AtomicU64>,
    pub(crate) file_in: Arc<AtomicU64>,
    /// First CAPS event seen at the parse sink pad (what the depayloader
    /// negotiated), captured once for diagnostics.
    pub(crate) parse_first_caps: Arc<Mutex<Option<String>>>,
    /// pt / ssrc / marker / len of the first few RTP buffers at the branch
    /// valve sink pad (proves only the video SSRC reaches the post-rtpbin
    /// tap — FlexFEC/RTX are consumed inside webrtcbin's rtpbin).
    pub(crate) valve_first_rtp: Arc<Mutex<Vec<String>>>,
    /// The negotiated video RTP payload type, learned from the first CAPS
    /// event at the valve sink pad. Defensive gate: drops any non-video RTP
    /// that ever reaches the branch. The tap is post-rtpbin, so in the field
    /// this filter drops nothing (dropped_non_video=0 — the session pad only
    /// carries the video SSRC); it guards the depayloader in case a future
    /// webrtcbin config exposes RTX/FEC ssrcs on the session pad.
    pub(crate) video_pt: Arc<AtomicU32>,
    /// How many RTP packets were dropped by the video_pt filter (non-video
    /// traffic that reached the shared tee), for diagnostics.
    pub(crate) dropped_non_video: Arc<AtomicU64>,
    pub(crate) event_sender: Option<Sender<Event>>,
}

impl PassThroughFlowDiag {
    pub(crate) fn new(label: String, event_sender: Option<Sender<Event>>) -> Self {
        Self {
            label,
            valve_in: Arc::new(AtomicU64::new(0)),
            queue_in: Arc::new(AtomicU64::new(0)),
            depay_in: Arc::new(AtomicU64::new(0)),
            parse_in: Arc::new(AtomicU64::new(0)),
            parse_out: Arc::new(AtomicU64::new(0)),
            file_in: Arc::new(AtomicU64::new(0)),
            parse_first_caps: Arc::new(Mutex::new(None)),
            valve_first_rtp: Arc::new(Mutex::new(Vec::new())),
            video_pt: Arc::new(AtomicU32::new(0)),
            dropped_non_video: Arc::new(AtomicU64::new(0)),
            event_sender,
        }
    }

    pub(crate) fn log(&self, tag: &str) {
        let caps = self
            .parse_first_caps
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .unwrap_or_else(|| "none".to_owned());
        let first_rtp = self
            .valve_first_rtp
            .lock()
            .ok()
            .map(|slot| slot.join(" "))
            .unwrap_or_default();
        let video_pt = self.video_pt.load(Ordering::Relaxed);
        let dropped = self.dropped_non_video.load(Ordering::Relaxed);
        send_log(
            &self.event_sender,
            "info",
            format!(
                "[RecBranchFlow:{tag}] {} valve_in={} queue_in={} depay_in={} parse_in={} parse_out={} file_in={} video_pt={video_pt} dropped_non_video={dropped} parse_caps={} first_rtp=[{first_rtp}]",
                self.label,
                self.valve_in.load(Ordering::Relaxed),
                self.queue_in.load(Ordering::Relaxed),
                self.depay_in.load(Ordering::Relaxed),
                self.parse_in.load(Ordering::Relaxed),
                self.parse_out.load(Ordering::Relaxed),
                self.file_in.load(Ordering::Relaxed),
                caps,
            ),
        );
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GstreamerRecordingState {
    /// Video branch valve, FIRST element after the video tap tee. Closed
    /// (drop=true) whenever no recording is in flight; opening it is the
    /// entire "start recording" operation.
    pub(crate) valve: gst::Element,
    /// Leaky decoupling queue (valve → queue → …). It never back-pressures
    /// the live decode path.
    pub(crate) queue: gst::Element,
    /// The H.264 encoder actually in use (d3d12h264enc GPU encoder by
    /// default when the D3D12 video-encode adapter is available, else the
    /// software x264enc). Only fed while the valve is open.
    pub(crate) encoder: gst::Element,
    /// Factory name of `encoder`, for logs and tests.
    pub(crate) encoder_factory: String,
    /// Recording encoder bitrate (kbps) for the OFFLINE pass-through remux
    /// re-encode. Copied from the render state at stop time (the session's
    /// negotiated "max bitrate"); defaults to RECORD_BITRATE_DEFAULT_KBPS
    /// for test callers that build a branch without a session.
    pub(crate) record_bitrate_kbps: Arc<AtomicU32>,
    /// Converts the encoder's byte-stream H.264 into avcC for qtmux. After a
    /// finalized recording (EOS) it stays EOS'd and would silently swallow
    /// the next recording's buffers, so the whole branch is torn down and
    /// rebuilt fresh for the next recording.
    pub(crate) h264_parse: gst::Element,
    /// First videoconvert: converts the decoder's native output (NV12) to the
    /// encoder's input format (NV12 for d3d12h264enc, I420 for the software
    /// encoders). Stateless; rebuilt fresh with the branch between
    /// recordings.
    pub(crate) video_convert: gst::Element,
    /// Declares the branch input as FULL-RANGE BT.709 (0-255) — what the
    /// decoder actually outputs (the same data the screenshot branch writes
    /// straight into a PNG). The branch then scales it 0-255 → 16-235 with a
    /// LUT-based Y-plane probe (`range_lut_probe`), matching the official
    /// GeForce Now recordings (limited + untagged), which render correctly on
    /// the field. H.264 playback expects LIMITED range and every player on
    /// the field expands H.264 content as limited, so full-range (or
    /// clip-only "limited") data comes out with crushed blacks
    /// ("hitam pekat").
    pub(crate) video_declare_caps: gst::Element,
    /// Demands LIMITED-range BT.709 (16-235) at the encoder input.
    pub(crate) video_encode_caps: gst::Element,
    /// The FULL→LIMITED (0-255 → 16-235) Y-plane scaler probe attached to
    /// the range bridge's (`range_convert`) sink pad. Behind a mutex because
    /// PadProbeId is not Clone, hence the interior mutability instead of a
    /// plain field. The probe is attached at build time (in the builder) and
    /// torn down with the whole branch between recordings.
    pub(crate) range_lut_probe: Arc<Mutex<Option<gst::PadProbeId>>>,
    /// The videoconvert that bridges the FULL→LIMITED colourimetry change
    /// between the declare caps and the encoder caps (the caps must intersect
    /// for the elements to link; the LUT probe rescales the data).
    pub(crate) range_convert: gst::Element,
    /// Optional D3D11/D3D12 texture→system-memory downloader (only when
    /// zero-copy is active). Rebuilt fresh with the branch between
    /// recordings.
    pub(crate) video_download: Option<gst::Element>,
    /// The qtmux element (standard seekable MP4, faststart). TRANSCODE mode
    /// only — pass-through muxes OFFLINE at stop in a throwaway pipeline (the
    /// live-path muxer was the wedge: both the video and audio branches froze
    /// at the muxer input mid-recording while the live stream stayed healthy).
    /// After a finalized recording it is spent (moov already emitted / EOS
    /// seen), so the whole branch is rebuilt fresh between recordings.
    pub(crate) muxer: Option<gst::Element>,
    /// Swallow queue at the muxer output. TRANSCODE mode only. The chunk +
    /// EOS probes live on its sink pad and return DROP, so the muxer's push
    /// always returns FLOW_OK and its src task keeps running. Deliberately
    /// NO sink in the branch: a sink that cannot preroll while the valve is
    /// closed would complete its deferred async transition later and
    /// re-preroll the whole pipeline (non-sinks transition synchronously, so
    /// the branch is hot-plug-safe).
    pub(crate) swallow: Option<gst::Element>,
    /// PASS-THROUGH mode: the temp elementary-stream file the video branch
    /// (depay → parse → filesink) writes into while recording. Remuxed
    /// offline to MP4 at stop, then deleted. None for transcode mode.
    pub(crate) video_es_path: Option<std::path::PathBuf>,
    /// PASS-THROUGH mode: the temp AAC elementary-stream file (game audio,
    /// avenc_aac output) remuxed offline into the MP4 at stop.
    pub(crate) audio_es_path: Option<std::path::PathBuf>,
    /// PASS-THROUGH mode: capsfilter between the parser and the video ES
    /// filesink. Forces the self-identifying byte-stream (H264/H265) or
    /// OBU-stream (AV1) format — the parsers' DEFAULT output against an
    /// unconstrained filesink is length-prefixed avcC, which the OFFLINE
    /// remux parser cannot auto-detect from a bare file ("No caps set").
    pub(crate) es_caps: Option<gst::Element>,
    /// PASS-THROUGH mode: the filesink writing `video_es_path`.
    pub(crate) video_filesink: Option<gst::Element>,
    /// PASS-THROUGH mode: the filesink writing `audio_es_path`.
    pub(crate) audio_filesink: Option<gst::Element>,
    /// PASS-THROUGH mode: set by an EOS probe on the video filesink sink pad
    /// — once true, the ES file is closed and safe to remux offline.
    filesink_eos: Arc<AtomicBool>,
    /// PASS-THROUGH mode: EOS seen at the audio filesink.
    audio_filesink_eos: Arc<AtomicBool>,
    eos_seen: Arc<AtomicBool>,
    active: Arc<AtomicBool>,
    /// Recording mode this branch was built for. `PassThrough` taps the raw
    /// RTP stream (zero cost); `Transcode` re-encodes decoded frames. The
    /// rebuild path re-builds the SAME mode so the next recording matches.
    pub(crate) mode: RecordingMode,
    /// Codec of the video bitstream for `PassThrough` branches ("H265",
    /// "H264", "AV1") — picks the depayloader/parser. For `Transcode` the
    /// output codec is always "H264".
    pub(crate) codec: String,
    /// Pass-through mode: the negotiated stream frame rate. The OFFLINE remux
    /// uses it to reconstruct the PTS/DTS that raw ES files cannot carry
    /// (timestamps do not survive file I/O — qtmux rejects buffers with no
    /// PTS). GFN's low-latency encoders emit no B-frames (has_b_frames=0 on
    /// every real H264 AND H265 recording), so decode order == presentation
    /// order and a monotonic PTS ladder at `fps` is exact. Transcode mode
    /// never uses it (0).
    pub(crate) fps: u32,
    /// ENCODED mode: measured PTS cadence of the decoded frames that flowed
    /// through the recording valve (i.e. the frames that became the ES):
    /// first PTS, last PTS (ns) and frame count. At stop the OFFLINE remux
    /// derives the PTS ladder from the MEASURED average interval instead of
    /// the negotiated fps, so a stream that delivers fewer unique frames than
    /// negotiated (rate-adaptive congestion, or a game that renders slower
    /// than the negotiated rate WITHOUT frame-fill) still records real-time —
    /// a fixed negotiated ladder would stretch the video 2× (30 fps of
    /// content stamped at 60 fps → half the real duration, audio stranded).
    /// GFN fills the negotiated cadence with duplicated frames in the normal
    /// case, so the measured rate lands at the negotiated rate and nothing
    /// changes there.
    pub(crate) rec_frames_first_pts: Arc<AtomicU64>,
    pub(crate) rec_frames_last_pts: Arc<AtomicU64>,
    pub(crate) rec_frames_count: Arc<AtomicU64>,
    /// Buffers pushed into the recording branch's decoupling queue (valve
    /// sink side). Every recording frame enters here; buffers dropped by the
    /// leaky queue (encoder/queue could not keep up — the choppy-recording
    /// case on weak devices) are counted in `queue_input` but never reach
    /// `queue_output`. Reset on `start()`; the drop count is exact at `stop()`
    /// after the queue drains.
    pub(crate) queue_input: Arc<AtomicU64>,
    /// Buffers that left the recording branch's decoupling queue (src side)
    /// and were handed to the encoder/filesink.
    pub(crate) queue_output: Arc<AtomicU64>,
    /// Dropped recording frames of the LAST finished recording
    /// (`queue_input − queue_output` after the drain at stop) — sent to
    /// Electron in `recording-finished` so the UI can tell the user why the
    /// clip is choppier than the stream.
    pub(crate) dropped_frames: Arc<AtomicU64>,
    /// Pass-through IDR gate: while false, buffers are dropped until the
    /// first sync (IDR) access unit arrives, so the remuxed file never starts
    /// mid-GOP with orphan P-frames (the old remux glitch). `start()` resets
    /// it to false so every recording starts at a fresh IDR.
    pub(crate) idr_gate: Arc<AtomicBool>,
    /// RTP-level tap tee on the game-audio stream (between the webrtcbin
    /// audio src pad and the decode chain). Created when the audio RTP pad
    /// arrives in `wire_incoming_media_sink`; the audio transcode branch
    /// hangs off it so the recording gets the game audio without touching the
    /// live audio decode/playback path.
    pub(crate) audio_rtp_tee: Option<gst::Element>,
    /// Audio branch valve. Like the video valve, record start/stop is just
    /// drop=true/false; EOS for finalize is injected BELOW this valve.
    pub(crate) audio_valve: Option<gst::Element>,
    /// Audio branch decoupling queue (valve → queue → rtpopusdepay). Drained
    /// before EOS, same as the video queue.
    pub(crate) audio_queue: Option<gst::Element>,
    /// Audio RTP capsfilter, which strips SDP-only fields before rtpopusdepay.
    /// The tap is before the decode chain's capsfilter, so the recorder must
    /// normalize the raw WebRTC RTP caps independently.
    audio_capsfilter: Option<gst::Element>,
    /// Audio RTP depayloader. Its segment state cannot leak across MP4
    /// recordings, so it is rebuilt fresh with the audio branch between
    /// recordings.
    audio_depayloader: Option<gst::Element>,
    /// Audio decode/encode transforms (rtpopusdepay → opusdec → audioconvert
    /// → AAC). Kept so `teardown()` can remove the ENTIRE audio branch from
    /// the pipeline when the recording state is rebuilt fresh between
    /// recordings (the in-place recycle is unreliable in this GStreamer
    /// build: NULL→PLAYING on a queue kills its src task, and the shared
    /// qtmux keeps round-1 EOS/interleave state).
    audio_opusdec: Option<gst::Element>,
    audio_audioconvert: Option<gst::Element>,
    audio_aac_encoder: Option<gst::Element>,
    /// PASS-THROUGH mode: aacparse + ADTS capsfilter between the AAC encoder
    /// and the audio ES filesink. The encoders (avenc_aac/voaacenc) emit raw
    /// AAC frames with no sync word; aacparse re-frames them into ADTS so the
    /// OFFLINE remux (`filesrc → aacparse`) can frame the file without
    /// external caps.
    pub(crate) audio_aac_parse: Option<gst::Element>,
    pub(crate) audio_adts_caps: Option<gst::Element>,
    /// The video tap tee (post-decode, before the sink) that feeds this
    /// recording branch. Keeping the tee (which is NOT torn down) lets
    /// start() replay its retained sticky stream events
    /// (stream-start/caps/segment) into the freshly built branch.
    pub(crate) video_tap_tee: gst::Element,
    /// True once the audio branch is linked into the muxer. The audio RTP pad
    /// may arrive before or after the video pad; the branch is built from
    /// whichever side is armed first, exactly once.
    pub(crate) audio_branch_built: Arc<AtomicBool>,
    /// True after a finalized (EOS) recording: the branch must be torn down
    /// and rebuilt fresh before the next recording, because the muxer has
    /// already emitted its moov and/or seen EOS and would otherwise produce a
    /// second file without a fresh moov (unplayable), and in-place resets of
    /// the queue/encoder/muxer are unreliable in this GStreamer build.
    spent: Arc<AtomicBool>,
    /// PASS-THROUGH mode only: hop-by-hop buffer counters + first-caps for
    /// field flow diagnostics (see `PassThroughFlowDiag`). Logged on
    /// start/stop/first-buffer so a 0-byte ES file can be pinned to the hop
    /// that stopped flowing.
    pub(crate) pass_flow_diag: Option<PassThroughFlowDiag>,
    event_sender: Option<Sender<Event>>,
}

impl GstreamerRecordingState {
    /// Build the game-audio transcode branch into the SAME qtmux as the video
    /// branch: audio_rtp_tee → valve → capsfilter → queue → rtpopusdepay →
    /// opusdec → audioconvert → AAC encoder → muxer. Game audio is decoded
    /// and re-encoded as AAC so the MP4 plays on ANY device (Opus-in-MP4 is
    /// not universal). No sink — the same hot-plug-safe pattern as the video
    /// branch. Every element is synced to the pipeline state BEFORE the
    /// branch pad is linked into the audio tap tee (the tee is already
    /// PLAYING by the time this runs, so an unsynced NULL element would fail
    /// the first push and could kill the audio RTP flow upstream).
    pub(crate) fn build_audio_branch(&mut self, pipeline: &gst::Pipeline) -> Result<(), String> {
        let Some(audio_rtp_tee) = self.audio_rtp_tee.clone() else {
            return Ok(());
        };
        if self.audio_branch_built.load(Ordering::SeqCst) {
            return Ok(());
        }
        let capsfilter = make_element("capsfilter")?;
        let valve = make_element("valve")?;
        let queue = make_element("queue")?;
        let depayloader = make_element("rtpopusdepay")?;
        capsfilter.set_property(
            "caps",
            "application/x-rtp,media=(string)audio,encoding-name=(string)OPUS,clock-rate=(int)48000"
                .parse::<gst::Caps>()
                .map_err(|error| format!("Invalid recording audio RTP caps: {error}"))?,
        );

        valve.set_property("drop", true);
        // Forward sticky RTP events while buffers are gated. Without this the
        // newly attached audio branch can receive buffers before its segment.
        set_property_from_str_if_supported(&valve, "drop-mode", "forward-sticky-events");
        // Same contract as the video queue: never back-pressure the live path.
        // leaky=upstream drops incoming RTP packets when full, so the audio
        // tap tee's push never blocks on this branch.
        queue.set_property_from_str("leaky", "upstream");
        queue.set_property("max-size-buffers", 30u32);
        queue.set_property("max-size-bytes", 0u32);
        queue.set_property("max-size-time", 0u64);

        let opusdec = make_element("opusdec")?;
        let audioconvert = make_element("audioconvert")?;
        let (aac_factory, aac_encoder) = pick_aac_encoder()?;
        // The opusdec/aacenc chain must not back-pressure the RTP tap either:
        // the audio queue is already leaky (drops oldest when full), so the
        // chain cannot stall the tap tee.

        let mut elements: Vec<gst::Element> = vec![
            valve.clone(),
            capsfilter.clone(),
            queue.clone(),
            depayloader.clone(),
            opusdec.clone(),
            audioconvert.clone(),
            aac_encoder.clone(),
        ];
        // PASS-THROUGH / ENCODED modes: the audio branch ends at its own
        // filesink writing the AAC elementary stream into a temp file
        // (remuxed OFFLINE at stop, exactly like the video ES — no live
        // muxer, so the branch cannot wedge). The raw AAC encoder frames are
        // re-framed into self-describing ADTS (aacparse + adts capsfilter) so
        // the offline remux's aacparse can consume the file with no external
        // caps.
        if self.mode != RecordingMode::Transcode {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let audio_es_path = std::env::temp_dir().join(format!(
                "opennow-rec-{}-{stamp}.aac.es",
                std::process::id()
            ));
            let audio_filesink = make_element("filesink")?;
            audio_filesink.set_property(
                "location",
                audio_es_path
                    .to_str()
                    .ok_or_else(|| "Pass-through audio ES path is not valid UTF-8.".to_owned())?,
            );
            audio_filesink.set_property("sync", false);
            audio_filesink.set_property("async", false);
            let audio_aac_parse = make_element("aacparse")?;
            let adts_caps = make_element("capsfilter")?;
            adts_caps.set_property(
                "caps",
                "audio/mpeg,mpegversion=(int)4,stream-format=(string)adts"
                    .parse::<gst::Caps>()
                    .map_err(|error| format!("Invalid pass-through ADTS caps: {error}"))?,
            );
            let eos_flag = self.audio_filesink_eos.clone();
            let audio_sink_pad = audio_filesink
                .static_pad("sink")
                .ok_or_else(|| "Pass-through audio filesink has no sink pad.".to_owned())?;
            audio_sink_pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
                if let Some(event) = info.event() {
                    if event.type_() == gst::EventType::Eos {
                        eos_flag.store(true, Ordering::SeqCst);
                    }
                }
                gst::PadProbeReturn::Ok
            });
            elements.push(audio_aac_parse.clone());
            elements.push(adts_caps.clone());
            elements.push(audio_filesink.clone());
            self.audio_es_path = Some(audio_es_path);
            self.audio_filesink = Some(audio_filesink);
            self.audio_aac_parse = Some(audio_aac_parse);
            self.audio_adts_caps = Some(adts_caps);
        }
        for element in &elements {
            pipeline.add(element).map_err(|error| {
                format!("Failed to add recording audio branch element: {error}")
            })?;
        }
        for pair in elements.windows(2) {
            pair[0]
                .link(&pair[1])
                .map_err(|error| format!("Failed to link recording audio branch: {error:?}"))?;
        }
        // TRANSCODE mode only: link the AAC encoder into the EXISTING qtmux
        // (already PLAYING). qtmux names its sink-pad templates by media type
        // (video_%u, audio_%u) — request the audio one for this stream.
        // qtmux negotiates the pad on the first caps; this is a plain pad
        // link, no state change.
        if self.mode == RecordingMode::Transcode {
            let muxer = self
                .muxer
                .clone()
                .ok_or_else(|| "Recording muxer is missing for the transcode audio branch.".to_owned())?;
            let aac_src = aac_encoder
                .static_pad("src")
                .ok_or_else(|| "Recording audio encoder has no src pad.".to_owned())?;
            let muxer_sink = muxer
                .request_pad_simple("audio_%u")
                .ok_or_else(|| "Recording muxer refused an audio sink pad.".to_owned())?;
            if let Err(error) = aac_src.link(&muxer_sink) {
                let _ = muxer.release_request_pad(&muxer_sink);
                return Err(format!(
                    "Failed to link recording audio branch into muxer: {error:?}"
                ));
            }
        }
        // Sync BEFORE linking the branch into the audio tap tee (the tee is
        // already PLAYING). Non-sinks: synchronous, re-preroll-free.
        for element in &elements {
            element.sync_state_with_parent().map_err(|error| {
                format!("Failed to sync recording audio branch element state: {error}")
            })?;
        }
        let valve_sink = valve
            .static_pad("sink")
            .ok_or_else(|| "Recording audio valve has no sink pad.".to_owned())?;
        let branch_pad = audio_rtp_tee.request_pad_simple("src_%u").ok_or_else(|| {
            "Failed to request an audio recording pad from the RTP audio tap tee.".to_owned()
        })?;
        if let Err(error) = branch_pad.link(&valve_sink) {
            let _ = audio_rtp_tee.release_request_pad(&branch_pad);
            return Err(format!(
                "Failed to link RTP audio tap into the recording branch: {error:?}"
            ));
        }
        // Audio often arrives before video, so this branch is commonly linked
        // after the RTP tee has already received stream-start/caps/segment.
        // Replay from the tee SINK pad (the authoritative retained sticky-event
        // source), not only from the newly-created tee request pad.
        if let Some(tee_sink) = audio_rtp_tee.static_pad("sink") {
            replay_recording_sticky_events_from_pad(&tee_sink, &valve);
        }

        self.audio_valve = Some(valve.clone());
        self.audio_queue = Some(queue.clone());
        self.audio_capsfilter = Some(capsfilter.clone());
        self.audio_depayloader = Some(depayloader.clone());
        self.audio_opusdec = Some(opusdec.clone());
        self.audio_audioconvert = Some(audioconvert.clone());
        self.audio_aac_encoder = Some(aac_encoder.clone());
        self.audio_branch_built.store(true, Ordering::SeqCst);
        match self.mode {
            RecordingMode::Transcode => send_log(
                &self.event_sender,
                "info",
                format!("Attached native game-audio transcode branch (rtp → rtpopusdepay → opusdec → audioconvert → {aac_factory} → qtmux; never touches the audio playback chain)."),
            ),
            RecordingMode::PassThrough | RecordingMode::Encoded => send_log(
                &self.event_sender,
                "info",
                format!("Attached native game-audio PASS-THROUGH branch (rtp → rtpopusdepay → opusdec → audioconvert → {aac_factory} → filesink AAC ES; remuxed offline at stop — never touches the audio playback chain)."),
            ),
        }
        Ok(())
    }

    pub(crate) fn start(&self) -> Result<(), String> {
        self.eos_seen.store(false, Ordering::SeqCst);
        // Each recording measures its own PTS cadence window: reset the
        // counters so an aborted + restarted recording (branch kept) never
        // blends two windows.
        self.rec_frames_first_pts.store(0, Ordering::SeqCst);
        self.rec_frames_last_pts.store(0, Ordering::SeqCst);
        self.rec_frames_count.store(0, Ordering::SeqCst);
        // Reset the queue drop counters so each recording measures its own
        // window (an aborted + restarted recording on the KEPT branch must
        // never blend two windows).
        self.queue_input.store(0, Ordering::SeqCst);
        self.queue_output.store(0, Ordering::SeqCst);
        self.dropped_frames.store(0, Ordering::SeqCst);
        // Set active before opening the valves so the chunk probe never drops
        // the first muxer output (ftyp + moov).
        self.active.store(true, Ordering::SeqCst);
        // NOTE: no GstForceKeyUnit / CustomUpstream send on the WebRTC video
        // src pad here. That custom upstream event travels back INTO the
        // webrtcbin transport and the bundled GStreamer runtime errors out
        // the UDP receiver (`nicesrc: Internal data stream error, reason
        // not-negotiated`) — the exact field symptom: pressing record kills
        // the whole stream (22:35 log: recording start → 11ms later the bus
        // error → encoded/decoded/sink all stuck at 0 while the recording
        // branch drains nothing). Keyframe recovery is handled by the
        // liveness watchdog via RTCP/signaling (data channel), which never
        // touches the media pipeline, and the remux depayloader carries
        // `request-keyframe=true` so the branch asks locally on its own side
        // of the tap tee.
        // Replay the mandatory sticky events into qtmux while the valve is
        // still closed. Opening first lets the live video deliver a buffer
        // concurrently, which produces the exact field warnings
        // `queue:sink Got data flow before stream-start/segment` and leaves
        // qtmux with an empty track. Once stream-start/caps/segment are
        // queued downstream, open the valves.
        //
        // The video valve gates the whole transcode chain, so the events
        // qtmux needs come from the video TAP TEE's sink pad (the
        // authoritative retained stream-start/caps/segment of the live
        // decode chain). The audio branch still has its valve before
        // rtpopusdepay, so its events are replayed from the audio tap tee as
        // before.
        // The VIDEO branch replays stream-start + segment WITHOUT the tee's
        // caps: the tee's raw decoded caps would override the branch's
        // declared full-range BT.709 caps and disable the FULL→LIMITED range
        // conversion (the field "too contrasty colors" bug) — that fear is
        // obsolete since the range rescale moved into the LUT pad probe (which
        // acts on the BUFFER regardless of caps) and the container's colour
        // metadata is stripped (colr→free) before the file is handed out.
        // Replaying the CAPS event is REQUIRED: every element below the
        // valve was built while the pipeline was already PLAYING, so the
        // freshly built branch has no sticky events yet, and a first buffer
        // that arrives at the branch videoconvert without a caps event
        // stalls the whole branch (queue fills to its leaky cap and never
        // drains — the field "record #2 froze the whole stream" bug: stop()
        // then had to wait out the drain timeout and the EOS below the valve
        // was not accepted). The audio branch replays its RTP caps —
        // rtpopusdepay needs the payload type.
        if let Some(tee_sink) = self.video_tap_tee.static_pad("sink") {
            replay_recording_sticky_events_from_pad(&tee_sink, &self.valve);
        } else {
            replay_recording_sticky_events(&self.valve);
        }
        if let Some(audio_valve) = &self.audio_valve {
            if let Some(audio_tee) = &self.audio_rtp_tee {
                if let Some(audio_tee_sink) = audio_tee.static_pad("sink") {
                    replay_recording_sticky_events_from_pad(&audio_tee_sink, audio_valve);
                } else {
                    replay_recording_sticky_events(audio_valve);
                }
            } else {
                replay_recording_sticky_events(audio_valve);
            }
        }
        // Deterministic start gate for the REBUILT branches (video AND audio).
        // The replay above pushes from each valve's SRC pad; the events still
        // have to travel through the (PLAYING, async) queue task, and on a
        // rebuild the tap tee additionally re-emits its retained sticky events
        // onto the fresh branch pad asynchronously on the tee's streaming
        // thread. If a valve opens before stream-start/caps/segment land on
        // its queue, the first live buffer hits it with no stream-start — the
        // field warning `Got data flow before stream-start` on the rebuilt
        // queue — and the branch can stall so badly that stop() times out.
        // Wait (bounded) for each queue sink pad to actually show STREAM_START
        // + CAPS before opening its valve. CAPS is sticky and arrives AFTER
        // stream-start in the replay order, so its presence proves the whole
        // sequence landed. Bounded — a genuinely broken branch degrades to a
        // warning, never a hang.
        let video_ready = self.gate_branch_ready(&self.queue, "video");
        let audio_ready = self
            .audio_queue
            .as_ref()
            .map(|audio_queue| self.gate_branch_ready(audio_queue, "audio"))
            .unwrap_or(true);
        if !(video_ready && audio_ready) {
            // Fail-safe: NEVER open a valve into a queue that lacks the full
            // stream-start/caps/segment sequence. The closed valve forwards
            // sticky events only, so a missing segment means the replayed one
            // never landed — opening now would push the first live buffer into
            // the queue with `Got data flow before segment`, GStreamer returns
            // FLOW_ERROR, and the whole RTP flow stalls through the shared
            // tap tee (the field wedge: 2 s of video in a 24 s recording).
            // Failing the record start keeps the live stream untouched.
            self.active.store(false, Ordering::SeqCst);
            return Err(
                "Recording start aborted: the recording branch did not receive the mandatory \
                 stream-start/caps/segment sequence in time; the live stream was left untouched."
                    .to_owned(),
            );
        }
        match self.mode {
            RecordingMode::PassThrough => {
                // Re-arm the IDR gate so THIS recording starts at a fresh
                // keyframe (the probe drops mid-GOP frames until the first
                // sync access unit, so the file never opens with orphan
                // P-frames).
                self.idr_gate.store(true, Ordering::SeqCst);
                // Ask the server for an IDR right now (via the signaling/data
                // channel — NEVER an in-pipeline GstForceKeyUnit, which kills
                // the UDP receiver; see the NOTE above). The IDR lands a
                // frame later and the gate opens on it: clean file start.
                if let Some(sender) = &self.event_sender {
                    let _ = sender.send(Event::VideoKeyframeRequest(
                        crate::protocol::VideoKeyframeRequest {
                            reason: "record-start".to_owned(),
                            attempt: 0,
                        },
                    ));
                }
            }
            // ENCODED taps the DECODED frames, which are always complete —
            // no keyframe gate needed (the encoder starts wherever the live
            // decoder is; a fresh recording simply begins there).
            RecordingMode::Encoded | RecordingMode::Transcode => {}
        }
        self.valve.set_property("drop", false);
        if let Some(audio_valve) = &self.audio_valve {
            audio_valve.set_property("drop", false);
        }
        if let Some(diag) = &self.pass_flow_diag {
            diag.log("start");
        }
        let with_audio = self.audio_branch_built.load(Ordering::SeqCst);
        let started_log = match self.mode {
            RecordingMode::PassThrough => {
                let codec = &self.codec;
                if with_audio {
                    format!(
                        "Native recording started (video + game audio remuxed from the received {codec} bitstream; zero re-encode — no GPU/CPU cost to the live stream)."
                    )
                } else {
                    format!(
                        "Native recording started (video remuxed from the received {codec} bitstream; zero re-encode — no GPU/CPU cost to the live stream)."
                    )
                }
            }
            RecordingMode::Encoded => {
                if with_audio {
                    "Native recording started (video + game audio encoded from the decoded stream to a universal H.264/AAC MP4; immune to WAN RTP jitter/loss — captures exactly what you see)."
                        .to_owned()
                } else {
                    "Native recording started (video encoded from the decoded stream to a universal H.264 MP4; immune to WAN RTP jitter/loss — captures exactly what you see)."
                        .to_owned()
                }
            }
            RecordingMode::Transcode => {
                if with_audio {
                    "Native recording started (video + game audio transcoded to a universal H.264/AAC MP4)."
                        .to_owned()
                } else {
                    "Native recording started (video transcoded to a universal H.264 MP4).".to_owned()
                }
            }
        };
        send_log(&self.event_sender, "info", started_log);
        Ok(())
    }

    /// Bounded wait for one recording branch queue to show the FULL replayed
    /// media sequence STREAM_START + CAPS + SEGMENT before its valve opens.
    /// Polls the queue SINK pad's sticky-event list every 5 ms up to
    /// `RECORDING_STICKY_VERIFY_TIMEOUT_MS`. Returns true when the sequence
    /// landed. A timed-out gate returns false and the caller MUST NOT open the
    /// valve: a closed recording valve only forwards sticky events
    /// (stream-start/caps) — the SEGMENT never reaches the queue from the live
    /// stream, so the replay is its only source, and opening into a queue with
    /// no segment makes the first live buffer hit `Got data flow before
    /// segment` → FLOW_ERROR → the whole RTP flow stalls through the shared
    /// tap tee (the pass-through wedge: 2 s of video in a 24 s recording while
    /// audio kept flowing). Failing the record start is strictly safer than
    /// stalling the game stream. `what` names the branch ("video"/"audio").
    fn gate_branch_ready(&self, queue: &gst::Element, what: &str) -> bool {
        let Some(queue_sink) = queue.static_pad("sink") else {
            return false;
        };
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(RECORDING_STICKY_VERIFY_TIMEOUT_MS);
        while !pad_has_media_sticky_events(&queue_sink) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if !pad_has_media_sticky_events(&queue_sink) {
            send_log(
                &self.event_sender,
                "warn",
                format!(
                    "Recording {what} branch queue did not show the replayed \
                     STREAM_START/CAPS/SEGMENT within the sticky-verify window; aborting \
                     record start (opening the valve here would stall the stream with \
                     `Got data flow before segment`)."
                ),
            );
            false
        } else {
            true
        }
    }

    /// Drain a branch queue and inject EOS below its valve, so qtmux sees the
    /// EOS serialized after the buffered tail (an EOS that overtakes a full
    /// queue can be dropped inside the muxer: buffers-after-EOS).
    fn drain_and_eos(&self, valve: &gst::Element, queue: &gst::Element) -> Result<(), String> {
        let drain_start = std::time::Instant::now();
        let drain_deadline =
            drain_start + std::time::Duration::from_millis(RECORDING_DRAIN_TIMEOUT_MS);
        let mut queue_level = 0u32;
        loop {
            queue_level = read_queue_level(queue);
            if queue_level == 0 || std::time::Instant::now() >= drain_deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if queue_level != 0 {
            send_log(
                &self.event_sender,
                "warn",
                format!(
                    "Recording branch queue did not drain before EOS (still {queue_level} buffers); injecting EOS anyway."
                ),
            );
        }

        let valve_src = valve
            .static_pad("src")
            .ok_or_else(|| "Recording valve has no src pad.".to_owned())?;
        let below = valve_src
            .peer()
            .ok_or_else(|| "Recording valve is not linked to the remux chain.".to_owned())?;
        let accepted = below.send_event(gst::event::Eos::new());
        send_log(
            &self.event_sender,
            "info",
            format!(
                "Native recording stop: sent EOS below {label} (accepted={accepted}) after draining {queue_level} queued buffer(s).",
                label = valve.name()
            ),
        );
        Ok(())
    }

    pub(crate) fn stop(&self, finalize: bool) -> Result<(), String> {
        // Stop new data entering the branches first; data already inside
        // (queue → depayloader → parse → …) keeps flowing.
        self.valve.set_property("drop", true);
        if let Some(audio_valve) = &self.audio_valve {
            audio_valve.set_property("drop", true);
        }
        if !finalize {
            self.active.store(false, Ordering::SeqCst);
            if self.mode == RecordingMode::PassThrough {
                // A pass-through filesink stays OPEN after an abort and would
                // APPEND to the same ES file on a later start(), so flush +
                // close it now with EOS and mark the branch spent — the next
                // recording rebuilds it fresh with a new temp file.
                self.close_es_files();
                self.spent.store(true, Ordering::SeqCst);
                send_log(
                    &self.event_sender,
                    "info",
                    "Native recording aborted; capture valves closed (pass-through ES files flushed; branch rebuilt fresh for the next recording)."
                        .to_owned(),
                );
            } else {
                send_log(
                    &self.event_sender,
                    "info",
                    "Native recording aborted; capture valves closed (branch kept for the next recording)."
                        .to_owned(),
                );
            }
            return Ok(());
        }

        // IMPORTANT: the valves are closed (drop=true) BEFORE this point, and
        // in the bundled GStreamer the valve drops EOS events while closed —
        // sending EOS into a valve's sink pad never reaches the muxer /
        // filesink, AND as an upstream event it also propagates back through
        // the shared RTP tap tee into the live decode chain. Enter EOS BELOW
        // each valve instead (the next element's sink pad): data already
        // buffered in the queue drains first, then EOS, so the tail finalizes
        // normally and the live path is untouched.
        let drain_ms = std::time::Instant::now();
        // Video branch: the valve sits at the tap (before the queue), so the
        // queue must drain before EOS — an EOS that overtakes a full queue can
        // be dropped inside the muxer/filesink (buffers-after-EOS). Drain then
        // inject EOS below the valve, exactly like the audio branch.
        self.drain_and_eos(&self.valve, &self.queue)?;
        if let (Some(audio_valve), Some(audio_queue)) = (&self.audio_valve, &self.audio_queue) {
            self.drain_and_eos(audio_valve, audio_queue)?;
        }
        let drain_ms = drain_ms.elapsed().as_millis();

        // The video queue is drained (empty) after the wait above, so every
        // input buffer either left the queue into the encoder/filesink or was
        // dropped by the leaky queue while it was full — `dropped_frames` is
        // exact at this point. Surfaced to the user at stop so a choppy
        // recording (encoder could not keep up on a weak device) is
        // explained instead of silently degraded.
        //
        // Pass-through additionally loses frames when the depayloader emits
        // an AU with holes (a NACK-unrecoverable packet gap) and the parser
        // sheds it (field: parse_in=2197 vs parse_out=1813 on a lossy link,
        // while the queue shed nothing). Report the larger of the two — both
        // mechanisms mean "video lost from the clip", and with
        // wait-for-keyframe the parse gap is the one that stays observable.
        let mut dropped = self
            .queue_input
            .load(Ordering::Relaxed)
            .saturating_sub(self.queue_output.load(Ordering::Relaxed));
        if self.mode == RecordingMode::PassThrough {
            if let Some(diag) = &self.pass_flow_diag {
                let parse_dropped = diag
                    .parse_in
                    .load(Ordering::Relaxed)
                    .saturating_sub(diag.parse_out.load(Ordering::Relaxed));
                dropped = dropped.max(parse_dropped);
            }
        }
        self.dropped_frames.store(dropped, Ordering::SeqCst);

        match self.mode {
            RecordingMode::PassThrough | RecordingMode::Encoded => self.stop_pass_through(drain_ms),
            RecordingMode::Transcode => self.stop_transcode(drain_ms),
        }
    }

    /// Measured decoded-frame cadence of the recording window (see
    /// `measured_recording_frame_duration_ns`): the average PTS interval of
    /// the frames that flowed through the valve. Falls back to the negotiated
    /// fps cadence when the measurement is unavailable or implausible.
    fn measured_frame_duration_ns(&self) -> u64 {
        measured_recording_frame_duration_ns(
            self.rec_frames_count.load(Ordering::SeqCst),
            self.rec_frames_first_pts.load(Ordering::SeqCst),
            self.rec_frames_last_pts.load(Ordering::SeqCst),
            self.fps,
        )
    }

    /// PASS-THROUGH finalize: wait for EOS to reach both ES filesinks (the
    /// temp files are fully flushed/closed only then), remux them into a
    /// standard seekable MP4 OFFLINE in a throwaway pipeline — the live
    /// pipeline contains NO muxer for pass-through recordings, so a muxer
    /// stall can never freeze the recording branch (the field wedge: both
    /// branches froze at the live qtmux input ~16 s in while the stream
    /// stayed healthy) — and hand the finished MP4 to Electron as a single
    /// `recording-chunk` (the same handoff as the transcode branch's
    /// faststart single chunk). Then the temp files are deleted and the
    /// branch is marked spent for a fresh rebuild.
    fn stop_pass_through(&self, drain_ms: u128) -> Result<(), String> {
        let has_audio = self.audio_branch_built.load(Ordering::SeqCst);
        let video_es = self.video_es_path.clone();
        let audio_es = if has_audio {
            self.audio_es_path.clone()
        } else {
            None
        };
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(RECORDING_FINALIZE_TIMEOUT_MS);
        while !self.filesink_eos.load(Ordering::SeqCst)
            && (!has_audio || !self.audio_filesink_eos.load(Ordering::SeqCst))
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if !(self.filesink_eos.load(Ordering::SeqCst)
            && (!has_audio || self.audio_filesink_eos.load(Ordering::SeqCst)))
        {
            // The first EOS may have been lost racing the last in-flight
            // data; a second EOS after the drain is harmless and often
            // completes the flush. If the flag still never lands, the ES file
            // is merely truncated at the tail — the offline remux still
            // produces a playable file from what was written.
            for valve in [Some(&self.valve), self.audio_valve.as_ref()]
                .into_iter()
                .flatten()
            {
                if let Some(src_pad) = valve.static_pad("src") {
                    if let Some(below) = src_pad.peer() {
                        let _ = below.send_event(gst::event::Eos::new());
                    }
                }
            }
            let retry_deadline =
                std::time::Instant::now() + std::time::Duration::from_millis(1_000);
            while !(self.filesink_eos.load(Ordering::SeqCst)
                && (!has_audio || self.audio_filesink_eos.load(Ordering::SeqCst)))
                && std::time::Instant::now() < retry_deadline
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        if let Some(diag) = &self.pass_flow_diag {
            diag.log("stop");
        }
        send_log(
            &self.event_sender,
            "info",
            format!(
                "Native recording stop: ES files EOS seen (video={}, audio={}; queue drained in {drain_ms} ms); remuxing the MP4 offline.",
                self.filesink_eos.load(Ordering::SeqCst),
                self.audio_filesink_eos.load(Ordering::SeqCst),
            ),
        );

        // Close the ES files NOW: EOS alone does not close the fd until a
        // state change (an open handle makes remove_file fail silently on
        // Windows — the field left temp ES files behind). The valves are
        // closed and EOS'd, so no data can reach the sinks anymore.
        if let Some(filesink) = &self.video_filesink {
            let _ = filesink.set_state(gst::State::Null);
        }
        if let Some(filesink) = &self.audio_filesink {
            let _ = filesink.set_state(gst::State::Null);
        }

        let remux_out = std::env::temp_dir().join(format!(
            "opennow-rec-{}-{}.mp4",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0),
        ));
        // The remux can fail (e.g. a 0-byte ES file: the video branch never
        // received data — the exact field 22:58 bug). Whatever the cause, the
        // recording is OVER: reset `active` so the next start_recording is a
        // real start (not a silent no-op that leaves the valves closed — the
        // field's zombie second recording), and mark the branch spent so the
        // next recording rebuilds fresh with new temp files.
        let remux_result = match self.mode {
            // ENCODED: the video ES is already limited-range untagged H.264
            // from the branch's encoder (the LUT + encoder handled colour) —
            // a plain parse-and-mux suffices, no decode/LUT/re-encode. The
            // ladder is built at the MEASURED decoded-frame cadence of the
            // recording window (not the static negotiated fps), so a stream
            // that delivered fewer unique frames than negotiated still
            // records real-time instead of 2×-stretched video.
            RecordingMode::Encoded => remux_encoded_recording(
                video_es.as_deref(),
                audio_es.as_deref(),
                &remux_out,
                self.measured_frame_duration_ns(),
                &self.event_sender,
            ),
            _ => remux_pass_through_recording(
                video_es.as_deref(),
                audio_es.as_deref(),
                &remux_out,
                &self.codec,
                self.fps,
                &self.event_sender,
            ),
        };
        let finalize_size = match remux_result {
            Ok(size) => size,
            Err(error) => {
                let _ = std::fs::remove_file(&remux_out);
                for path in [video_es.as_ref(), audio_es.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    let _ = std::fs::remove_file(path);
                }
                self.active.store(false, Ordering::SeqCst);
                self.spent.store(true, Ordering::SeqCst);
                let message = format!("Pass-through remux failed: {error}");
                send_log(&self.event_sender, "error", message.clone());
                return Err(message);
            }
        };
        if finalize_size == 0 {
            let _ = std::fs::remove_file(&remux_out);
            for path in [video_es.as_ref(), audio_es.as_ref()]
                .into_iter()
                .flatten()
            {
                let _ = std::fs::remove_file(path);
            }
            self.active.store(false, Ordering::SeqCst);
            self.spent.store(true, Ordering::SeqCst);
            send_log(
                &self.event_sender,
                "error",
                "Recording captured no media (empty elementary streams); nothing saved."
                    .to_owned(),
            );
            return Err(
                "Recording captured no media (empty elementary streams); nothing saved."
                    .to_owned(),
            );
        }
        // Stream the finalized MP4 to Electron in fixed-size chunks — the
        // whole file is never read into memory (each helper reads one window
        // at a time), so a long recording cannot OOM the finalize on a weak
        // device. PassThrough recordings additionally get their `colr` box
        // (BT.709 full-range, matching the received bitstream) tagged into
        // the head while streaming — no re-encode, so the colour fix is pure
        // metadata.
        if let Some(sender) = &self.event_sender {
            let streamed = if self.mode == RecordingMode::PassThrough {
                stream_recording_with_colr_tag(sender, &remux_out, &self.event_sender)?
            } else {
                stream_recording_file_as_chunks(sender, &remux_out)?
            };
            send_log(
                &self.event_sender,
                "info",
                format!(
                    "Native recording finalized: {streamed} bytes remuxed offline into a standard seekable MP4, delivered in {} chunk(s).",
                    (streamed as usize).div_ceil(RECORDING_CHUNK_BYTES),
                ),
            );
        }
        // Cleanup: delete the temp ES files and the remux output; the branch
        // is spent (filesinks saw EOS) so the next recording rebuilds fresh.
        for path in [Some(&remux_out), video_es.as_ref(), audio_es.as_ref()]
            .into_iter()
            .flatten()
        {
            let _ = std::fs::remove_file(path);
        }
        self.active.store(false, Ordering::SeqCst);
        self.spent.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// TRANSCODE finalize: the existing live-muxer path — wait for the muxer
    /// EOS (the whole faststart MP4 is emitted as one chunk by the swallow
    /// probe), retry the EOS once, then fall back to sending EOS directly on
    /// the muxer's sink pads.
    fn stop_transcode(&self, drain_ms: u128) -> Result<(), String> {
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(RECORDING_FINALIZE_TIMEOUT_MS);
        while !self.eos_seen.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if !self.eos_seen.load(Ordering::SeqCst) {
            // The first EOS may have been lost racing the last in-flight
            // data; a second EOS after the drain is harmless and often
            // completes the flush.
            let retry_deadline =
                std::time::Instant::now() + std::time::Duration::from_millis(1_000);
            let mut retried = false;
            while !self.eos_seen.load(Ordering::SeqCst)
                && std::time::Instant::now() < retry_deadline
            {
                if !retried {
                    retried = true;
                    for valve in [Some(&self.valve), self.audio_valve.as_ref()]
                        .into_iter()
                        .flatten()
                    {
                        if let Some(src_pad) = valve.static_pad("src") {
                            if let Some(below) = src_pad.peer() {
                                let _ = below.send_event(gst::event::Eos::new());
                            }
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }

        // FAILSAFE: if the EOS below the valve did not finalize qtmux (e.g.
        // it never flowed because the session was still negotiating), send EOS
        // directly on the muxer's sink pads so it finalizes regardless.
        if !self.eos_seen.load(Ordering::SeqCst) {
            send_log(
                &self.event_sender,
                "warn",
                "Native recording stop: normal EOS path did not finalize the muxer; sending EOS directly on the muxer sink pads.".to_owned(),
            );
            if let Some(muxer) = &self.muxer {
                for pad in muxer.sink_pads() {
                    let _ = pad.send_event(gst::event::Eos::new());
                }
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
            return Err("Timed out waiting for the recording muxer to flush.".to_owned());
        }
        send_log(
            &self.event_sender,
            "info",
            format!("Native recording stop: muxer EOS seen (queue drained in {drain_ms} ms)."),
        );
        self.active.store(false, Ordering::SeqCst);
        self.spent.store(true, Ordering::SeqCst);
        send_log(
            &self.event_sender,
            "info",
            "Native recording finalized; MP4 chunks flushed.".to_owned(),
        );
        Ok(())
    }

    /// Abort path for pass-through: send EOS below each valve so the ES
    /// filesinks flush and CLOSE their temp files (a filesink left open after
    /// an abort would append stale pre-abort data on a later start()). The
    /// caller marks the branch spent so the next recording rebuilds it fresh.
    fn close_es_files(&self) {
        for valve in [Some(&self.valve), self.audio_valve.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some(src_pad) = valve.static_pad("src") {
                if let Some(below) = src_pad.peer() {
                    let _ = below.send_event(gst::event::Eos::new());
                }
            }
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while !self.filesink_eos.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Tear the ENTIRE recording branch (video + audio) out of the pipeline:
    /// release the tee request pads, set every element to NULL and remove it
    /// from the pipeline. Used before REBUILDING the branch fresh for the
    /// next recording — the in-place recycle() is unreliable in this
    /// GStreamer build (a direct NULL→PLAYING on a queue kills its src task,
    /// and the shared qtmux carries round-1 EOS/interleave state that blocks
    /// round-2), while a FRESH branch reproduces the exact state round 1
    /// always succeeds from. The tap tees themselves and the live decode/
    /// present chains are NOT touched.
    pub(crate) fn teardown(&self, pipeline: &gst::Pipeline) -> Result<(), String> {
        // Release the video tap tee's request pad (valve sink pad's peer).
        // gst_pad_unlink() requires the SRC pad first, so unlink from the
        // tee's request pad (src) into the valve's sink pad.
        if let Some(sink_pad) = self.valve.static_pad("sink") {
            if let Some(peer) = sink_pad.peer() {
                let _ = peer.unlink(&sink_pad);
                let _ = self.video_tap_tee.release_request_pad(&peer);
            }
        }
        // Release the audio tap tee's request pad.
        if let (Some(audio_valve), Some(audio_tee)) = (&self.audio_valve, &self.audio_rtp_tee) {
            if let Some(sink_pad) = audio_valve.static_pad("sink") {
                if let Some(peer) = sink_pad.peer() {
                    let _ = peer.unlink(&sink_pad);
                    let _ = audio_tee.release_request_pad(&peer);
                }
            }
        }

        let mut elements = vec![
            self.valve.clone(),
            self.queue.clone(),
            self.encoder.clone(),
            self.h264_parse.clone(),
            self.video_convert.clone(),
            self.video_declare_caps.clone(),
            self.range_convert.clone(),
            self.video_encode_caps.clone(),
        ];
        if let Some(download) = &self.video_download {
            elements.push(download.clone());
        }
        if let Some(muxer) = &self.muxer {
            elements.push(muxer.clone());
        }
        if let Some(swallow) = &self.swallow {
            elements.push(swallow.clone());
        }
        if let Some(es_caps) = &self.es_caps {
            elements.push(es_caps.clone());
        }
        if let Some(filesink) = &self.video_filesink {
            elements.push(filesink.clone());
        }
        if let Some(filesink) = &self.audio_filesink {
            elements.push(filesink.clone());
        }
        for element in [
            &self.audio_valve,
            &self.audio_queue,
            &self.audio_capsfilter,
            &self.audio_depayloader,
            &self.audio_opusdec,
            &self.audio_audioconvert,
            &self.audio_aac_encoder,
            &self.audio_aac_parse,
            &self.audio_adts_caps,
        ]
        .into_iter()
        .flatten()
        {
            elements.push(element.clone());
        }
        // The pass-through branch aliases its RTP capsfilter across several
        // fields (video_convert/video_declare_caps/video_encode_caps/
        // range_convert all point at the same element), so de-duplicate by
        // element name before NULLing/removing — pipeline.remove() on the
        // same element twice errors out.
        let mut seen = std::collections::HashSet::new();
        elements.retain(|element| seen.insert(element.name().to_string()));
        for element in &elements {
            element.set_state(gst::State::Null).map_err(|error| {
                format!(
                    "Failed to stop recording element {} for teardown: {error:?}",
                    element.name()
                )
            })?;
        }
        for element in &elements {
            pipeline.remove(element).map_err(|error| {
                format!(
                    "Failed to remove recording element {} from the pipeline: {error:?}",
                    element.name()
                )
            })?;
        }
        // Remove leftover pass-through temp ES files (already remuxed+deleted
        // on finalize; this covers aborts and rebuilds).
        for path in [self.video_es_path.as_ref(), self.audio_es_path.as_ref()]
            .into_iter()
            .flatten()
        {
            let _ = std::fs::remove_file(path);
        }
        send_log(
            &self.event_sender,
            "info",
            "Native recording branch torn down; next recording rebuilds it fresh.".to_owned(),
        );
        Ok(())
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
    /// Recording encoder bitrate cap (kbps). The recording branch re-encodes
    /// the decoded stream, so its bitrate follows the session's negotiated
    /// maximum (the user's "max bitrate" slider) instead of a fixed default:
    /// the backend sets it at session start / bitrate change, and the
    /// recording branch reads it whenever it is (re)built.
    record_bitrate_kbps: Arc<AtomicU32>,
    d3d_fullscreen_sink: Arc<AtomicBool>,
    /// When true, WebRTC RTP video pads are ignored (classic NVST UDP owns video).
    skip_webrtc_video: Arc<AtomicBool>,
    nvst_receive: Option<NvstVideoReceiveHandle>,
    video_liveness: VideoLivenessMonitor,
    screenshot_grab: Arc<Mutex<Option<GstreamerScreenshotGrab>>>,
    video_tap: Arc<Mutex<Option<GstreamerVideoTap>>>,
    recording: Arc<Mutex<Option<GstreamerRecordingState>>>,
    /// Game-audio RTP tap tee, owned at the PIPELINE level (not inside
    /// `recording`): the audio RTP pad arrives BEFORE the video pad, so the
    /// recording state (built from the video pad) does not exist yet when the
    /// audio handler runs. The tee is created and linked into the audio path
    /// immediately; when the recording branch is later built (video pad), it
    /// is transferred into `GstreamerRecordingState` and the audio remux
    /// branch hangs off it.
    audio_rtp_tee: Arc<Mutex<Option<gst::Element>>>,
    /// Recording tap tee on the mic chain after the mute volume, so muting the
    /// mic also silences it in recordings.
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
        wire_ice_observability(&webrtc, event_sender.clone());
        wire_remote_data_channels(&webrtc, event_sender.clone());
        start_gstreamer_bus_diagnostics(
            &pipeline,
            event_sender.clone(),
            video_liveness.stop_flag(),
            video_liveness.clone(),
        );
        let present_max_fps = Arc::new(AtomicU32::new(0));
        let record_bitrate_kbps = Arc::new(AtomicU32::new(RECORD_BITRATE_DEFAULT_KBPS));
        let d3d_fullscreen_sink = Arc::new(AtomicBool::new(false));
        let skip_webrtc_video = Arc::new(AtomicBool::new(false));
        let screenshot_grab = Arc::new(Mutex::new(None));
        let video_tap = Arc::new(Mutex::new(None));
        // The remux recording branch is built at session start (in
        // link_rtp_video_pad) off the RTP record tap tee; the same Arc is
        // stored in the struct below so start_recording/stop_recording reach
        // it.
        let recording = Arc::new(Mutex::new(None));
        let audio_rtp_tee = Arc::new(Mutex::new(None));
        wire_incoming_media_sink(
            &pipeline,
            &webrtc,
            event_sender.clone(),
            render_state.clone(),
            present_max_fps.clone(),
            record_bitrate_kbps.clone(),
            d3d_fullscreen_sink.clone(),
            skip_webrtc_video.clone(),
            video_liveness.clone(),
            screenshot_grab.clone(),
            video_tap.clone(),
            recording.clone(),
            audio_rtp_tee.clone(),
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
            record_bitrate_kbps,
            d3d_fullscreen_sink,
            skip_webrtc_video,
            nvst_receive: None,
            video_liveness,
            screenshot_grab,
            video_tap,
            recording,
            audio_rtp_tee,
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

    /// Set the bitrate cap for the recording encoder (kbps). Called by the
    /// backend at session start and on every mid-session bitrate change, so
    /// the next recording (its branch is rebuilt fresh on each start) uses
    /// the same cap the user negotiated for the stream.
    pub(crate) fn set_record_bitrate_kbps(&self, kbps: u32) {
        self.record_bitrate_kbps.store(kbps, Ordering::SeqCst);
    }

    pub(crate) fn configure_stats(
        &self,
        context: &NativeStreamerSessionContext,
        target_bitrate_kbps: u32,
    ) {
        self.video_liveness.configure(context, target_bitrate_kbps);
    }

    /// Record the video codec actually negotiated in the WebRTC answer so the
    /// liveness startup watchdog can make a codec-downgrade decision against
    /// the codec the server really sends (not the requested one).
    pub(crate) fn set_negotiated_video_codec(&self, codec: &str) {
        self.video_liveness.update_negotiated_codec(codec);
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
                        branch_attached: false,
                        rtp_tee: None,
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

        // GFN offers transport-cc feedback only for its flexfec payload (pt
        // 98), so webrtcbin's rtpbin never attaches TWCC to the video source
        // and rtpsession generates no transport-wide feedback — the server
        // BWE runs blind and holds a conservative bitrate no matter what the
        // NVST SDP requested. Rewrite the answer's transport-cc rtcp-fb to the
        // wildcard form (valid per RFC 4585: the mechanism was already
        // offered, the wildcard just applies it to every payload) and hand
        // THAT answer to set-local-description, so GStreamer's own rtpbin
        // wires TWCC onto the video source. The server sees the same
        // rewritten text in the returned answer.
        let rewritten_answer_sdp = crate::sdp::rewrite_transport_cc_rtcp_fb(&answer_sdp);
        if rewritten_answer_sdp != answer_sdp {
            send_log(
                &self.event_sender,
                "debug",
                "Rewrote answer transport-cc rtcp-fb to the wildcard form so rtpbin attaches TWCC feedback to the video source (server BWE gets feedback instead of running blind)."
                    .to_owned(),
            );
        }
        let rewritten_sdp = gst_sdp::SDPMessage::parse_buffer(rewritten_answer_sdp.as_bytes())
            .map_err(|error| format!("Failed to re-parse the rewritten answer SDP: {error:?}"))?;
        let answer = gst_webrtc::WebRTCSessionDescription::new(
            gst_webrtc::WebRTCSDPType::Answer,
            rewritten_sdp,
        );
        let answer_sdp = rewritten_answer_sdp;

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
        log_remote_ice_candidate(&candidate.candidate, &self.event_sender);
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
        let grab = insert_screenshot_grab_branch(
            &self.pipeline,
            &tee,
            video_api,
            zero_copy,
            &self.event_sender,
        )?;
        if let Ok(mut tap_slot) = self.video_tap.lock() {
            if let Some(tap) = tap_slot.as_mut() {
                tap.mark_branch_attached();
            }
        }
        Ok(grab)
    }

    /// Start a native recording: open (or build, if missing/spent) the
    /// H.264/MP4 recording branch on the shared video tap.
    pub(crate) fn start_recording(&self) -> Result<(), String> {
        // The remux recording branch is built at session start (in
        // link_rtp_video_pad) off the RTP record tap tee — record start here
        // is normally ONLY a valve open. No element is added, synced, or
        // linked, so the decode/present chain is never touched and cannot
        // re-preroll.
        let spent = {
            let slot = self
                .recording
                .lock()
                .map_err(|_| "Recording state lock poisoned.".to_owned())?;
            let state = slot.as_ref().ok_or_else(|| {
                "Recording is not ready: the native RTP record tap is not built (no WebRTC video session)."
                    .to_owned()
            })?;
            if state.active.load(Ordering::SeqCst) {
                return Ok(());
            }
            state.spent.load(Ordering::SeqCst)
        };
        if spent {
            // The qtmux is spent after an EOS-finalized recording. The old
            // in-place recycle() is unreliable in this GStreamer build (a
            // direct NULL→PLAYING on a queue kills its src task, and the
            // shared qtmux keeps round-1 EOS/interleave state — the field
            // "record again froze the whole stream" bug), so the branch is
            // torn down and REBUILT FRESH: the exact state round 1 always
            // succeeds from. The live decode/present chains and the tap tees
            // themselves are untouched, and every rebuild step is the same
            // hot-plug-safe non-sink pattern the initial build uses.
            self.rebuild_recording_branch()?;
        }
        let slot = self
            .recording
            .lock()
            .map_err(|_| "Recording state lock poisoned.".to_owned())?;
        let state = slot.as_ref().ok_or_else(|| {
            "Recording is not ready: the native RTP record tap is not built (no WebRTC video session)."
                .to_owned()
        })?;
        state.start()
    }

    /// Tear down the spent recording branch and build a fresh one on the same
    /// tap tees (video + game audio), swapping it into `self.recording`. All
    /// rebuild steps are the proven initial-build path
    /// (`build_transcode_record_branch` + `build_audio_branch`), so a second
    /// recording starts from exactly the state the first one did.
    fn rebuild_recording_branch(&self) -> Result<(), String> {
        // Take the pieces the fresh branch needs BEFORE the teardown removes
        // them: the old state's audio RTP tee (the pipeline-level tee is
        // transferred into the recording state on first build) and the
        // video tap parameters.
        let (old, old_audio_rtp_tee) = {
            let slot = self
                .recording
                .lock()
                .map_err(|_| "Recording state lock poisoned.".to_owned())?;
            let state = slot
                .as_ref()
                .ok_or_else(|| "Recording state vanished.".to_owned())?;
            (state.clone(), state.audio_rtp_tee.clone())
        };
        let (rtp_tee, tap_tee, video_api, zero_copy) = {
            let slot = self
                .video_tap
                .lock()
                .ok()
                .and_then(|slot| slot.as_ref().cloned())
                .ok_or_else(|| "Recording rebuild: video tap tee is gone.".to_owned())?;
            (slot.rtp_tee, slot.tee, slot.video_api, slot.zero_copy)
        };
        // Rebuild the SAME mode the old branch was built for, so a second
        // recording matches the first (pass-through stays zero-cost, transcode
        // stays universal H.264).
        let mode = old.mode;
        let codec = old.codec.clone();
        old.teardown(&self.pipeline)?;

        let mut fresh = match mode {
            RecordingMode::PassThrough => {
                let rtp_tee = rtp_tee.ok_or_else(|| {
                    "Recording rebuild: RTP video tap tee is gone.".to_owned()
                })?;
                build_pass_through_record_branch(
                    &self.pipeline,
                    &rtp_tee,
                    &codec,
                    old.fps,
                    self.event_sender.clone(),
                )
                .map_err(|error| format!("Recording rebuild: fresh video branch failed: {error}"))?
            }
            RecordingMode::Encoded => {
                let tap_tee = tap_tee
                    .ok_or_else(|| "Recording rebuild: decoded video tap tee is gone.".to_owned())?;
                let record_bitrate_kbps = self.record_bitrate_kbps.load(Ordering::SeqCst);
                build_encoded_record_branch(
                    &self.pipeline,
                    &tap_tee,
                    video_api,
                    zero_copy,
                    self.event_sender.clone(),
                    record_bitrate_kbps,
                    old.fps,
                )
                .map_err(|error| format!("Recording rebuild: fresh video branch failed: {error}"))?
            }
            RecordingMode::Transcode => {
                let tap_tee = tap_tee
                    .ok_or_else(|| "Recording rebuild: decoded video tap tee is gone.".to_owned())?;
                let record_bitrate_kbps = self.record_bitrate_kbps.load(Ordering::SeqCst);
                build_transcode_record_branch(
                    &self.pipeline,
                    &tap_tee,
                    video_api,
                    zero_copy,
                    self.event_sender.clone(),
                    record_bitrate_kbps,
                    false,
                )
                .map_err(|error| format!("Recording rebuild: fresh video branch failed: {error}"))?
            }
        };
        if let Some(audio_tee) = old_audio_rtp_tee {
            fresh.audio_rtp_tee = Some(audio_tee);
            fresh.build_audio_branch(&self.pipeline).map_err(|error| {
                format!("Recording rebuild: fresh audio branch failed: {error}")
            })?;
        }
        {
            let mut slot = self
                .recording
                .lock()
                .map_err(|_| "Recording state lock poisoned.".to_owned())?;
            *slot = Some(fresh);
        }
        send_log(
            &self.event_sender,
            "info",
            "Native recording branch rebuilt fresh for the next recording.".to_owned(),
        );
        Ok(())
    }

    /// Stop the active recording. `finalize=true` flushes the remux branch
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
        // The OFFLINE pass-through remux re-encodes at the session's
        // negotiated max bitrate (the same value the transcode branch uses).
        state.record_bitrate_kbps.store(
            self.record_bitrate_kbps.load(Ordering::SeqCst),
            Ordering::SeqCst,
        );
        state.stop(finalize)?;
        if finalize {
            // The remux recorder has no decoded frames, so no thumbnail is
            // captured (the gallery can generate one from the file later).
            // `dropped_frames` is the queue-drop count computed inside
            // `state.stop` after the drain — 0 when the encoder kept up.
            if let Some(event_sender) = &self.event_sender {
                let _ = event_sender.send(Event::RecordingFinished {
                    thumbnail_base64: None,
                    dropped_frames: state.dropped_frames.load(Ordering::SeqCst),
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

    pub(crate) fn stop(&mut self) -> Result<(), String> {
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
    // Register the custom lenient GFN AV1 depayloader. The stock
    // gst-plugins-rs `rtpav1depay` produces zero frames on some GFN AV1
    // streams (see gfn_av1_depay.rs), so AV1 chains use `gfnav1depay`.
    static CUSTOM_AV1_DEPAY_REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
    CUSTOM_AV1_DEPAY_REGISTERED
        .get_or_init(|| {
            crate::gfn_av1_depay::register_element()
                .map_err(|error| format!("Failed to register gfnav1depay: {error}"))
        })
        .clone()?;
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

pub(crate) fn glib_value_to_u32(value: &glib::Value) -> Option<u32> {
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
                gst::MessageView::Latency(_) => {
                    // Report the actual latency value, not just the element:
                    // the per-element latencies (RTP jitter buffers, WASAPI
                    // device buffer) are exactly what audio-video sync issues
                    // hang on, and the plain element name makes them
                    // unverifiable from the field log.
                    let latency_ns = message
                        .structure()
                        .and_then(|structure| {
                            structure.get::<gst::ClockTime>("latency").ok()
                        })
                        .map(|latency| latency.nseconds())
                        .unwrap_or(0);
                    send_log(
                        &event_sender,
                        "debug",
                        format!(
                            "GStreamer bus latency update from {}: {} ms.",
                            message_src_name(&message),
                            latency_ns / 1_000_000
                        ),
                    );
                }
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
    record_bitrate_kbps: Arc<AtomicU32>,
    d3d_fullscreen_sink: Arc<AtomicBool>,
    skip_webrtc_video: Arc<AtomicBool>,
    video_liveness: VideoLivenessMonitor,
    _screenshot_grab: Arc<Mutex<Option<GstreamerScreenshotGrab>>>,
    video_tap: Arc<Mutex<Option<GstreamerVideoTap>>>,
    recording: Arc<Mutex<Option<GstreamerRecordingState>>>,
    audio_rtp_tee: Arc<Mutex<Option<gst::Element>>>,
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

        // Incoming game-audio RTP: tap it for the recording branch BEFORE
        // decodebin, so the recording gets the game audio without touching
        // the live audio playback path. The tee lives at the PIPELINE level
        // because the audio pad arrives BEFORE the video pad — the recording
        // state (built from the video pad in link_rtp_video_pad) does not
        // exist yet. The tee is created and linked into the audio path right
        // away; when the recording branch is later built, the tee is
        // transferred into the recording state and the audio remux branch
        // hangs off it.
        if rtp_audio_encoding(src_pad).is_some() {
            // Keep the audio-tee lock out of the recording lock. The video-pad
            // path takes these locks in the opposite order while transferring
            // the tee; holding both here could deadlock if pad-added callbacks
            // arrive concurrently.
            let audio_tee = {
                let mut tee_slot = match audio_rtp_tee.lock() {
                    Ok(slot) => slot,
                    Err(_) => {
                        send_log(&event_sender, "warn", "Audio RTP tee slot poisoned.".to_owned());
                        return;
                    }
                };
                if tee_slot.is_none() {
                    match make_element("tee") {
                        Ok(audio_tee) => {
                            if let Err(error) = pipeline.add(&audio_tee) {
                                send_log(
                                    &event_sender,
                                    "warn",
                                    format!("Failed to add game-audio RTP tap tee: {error}"),
                                );
                            } else if let Err(error) = audio_tee.sync_state_with_parent() {
                                send_log(
                                    &event_sender,
                                    "warn",
                                    format!("Failed to sync game-audio RTP tap tee: {error}"),
                                );
                            } else {
                                *tee_slot = Some(audio_tee.clone());
                                send_log(
                                    &event_sender,
                                    "info",
                                    "Attached game-audio RTP tap tee (recording branch will remux game audio with the video).".to_owned(),
                                );
                            }
                        }
                        Err(error) => send_log(
                            &event_sender,
                            "warn",
                            format!("Failed to create game-audio RTP tap tee: {error}"),
                        ),
                    }
                }
                tee_slot.clone()
            };

            // If the recording muxer already exists (video pad came first),
            // transfer the tee into the recording state and build the audio
            // branch right away.
            if let Ok(mut slot) = recording.lock() {
                if let Some(state) = slot.as_mut() {
                    if let Some(audio_tee) = audio_tee {
                        state.audio_rtp_tee = Some(audio_tee);
                    }
                    if let Err(error) = state.build_audio_branch(&pipeline) {
                        send_log(&event_sender, "warn", error);
                    }
                }
            }
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
                record_bitrate_kbps.clone(),
                d3d_fullscreen_sink.load(Ordering::SeqCst),
                video_liveness.clone(),
                &video_tap,
                &recording,
                &audio_rtp_tee,
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
        // Route the audio RTP pad through the tap tee (if one was attached
        // above) so the recording branch shares the same stream. The tee is
        // already PLAYING, so this link is a plain pad link.
        let _linked = if rtp_audio_encoding(src_pad).is_some() {
            let tee = recording
                .lock()
                .ok()
                .and_then(|slot| slot.as_ref().and_then(|state| state.audio_rtp_tee.clone()))
                .or_else(|| {
                    audio_rtp_tee
                        .lock()
                        .ok()
                        .and_then(|slot| slot.clone())
                });
            if let Some(audio_tee) = tee {
                let tee_sink = audio_tee
                    .static_pad("sink")
                    .ok_or_else(|| "Game-audio RTP tap tee has no sink pad.".to_owned());
                match tee_sink {
                    Ok(tee_sink) => match src_pad.link(&tee_sink) {
                        Ok(_) => {
                            // Main audio path: tee → decodebin (the recording
                            // branch hangs off a requested src pad).
                            let decode_sink = decodebin.static_pad("sink");
                            match decode_sink {
                                Some(decode_sink) => {
                                    let tee_src = audio_tee.request_pad_simple("src_%u");
                                    match tee_src {
                                        Some(tee_src) => {
                                            if let Err(error) = tee_src.link(&decode_sink) {
                                                let _ = audio_tee.release_request_pad(&tee_src);
                                                send_log(
                                                    &event_sender,
                                                    "warn",
                                                    format!("Failed to link game-audio tap tee to decodebin: {error:?}"),
                                                );
                                            }
                                        }
                                        None => send_log(
                                            &event_sender,
                                            "warn",
                                            "Failed to request a decode pad from the game-audio tap tee.".to_owned(),
                                        ),
                                    }
                                }
                                None => send_log(
                                    &event_sender,
                                    "warn",
                                    "decodebin has no sink pad.".to_owned(),
                                ),
                            }
                            true
                        }
                        Err(error) => {
                            send_log(
                                &event_sender,
                                "warn",
                                format!("Failed to link WebRTC RTP audio pad to tap tee: {error:?}"),
                            );
                            false
                        }
                    },
                    Err(error) => {
                        send_log(&event_sender, "warn", error);
                        false
                    }
                }
            } else {
                match src_pad.link(&sink_pad) {
                    Ok(_) => true,
                    Err(error) => {
                        send_log(
                            &event_sender,
                            "warn",
                            format!("Failed to link WebRTC RTP pad to decodebin: {error:?}"),
                        );
                        false
                    }
                }
            }
        } else {
            match src_pad.link(&sink_pad) {
                Ok(_) => true,
                Err(error) => {
                    send_log(
                        &event_sender,
                        "warn",
                        format!("Failed to link WebRTC RTP pad to decodebin: {error:?}"),
                    );
                    false
                }
            }
        };
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

/// Whether a webrtcbin src pad carries the incoming game-audio RTP stream
/// (the server's Opus audio m-line — distinct from the mic send pad, which is
/// SINK direction and never reaches this check).
fn rtp_audio_encoding(pad: &gst::Pad) -> Option<String> {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    let structure = caps.structure(0)?;
    if structure.name() != "application/x-rtp" {
        return None;
    }

    let media = structure.get::<String>("media").ok()?;
    if media != "audio" {
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
        // The stock gst-plugins-rs `rtpav1depay` produces zero output on some
        // GFN AV1 streams; the custom lenient `gfnav1depay` auto-detects the
        // payload layout and never wedges (see gfn_av1_depay.rs).
        "AV1" => Some("gfnav1depay"),
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

    let is_d3d = matches!(video_api, RtpVideoApi::D3D11 | RtpVideoApi::D3D12);
    // Every D3D path (D3D11 H264 included) uses the download + system-memory
    // present chain. The field logs showed d3d12h264dec zero-copy stopping
    // decode after the first frame while RTP kept flowing, and D3D11 H264
    // zero-copy D3DMemory presenting gray/pink garbage on some GPU/driver
    // combos (the same class of bug the comment below flags for ten-bit
    // codecs). Downloading to system NV12 avoids every driver/sink present
    // deadlock and makes the colorimetry pin deterministic.
    let needs_safe_system_memory_present = is_d3d;
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
            DISPLAY_NV12_FULL_RANGE_CAPS,
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
    {
        // Same safe present path as the main D3D chain for EVERY codec:
        // 10-bit-capable DXVA textures present as gray/pink garbage through
        // zero-copy D3DMemory, and H264 zero-copy has the same failure mode on
        // some drivers. Download + convert to system NV12 always.
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
            DISPLAY_NV12_FULL_RANGE_CAPS,
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
    if configured_present_max_fps == PRESENT_LIMITER_STREAM_SENTINEL {
        // Default policy without Cloud G-Sync: pace presentation to the stream
        // fps. Zero added latency in steady state; jitter catch-up bursts are
        // thinned back to real-time so the picture never "blinks" through
        // backlogged frames.
        return requested_fps.filter(|fps| *fps > 0).unwrap_or(0);
    }

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
    record_bitrate_kbps: Arc<AtomicU32>,
    d3d_fullscreen_sink: bool,
    video_liveness: VideoLivenessMonitor,
    video_tap: &Arc<Mutex<Option<GstreamerVideoTap>>>,
    recording: &Arc<Mutex<Option<GstreamerRecordingState>>>,
    audio_rtp_tee: &Arc<Mutex<Option<gst::Element>>>,
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

    // Build the native recording branch off the video tap tees. It is built
    // HERE (session start, pipeline NULL/READY) and its tee pad stays linked
    // for the session, so record start/stop never touches the live chain. A
    // decoder-fallback rebuild reconnects the SAME tap tee, so the branch
    // survives it intact.
    //
    // Mode selection: PASS-THROUGH by default (raw RTP → MP4, zero re-encode
    // — recording costs nothing to the live stream; the d3d12h264enc transcode
    // competes with the decoder for the same GPU and stutters the stream on
    // single-iGPU laptops). Set OPENNOW_RECORDING_MODE=transcode to opt back
    // into the universal H.264 transcode.
    let rtp_tee = video_tap
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().and_then(|tap| tap.rtp_tee.clone()));
    let tap_tee = video_tap
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().and_then(|tap| tap.tee.clone()));
    let (video_api, zero_copy) = video_tap
        .lock()
        .ok()
        .map(|slot| {
            slot.as_ref()
                .map(|tap| (tap.video_api, tap.zero_copy))
                .unwrap_or((RtpVideoApi::Software, false))
        })
        .unwrap_or((RtpVideoApi::Software, false));
    let recording_mode = RecordingMode::from_env();
    {
        let mut slot = recording
            .lock()
            .map_err(|_| "Recording state lock poisoned.".to_owned())?;
        if slot.is_none() {
            let built = match recording_mode {
                RecordingMode::PassThrough => match rtp_tee.as_ref() {
                    Some(tee) => build_pass_through_record_branch(
                        pipeline,
                        tee,
                        encoding,
                        requested_fps.unwrap_or(60),
                        event_sender.clone(),
                    )
                    .map(Some),
                    None => {
                        send_log(
                            event_sender,
                            "warn",
                            "Recording not armed: no RTP video tap tee (recording will be unavailable until a WebRTC video session starts)."
                                .to_owned(),
                        );
                        Ok(None)
                    }
                },
                RecordingMode::Encoded => match tap_tee.as_ref() {
                    Some(tee) => {
                        let record_bitrate_kbps = record_bitrate_kbps.load(Ordering::SeqCst);
                        build_encoded_record_branch(
                            pipeline,
                            tee,
                            video_api,
                            zero_copy,
                            event_sender.clone(),
                            record_bitrate_kbps,
                            requested_fps.unwrap_or(60),
                        )
                        .map(Some)
                    }
                    None => {
                        send_log(
                            event_sender,
                            "warn",
                            "Recording not armed: no decoded video tap tee (recording will be unavailable until a WebRTC video session starts)."
                                .to_owned(),
                        );
                        Ok(None)
                    }
                },
                RecordingMode::Transcode => match tap_tee.as_ref() {
                    Some(tee) => {
                        let record_bitrate_kbps = record_bitrate_kbps.load(Ordering::SeqCst);
                        build_transcode_record_branch(
                            pipeline,
                            tee,
                            video_api,
                            zero_copy,
                            event_sender.clone(),
                            record_bitrate_kbps,
                            false,
                        )
                        .map(Some)
                    }
                    None => {
                        send_log(
                            event_sender,
                            "warn",
                            "Recording not armed: no decoded video tap tee (recording will be unavailable until a WebRTC video session starts)."
                                .to_owned(),
                        );
                        Ok(None)
                    }
                },
            }?;
            if let Some(state) = built {
                *slot = Some(state);
            }
        }
    }
    {
        // (Keep the lock guard scoped: the audio-branch build below also
        // locks the audio tee, so drop the recording lock first.)
        let mut slot = recording
            .lock()
            .map_err(|_| "Recording state lock poisoned.".to_owned())?;
        if let Some(state) = slot.as_mut() {
            if state.audio_rtp_tee.is_none() {
                if let Ok(mut tee_slot) = audio_rtp_tee.lock() {
                    if let Some(audio_tee) = tee_slot.take() {
                        state.audio_rtp_tee = Some(audio_tee);
                    }
                }
            }
            state.build_audio_branch(pipeline)?;
        }
    }
    // Link the WebRTC RTP source only after every recording branch that
    // belongs to this tee has been created and synced. A late tee branch
    // misses the original stream-start/caps/segment sticky events; qtmux
    // then receives buffers without a segment and writes an empty MP4.
    // Keeping the source unlinked until this point makes the initial source
    // link fan out the complete event sequence to both decode and remux.
    if let Some(rtp_tee) = rtp_tee {
        link_rtp_video_source_to_tee(src_pad, &rtp_tee, encoding)?;
    } else {
        send_log(
            event_sender,
            "warn",
            "Native recording not armed: no RTP video tap tee (recording will be unavailable until a WebRTC video session starts).".to_owned(),
        );
    }
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
            let reason = if configured_present_max_fps == PRESENT_LIMITER_STREAM_SENTINEL {
                "default: paced to the stream frame rate so network jitter bursts render at real-time instead of blinking"
                    .to_owned()
            } else if configured_present_max_fps == PRESENT_LIMITER_AUTO_SENTINEL {
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

        // Install the screenshot tee as part of the main chain before any
        // frames are allowed through. The old implementation unlinked this
        // queue from the sink and hot-plugged the tee on the first screenshot;
        // the resulting live caps renegotiation reset the D3D12 present path
        // and was the direct cause of the field flicker/stall.
        let permanent_video_tap_tee = if elements.len() >= 2 {
            let before_sink = elements[elements.len() - 2].clone();
            let sink_element = elements[elements.len() - 1].clone();
            let (existing_tee, branch_attached, preserved_rtp_tee) = video_tap
                .lock()
                .ok()
                .map(|slot| {
                    (
                        slot.as_ref().and_then(|tap| tap.tee.clone()),
                        slot.as_ref().is_some_and(|tap| tap.branch_attached),
                        slot.as_ref().and_then(|tap| tap.rtp_tee.clone()),
                    )
                })
                .unwrap_or((None, false, None));
            let tee = match existing_tee {
                Some(tee) => tee,
                None => {
                    let tee = make_element("tee")?;
                    pipeline.add(&tee).map_err(|error| {
                        format!("Failed to add permanent video tap tee: {error}")
                    })?;
                    tee
                }
            };
            before_sink.unlink(&sink_element);
            if let Ok(mut slot) = video_tap.lock() {
                *slot = Some(GstreamerVideoTap {
                    tee: Some(tee.clone()),
                    branch_attached,
                    before_sink,
                    sink: sink_element,
                    video_api,
                    zero_copy,
                    rtp_tee: preserved_rtp_tee,
                });
            }
            Some(tee)
        } else {
            None
        };

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
            watch_depay_output(parser, encoding, event_sender);
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
        if let Some(tap_tee) = permanent_video_tap_tee {
            tap_tee.sync_state_with_parent().map_err(|error| {
                format!("Failed to sync permanent RTP {encoding} video tap tee: {error}")
            })?;
            let tap = video_tap
                .lock()
                .ok()
                .and_then(|slot| slot.clone())
                .ok_or_else(|| "Permanent video tap state disappeared while linking.".to_owned())?;
            tap.before_sink.link(&tap_tee).map_err(|error| {
                format!(
                    "Failed to link RTP {encoding} chain into permanent video tap tee: {error:?}"
                )
            })?;
            tap_tee.link(&tap.sink).map_err(|error| {
                format!("Failed to link permanent video tap tee to RTP {encoding} sink: {error:?}")
            })?;
        }

        // Embed the RTP-level record tap tee between the webrtcbin src pad and
        // the decode chain. The native remux recorder taps the raw RTP stream
        // here (before decode), so record start/stop never touches the
        // decode/present chain — and the tee itself is deliberately NOT part
        // of `elements`, so a decoder-fallback rebuild tears down the decode
        // chain but leaves the tee (and the recording branch on it) intact.
        // Rebuilds reuse the existing tee; only the initial build creates it.
        let existing_rtp_tee = video_tap
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().and_then(|tap| tap.rtp_tee.clone()));
        let rtp_tee = match existing_rtp_tee {
            Some(tee) => tee,
            None => {
                let tee = make_element("tee")?;
                pipeline.add(&tee).map_err(|error| {
                    format!("Failed to add RTP record tap tee for {encoding}: {error}")
                })?;
                tee.sync_state_with_parent()
                    .map_err(|error| format!("Failed to sync RTP record tap tee state: {error}"))?;
                if tee.static_pad("sink").is_none() {
                    return Err("RTP record tap tee has no sink pad.".to_owned());
                }
                if let Ok(mut slot) = video_tap.lock() {
                    if let Some(tap) = slot.as_mut() {
                        tap.rtp_tee = Some(tee.clone());
                    }
                }
                send_log(
                    event_sender,
                    "info",
                    format!(
                        "Native RTP record tap tee embedded at the video source (recordings never touch the decode/present chain)."
                    ),
                );
                tee
            }
        };
        // Link the tee's fresh src pad into the decode chain head. On a
        // rebuild the old link was unlinked and released by try_rebuild, so
        // this requests a fresh pad. The tee is a non-sink: synchronous
        // transitions, no re-preroll. The source pad is intentionally linked
        // later by link_rtp_video_pad, after the remux branch is attached.
        let chain_pad = rtp_tee
            .request_pad_simple("src_%u")
            .ok_or_else(|| format!("RTP record tap tee has no free src pad for {encoding}."))?;
        chain_pad.link(&first_sink_pad).map_err(|error| {
            format!("Failed to link RTP {encoding} decode chain into the record tap tee: {error:?}")
        })?;
        watch_rtp_video_bitrate(src_pad, video_liveness.clone(), event_sender);
        watch_rtp_payload_dump(src_pad, encoding, video_liveness.clone(), event_sender);
        watch_rtcp_send_stats(pipeline, video_liveness.clone(), event_sender);
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
        // Screenshot and recording branches are deliberately outside the
        // decode-chain element list. The permanent video tap tee and its
        // valve-gated screenshot side branch can therefore survive a rebuild;
        // only the tee's old main before_sink/sink links are cut below and
        // reconnected by build_rtp_video_chain. Do not refuse recovery merely
        // because a screenshot was taken earlier in the session: that was the
        // reason the field D3D12 stall remained stuck until the user stopped
        // recording (the old guard returned false here).

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

            // Tear down the current chain: unlink its head from the RTP record
            // tap tee (when present) or the RTP src pad, Null + remove every
            // element. A failed rebuild leaves the old chain already removed,
            // so the next candidate starts from a clean slate. The RTP tap tee
            // and the recording branch on it are deliberately NOT in
            // `elements` and are NOT cleared from the video_tap slot: the
            // remux recorder taps the raw RTP stream before decode, so it
            // must survive the decode-chain rebuild untouched.
            if let Some(first) = self.elements.first() {
                if let Some(first_sink_pad) = first.static_pad("sink") {
                    let rtp_tee = self
                        .video_tap
                        .lock()
                        .ok()
                        .and_then(|slot| slot.as_ref().and_then(|tap| tap.rtp_tee.clone()));
                    if let Some(rtp_tee) = rtp_tee {
                        for pad in rtp_tee.src_pads() {
                            if pad.peer().as_ref() == Some(&first_sink_pad) {
                                let _ = pad.unlink(&first_sink_pad);
                                rtp_tee.release_request_pad(&pad);
                                break;
                            }
                        }
                    } else {
                        let _ = self.src_pad.unlink(&first_sink_pad);
                    }
                }
            }
            // The permanent screenshot tee survives the decode-chain rebuild,
            // but its main before-sink/sink links point into the old chain. Cut
            // those two links before removing the old elements; the next build
            // reconnects the same tee without any first-use hot-plug.
            if let Ok(slot) = self.video_tap.lock() {
                if let Some(tap) = slot.as_ref() {
                    if let Some(tee) = tap.tee.as_ref() {
                        tap.before_sink.unlink(tee);
                        tee.unlink(&tap.sink);
                    }
                }
            }
            for element in self.elements.drain(..) {
                let _ = element.set_state(gst::State::Null);
                let _ = self.pipeline.remove(&element);
            }
            self.video_liveness.clear_chain_elements();

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
            } else if media_label == "audio" {
                // Audio sink: the generic video low-latency config leaves the
                // WASAPI device buffer at its 200 ms default and low-latency
                // off, which adds a constant ~200 ms to the audio path (on top
                // of the 100 ms RTP jitter buffer and opus decode) — audio
                // then plays audibly behind the real-time-paced video. Shrink
                // the device buffer and enable the sink's low-latency mode;
                // clock pacing (sync) is left at the device's own rate so a
                // missing RTCP SR can never stall playback.
                configure_sink_for_low_latency(&element);
                set_property_if_supported(&element, "low-latency", true);
                set_property_if_supported(&element, "buffer-time", 50_000i64);
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

fn link_rtp_video_source_to_tee(
    src_pad: &gst::Pad,
    rtp_tee: &gst::Element,
    encoding: &str,
) -> Result<(), String> {
    if src_pad.is_linked() {
        return Ok(());
    }
    let tee_sink = rtp_tee
        .static_pad("sink")
        .ok_or_else(|| format!("RTP {encoding} record tap tee has no sink pad."))?;
    src_pad.link(&tee_sink).map_err(|error| {
        format!("Failed to link RTP {encoding} video pad into the record tap tee: {error:?}")
    })?;
    Ok(())
}

fn replay_recording_sticky_events_from_pad(upstream_src: &gst::Pad, valve: &gst::Element) {
    replay_recording_sticky_events_with_caps(upstream_src, valve, None, false);
}

/// Replay stream-start + segment only, WITHOUT the upstream caps event. Used
/// by the VIDEO transcode branch: the tee's raw decoded caps (e.g. NV12 with
/// the decoder's `1:3:5:1` full-range tag, or no colorimetry at all) would
/// override the branch's declared FULL-RANGE BT.709 caps on the videoconverts
/// and silently disable the FULL→LIMITED range conversion — the recorded
/// file then carries 0-255 pixel data tagged limited, which players render
/// with crushed/"too contrasty" colors. The branch's own capsfilters re-assert
/// the correct caps with the first buffer, so qtmux still gets its caps.
fn replay_recording_sticky_events_no_caps(upstream_src: &gst::Pad, valve: &gst::Element) {
    replay_recording_sticky_events_with_caps(upstream_src, valve, None, true);
}

fn replay_recording_sticky_events_with_caps(
    upstream_src: &gst::Pad,
    valve: &gst::Element,
    caps_override: Option<&'static str>,
    skip_caps: bool,
) {
    let Some(valve_src) = valve.static_pad("src") else {
        return;
    };

    // A tee request pad created after negotiation is not guaranteed to have
    // received the original sticky events. The tee SINK pad is authoritative:
    // it retains the stream-start/caps/segment sequence from the RTP source.
    // Some appsrc/test and bundled WebRTC paths retain only caps there, so fill
    // any missing mandatory events before opening the valve. Without a TIME
    // segment, qtmux emits a header-only MP4 and GStreamer reports
    // `gst_segment_to_running_time: segment->format == format`.
    let mut stream_start = None;
    let mut caps_event = None;
    let mut segment = None;
    upstream_src.sticky_events_foreach(|event| {
        match event.type_() {
            gst::EventType::StreamStart if stream_start.is_none() => {
                stream_start = Some(event.clone())
            }
            gst::EventType::Caps if caps_event.is_none() => caps_event = Some(event.clone()),
            gst::EventType::Segment if segment.is_none() => segment = Some(event.clone()),
            _ => {}
        }
        std::ops::ControlFlow::Continue(gst::EventForeachAction::Keep)
    });

    let stream_start = stream_start.unwrap_or_else(|| {
        gst::event::StreamStart::new(&format!("opennow-recording-{}", valve.name()))
    });
    let caps_event = caps_override
        .and_then(|caps| {
            caps.parse::<gst::Caps>()
                .ok()
                .map(|caps| gst::event::Caps::new(&caps))
        })
        .or(caps_event)
        .or_else(|| {
            upstream_src
                .current_caps()
                .filter(|caps| !caps.is_empty())
                .map(|caps| gst::event::Caps::new(&caps))
        });
    let segment = segment.unwrap_or_else(|| {
        let mut segment = gst::Segment::new();
        segment.set_format(gst::Format::Time);
        gst::event::Segment::new(&segment)
    });

    // Push in the mandatory downstream order. These events travel only from
    // the valve SRC pad down the recording branch, never back into the live
    // RTP/decode path. The caps event is the tee's REAL RTP caps (payload
    // type included): the live decode chain feeds the same raw caps to
    // rtph265depay successfully, and a stripped override (without `payload`)
    // makes the depayloader reject the first buffered RTP packet with
    // FLOW_NOT_NEGOTIATED, killing the whole stream through the shared tee.
    let stream_ok = valve_src.push_event(stream_start);
    let caps_ok = if skip_caps {
        true
    } else {
        caps_event
            .map(|event| valve_src.push_event(event))
            .unwrap_or(true)
    };
    let segment_ok = valve_src.push_event(segment);
    if !(stream_ok && caps_ok && segment_ok) {
        // This is diagnostic only; a closed valve may legitimately reject a
        // duplicate event. The branch still has the original sticky events on
        // its pads when it was first linked.
        eprintln!(
            "recording sticky-event replay partially rejected for {}: stream={stream_ok} caps={caps_ok} segment={segment_ok}",
            valve.name()
        );
    }
}

fn replay_recording_sticky_events(valve: &gst::Element) {
    let Some(valve_sink) = valve.static_pad("sink") else {
        return;
    };
    let Some(upstream_src) = valve_sink.peer() else {
        return;
    };
    replay_recording_sticky_events_from_pad(&upstream_src, valve);
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
    set_property_from_str_if_supported(&valve, "drop-mode", "forward-sticky-events");
    // Never let the grab branch back-pressure the video path: drop new buffers
    // when the queue fills instead of blocking the tee.
    queue.set_property_from_str("leaky", "downstream");
    queue.set_property("max-size-buffers", 2u32);
    queue.set_property("max-size-bytes", 0u32);
    queue.set_property("max-size-time", 0u64);
    // sync=false lets the branch run as fast as frames arrive; the pad probe
    // below keeps only the newest encoded PNG buffer. async=false is
    // critical: this branch is hot-plugged into a PLAYING pipeline with the
    // valve closed, so the appsink can never preroll. With async=true its
    // PLAYING transition would stay pending and complete LATER when the valve
    // opens for a capture — a deferred state change that re-prerolls the
    // whole pipeline and kills the D3D12 present chain (the exact failure the
    // no-sink recording branch was designed to avoid). async=false makes the
    // transition synchronous even without preroll data.
    appsink.set_property("sync", false);
    appsink.set_property("async", false);
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
    for pair in new_elements.windows(2) {
        pair[0]
            .link(pair[1])
            .map_err(|error| format!("Failed to link screenshot grab branch: {error:?}"))?;
    }
    // Sync the branch to the pipeline state BEFORE linking it into the video
    // tap tee. This branch is hot-plugged on first screenshot use, when the
    // pipeline is already PLAYING and the tap tee (from ensure_tee) is PLAYING
    // too — linking a PLAYING tee pad to a NULL valve would make the tee's
    // first push fail and propagate the error upstream, killing the video
    // flow (the exact field failure the remux recording branch hit). With
    // async=false on the appsink, every element here is a synchronous
    // transition, so syncing is re-preroll-free.
    for element in &new_elements {
        element
            .sync_state_with_parent()
            .map_err(|error| format!("Failed to sync screenshot grab element state: {error}"))?;
    }

    tee.link(&valve).map_err(|error| {
        format!("Failed to link screenshot grab branch to video tap tee: {error:?}")
    })?;
    // The main tap tee is permanent; only this valve-gated side branch is
    // attached on first screenshot use, so no live sink renegotiation occurs.

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

/// Build the native TRANSCODE recording branch off the video tap tee (the
/// tee embedded between the post-decode queue and the sink in
/// `build_rtp_video_chain` — the same tee screenshots use). The branch takes
/// the DECODED frames, converts them to standard BT.709 8-bit 4:2:0, scales
/// the luma FULL→LIMITED with a LUT pad probe (videoconvert only clips), and
/// re-encodes with H.264:
/// tee → valve → queue → [download] → videoconvert → capsfilter (FULL) →
/// capsfilter (LIMITED, LUT probe) → encoder → h264parse → qtmux → swallow.
///
/// The branch is built at session start (pipeline NULL/READY) and the tee pad
/// stays linked for the whole session, so record start/stop is ONLY a valve
/// open/close — the decode/present chain is never touched and the pipeline
/// cannot re-preroll (the exact failure of the old post-decode x264 branch
/// with a sink; this branch has NO sink). The encoder starts a fresh GOP (IDR)
/// at the first frame it sees, so recording begins instantly with a decodable
/// file (no mid-GOP orphan frames, no waiting for the next server keyframe).
/// Chunks are captured by DROP probes on the swallow sink pad (each becomes
/// one `recording-chunk` event); an EVENT probe records EOS so
/// `stop(finalize=true)` knows when the branch flushed. The game-audio RTP
/// stream is Opus and is transcoded to AAC into the same qtmux when available;
/// the local mic is not part of the server RTP stream.
/// Neutralizes the MP4 `colr` colour-information box(es) in the finished
/// recording so the file carries NO colour metadata at all — primaries,
/// transfer, matrix and range all "unknown" — byte-identical in spirit to
/// the official GeForce Now PC recordings. qtmux always writes a `colr` box
/// from the colourimetry that h264parse derives by resolution (BT.709 for
/// 1080p) even when the bitstream has no VUI (insert-vui=false), and no
/// capsfilter can remove the field from the caps, so the box is neutralized
/// after muxing by renaming its type `colr` → `free`. A `free` box is a
/// standard, universally-ignored MP4 box: the file's structure, sizes and
/// absolute sample offsets (stco/co64) stay untouched, so no box-tree surgery
/// or offset patching is needed and the file remains fully playable. This is
/// what makes the field players render the recording like the GFN file:
/// content with zero colour tags is expanded as limited by default, while a
/// present-but-misinterpreted colr box makes some players skip the range
/// expansion and show crushed blacks ("hitam pekat"). Returns the number of
/// boxes neutralized.
pub(crate) fn strip_mp4_colr_boxes(data: &mut Vec<u8>) -> usize {
    let mut count = 0usize;
    let mut idx = 0usize;
    while idx + 8 <= data.len() {
        if &data[idx..idx + 4] == b"colr" {
            data[idx..idx + 4].copy_from_slice(b"free");
            count += 1;
            idx += 4;
        } else {
            idx += 1;
        }
    }
    count
}

/// Scale a video buffer's Y plane from FULL range (0-255) to LIMITED range
/// (16-235) with a precomputed 256-entry LUT: `y' = 16 + y*219/255`.
/// GStreamer's videoconvert does NOT rescale YUV range — it only clips
/// extremes to [16,235] while leaving mid-tones at their full-range values
/// (verified experimentally), and RGB round-trips through it clip the same
/// way. A file full of clipped-full data is exactly what the field players
/// expand to "hitam pekat" (crushed blacks + blown highlights), which is
/// also why every earlier fix attempt (data full, data limited, tag or no
/// tag) looked equally dark: the data was never genuinely rescaled. Only the
/// Y plane is touched — chroma is left as-is (BT.709 1080p), and 4:2:0
/// formats (I420/NV12) both have the Y plane first, so no format lookup is
/// needed beyond the plane geometry from the caps. The probe passes the
/// element's current caps (stable for the whole session) rather than the
/// buffer's, keeping this free of BufferRef trait gymnastics.
pub(crate) fn scale_y_plane_full_to_limited(buffer: &mut gst::Buffer, caps: Option<&gst::Caps>) {
    use gstreamer_video::{VideoFormat, VideoInfo};
    let Some(caps) = caps else { return };
    let Ok(info) = VideoInfo::from_caps(caps) else {
        return;
    };
    if info.format() != VideoFormat::I420 && info.format() != VideoFormat::Nv12 {
        return;
    }
    // make_mut() guarantees a unique writable view (copies the data if the
    // buffer is shared elsewhere); the decode path hands us an exclusive
    // buffer here, so this is a no-op in practice.
    let Ok(mut map) = buffer.make_mut().map_writable() else {
        return;
    };
    let data = map.as_mut_slice();
    let base = info.offset()[0];
    let stride = usize::try_from(info.stride()[0]).unwrap_or(0);
    let width = info.width() as usize;
    let height = info.height() as usize;
    // Bounds-check the LAST row's last pixel only (the Y plane is the first
    // `height*stride` bytes; the guard must not require `height*stride` to
    // fit when the buffer's total size includes just the chroma planes — e.g.
    // I420 2x2 is 6 bytes but height*stride is 8).
    if width > stride
        || base + (height.saturating_sub(1)).saturating_mul(stride) + width > data.len()
    {
        return;
    }
    let lut = full_to_limited_lut();
    for row in 0..height {
        let start = base + row * stride;
        let end = start + width;
        for pixel in &mut data[start..end] {
            *pixel = lut[*pixel as usize];
        }
    }
}

/// Precomputed 256-entry FULL→LIMITED luma LUT: `y' = 16 + y*219/255`
/// (0 → 16, 128 → 126, 255 → 235). Pure — used both by the pad probe and by
/// the unit test, which validates the exact values without needing to
/// construct pipeline buffers.
pub(crate) fn full_to_limited_lut() -> [u8; 256] {
    let mut lut = [0u8; 256];
    for (x, slot) in lut.iter_mut().enumerate() {
        *slot = (16u32 + (x as u32 * 219u32 + 127u32) / 255u32) as u8;
    }
    lut
}

pub(crate) fn build_transcode_record_branch(
    pipeline: &gst::Pipeline,
    tap_tee: &gst::Element,
    video_api: RtpVideoApi,
    zero_copy: bool,
    event_sender: Option<Sender<Event>>,
    record_bitrate_kbps: u32,
    es_tail: bool,
) -> Result<GstreamerRecordingState, String> {
    let valve = make_element("valve")?;
    let queue = make_element("queue")?;
    // Format conversion: the decoder's native NV12 → the encoder's input
    // format (NV12 for d3d12h264enc, I420 for the software encoders).
    let convert = make_element("videoconvert")?;
    // The GFN H.265 stream is FULL-RANGE PC video (decoder output 0-255 — the
    // screenshot branch on the same tee writes it straight into a PNG, which
    // has no range ambiguity and shows the true colors), but H.264 playback
    // expects LIMITED range: the official GeForce Now PC recordings are
    // limited (16-235, verified by histogram — the 0-255 readings in earlier
    // analysis were codec overshoot, <0.1% of pixels), and every player on
    // the field expands H.264 content as limited, so full-range data comes
    // out with crushed blacks ("hitam pekat"). The branch therefore declares
    // the input FULL-RANGE and converts it to LIMITED 16-235 with a LUT-based
    // Y-plane scaler (a Rust pad probe — videoconvert only CLIPS extremes to
    // [16,235] without rescaling mid-tones, which is exactly the "hitam
    // pekat" + blown-highlight look, and RGB round-trips do the same clip),
    // then feeds an encoder configured with insert-vui=false so the file
    // carries NO range/colorimetry tag — byte-identical in spirit to the GFN
    // recordings.
    // NOTE: no videorate in this chain — videorate re-bases its output onto
    // the replayed live segment (start=0, session start) and INSERTS
    // duplicate frames for the whole gap between session start and the valve
    // opening, so a 31 s recording came out as 79.8 s of video with the
    // audio track stranded at the tail (the field "stuck / audio missing"
    // bug). The muxer normalizes the first sample of each track to 0 on its
    // own, so the live stream's own pacing is preserved and both tracks stay
    // in sync.
    let declare_caps = make_element("capsfilter")?;
    // Bridges the FULL→LIMITED colourimetry change between the two caps
    // filters (the caps must intersect for the elements to link; the range
    // is actually rescaled by the LUT probe below — videoconvert only clips
    // extremes, which is exactly the bug being fixed). Keeps the data in the
    // encoder's format and limited range, and the LUT probe hangs on its
    // sink pad.
    let range_convert = make_element("videoconvert")?;
    // Declares the LIMITED range at the encoder input (what the LUT scaler
    // below produces).
    let encode_caps = make_element("capsfilter")?;
    let (encoder_factory, encoder) = pick_h264_encoder(record_bitrate_kbps)?;
    // Converts the encoder's byte-stream H.264 to avcC for qtmux: the
    // hardware encoder (d3d12h264enc) and openh264/x264 emit byte-stream,
    // which qtmux cannot accept directly (link fails / unplayable).
    let parse = make_element("h264parse")?;

    valve.set_property("drop", true);
    // Forward sticky events while buffers are gated, so qtmux never receives
    // a first buffer before its stream-start/caps/segment. We also replay
    // these events explicitly in start(), because the bundled GStreamer
    // build can still lose the segment when a valve is closed.
    set_property_from_str_if_supported(&valve, "drop-mode", "forward-sticky-events");
    // The branch must never back-pressure the live decode path. leaky=upstream
    // drops the INCOMING buffer the moment the queue is full, so the tap tee's
    // push always returns instantly and every frame the branch cannot consume
    // returns to the decoder's buffer pool immediately. leaky=downstream is
    // NOT enough here: it only leaks once the queue reaches its max, and if
    // the decoder's buffer pool is smaller than the queue limit the queue
    // simply holds all pool buffers and the decoder starves — the live sink
    // then repeats its last frame (field flicker while recording). Keep the
    // limit small (4 decoded 1080p frames ≈ 12 MB) so the branch can never
    // hoard the decoder's pool; when the encoder keeps up it never fills and
    // no frames are lost.
    queue.set_property_from_str("leaky", "upstream");
    queue.set_property("max-size-buffers", 4u32);
    queue.set_property("max-size-bytes", 0u32);
    queue.set_property("max-size-time", 0u64);

    // Count recording-frame drops: every frame enters the queue (sink pad)
    // and either leaves it (src pad) or is dropped by leaky=upstream while
    // the queue is full — the exact "encoder could not keep up" case on weak
    // devices. After the drain at stop the queue is empty, so `input − output`
    // is the precise drop count for the recording (surfaced via
    // `recording-finished.droppedFrames`).
    let queue_input = Arc::new(AtomicU64::new(0));
    let queue_output = Arc::new(AtomicU64::new(0));
    {
        let input = queue_input.clone();
        let queue_sink = queue
            .static_pad("sink")
            .ok_or_else(|| "Recording queue has no sink pad.".to_owned())?;
        queue_sink.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            input.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
    }
    {
        let output = queue_output.clone();
        let queue_src = queue
            .static_pad("src")
            .ok_or_else(|| "Recording queue has no src pad.".to_owned())?;
        queue_src.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            output.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
    }
    // Swallow queue: the chunk/EOS probes live on its sink pad and return
    // DROP, so qtmux's push always returns FLOW_OK and its src task keeps
    // running with an otherwise-unlinked tail (no NOT_LINKED parking).
    let swallow = make_element("queue")?;
    swallow.set_property_from_str("leaky", "downstream");
    swallow.set_property("max-size-buffers", 1u32);
    swallow.set_property("max-size-bytes", 0u32);
    swallow.set_property("max-size-time", 0u64);

    // D3D paths with zero-copy produce texture-backed decoded frames;
    // videoconvert/x264 cannot import D3D memory, so download to system
    // memory first (same as the screenshot grab branch).
    let download_factory = match (video_api, zero_copy) {
        (RtpVideoApi::D3D11, true) => Some("d3d11download"),
        (RtpVideoApi::D3D12, true) => Some("d3d12download"),
        _ => None,
    };
    let download = match download_factory {
        Some(factory) => Some(make_element(factory)?),
        None => None,
    };

    // Hardware encoders (d3d12/d3d11/qsv/mf/vaapi/nvenc/videotoolbox) take
    // NV12 directly; the software encoders (x264/openh264) take I420. The
    // FULL-range colorimetry is declared at the branch input (what the
    // decoder actually outputs), and the LIMITED colorimetry at the encoder
    // input (what the LUT rescale produces and what H.264 players expect).
    let encoder_format = if is_hardware_h264_factory(&encoder_factory) {
        "NV12"
    } else {
        "I420"
    };
    declare_caps.set_property(
        "caps",
        format!(
            "video/x-raw,format=(string){encoder_format},colorimetry=(string)bt709/bt709/bt709/0-255,chroma-site=(string)mpeg2"
        )
        .parse::<gst::Caps>()
        .map_err(|error| format!("Invalid recording transcode caps: {error}"))?,
    );
    // LIMITED range, encoder format (NV12 for d3d12h264enc, I420 for the
    // software encoders) — the LUT probe on this element's sink pad has
    // already rescaled the Y plane 0-255 → 16-235 by the time the caps here
    // are negotiated, so the caps match the data.
    encode_caps.set_property(
        "caps",
        format!(
            "video/x-raw,format=(string){encoder_format},colorimetry=(string)bt709/bt709/bt709/16-235,chroma-site=(string)mpeg2"
        )
        .parse::<gst::Caps>()
        .map_err(|error| format!("Invalid recording transcode caps: {error}"))?,
    );

    // STANDARD seekable MP4 (faststart), NOT fragmented: a fragmented
    // streamable MP4 (ftyp + moov + moof/mdat every ~500ms) is what the
    // earlier versions wrote, and players like VLC cannot seek in it and show
    // glitches — the official GeForce Now recorder writes a standard MP4 with
    // a complete index. faststart puts the moov (with the full sample index)
    // at the front and writes zero moof boxes, so the file is fully seekable
    // and glitch-free in any player. qtmux buffers the (short) recording in
    // memory and writes it out when EOS finalizes the muxer — recordings are
    // short clips (tens of seconds), so this is bounded and cheap.
    //
    // With `es_tail` the branch ends in a temp ES FILE instead (the same
    // shape as the pass-through branch): the encoder's H.264 byte-stream is
    // written raw and muxed OFFLINE at stop by `remux_encoded_recording` — no
    // live muxer in the pipeline, so a muxer stall can never freeze the
    // recording branch (the field wedge that killed the live-qtmux versions
    // of this branch). `Encoded` mode (opt-in via OPENNOW_RECORDING_MODE) is
    // exactly this.
    let muxer = if es_tail { None } else { Some(make_element("qtmux")?) };
    if let Some(muxer) = muxer.as_ref() {
        muxer.set_property("faststart", true);
        muxer.set_property("fragment-duration", 0u32);
        muxer.set_property("streamable", false);
    }
    // ES tail: byte-stream H.264 capsfilter + filesink (self-identifying via
    // 00 00 01 start codes, so the OFFLINE filesrc → h264parse can frame it
    // with no external caps).
    let es_caps = if es_tail {
        Some(make_element("capsfilter")?)
    } else {
        None
    };
    if let Some(es_caps) = es_caps.as_ref() {
        es_caps.set_property(
            "caps",
            "video/x-h264,stream-format=(string)byte-stream,alignment=(string)au"
                .parse::<gst::Caps>()
                .map_err(|error| format!("Invalid encoded-recording ES caps: {error}"))?,
        );
    }
    let mut video_es_path: Option<std::path::PathBuf> = None;
    let video_filesink = if es_tail { Some(make_element("filesink")?) } else { None };
    if let Some(filesink) = video_filesink.as_ref() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir()
            .join(format!("opennow-rec-{}-{stamp}.h264.es", std::process::id()));
        filesink.set_property(
            "location",
            path
                .to_str()
                .ok_or_else(|| "Encoded ES path is not valid UTF-8.".to_owned())?,
        );
        filesink.set_property("sync", false);
        filesink.set_property("async", false);
        video_es_path = Some(path);
    }

    // Chain order: valve → queue → [download] → videoconvert (to the
    // encoder's format) → capsfilter (declare FULL bt709) → videoconvert
    // (range bridge, with the LUT full→limited Y-plane scaler probe on its
    // sink pad) → capsfilter (LIMITED bt709) → encoder → h264parse → qtmux
    // → swallow (or → ES capsfilter → filesink with es_tail). The valve is
    // deliberately FIRST so the encoder only runs while a recording is
    // active (zero CPU when idle) and every element below it is stateless
    // enough to reset in place.
    let mut elements: Vec<&gst::Element> = vec![&valve, &queue];
    if let Some(download) = download.as_ref() {
        elements.push(download);
    }
    elements.extend([&convert, &declare_caps, &range_convert, &encode_caps, &encoder, &parse]);
    if es_tail {
        elements.push(es_caps.as_ref().expect("es caps built"));
        elements.push(video_filesink.as_ref().expect("filesink built"));
    } else {
        elements.push(muxer.as_ref().expect("muxer built"));
        elements.push(&swallow);
    }
    for element in &elements {
        pipeline.add(*element).map_err(|error| {
            format!("Failed to add recording transcode branch element: {error}")
        })?;
    }
    for pair in elements.windows(2) {
        pair[0]
            .link(pair[1])
            .map_err(|error| format!("Failed to link recording transcode branch: {error:?}"))?;
    }
    // Sync the branch to the pipeline state BEFORE linking it into the video
    // tap tee. This function is called from link_rtp_video_pad when the video
    // chain is built, which is AFTER the pipeline is PLAYING — an element
    // left in NULL has an inactive sink pad, and the tee's first push to it
    // would fail and propagate the error upstream, killing the whole video
    // flow. All branch elements are non-sinks, so the sync is synchronous: it
    // can never trigger the deferred-async re-preroll that a sink would (the
    // exact failure of the old post-decode appsink branch).
    for element in &elements {
        element.sync_state_with_parent().map_err(|error| {
            format!("Failed to sync recording transcode branch element state: {error}")
        })?;
    }
    // Link the branch into the video tap tee. The valve starts closed
    // (drop=true), so qtmux consumes nothing until a recording is active,
    // while the decode/present chain keeps flowing untouched.
    let first_sink = valve
        .static_pad("sink")
        .ok_or_else(|| "Recording branch valve has no sink pad.".to_owned())?;
    let branch_pad = tap_tee
        .request_pad_simple("src_%u")
        .ok_or_else(|| "Failed to request a recording pad from the video tap tee.".to_owned())?;
    if let Err(error) = branch_pad.link(&first_sink) {
        let _ = tap_tee.release_request_pad(&branch_pad);
        return Err(format!(
            "Failed to link video tap into the recording branch: {error:?}"
        ));
    }

    // The LUT scaler: full-range (0-255) → limited (16-235) on the Y plane.
    // GStreamer's videoconvert does NOT rescale YUV range (it only clips
    // extremes to [16,235], leaving mid-tones at full-range values — the
    // crushed-blacks/"hitam pekat" field bug), so the scale is done here in
    // Rust with a precomputed 256-entry LUT. Only the luma plane is touched
    // (chroma stays as-is; 1080p H.264 is always BT.709), and only while the
    // valve is open, so it costs nothing when idle. Attached to the range
    // bridge's sink pad at build time and torn down with the whole branch
    // between recordings.
    let lut_probe_id = Arc::new(Mutex::new({
        let range_sink = range_convert
            .static_pad("sink")
            .ok_or_else(|| "Recording range bridge has no sink pad.".to_owned())?;
        range_sink.add_probe(gst::PadProbeType::BUFFER, |pad, info| {
            if let Some(buffer) = info.buffer_mut() {
                scale_y_plane_full_to_limited(buffer, pad.current_caps().as_ref());
            }
            gst::PadProbeReturn::Ok
        })
    }));

    // Measure the PTS cadence of the frames that actually enter the recording
    // (the valve's SRC pad only passes buffers while the valve is open, so the
    // window == the recording window == the frames that become the ES). The
    // ENCODED-mode offline remux derives its PTS ladder from the MEASURED
    // average interval instead of the static negotiated fps, so a stream that
    // delivers fewer unique frames than negotiated (rate-adaptive congestion)
    // still records real-time instead of 2×-stretched video.
    let rec_frames_first_pts = Arc::new(AtomicU64::new(0));
    let rec_frames_last_pts = Arc::new(AtomicU64::new(0));
    let rec_frames_count = Arc::new(AtomicU64::new(0));
    {
        let first = rec_frames_first_pts.clone();
        let last = rec_frames_last_pts.clone();
        let count = rec_frames_count.clone();
        let valve_src = valve
            .static_pad("src")
            .ok_or_else(|| "Recording branch valve has no src pad.".to_owned())?;
        valve_src.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
            if let Some(buffer) = info.buffer() {
                if let Some(pts) = buffer.pts() {
                    let pts = pts.nseconds();
                    if first.load(Ordering::Relaxed) == 0 {
                        let _ = first.compare_exchange(
                            0,
                            pts,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        );
                    }
                    last.store(pts, Ordering::Relaxed);
                    count.fetch_add(1, Ordering::Relaxed);
                }
            }
            gst::PadProbeReturn::Ok
        });
    }

    let eos_seen = Arc::new(AtomicBool::new(false));
    // Inactive until the first `start()`; the chunk probe is gated on this so
    // no muxer output is ever captured while the branch is idle.
    let active = Arc::new(AtomicBool::new(false));

    let filesink_eos = Arc::new(AtomicBool::new(false));
    if es_tail {
        // ES tail: EOS flag on the filesink. `stop()` waits for it before the
        // offline remux reads the ES file, so the file is always fully
        // flushed/closed. The final MP4 chunks are emitted by the OFFLINE
        // remux at stop (`remux_encoded_recording`), not by a live probe.
        let eos_flag = filesink_eos.clone();
        let filesink_sink_pad = video_filesink
            .as_ref()
            .expect("filesink built")
            .static_pad("sink")
            .ok_or_else(|| "Encoded filesink has no sink pad.".to_owned())?;
        filesink_sink_pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
            if let Some(event) = info.event() {
                if event.type_() == gst::EventType::Eos {
                    eos_flag.store(true, Ordering::SeqCst);
                }
            }
            gst::PadProbeReturn::Ok
        });
    } else {
        // Chunk capture: every qtmux output buffer becomes one
        // `recording-chunk` event (with faststart the whole file is emitted
        // at EOS, so a single final chunk carries the complete seekable MP4).
        let chunk_sender = event_sender.clone();
        let chunk_active = active.clone();
        let probe_sender = event_sender.clone();
        let swallow_sink_pad = swallow
            .static_pad("sink")
            .ok_or_else(|| "Recording swallow queue has no sink pad.".to_owned())?;
        // DROP swallows each chunk AT the pad: qtmux's push returns FLOW_OK,
        // so its src task keeps running with an otherwise-unlinked branch
        // tail.
        swallow_sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
            if chunk_active.load(Ordering::SeqCst) {
                if let Some(buffer) = info.buffer() {
                    if let Ok(mapped) = buffer.map_readable() {
                        // Strip the container's colour-metadata box so the
                        // file carries NO colour tags at all (see
                        // strip_mp4_colr_boxes for why the player needs it
                        // byte-identical to the official GeForce Now
                        // recordings).
                        let mut bytes = mapped.as_slice().to_vec();
                        let stripped = strip_mp4_colr_boxes(&mut bytes);
                        if stripped > 0 {
                            send_log(
                                &probe_sender,
                                "info",
                                format!(
                                    "Recording finalized with {stripped} colour-metadata box(es) neutralized (colr→free); file carries no colour tags, matching the official GeForce Now recordings."
                                ),
                            );
                        }
                        // Send in fixed-size chunks: qtmux with faststart
                        // emits the ENTIRE seekable file as one final buffer,
                        // which as a single base64 line can exceed a hundred
                        // MB and freeze Electron's main process ("not
                        // responding" + crash with a 0-byte .tmp). Electron
                        // appends chunks in arrival order.
                        if let Some(sender) = &chunk_sender {
                            send_recording_chunks(sender, &bytes);
                        }
                    }
                }
            }
            gst::PadProbeReturn::Drop
        });

        let eos_flag = eos_seen.clone();
        swallow_sink_pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
            if let Some(event) = info.event() {
                if event.type_() == gst::EventType::Eos {
                    eos_flag.store(true, Ordering::SeqCst);
                }
            }
            gst::PadProbeReturn::Drop
        });
    }

    send_log(
        &event_sender,
        "info",
        if es_tail {
            format!(
                "Attached native ENCODED recording branch (decoded → {encoder_factory} (insert-vui=false, bitrate={record_bitrate_kbps} kbps) → ES file; FULL→LIMITED Y-plane LUT scaler → limited-range untagged H.264 remuxed offline at stop — never touches the decode/present chain)."
            )
        } else {
            format!(
                "Attached native TRANSCODE recording branch (decoded → {encoder_factory} (insert-vui=false, bitrate={record_bitrate_kbps} kbps) → qtmux → swallow; FULL→LIMITED Y-plane LUT scaler → limited-range untagged H.264 MP4 matching the official GeForce Now recordings, never touches the decode/present chain)."
            )
        },
    );

    Ok(GstreamerRecordingState {
        valve,
        queue,
        encoder,
        encoder_factory,
        h264_parse: parse,
        video_convert: convert,
        video_declare_caps: declare_caps,
        video_encode_caps: encode_caps,
        range_lut_probe: lut_probe_id,
        range_convert,
        video_download: download,
        muxer: if es_tail { None } else { Some(muxer.expect("muxer built")) },
        swallow: if es_tail { None } else { Some(swallow) },
        video_es_path,
        audio_es_path: None,
        es_caps: es_caps.clone(),
        video_filesink: video_filesink.clone(),
        audio_filesink: None,
        filesink_eos,
        audio_filesink_eos: Arc::new(AtomicBool::new(false)),
        eos_seen,
        active,
        mode: if es_tail {
            RecordingMode::Encoded
        } else {
            RecordingMode::Transcode
        },
        codec: "H264".to_owned(),
        fps: 0,
        rec_frames_first_pts,
        rec_frames_last_pts,
        rec_frames_count,
        queue_input,
        queue_output,
        dropped_frames: Arc::new(AtomicU64::new(0)),
        record_bitrate_kbps: Arc::new(AtomicU32::new(RECORD_BITRATE_DEFAULT_KBPS)),
        idr_gate: Arc::new(AtomicBool::new(true)),
        video_tap_tee: tap_tee.clone(),
        audio_rtp_tee: None,
        audio_valve: None,
        audio_queue: None,
        audio_capsfilter: None,
        audio_depayloader: None,
        audio_opusdec: None,
        audio_audioconvert: None,
        audio_aac_encoder: None,
        audio_aac_parse: None,
        audio_adts_caps: None,
        audio_branch_built: Arc::new(AtomicBool::new(false)),
        spent: Arc::new(AtomicBool::new(false)),
        pass_flow_diag: None,
        event_sender,
    })
}

/// Native recording branch — ENCODED (opt-in via OPENNOW_RECORDING_MODE).
/// Taps the DECODED video frames at the permanent post-decode tee (the same
/// tee the transcode branch and screenshots use) and re-encodes them to a
/// limited-range untagged H.264 elementary stream in a temp file, exactly
/// like the transcode branch but WITHOUT the live qtmux — muxing happens
/// OFFLINE at stop (`remux_encoded_recording`), so the branch can never
/// wedge.
///
/// Recording from the DECODED frames is what makes the recording reliable:
/// the live decode chain is shielded by webrtcbin's rtpbin (jitter buffer +
/// retransmission + SSRC-change handling), so every frame the user sees is
/// complete. The raw-RTP tap (`PassThrough`) sees the pre-rtpbin stream and
/// loses 35-44% of frames on WAN jitter/restarts — the field's truncated
/// video and A/V desync ("audio delay").
pub(crate) fn build_encoded_record_branch(
    pipeline: &gst::Pipeline,
    tap_tee: &gst::Element,
    video_api: RtpVideoApi,
    zero_copy: bool,
    event_sender: Option<Sender<Event>>,
    record_bitrate_kbps: u32,
    fps: u32,
) -> Result<GstreamerRecordingState, String> {
    let mut state = build_transcode_record_branch(
        pipeline,
        tap_tee,
        video_api,
        zero_copy,
        event_sender,
        record_bitrate_kbps,
        true,
    )?;
    // The transcode builder leaves `fps` at 0 (its live-qtmux path never
    // needs it), but the ENCODED branch remuxes OFFLINE at stop
    // (`remux_encoded_recording`) and that remux reconstructs the video PTS
    // as a monotonic ladder at THIS frame rate (raw ES files carry no
    // timestamps). fps=0 made the ladder fall back to 1 second per frame —
    // the field "frame per frame" bug: the MP4 played one frame per second
    // and its duration ballooned to the frame count in seconds (1973 frames
    // → a 1972 s "recording"). Carry the negotiated stream fps so the
    // ladder is exact.
    state.fps = fps;
    Ok(state)
}

/// Native recording branch — PASS-THROUGH (bitstream remux). Taps the RAW
/// RTP stream at the source tee (before decode), depayloads and parses it,
/// and writes the received H.265/H.264/AV1 elementary stream into a temp
/// file:
///
///   rtp tap tee → valve → queue → capsfilter (RTP caps) → rtph*depay →
///   h*parse → filesink (temp ES file)
///
/// There is NO decoder, NO encoder, NO videoconvert, NO colour transform and
/// NO GPU work anywhere in this branch — recording costs nothing to the live
/// stream (the reason the transcode branch stutters on single-iGPU laptops:
/// d3d12h264enc competes with the decoder for the same GPU). The file is the
/// exact received bitstream at the exact received quality, so it is never
/// "burik-er" than the stream itself. Playback on the user's own machine is
/// fine (HEVC decode is hardware-supported on the same iGPU).
///
/// MUXING HAPPENS OFFLINE AT STOP (`remux_pass_through_recording`) in a
/// throwaway pipeline: the temp ES file (and the game-audio AAC file) are
/// remuxed into a standard seekable MP4 only after the recording ends. No
/// qtmux lives in the live pipeline, so a muxer stall can never freeze the
/// recording branch (the field wedge: a pass-through recording whose video
/// AND audio branches both froze at the qtmux input ~16 s in while the live
/// stream stayed healthy). A filesink always accepts, so the branch cannot
/// wedge.
///
/// The IDR gate (drop everything until the first sync access unit, so the
/// file never starts mid-GOP with orphan P-frames) is applied at the OFFLINE
/// remux, where it cannot stall the live stream. Because the live branch
/// writes EVERYTHING it receives, a server keyframe that lands late (GFN
/// H265 GOPs can run ~9 s) no longer black-holes the head of the recording.
pub(crate) fn build_pass_through_record_branch(
    pipeline: &gst::Pipeline,
    rtp_tee: &gst::Element,
    codec: &str,
    fps: u32,
    event_sender: Option<Sender<Event>>,
) -> Result<GstreamerRecordingState, String> {
    let codec_upper = codec.to_ascii_uppercase();
    let (depay_factory, parse_factory, rtp_encoding) = match codec_upper.as_str() {
        "H264" => ("rtph264depay", "h264parse", "H264"),
        "H265" | "HEVC" => ("rtph265depay", "h265parse", "H265"),
        "AV1" => ("gfnav1depay", "av1parse", "AV1"),
        other => {
            return Err(format!(
                "Pass-through recording: unsupported codec {other} (H264/H265/AV1 expected)."
            ))
        }
    };

    let valve = make_element("valve")?;
    let queue = make_element("queue")?;
    // as the decode chain — both see the SAME stream. The tap sits at the
    // webrtcbin element's video SRC pad, which is the internal rtpbin session
    // output: NACK/RTX/FEC recovery (rtpsession) and jitter reordering
    // (rtpjitterbuffer) already happened INSIDE webrtcbin before the pad.
    // Field proof: the decode chain — fed from the SAME tee with NO jitter
    // buffer of its own — renders at a steady 60 fps with sink dropped=0;
    // the branch only ever sees the video SSRC (dropped_non_video=0, no
    // FlexFEC pt 98 / RTX ssrcs — they were consumed inside rtpbin); and the
    // depay consumes the stream packet-exactly (field: 26796 RTP → 2197 AUs
    // at ~12.2 pkt/AU, i.e. a correctly assembled stream). A recording branch
    // therefore needs NO reordering element of its own: the stream it taps is
    // already ordered and loss-recovered. What remains on a lossy link is a
    // NACK-unrecoverable hole (the session gives up on a packet) — the
    // depayloader then emits an AU with missing slices, h265parse sheds it,
    // and the ES has a gap until the next keyframe. That is handled below
    // with request-keyframe + wait-for-keyframe (resync from a PLI keyframe
    // instead of a broken GOP), NOT with a second jitter buffer — an extra
    // rtpjitterbuffer here can only hurt: mode=slave waits on an SR clock
    // that never reaches the tap, and mode=buffer+do-lost pushes lost events
    // into the depayloader (flush → garbage AU) and can mis-window the tail
    // of a stream whose timestamps restart on GFN renegotiation.
    let rtp_caps = make_element("capsfilter")?;
    let depay = make_element(depay_factory)?;
    let parse = make_element(parse_factory)?;
    // The branch must resync itself after a loss hole: rtph26xdepay with
    // request-keyframe=true pushes a force-key-unit event upstream on seq-gap
    // detection; it travels through the tee into webrtcbin's rtpbin session,
    // whose rtpsession turns it into a PLI to the server — the same path the
    // decode chain's depay uses (field-proven: the live path recovers after
    // PLI floods). wait-for-keyframe=true then holds the ES output until that
    // keyframe lands, so the file never contains a broken half-GOP: the
    // recording jumps cleanly from the last good frame to the next keyframe
    // (frames in the hole are honestly counted by the stop drop counter via
    // parse_in−parse_out). The decode chain sets wait-for-keyframe=false (it
    // must keep rendering); a recording has no live viewer and a clean GOP
    // structure is worth more than the ~300 ms between loss and resync.
    set_property_if_supported(&depay, "request-keyframe", true);
    set_property_if_supported(&depay, "wait-for-keyframe", true);
    // Force the parser to always parse (never passthrough) and re-emit the
    // codec config (SPS/PPS) before every keyframe: the ES file becomes
    // self-contained — every GOP starts with its own parameter sets, so the
    // offline remux (and any player seeking mid-file) never depends on a
    // config that lived in a lost packet. Same settings as the decode chain.
    set_property_if_supported(&parse, "disable-passthrough", true);
    set_property_if_supported(&parse, "config-interval", -1i32);
    // Force the ES file format the OFFLINE remux can auto-detect: the
    // parsers' default output against an unconstrained filesink is
    // length-prefixed avcC (h264parse/h265parse) or OBU-stream (av1parse).
    // A bare avcC file carries no start codes, so the offline filesrc →
    // parse chain cannot detect it ("No caps set" → not-negotiated).
    // Byte-stream H264/H265 self-identifies via 00 00 01 start codes; AV1
    // OBU-stream is what av1parse emits natively.
    let es_caps = make_element("capsfilter")?;
    let es_caps_string = match codec_upper.as_str() {
        "H264" => "video/x-h264,stream-format=(string)byte-stream,alignment=(string)au",
        "H265" | "HEVC" => "video/x-h265,stream-format=(string)byte-stream,alignment=(string)au",
        _ => "video/x-av1,stream-format=(string)obu-stream,alignment=(string)frame",
    };
    es_caps.set_property(
        "caps",
        es_caps_string
            .parse::<gst::Caps>()
            .map_err(|error| format!("Invalid pass-through ES caps: {error}"))?,
    );

    valve.set_property("drop", true);
    // Forward sticky events while buffers are gated, so qtmux never receives
    // a first buffer before its stream-start/caps/segment (same pattern as
    // the transcode branch).
    set_property_from_str_if_supported(&valve, "drop-mode", "forward-sticky-events");
    // Same leaky decoupling as the transcode branch: the branch can NEVER
    // back-pressure the live RTP flow — if the muxer lags, the queue drops
    // the oldest RTP packets (a few dropped packets are far better than a
    // stuttering stream). The limit must hold a WHOLE FRAME, not 4 packets:
    // this branch carries raw RTP (≈13 packets/frame for 1080p60 H265, more
    // for AV1), so max-size-buffers=4 held a third of a frame and the queue
    // dropped packets mid-frame constantly — the depayloader then could not
    // assemble a single complete frame (the pass-through wedge: 3 AUs in 10 s
    // in the harness; the 2.06 s video track in the field recording). 512
    // packets ≈ 0.5-1 s of video at GFN rates — enough to ride out a brief
    // muxer hiccup and assemble frames, while leaky=upstream still guarantees
    // the tap tee's push never blocks.
    queue.set_property_from_str("leaky", "upstream");
    // 2048 packets ≈ 6 s of 1080p60 H265 at ~345 pkt/s (512 packets was only
    // ~1.5 s, so a brief downstream stall — depay/parse/filesink hiccup, disk
    // or Defender scan on the fresh temp .es file — filled it and the leaky
    // head-drop killed whole mid-frame fragments, which cascaded into the
    // garbage-AU/parse-drop holes above). The branch still never back-poses
    // the live flow (leaky=upstream), but now only a >6 s stall sheds data.
    queue.set_property("max-size-buffers", 2048u32);
    queue.set_property("max-size-bytes", 0u32);
    queue.set_property("max-size-time", 0u64);

    // Count shed data: same sink/src probes as the transcode branch — every
    // RTP packet enters the queue and either leaves it or is dropped by
    // leaky=upstream (a >6 s downstream stall). After the drain at stop the
    // queue is empty, so `input − output` is the exact packet-drop count,
    // surfaced as droppedFrames (packets shed ≈ frames lost from the clip).
    let queue_input = Arc::new(AtomicU64::new(0));
    let queue_output = Arc::new(AtomicU64::new(0));
    {
        let input = queue_input.clone();
        let queue_sink = queue
            .static_pad("sink")
            .ok_or_else(|| "Pass-through queue has no sink pad.".to_owned())?;
        queue_sink.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            input.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
    }
    {
        let output = queue_output.clone();
        let queue_src = queue
            .static_pad("src")
            .ok_or_else(|| "Pass-through queue has no src pad.".to_owned())?;
        queue_src.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            output.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
    }
    // Normalize the RTP caps (media/encoding-name/clock-rate only) before the
    // depayloader, mirroring the game-audio branch's capsfilter.
    rtp_caps.set_property(
        "caps",
        format!(
            "application/x-rtp,media=(string)video,encoding-name=(string){rtp_encoding},clock-rate=(int)90000"
        )
        .parse::<gst::Caps>()
        .map_err(|error| format!("Invalid pass-through recording RTP caps: {error}"))?,
    );

    // The tail is a plain filesink writing the raw elementary stream into a
    // temp file. NO muxer lives in the live pipeline: muxing happens OFFLINE
    // at stop (`remux_pass_through_recording`) in a throwaway pipeline, so a
    // muxer stall can never freeze the recording branch (the field wedge: a
    // pass-through recording whose video AND audio branches both froze at the
    // qtmux input ~16 s in while the live stream stayed healthy). A filesink
    // always accepts — the branch cannot wedge. `async=false` + `sync=false`
    // keep the sink a synchronous non-prerolling element like the rest of
    // the branch (a prerolling sink would complete its deferred transition
    // later and re-preroll the whole pipeline).
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let es_ext = match codec_upper.as_str() {
        "H264" => "h264",
        "AV1" => "av1",
        _ => "h265",
    };
    let video_es_path = std::env::temp_dir()
        .join(format!("opennow-rec-{}-{stamp}.{es_ext}.es", std::process::id()));
    let video_filesink = make_element("filesink")?;
    video_filesink.set_property(
        "location",
        video_es_path
            .to_str()
            .ok_or_else(|| "Pass-through ES path is not valid UTF-8.".to_owned())?,
    );
    video_filesink.set_property("sync", false);
    video_filesink.set_property("async", false);

    // Chain order: valve → queue → capsfilter (RTP caps) → depayloader →
    // parser (normalizes the AUs for the offline remux) → ES-format
    // capsfilter (byte-stream/OBU) → filesink. NO jitter buffer: the tap is
    // post-rtpbin (ordered + NACK/RTX/FEC-recovered), see the branch header.
    let elements: Vec<&gst::Element> = vec![
        &valve,
        &queue,
        &rtp_caps,
        &depay,
        &parse,
        &es_caps,
        &video_filesink,
    ];
    for element in &elements {
        pipeline.add(*element).map_err(|error| {
            format!("Failed to add recording pass-through branch element: {error}")
        })?;
    }
    for pair in elements.windows(2) {
        pair[0]
            .link(pair[1])
            .map_err(|error| format!("Failed to link recording pass-through branch: {error:?}"))?;
    }
    // Sync the branch to the pipeline state BEFORE linking it into the RTP
    // tap tee (the pipeline is PLAYING by the time this runs — an unsynced
    // element would fail the tee's first push and kill the whole RTP flow).
    for element in &elements {
        element.sync_state_with_parent().map_err(|error| {
            format!("Failed to sync recording pass-through branch element state: {error}")
        })?;
    }

    // EOS flag on the filesink: `stop()` waits for it before the offline
    // remux reads the ES file, so the file is always fully flushed/closed.
    let filesink_eos = Arc::new(AtomicBool::new(false));
    let eos_flag = filesink_eos.clone();
    let filesink_sink_pad = video_filesink
        .static_pad("sink")
        .ok_or_else(|| "Pass-through filesink has no sink pad.".to_owned())?;
    filesink_sink_pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
        if let Some(event) = info.event() {
            if event.type_() == gst::EventType::Eos {
                eos_flag.store(true, Ordering::SeqCst);
            }
        }
        gst::PadProbeReturn::Ok
    });

    // Link the branch into the RTP tap tee. The valve starts closed
    // (drop=true), so the filesink writes nothing until a recording is
    // active, while the decode/present chain keeps flowing untouched.
    let first_sink = valve
        .static_pad("sink")
        .ok_or_else(|| "Pass-through recording valve has no sink pad.".to_owned())?;
    let branch_pad = rtp_tee
        .request_pad_simple("src_%u")
        .ok_or_else(|| "Failed to request a recording pad from the RTP tap tee.".to_owned())?;
    if let Err(error) = branch_pad.link(&first_sink) {
        let _ = rtp_tee.release_request_pad(&branch_pad);
        return Err(format!(
            "Failed to link RTP tap into the pass-through recording branch: {error:?}"
        ));
    }

    // PASS-THROUGH flow diagnostics: zero-cost buffer counters at every hop
    // of the live branch. A field session whose video ES stays 0 bytes while
    // the live path renders gets pinned to the exact hop that stopped
    // flowing by `[RecBranchFlow]` log lines (valve_in=tee→branch, queue_in,
    // depay_in, parse_in, file_in=ES bytes written). The parse sink CAPS
    // event shows what the depayloader negotiated; the valve's first RTP
    // buffers expose any non-video traffic (e.g. FlexFEC, pt 98) riding the
    // shared tap tee.
    let pass_flow_diag = PassThroughFlowDiag::new(
        format!("codec={rtp_encoding}"),
        event_sender.clone(),
    );
    {
        let diag = pass_flow_diag.clone();
        let valve_sink = valve
            .static_pad("sink")
            .ok_or_else(|| "Pass-through valve has no sink pad.".to_owned())?;
        // Learn the negotiated video RTP payload type from the first CAPS
        // event at the valve sink pad. The tap sits at the webrtcbin video
        // SRC pad — the internal rtpbin session output — so FlexFEC (pt 98)
        // and RTX retransmissions are already consumed inside rtpbin before
        // the branch sees a packet (field proof: only the video SSRC ever
        // reaches the branch, dropped_non_video=0 across sessions). The
        // payload-type filter below is kept as a DEFENSIVE net for any
        // future webrtcbin config that exposes a second SSRC on the session
        // pad — it is expected to drop nothing today. Caps events always
        // precede buffers (sticky), so video_pt is known before the first
        // buffer arrives.
        let pt_slot = diag.video_pt.clone();
        valve_sink.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
            if let Some(event) = info.event() {
                if let gst::EventView::Caps(caps) = event.view() {
                    let caps = caps.caps();
                    if let Some(structure) = caps.structure(0) {
                        if let Ok(payload) = structure.get::<i32>("payload") {
                            if payload > 0 && payload < 128 {
                                pt_slot.store(payload as u32, Ordering::Relaxed);
                            }
                        }
                    }
                }
            }
            gst::PadProbeReturn::Ok
        });
        valve_sink.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
            let count = diag
                .valve_in
                .fetch_add(1, Ordering::Relaxed);
            // Capture the first few RTP packets (12-byte header present on the
            // raw tap): pt / ssrc / marker / len reveal any non-video flow.
            if count < 5 {
                if let Some(buffer) = info.buffer() {
                    let map = buffer.map_readable();
                    let bytes = map.as_deref().unwrap_or(&[]);
                    let pt = bytes.get(1).map(|b| b & 0x7f).unwrap_or(0);
                    let ssrc = if bytes.len() >= 12 {
                        u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]])
                    } else {
                        0
                    };
                    let marker = bytes.get(1).map(|b| b & 0x80 != 0).unwrap_or(false);
                    if let Ok(mut slot) = diag.valve_first_rtp.lock() {
                        if slot.len() < 5 {
                            slot.push(format!(
                                "pt={pt} ssrc={ssrc} m={marker} len={}",
                                bytes.len()
                            ));
                        }
                    }
                }
            }
            if count == 0 {
                // First buffer ever to reach the branch (record armed, valve
                // closed): log the zeroed counters once so the log shows the
                // branch is fed.
                diag.log("armed");
            }
            // Defensive payload-type gate before the depayloader: only the
            // negotiated video payload type belongs in the recorded ES. On
            // today's tap (post-rtpbin) this drops nothing (the session pad
            // carries only the video SSRC); it guards against a future
            // webrtcbin exposing RTX/FEC ssrcs on the session pad. If the pt
            // filter is still unknown (0), pass everything — sticky caps
            // events arrive before buffers, so this only happens during the
            // brief attach window.
            let video_pt = diag.video_pt.load(Ordering::Relaxed);
            if video_pt != 0 {
                if let Some(buffer) = info.buffer() {
                    let map = buffer.map_readable();
                    let bytes = map.as_deref().unwrap_or(&[]);
                    let pt = bytes.get(1).map(|b| b & 0x7f).unwrap_or(0) as u32;
                    if pt != video_pt {
                        diag.dropped_non_video.fetch_add(1, Ordering::Relaxed);
                        return gst::PadProbeReturn::Drop;
                    }
                }
            }
            gst::PadProbeReturn::Ok
        });
    }
    {
        let counter = pass_flow_diag.queue_in.clone();
        let queue_sink = queue
            .static_pad("sink")
            .ok_or_else(|| "Pass-through queue has no sink pad.".to_owned())?;
        queue_sink.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            counter.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
    }
    {
        let counter = pass_flow_diag.depay_in.clone();
        let depay_sink = depay
            .static_pad("sink")
            .ok_or_else(|| "Pass-through depay has no sink pad.".to_owned())?;
        depay_sink.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            counter.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
    }
    {
        let counter = pass_flow_diag.parse_in.clone();
        let caps_slot = pass_flow_diag.parse_first_caps.clone();
        let parse_sink = parse
            .static_pad("sink")
            .ok_or_else(|| "Pass-through parse has no sink pad.".to_owned())?;
        parse_sink.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            counter.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
        parse_sink.add_probe(
            gst::PadProbeType::EVENT_DOWNSTREAM,
            move |_pad, info| {
                if let Some(event) = info.event() {
                    if let gst::EventView::Caps(caps) = event.view() {
                        if let Ok(mut slot) = caps_slot.lock() {
                            if slot.is_none() {
                                *slot = Some(caps.caps().to_string());
                            }
                        }
                    }
                }
                gst::PadProbeReturn::Ok
            },
        );
    }
    {
        let counter = pass_flow_diag.parse_out.clone();
        let parse_src = parse
            .static_pad("src")
            .ok_or_else(|| "Pass-through parse has no src pad.".to_owned())?;
        parse_src.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            counter.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
    }
    {
        let counter = pass_flow_diag.file_in.clone();
        let filesink_sink = video_filesink
            .static_pad("sink")
            .ok_or_else(|| "Pass-through filesink has no sink pad.".to_owned())?;
        filesink_sink.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            counter.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
    }

    // No live IDR gate: the ES file captures EVERYTHING the branch receives,
    // and the gate (drop-until-first-keyframe) is applied at the OFFLINE
    // remux where it cannot stall anything. This also means a server keyframe
    // that lands late (GFN H265 GOP) no longer black-holes the head of the
    // file — the pre-keyframe data is written and the remux simply starts at
    // the first keyframe that exists.
    let idr_gate = Arc::new(AtomicBool::new(true));

    // Inactive until the first `start()` (the transcode chunk probe pattern).
    let active = Arc::new(AtomicBool::new(false));

    send_log(
        &event_sender,
        "info",
        format!(
            "Attached native PASS-THROUGH recording branch (post-rtpbin recovered RTP → {depay_factory} → {parse_factory} → filesink ES file; zero re-encode — no decode/encode/GPU cost to the live stream; NACK/RTX/FEC recovery inherited from the rtpbin session; file = received {rtp_encoding} bitstream, muxed offline at stop)."
        ),
    );

    Ok(GstreamerRecordingState {
        valve,
        queue,
        encoder: depay.clone(),
        encoder_factory: format!("{depay_factory}→{parse_factory}"),
        h264_parse: parse,
        video_convert: rtp_caps.clone(),
        video_declare_caps: rtp_caps.clone(),
        video_encode_caps: rtp_caps.clone(),
        range_lut_probe: Arc::new(Mutex::new(None)),
        range_convert: rtp_caps.clone(),
        video_download: None,
        muxer: None,
        swallow: None,
        video_es_path: Some(video_es_path),
        audio_es_path: None,
        es_caps: Some(es_caps),
        video_filesink: Some(video_filesink),
        audio_filesink: None,        filesink_eos,
        audio_filesink_eos: Arc::new(AtomicBool::new(false)),
        eos_seen: Arc::new(AtomicBool::new(false)),
        active,
        rec_frames_first_pts: Arc::new(AtomicU64::new(0)),
        rec_frames_last_pts: Arc::new(AtomicU64::new(0)),
        rec_frames_count: Arc::new(AtomicU64::new(0)),
        queue_input,
        queue_output,
        dropped_frames: Arc::new(AtomicU64::new(0)),

        mode: RecordingMode::PassThrough,
        codec: rtp_encoding.to_owned(),
        fps,
        record_bitrate_kbps: Arc::new(AtomicU32::new(RECORD_BITRATE_DEFAULT_KBPS)),
        idr_gate,
        video_tap_tee: rtp_tee.clone(),
        audio_rtp_tee: None,
        audio_valve: None,
        audio_queue: None,
        audio_capsfilter: None,
        audio_depayloader: None,
        audio_opusdec: None,
        audio_audioconvert: None,
        audio_aac_encoder: None,
        audio_aac_parse: None,
        audio_adts_caps: None,
        audio_branch_built: Arc::new(AtomicBool::new(false)),
        spent: Arc::new(AtomicBool::new(false)),
        pass_flow_diag: Some(pass_flow_diag),
        event_sender,
    })
}

/// Remux the pass-through temp ES files (video + optional game-audio AAC)
/// into a standard seekable MP4 (faststart) in a THROWAWAY pipeline. The live
/// pipeline contains NO muxer for pass-through recordings — a live qtmux
/// froze both recording branches ~16 s in while the stream stayed healthy
/// (the field wedge) — so all muxing happens here, offline, at stop, where a
/// stall cannot touch the stream. Returns the output file size in bytes
/// (0 = nothing was written, e.g. both ES files were empty).
/// Deliver a finalized recording MP4 to Electron as fixed-size base64
/// `recording-chunk` events, in file order. The chunking is load-bearing:
/// Electron parses and base64-decodes each stdout line synchronously in its
/// main process, so a whole-file single line (a 70 MB recording is ~94 MB of
/// base64) freezes the UI ("Electron is not responding") and can crash the
/// app before the .tmp is ever written. Fixed-size chunks keep every line
/// ~1.4 MB; Electron appends them in arrival order.
fn send_recording_chunks(sender: &Sender<Event>, bytes: &[u8]) {
    for chunk in bytes.chunks(RECORDING_CHUNK_BYTES) {
        let _ = sender.send(Event::RecordingChunk {
            chunk_base64: BASE64_STANDARD.encode(chunk),
        });
    }
}

/// Stream a finalized MP4 to Electron as fixed-size `recording-chunk` events
/// WITHOUT loading the whole file into memory (the old finalize read the
/// entire file with `std::fs::read` — a multi-GB long recording peaked at
/// ~4× the file size across the Rust base64 + Electron buffers and could OOM
/// an 8 GB device exactly at the moment the user clicked stop). The file is
/// opened once and read in RECORDING_CHUNK_BYTES windows, each window encoded
/// and sent immediately, so peak memory is one chunk + its base64. Returns
/// the number of bytes streamed.
fn stream_recording_file_as_chunks(
    sender: &Sender<Event>,
    path: &std::path::Path,
) -> Result<u64, String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|error| {
        format!("Failed to open finalized recording for chunk streaming: {error}")
    })?;
    let mut window = vec![0u8; RECORDING_CHUNK_BYTES];
    let mut total: u64 = 0;
    loop {
        let read = file.read(&mut window).map_err(|error| {
            format!("Failed to read finalized recording for chunk streaming: {error}")
        })?;
        if read == 0 {
            break;
        }
        send_recording_chunks(sender, &window[..read]);
        total += read as u64;
    }
    Ok(total)
}

/// Upper bound on the MP4 head (the moov/faststart index) read for colr
/// tagging. Even long recordings keep the index at a few KB to a couple MB
/// (the stco chunk table dominates); if a moov ever exceeds this, the box is
/// left untagged rather than risk partial-box parsing (the SPS VUI still
/// carries the range info).
const MP4_COLR_HEAD_LIMIT: usize = 16 * 1024 * 1024;

/// A parsed MP4 box header (size field location, content offset, size, type).
struct Mp4BoxInfo {
    /// Offset of the box's 32-bit size field (or its 64-bit largesize field
    /// at `size_offset + 8` when `largesize`).
    size_offset: usize,
    largesize: bool,
    /// Offset of the first content byte (after the full header).
    content_offset: usize,
    /// Full box size including the header.
    size: usize,
    fourcc: [u8; 4],
}

/// Walk the child boxes of a parent whose content spans `[start, end)` and
/// return their headers. Stops safely at `end` or the data end; boxes that
/// would overrun are skipped.
fn mp4_box_children(data: &[u8], start: usize, end: usize) -> Vec<Mp4BoxInfo> {
    let mut out = Vec::new();
    let mut pos = start;
    while pos + 8 <= end && pos + 8 <= data.len() {
        let size32 =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        let fourcc = [data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]];
        let (largesize, header_size, size) = if size32 == 1 {
            if pos + 16 > end || pos + 16 > data.len() {
                break;
            }
            let size64 = u64::from_be_bytes([
                data[pos + 8],
                data[pos + 9],
                data[pos + 10],
                data[pos + 11],
                data[pos + 12],
                data[pos + 13],
                data[pos + 14],
                data[pos + 15],
            ]);
            (true, 16, usize::try_from(size64).unwrap_or(usize::MAX))
        } else if size32 == 0 {
            // size 0 = extends to the end of the containing box.
            (false, 8, end.saturating_sub(pos))
        } else {
            (false, 8, size32 as usize)
        };
        if size < header_size {
            break;
        }
        out.push(Mp4BoxInfo {
            size_offset: pos,
            largesize,
            content_offset: pos + header_size,
            size,
            fourcc,
        });
        pos = pos.saturating_add(size);
    }
    out
}

fn mp4_find_child<'a>(children: &'a [Mp4BoxInfo], fourcc: &[u8; 4]) -> Option<&'a Mp4BoxInfo> {
    children.iter().find(|child| &child.fourcc == fourcc)
}

/// Add `delta` to a box's stored size (handles the 64-bit largesize form).
fn bump_mp4_box_size(data: &mut [u8], info: &Mp4BoxInfo, delta: usize) {
    if info.largesize {
        let field = info.size_offset + 8;
        let value =
            u64::from_be_bytes(data[field..field + 8].try_into().expect("largesize field"))
                + delta as u64;
        data[field..field + 8].copy_from_slice(&value.to_be_bytes());
    } else {
        let value =
            u32::from_be_bytes(data[info.size_offset..info.size_offset + 4].try_into().expect("size field"))
                + delta as u32;
        data[info.size_offset..info.size_offset + 4].copy_from_slice(&value.to_be_bytes());
    }
}

/// Bump every chunk offset in an stco/co64 table by `delta` (the insertion
/// shifted mdat's absolute position).
fn bump_mp4_chunk_offsets(data: &mut [u8], info: &Mp4BoxInfo, delta: usize) {
    let is_co64 = &info.fourcc == b"co64";
    let entry_size = if is_co64 { 8 } else { 4 };
    let payload = &mut data[info.content_offset..];
    if payload.len() < 4 {
        return;
    }
    let count = u32::from_be_bytes(payload[0..4].try_into().expect("entry count")) as usize;
    let table = &mut payload[4..];
    let entries = count.min(table.len() / entry_size);
    for i in 0..entries {
        let at = i * entry_size;
        if is_co64 {
            let value = u64::from_be_bytes(table[at..at + 8].try_into().expect("co64 entry"))
                + delta as u64;
            table[at..at + 8].copy_from_slice(&value.to_be_bytes());
        } else {
            let value =
                u32::from_be_bytes(table[at..at + 4].try_into().expect("stco entry")) + delta as u32;
            table[at..at + 4].copy_from_slice(&value.to_be_bytes());
        }
    }
}

/// Inject a `colr` box (nclx: BT.709 primaries/transfer/matrix,
/// full_range_flag=1) into the FIRST video sample entry of the MP4 held in
/// `data`, bumping every affected ancestor size and chunk offset so the file
/// stays structurally valid. Returns 1 when a box was injected, 0 when the
/// sample entry already carries a colr box or the structure could not be
/// parsed (leaves the file untouched).
///
/// The GFN stream is FULL-RANGE BT.709 PC video (SPS VUI
/// video_full_range_flag=1); tagging the container the same way lets players
/// that read colr render the received full-range bitstream with correct
/// colours — the pass-through recording keeps the exact server bitstream (no
/// re-encode) and the colour fix is pure metadata.
fn inject_mp4_colr_box(data: &mut Vec<u8>) -> usize {
    let top = mp4_box_children(data, 0, data.len());
    let Some(moov) = mp4_find_child(&top, b"moov") else {
        return 0;
    };
    // Never operate on a partially-loaded moov (a recording whose index is
    // larger than the head read) — the SPS VUI still carries the range info.
    if moov.size > data.len() {
        return 0;
    }
    let moov_end = moov.content_offset + moov.size;
    let moov_children = mp4_box_children(data, moov.content_offset, moov_end);
    let Some(trak) = mp4_find_child(&moov_children, b"trak") else {
        return 0;
    };
    let trak_children =
        mp4_box_children(data, trak.content_offset, trak.content_offset + trak.size);
    let Some(mdia) = mp4_find_child(&trak_children, b"mdia") else {
        return 0;
    };
    let mdia_children =
        mp4_box_children(data, mdia.content_offset, mdia.content_offset + mdia.size);
    let Some(minf) = mp4_find_child(&mdia_children, b"minf") else {
        return 0;
    };
    let minf_children =
        mp4_box_children(data, minf.content_offset, minf.content_offset + minf.size);
    let Some(stbl) = mp4_find_child(&minf_children, b"stbl") else {
        return 0;
    };
    let stbl_end = stbl.content_offset + stbl.size;
    let stbl_children = mp4_box_children(data, stbl.content_offset, stbl_end);
    let Some(stsd) = mp4_find_child(&stbl_children, b"stsd") else {
        return 0;
    };
    let stsd_end = stsd.content_offset + stsd.size;
    // stsd payload: version/flags (4) + entry_count (4), then sample entries.
    let stsd_children = mp4_box_children(data, stsd.content_offset + 8, stsd_end);
    let Some(entry) = stsd_children.first() else {
        return 0;
    };
    // Only video sample entries; audio tracks (mp4a/Opus) are skipped.
    if !matches!(
        &entry.fourcc,
        b"avc1" | b"hvc1" | b"hev1" | b"av01" | b"vp09"
    ) {
        return 0;
    }
    // VisualSampleEntry: 8 bytes of header (6 reserved + 2 data_reference)
    // after the box header, then the child boxes (avcC/hvcC/av1C/vpcC...).
    let children_start = entry.content_offset + 8;
    let entry_children = mp4_box_children(data, children_start, entry.content_offset + entry.size);
    if mp4_find_child(&entry_children, b"colr").is_some() {
        return 0;
    }
    // colr box, 20 bytes: nclx, primaries=transfer=matrix=BT.709 (1),
    // full_range_flag=1, reserved.
    let colr: [u8; 20] = [
        0x00, 0x00, 0x00, 0x14, b'c', b'o', b'l', b'r', b'n', b'c', b'l', b'x', 0x00, 0x01, 0x00,
        0x01, 0x00, 0x01, 0x01, 0x00,
    ];
    let delta = colr.len();
    data.splice(children_start..children_start, colr);
    // All size fields sit BEFORE the insertion point, so the pre-splice box
    // infos stay valid for the bumps.
    for info in [entry, stsd, stbl, minf, mdia, trak, moov] {
        bump_mp4_box_size(data, info, delta);
    }
    // When moov precedes mdat (faststart — always the case here), the
    // insertion shifted mdat's absolute position, so the stco/co64 chunk
    // offsets must be bumped to keep the file seekable.
    let mdat_before_moov = mp4_find_child(&top, b"mdat")
        .map(|mdat| mdat.content_offset < moov.content_offset)
        .unwrap_or(false);
    if !mdat_before_moov {
        for stbl_box in &stbl_children {
            if stbl_box.fourcc == *b"stco" || stbl_box.fourcc == *b"co64" {
                bump_mp4_chunk_offsets(data, stbl_box, delta);
            }
        }
    }
    1
}

/// Stream a finalized PassThrough recording to Electron as fixed-size
/// `recording-chunk` events, tagging the colour metadata (colr nclx BT.709
/// full-range) into the MP4 head on the way. Memory stays bounded: only the
/// head (the moov index) is held, and the body is streamed window-by-window
/// from the file — never the whole file in RAM.
fn stream_recording_with_colr_tag(
    sender: &Sender<Event>,
    path: &std::path::Path,
    event_sender: &Option<Sender<Event>>,
) -> Result<u64, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).map_err(|error| {
        format!("Failed to open finalized recording for colour tagging: {error}")
    })?;
    let file_len = file
        .metadata()
        .map_err(|error| format!("Failed to stat finalized recording: {error}"))?
        .len() as usize;
    if file_len == 0 {
        return Ok(0);
    }
    let head_len = file_len.min(MP4_COLR_HEAD_LIMIT);
    let mut head = vec![0u8; head_len];
    file.read_exact(&mut head)
        .map_err(|error| format!("Failed to read finalized recording head: {error}"))?;
    let injected = inject_mp4_colr_box(&mut head);
    send_log(
        event_sender,
        "info",
        if injected > 0 {
            "Recording colour metadata: injected colr (nclx BT.709 full-range) into the MP4 — players render the received full-range bitstream with correct colours, no re-encode.".to_owned()
        } else {
            "Recording colour metadata: colr already present or moov not fully in the head — the SPS VUI still carries the range info.".to_owned()
        },
    );
    send_recording_chunks(sender, &head);
    let mut total = head.len() as u64;
    file.seek(SeekFrom::Start(head_len as u64))
        .map_err(|error| format!("Failed to seek finalized recording for streaming: {error}"))?;
    let mut window = vec![0u8; RECORDING_CHUNK_BYTES];
    loop {
        let read = file.read(&mut window).map_err(|error| {
            format!("Failed to read finalized recording for streaming: {error}")
        })?;
        if read == 0 {
            break;
        }
        send_recording_chunks(sender, &window[..read]);
        total += read as u64;
    }
    Ok(total)
}

fn remux_pass_through_recording(
    video_es: Option<&std::path::Path>,
    audio_es: Option<&std::path::Path>,
    out_path: &std::path::Path,
    codec: &str,
    fps: u32,
    event_sender: &Option<Sender<Event>>,
) -> Result<u64, String> {
    let Some(video_es) = video_es else {
        return Err("Pass-through remux: no video elementary stream file.".to_owned());
    };
    let codec_upper = codec.to_ascii_uppercase();
    let parse_factory = match codec_upper.as_str() {
        "H264" => "h264parse",
        "H265" | "HEVC" => "h265parse",
        "AV1" => "av1parse",
        other => return Err(format!("Pass-through remux: unsupported codec {other}.")),
    };

    let pipeline = gst::Pipeline::new();
    let video_src = make_element("filesrc")?;
    video_src.set_property(
        "location",
        video_es
            .to_str()
            .ok_or_else(|| "Pass-through remux video ES path is not valid UTF-8.".to_owned())?,
    );
    let parse = make_element(parse_factory)?;
    // PLAIN remux — no decode, no re-encode, no colour transform. The file is
    // the exact received server bitstream at the exact received quality,
    // zero GPU/CPU at finalize (the official GeForce Now approach: "just
    // write what we receive"). Colour is carried by METADATA instead of a
    // data transform: the GFN stream is FULL-RANGE BT.709 (SPS VUI
    // video_full_range_flag=1), and after the remux the MP4's video sample
    // entry is given a `colr` box (nclx, primaries/transfer/matrix=BT.709,
    // full_range_flag=1) by `stream_recording_with_colr_tag` — players that
    // read colr/VUI render the recording with the same colours as the live
    // stream, without the old decode→LUT→re-encode generation. The old
    // limited-range universal-H.264 path is still one env var away
    // (OPENNOW_RECORDING_MODE=encoded).
    let muxer = make_element("qtmux")?;
    muxer.set_property("faststart", true);
    muxer.set_property("fragment-duration", 0u32);
    muxer.set_property("streamable", false);
    let out_sink = make_element("filesink")?;
    out_sink.set_property(
        "location",
        out_path
            .to_str()
            .ok_or_else(|| "Pass-through remux output path is not valid UTF-8.".to_owned())?,
    );
    out_sink.set_property("sync", false);
    out_sink.set_property("async", false);

    // Chain order: filesrc → parse → qtmux → filesink (plain remux, no
    // decode/re-encode — see above).
    let mut video_chain: Vec<&gst::Element> = vec![&video_src, &parse];
    for element in &video_chain {
        pipeline
            .add(*element)
            .map_err(|error| format!("Failed to add pass-through remux element: {error}"))?;
    }
    pipeline
        .add(&muxer)
        .map_err(|error| format!("Failed to add pass-through remux muxer: {error}"))?;
    pipeline
        .add(&out_sink)
        .map_err(|error| format!("Failed to add pass-through remux sink: {error}"))?;
    for pair in video_chain.windows(2) {
        pair[0]
            .link(pair[1])
            .map_err(|error| format!("Failed to link pass-through remux video chain: {error:?}"))?;
    }
    // qtmux sink pads are REQUEST pads (video_%u / audio_%u) — request and
    // link explicitly, then link the muxer src to the output filesink.
    let video_muxer_pad = muxer
        .request_pad_simple("video_%u")
        .ok_or_else(|| "Pass-through remux: qtmux refused a video sink pad.".to_owned())?;
    let parse_src = parse
        .static_pad("src")
        .ok_or_else(|| "Pass-through remux: codec parser has no src pad.".to_owned())?;
    if let Err(error) = parse_src.link(&video_muxer_pad) {
        let _ = muxer.release_request_pad(&video_muxer_pad);
        return Err(format!("Failed to link remux parser into qtmux: {error:?}"));
    }
    let muxer_out = muxer
        .static_pad("src")
        .ok_or_else(|| "Pass-through remux: qtmux has no src pad.".to_owned())?;
    let out_sink_sink = out_sink
        .static_pad("sink")
        .ok_or_else(|| "Pass-through remux: output filesink has no sink pad.".to_owned())?;
    muxer_out
        .link(&out_sink_sink)
        .map_err(|error| format!("Failed to link remux muxer to output: {error:?}"))?;

    // Reconstruct the VIDEO timestamps the raw ES file cannot carry: files
    // store no PTS, and qtmux rejects buffers with none ("Buffer has no PTS").
    // GFN's low-latency encoders emit no B-frames (has_b_frames=0 on every
    // real H264 AND H265 recording), so decode order == presentation order and
    // a monotonic PTS ladder at the negotiated fps is exact. Attached to the
    // codec parser's src pad (right before qtmux).
    let frame_duration_ns = (1_000_000_000u64 / (fps.max(1) as u64)).max(1);
    let video_counter = Arc::new(std::sync::Mutex::new(0u64));
    {
        let counter = video_counter.clone();
        let parse_src_pad = parse
            .static_pad("src")
            .ok_or_else(|| "Pass-through remux: codec parser has no src pad.".to_owned())?;
        parse_src_pad.add_probe(
            gst::PadProbeType::BUFFER,
            move |_pad: &gst::Pad, info: &mut gst::PadProbeInfo| {
                if let Some(buffer) = info.buffer_mut() {
                    // gstreamer-rs 0.25 keeps the mutating setters on the
                    // borrowed BufferRef; Buffer has no DerefMut, so get the
                    // mutable view with make_mut() (copy-on-write).
                    let buffer = buffer.make_mut();
                    if buffer.pts().is_none() {
                        let mut guard =
                            counter.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                        let pts = gst::ClockTime::from_nseconds(*guard * frame_duration_ns);
                        *guard += 1;
                        let duration = gst::ClockTime::from_nseconds(frame_duration_ns);
                        buffer.set_pts(pts);
                        buffer.set_dts(pts);
                        buffer.set_duration(duration);
                    }
                }
                gst::PadProbeReturn::Ok
            },
        );
    }

    // Optional game-audio AAC track (ADTS elementary stream from the live
    // branch's AAC encoder). Only linked when the file actually has data.
    if let Some(audio_es) = audio_es {
        if audio_es.exists()
            && std::fs::metadata(audio_es)
                .map(|metadata| metadata.len())
                .unwrap_or(0)
                > 0
        {
            let audio_src = make_element("filesrc")?;
            audio_src.set_property(
                "location",
                audio_es.to_str().ok_or_else(|| {
                    "Pass-through remux audio ES path is not valid UTF-8.".to_owned()
                })?,
            );
            let aac_parse = make_element("aacparse")?;
            pipeline
                .add_many(&[&audio_src, &aac_parse])
                .map_err(|error| format!("Failed to add pass-through remux audio: {error}"))?;
            audio_src
                .link(&aac_parse)
                .map_err(|error| format!("Failed to link remux audio source: {error:?}"))?;
            // Same timestamp reconstruction for AUDIO: each AAC frame is
            // exactly 1024 samples, so a monotonic PTS ladder at 1024/rate is
            // exact (rate read from the aacparse caps, e.g. 48000).
            let audio_counter = Arc::new(std::sync::Mutex::new(0u64));
            {
                let counter = audio_counter.clone();
                let aac_src_pad = aac_parse
                    .static_pad("src")
                    .ok_or_else(|| "Pass-through remux: aacparse has no src pad.".to_owned())?;
                aac_src_pad.add_probe(
                    gst::PadProbeType::BUFFER,
                    move |pad: &gst::Pad, info: &mut gst::PadProbeInfo| {
                        if let Some(buffer) = info.buffer_mut() {
                            let buffer = buffer.make_mut();
                            if buffer.pts().is_none() {
                                // Opus is always 48 kHz; the AAC encoder keeps
                                // the decoded rate, so 48000 is the norm. Read
                                // it from the caps when present (the borrowed
                                // StructureRef is copied out to an owned i32
                                // before the Caps temporary drops).
                                let rate = match pad.current_caps() {
                                    Some(caps) => match caps.structure(0) {
                                        Some(structure) => structure
                                            .get::<i32>("rate")
                                            .unwrap_or(48_000)
                                            .max(1) as u64,
                                        None => 48_000,
                                    },
                                    None => 48_000,
                                };
                                let frame_ns = 1_024u64 * 1_000_000_000u64 / rate;
                                let mut guard = counter
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                                let pts = gst::ClockTime::from_nseconds(*guard * frame_ns);
                                *guard += 1;
                                buffer.set_pts(pts);
                                buffer.set_dts(pts);
                                buffer.set_duration(gst::ClockTime::from_nseconds(frame_ns));
                            }
                        }
                        gst::PadProbeReturn::Ok
                    },
                );
            }
            let audio_muxer_pad = muxer
                .request_pad_simple("audio_%u")
                .ok_or_else(|| "Pass-through remux: qtmux refused an audio sink pad.".to_owned())?;
            let aac_src = aac_parse
                .static_pad("src")
                .ok_or_else(|| "Pass-through remux: aacparse has no src pad.".to_owned())?;
            if let Err(error) = aac_src.link(&audio_muxer_pad) {
                let _ = muxer.release_request_pad(&audio_muxer_pad);
                return Err(format!("Failed to link remux AAC into qtmux: {error:?}"));
            }
        }
    }

    if let Err(error) = pipeline.set_state(gst::State::Playing) {
        let _ = pipeline.set_state(gst::State::Null);
        return Err(format!("Pass-through remux failed to start: {error:?}"));
    }
    let Some(bus) = pipeline.bus() else {
        let _ = pipeline.set_state(gst::State::Null);
        return Err("Pass-through remux pipeline has no bus.".to_owned());
    };
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(RECORDING_REMUX_TIMEOUT_MS);
    let mut finished = false;
    while std::time::Instant::now() < deadline {
        let Some(message) = bus.timed_pop_filtered(
            gst::ClockTime::from_mseconds(100),
            &[
                gst::MessageType::Eos,
                gst::MessageType::Error,
                gst::MessageType::Warning,
            ],
        ) else {
            continue;
        };
        match message.view() {
            gst::MessageView::Eos(_) => {
                finished = true;
                break;
            }
            gst::MessageView::Error(error) => {
                let _ = pipeline.set_state(gst::State::Null);
                return Err(format!(
                    "Pass-through remux failed: {} (debug: {:?})",
                    error.error(),
                    error.debug()
                ));
            }
            gst::MessageView::Warning(warning) => send_log(
                event_sender,
                "warn",
                format!(
                    "Pass-through remux warning: {} (debug: {:?})",
                    warning.error(),
                    warning.debug()
                ),
            ),
            _ => {}
        }
    }
    let _ = pipeline.set_state(gst::State::Null);
    if !finished {
        return Err("Pass-through remux timed out while muxing the recording.".to_owned());
    }
    let size = std::fs::metadata(out_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    send_log(
        event_sender,
        "info",
        format!(
            "Pass-through remux finished: {size} bytes (plain remux of the received {codec_upper} bitstream — no decode/re-encode; colour metadata is tagged at finalize)."
        ),
    );
    Ok(size)
}

/// Derive the per-frame PTS interval (ns) the ENCODED-mode remux should use
/// for its video PTS ladder from the MEASURED decoded-frame cadence of the
/// recording window (first/last PTS + frame count, captured by the branch's
/// valve-src probe). The ES file carries no timestamps, so the remux builds a
/// monotonic ladder; using the NEGOTIATED fps is exact only while GFN
/// delivers at the negotiated rate (it fills the cadence with duplicated
/// frames when the game renders slower — distinct RTP timestamps, so the
/// measured cadence stays at the negotiated rate). If GFN ever delivers fewer
/// unique frames than negotiated (rate-adaptive congestion), a fixed
/// negotiated ladder would stretch the video (30 fps of content stamped at
/// 60 fps → half the real duration, audio stranded at the tail): measuring
/// the actual cadence keeps the recording real-time in both cases. Pure so
/// it's unit-testable.
fn measured_recording_frame_duration_ns(
    count: u64,
    first_pts_ns: u64,
    last_pts_ns: u64,
    negotiated_fps: u32,
) -> u64 {
    // Preserve the old remux's defensive fallback: a degenerate 0 (the
    // fps=0 state bug) means 60, never 1 frame/second.
    let fallback = if negotiated_fps == 0 { 60 } else { negotiated_fps.max(1) };
    let fallback_interval = (1_000_000_000u64 / u64::from(fallback)).max(1);
    if count >= 2 && last_pts_ns > first_pts_ns {
        let measured = (last_pts_ns - first_pts_ns) / (count - 1);
        // Plausibility band around the negotiated interval: a measured cadence
        // ABOVE 2× the negotiated rate is impossible for a real delivery (GFN
        // cannot exceed the negotiated rate) and means a PTS reset / garbage
        // stream — EXCEPT the same-timestamp frame-duplication pattern (avg =
        // exactly half the negotiated interval), where the frame COUNT still
        // matches the delivery rate, so the negotiated ladder is the right
        // fallback there too. A cadence SLOWER than 1/4 of the negotiated
        // rate (or below 1 fps) is likewise not trusted. Everything inside the
        // band is real media time and is used as-is (e.g. 33 ms for a 30 fps
        // delivery in a 60 fps stream → real-time recording, the fix).
        let too_fast = measured <= fallback_interval / 2;
        let too_slow = measured > fallback_interval.saturating_mul(4)
            || measured > 1_000_000_000;
        if !too_fast && !too_slow {
            return measured;
        }
    }
    fallback_interval
}

/// Remux the ENCODED recording's temp ES files (H.264 video from the
/// branch's encoder + optional game-audio AAC) into a standard seekable MP4
/// (faststart) in a THROWAWAY pipeline. The video ES is ALREADY limited-range
/// untagged H.264 — the branch's FULL→LIMITED LUT scaler + encoder handled
/// colour, exactly like the official GeForce Now recordings — so this is a
/// plain parse-and-mux: no decode, no re-encode, no colour transform, fast.
/// The branch's decoded-frame source is complete (immune to the WAN jitter /
/// stream-restart loss that plagues the raw-RTP pass-through), so no decode
/// recovery is needed either. Returns the output file size in bytes
/// (0 = nothing was written).
///
/// `frame_duration_ns` is the MEASURED average decoded-frame cadence of the
/// recording window (see `measured_recording_frame_duration_ns`): the PTS
/// ladder is built at that interval, so the MP4 plays at real speed whether
/// GFN delivered the full negotiated rate or fewer unique frames. 0 falls
/// back to the negotiated 60 fps cadence.
fn remux_encoded_recording(
    video_es: Option<&std::path::Path>,
    audio_es: Option<&std::path::Path>,
    out_path: &std::path::Path,
    frame_duration_ns: u64,
    event_sender: &Option<Sender<Event>>,
) -> Result<u64, String> {
    let Some(video_es) = video_es else {
        return Err("Encoded remux: no video elementary stream file.".to_owned());
    };
    let pipeline = gst::Pipeline::new();
    let video_src = make_element("filesrc")?;
    video_src.set_property(
        "location",
        video_es
            .to_str()
            .ok_or_else(|| "Encoded remux video ES path is not valid UTF-8.".to_owned())?,
    );
    let parse = make_element("h264parse")?;
    let muxer = make_element("qtmux")?;
    muxer.set_property("faststart", true);
    muxer.set_property("fragment-duration", 0u32);
    muxer.set_property("streamable", false);
    let out_sink = make_element("filesink")?;
    out_sink.set_property(
        "location",
        out_path
            .to_str()
            .ok_or_else(|| "Encoded remux output path is not valid UTF-8.".to_owned())?,
    );
    out_sink.set_property("sync", false);
    out_sink.set_property("async", false);

    pipeline
        .add_many(&[&video_src, &parse, &muxer, &out_sink])
        .map_err(|error| format!("Failed to add encoded remux chain: {error}"))?;
    video_src
        .link(&parse)
        .map_err(|error| format!("Failed to link encoded remux source: {error:?}"))?;
    parse.link(&muxer)
        .map_err(|error| format!("Failed to link encoded remux parser: {error:?}"))?;
    muxer.link(&out_sink)
        .map_err(|error| format!("Failed to link encoded remux muxer: {error:?}"))?;

    // Reconstruct the VIDEO timestamps the raw ES file cannot carry: files
    // store no PTS, and qtmux rejects buffers with none ("Buffer has no
    // PTS"). GFN's low-latency encoders emit no B-frames, so decode order ==
    // presentation order and a monotonic PTS ladder at the measured cadence is
    // exact. Attached to the h264parse src pad (right before qtmux).
    //
    // The ladder is applied UNCONDITIONALLY: the ES file cannot carry real
    // timestamps, so whatever PTS the parser synthesises on its own (it can
    // derive one from SPS VUI timing when the encoder wrote it) is not the
    // ground truth — the measured cadence is. A previous guard
    // (`if buffer.pts().is_none()`) let the parser's own degenerate 1 fps
    // synthesis through, producing the field "frame per frame" recordings
    // (1 s per frame; the MP4 duration ballooned to the frame count in
    // seconds). And a FIXED negotiated-rate ladder (the fps=0 state bug's
    // original design) stretched a 30 fps delivery into 2×-fast video with
    // the audio stranded — the caller now passes the measured cadence.
    //
    // Defense-in-depth: never let a degenerate 0 produce a 1 s-per-frame
    // ladder — 60 fps (the negotiated GFN rate) is the fallback, not 1.
    let frame_duration_ns = if frame_duration_ns == 0 {
        1_000_000_000u64 / 60
    } else {
        frame_duration_ns
    };
    let video_counter = Arc::new(std::sync::Mutex::new(0u64));
    {
        let counter = video_counter.clone();
        let parse_src_pad = parse
            .static_pad("src")
            .ok_or_else(|| "Encoded remux: h264parse has no src pad.".to_owned())?;
        parse_src_pad.add_probe(
            gst::PadProbeType::BUFFER,
            move |_pad: &gst::Pad, info: &mut gst::PadProbeInfo| {
                if let Some(buffer) = info.buffer_mut() {
                    let buffer = buffer.make_mut();
                    let mut guard =
                        counter.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    let pts = gst::ClockTime::from_nseconds(*guard * frame_duration_ns);
                    *guard += 1;
                    let duration = gst::ClockTime::from_nseconds(frame_duration_ns);
                    buffer.set_pts(pts);
                    buffer.set_dts(pts);
                    buffer.set_duration(duration);
                }
                gst::PadProbeReturn::Ok
            },
        );
    }

    // Optional game-audio AAC track (ADTS elementary stream from the live
    // branch's AAC encoder). Only linked when the file actually has data.
    if let Some(audio_es) = audio_es {
        if audio_es.exists()
            && std::fs::metadata(audio_es)
                .map(|metadata| metadata.len())
                .unwrap_or(0)
                > 0
        {
            let audio_src = make_element("filesrc")?;
            audio_src.set_property(
                "location",
                audio_es.to_str().ok_or_else(|| {
                    "Encoded remux audio ES path is not valid UTF-8.".to_owned()
                })?,
            );
            let aac_parse = make_element("aacparse")?;
            pipeline
                .add_many(&[&audio_src, &aac_parse])
                .map_err(|error| format!("Failed to add encoded remux audio: {error}"))?;
            audio_src
                .link(&aac_parse)
                .map_err(|error| format!("Failed to link encoded remux audio source: {error:?}"))?;
            // Same timestamp reconstruction for AUDIO: each AAC frame is
            // exactly 1024 samples, so a monotonic PTS ladder at 1024/rate is
            // exact (rate read from the aacparse caps, e.g. 48000). Like the
            // video ladder above, it is applied unconditionally — the ADTS
            // file cannot carry real timestamps.
            let audio_counter = Arc::new(std::sync::Mutex::new(0u64));
            {
                let counter = audio_counter.clone();
                let aac_src_pad = aac_parse
                    .static_pad("src")
                    .ok_or_else(|| "Encoded remux: aacparse has no src pad.".to_owned())?;
                aac_src_pad.add_probe(
                    gst::PadProbeType::BUFFER,
                    move |pad: &gst::Pad, info: &mut gst::PadProbeInfo| {
                        if let Some(buffer) = info.buffer_mut() {
                            let buffer = buffer.make_mut();
                            let rate = match pad.current_caps() {
                                Some(caps) => match caps.structure(0) {
                                    Some(structure) => structure
                                        .get::<i32>("rate")
                                        .unwrap_or(48_000)
                                        .max(1) as u64,
                                    None => 48_000,
                                },
                                None => 48_000,
                            };
                            let frame_ns = 1_024u64 * 1_000_000_000u64 / rate;
                            let mut guard = counter
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            let pts = gst::ClockTime::from_nseconds(*guard * frame_ns);
                            *guard += 1;
                            buffer.set_pts(pts);
                            buffer.set_dts(pts);
                            buffer.set_duration(gst::ClockTime::from_nseconds(frame_ns));
                        }
                        gst::PadProbeReturn::Ok
                    },
                );
            }
            let audio_muxer_pad = muxer
                .request_pad_simple("audio_%u")
                .ok_or_else(|| "Encoded remux: qtmux refused an audio sink pad.".to_owned())?;
            let aac_src = aac_parse
                .static_pad("src")
                .ok_or_else(|| "Encoded remux: aacparse has no src pad.".to_owned())?;
            if let Err(error) = aac_src.link(&audio_muxer_pad) {
                let _ = muxer.release_request_pad(&audio_muxer_pad);
                return Err(format!("Failed to link encoded remux AAC into qtmux: {error:?}"));
            }
        }
    }

    if let Err(error) = pipeline.set_state(gst::State::Playing) {
        let _ = pipeline.set_state(gst::State::Null);
        return Err(format!("Encoded remux failed to start: {error:?}"));
    }
    let Some(bus) = pipeline.bus() else {
        let _ = pipeline.set_state(gst::State::Null);
        return Err("Encoded remux pipeline has no bus.".to_owned());
    };
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(RECORDING_REMUX_TIMEOUT_MS);
    let mut finished = false;
    while std::time::Instant::now() < deadline {
        let Some(message) = bus.timed_pop_filtered(
            gst::ClockTime::from_mseconds(100),
            &[
                gst::MessageType::Eos,
                gst::MessageType::Error,
                gst::MessageType::Warning,
            ],
        ) else {
            continue;
        };
        match message.view() {
            gst::MessageView::Eos(_) => {
                finished = true;
                break;
            }
            gst::MessageView::Error(error) => {
                let _ = pipeline.set_state(gst::State::Null);
                return Err(format!(
                    "Encoded remux failed: {} (debug: {:?})",
                    error.error(),
                    error.debug()
                ));
            }
            gst::MessageView::Warning(warning) => send_log(
                event_sender,
                "warn",
                format!(
                    "Encoded remux warning: {} (debug: {:?})",
                    warning.error(),
                    warning.debug()
                ),
            ),
            _ => {}
        }
    }
    let _ = pipeline.set_state(gst::State::Null);
    if !finished {
        return Err("Encoded remux timed out while muxing the recording.".to_owned());
    }
    // Neutralize any colour-metadata box qtmux wrote (it derives a BT.709
    // `colr` from the encode caps): the official GeForce Now recordings carry
    // NO colour tags, and some field players misinterpret a tag and render
    // the limited 16-235 data without expansion ("hitam pekat"). The box
    // type is renamed colr→free (same size) so offsets stay valid.
    if let Ok(mut bytes) = std::fs::read(out_path) {
        let stripped = strip_mp4_colr_boxes(&mut bytes);
        if stripped > 0 {
            if std::fs::write(out_path, &bytes).is_ok() {
                send_log(
                    event_sender,
                    "info",
                    format!(
                        "Remuxed recording: neutralized {stripped} colour-metadata box(es) (colr→free); the file carries no colour tags, matching the official GeForce Now recordings."
                    ),
                );
            }
        }
    }
    let size = std::fs::metadata(out_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    send_log(
        event_sender,
        "info",
        format!("Encoded remux finished: {size} bytes."),
    );
    Ok(size)
}

/// Pick a decoder for the OFFLINE pass-through remux (temp ES file → limited
/// H.264 MP4, in a throwaway pipeline at stop — never touches the live
/// stream). Hardware first (fast, matches the live decode path), software
/// Pick the H.264 encoder for the recording branch. The recording carries
/// LIMITED (16-235) data — the RGB round-trip converts the decoder's
/// full-range output to the limited range H.264 playback expects — and the
/// encoder must NOT write a range tag in its VUI: the official GeForce Now
/// recordings are untagged, and any tag (especially `tv`, which the hardware
/// encoders hardcode) is what the field players mis-render (crushed blacks).
/// That rules out the hardware encoders (d3d12h264enc and mfh264enc hardcode
/// a `tv` VUI regardless of the input caps — verified against the bundled
/// runtime), so the default is x264enc (ultrafast + insert-vui=false → no
/// colorimetry/range VUI at all), with openh264enc (which writes no range
/// tag either) as fallback. `OPENNOW_RECORD_ENCODER` forces a specific
/// factory for debugging/problem machines.
fn pick_h264_encoder(record_bitrate_kbps: u32) -> Result<(String, gst::Element), String> {
    let forced = std::env::var("OPENNOW_RECORD_ENCODER")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let mut candidates: Vec<&str> = Vec::new();
    if let Some(forced) = &forced {
        candidates.push(forced);
    } else {
        // Hardware first: the hardware encoder offloads encode to the GPU/SFU
        // so a recording costs ~0 CPU (the official GeForce Now client is
        // also essentially free — it only writes the already-encoded stream).
        // The branch feeds it LIMITED 16-235 data (LUT-rescaled), and the D3D
        // encoders tag their VUI `tv` (limited) — metadata that now MATCHES
        // the data, so the old "hitam pekat" mismatch (full-range data + tv
        // tag) cannot recur. The READY-state init probe below rejects any
        // hardware encoder whose device is unavailable on this machine,
        // falling back down the ladder to software x264 so recording is never
        // silently disabled.
        #[cfg(target_os = "windows")]
        {
            // d3d12h264enc is the primary path (the streamer's D3D12 video
            // backend). d3d11h264enc is kept for runtimes that ship it; the
            // bundled runtime instead offers qsvh264enc (Intel Quick Sync)
            // and mfh264enc (Media Foundation MFT) as further hardware
            // fallbacks before software.
            candidates.extend([
                "d3d12h264enc",
                "d3d11h264enc",
                "qsvh264enc",
                "mfh264enc",
                "x264enc",
                "openh264enc",
            ]);
        }
        #[cfg(target_os = "linux")]
        {
            // VAAPI covers Intel/AMD iGPUs, NVENC covers discrete NVIDIA GPUs.
            candidates.extend(["vaapih264enc", "nvh264enc", "x264enc", "openh264enc"]);
        }
        #[cfg(target_os = "macos")]
        {
            // VideoToolbox is the macOS hardware H.264 encoder.
            candidates.extend(["vtenc_h264", "x264enc", "openh264enc"]);
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            candidates.extend(["x264enc", "openh264enc"]);
        }
    }
    for factory in candidates {
        if let Ok(element) = gst::ElementFactory::make(factory).build() {
            configure_h264_encoder(&element, factory, record_bitrate_kbps);
            // Hardware encoders create their device on the READY transition —
            // probe it now (back to NULL before adding to the pipeline) and
            // fall through to the next candidate when the device is missing.
            if is_hardware_h264_factory(factory) && !hw_encoder_initializes(&element) {
                continue;
            }
            return Ok((factory.to_owned(), element));
        }
    }
    Err(format!(
        "No H.264 encoder is available in the GStreamer runtime (tried {}); recording disabled.",
        if forced.is_some() {
            "forced factory + defaults"
        } else {
            "hardware (d3d12/d3d11/qsv/mf/vaapi/nvenc/videotoolbox) + x264enc + openh264enc"
        }
    ))
}

/// Hardware H.264 encoder factories across platforms (they take NV12 and
/// create their device on READY; the software encoders take I420).
fn is_hardware_h264_factory(factory: &str) -> bool {
    matches!(
        factory,
        "d3d12h264enc"
            | "d3d11h264enc"
            | "qsvh264enc"
            | "mfh264enc"
            | "vaapih264enc"
            | "nvh264enc"
            | "vtenc_h264"
    )
}

/// Bring a hardware encoder to READY (forcing device/encode creation) and
/// back to NULL. Returns true only if the transition succeeded — on machines
/// without a working device (D3D12/D3D11 adapter, Quick Sync, VAAPI, NVENC,
/// VideoToolbox) the READY transition fails and the caller falls back to the
/// next candidate.
fn hw_encoder_initializes(element: &gst::Element) -> bool {
    let ready_ok = element.set_state(gst::State::Ready).is_ok();
    let _ = element.set_state(gst::State::Null);
    ready_ok
}

fn configure_h264_encoder(element: &gst::Element, factory: &str, record_bitrate_kbps: u32) {
    // The recording bitrate follows the session's negotiated maximum (the
    // user's "max bitrate" slider) so a 15 Mbps setting records at 15 Mbps
    // instead of the old fixed 8 Mbps — the field "recorder burik" bug. VBR
    // lets the encoder spend up to ~25% above the target on motion spikes;
    // GOP 60 ≈ 1 s keyframe interval at 60 fps: instant seeking, and the
    // first GOP starts immediately at record start.
    let bitrate_kbps = record_bitrate_kbps.max(500);
    let max_bitrate_kbps = (bitrate_kbps * 5).div_ceil(4);
    match factory {
        // Hardware encoders (NV12 input). Property names differ per element
        // family, so every set is guarded — a wrong/missing property on a
        // given runtime is a safe no-op instead of a panic. All take bitrate
        // in kbps; GOP is `gop-size` on the D3D/QSG/NVENC families,
        // `keyframe-period` on VAAPI, `gop` on VideoToolbox.
        "d3d12h264enc" | "d3d11h264enc" | "qsvh264enc" | "mfh264enc" | "nvh264enc" => {
            element.set_property("bitrate", bitrate_kbps);
            set_property_from_str_if_supported(element, "rate-control", "vbr");
            element.set_property("gop-size", 60u32);
            set_property_from_str_if_supported(element, "max-bitrate", &max_bitrate_kbps.to_string());
        }
        "vaapih264enc" => {
            element.set_property("bitrate", bitrate_kbps);
            set_property_from_str_if_supported(element, "rate-control", "vbr");
            set_property_from_str_if_supported(element, "keyframe-period", "60");
        }
        "vtenc_h264" => {
            element.set_property("bitrate", bitrate_kbps);
            element.set_property("gop", 60u32);
            set_property_from_str_if_supported(element, "max-bitrate", &max_bitrate_kbps.to_string());
            // Real-time encode: keeps encode latency from piling up behind
            // the live stream instead of buffering whole seconds of frames.
            set_property_from_str_if_supported(element, "realtime", "true");
        }
        "openh264enc" => {
            element.set_property("bitrate", bitrate_kbps);
            element.set_property("gop-size", 60u32);
        }
        "x264enc" => {
            element.set_property("bitrate", bitrate_kbps);
            set_property_from_str_if_supported(element, "speed-preset", "ultrafast");
            // No B-frames: lighter to decode on weak players AND no reordering.
            set_property_from_str_if_supported(element, "tune", "zerolatency");
            element.set_property("key-int-max", 60u32);
            element.set_property("bframes", 0u32);
            set_property_from_str_if_supported(element, "profile", "main");
            // insert-vui=false: emit NO VUI NAL, so the bitstream carries no
            // colorimetry and no video_full_range_flag — the recording stays
            // untagged exactly like the official GeForce Now files (limited
            // 16-235 data, no tag), which is what the field players render
            // correctly. Verified: ffprobe shows color_range=unknown.
            set_property_from_str_if_supported(element, "insert-vui", "false");
        }
        _ => {}
    }
}

/// Pick the AAC encoder for the recording audio branch (avenc_aac first —
/// better quality; voaacenc fallback). AAC makes the MP4 play on ANY device,
/// unlike Opus-in-MP4.
fn pick_aac_encoder() -> Result<(String, gst::Element), String> {
    for factory in ["avenc_aac", "voaacenc"] {
        if let Ok(element) = gst::ElementFactory::make(factory).build() {
            // avenc_aac's bitrate property is a signed gint (unlike the uint
            // h264 encoders), so set it as i32.
            element.set_property("bitrate", 128_000i32);
            return Ok((factory.to_owned(), element));
        }
    }
    Err(
        "No AAC encoder is available in the GStreamer runtime (tried avenc_aac, voaacenc); audio recording disabled."
            .to_owned(),
    )
}

#[cfg(test)]
mod mic_pipeline_tests {
    use super::*;

    /// The finished MP4 must carry NO colour metadata: qtmux writes a `colr`
    /// box (BT.709 for 1080p, derived by h264parse even when the bitstream
    /// has no VUI) which some field players misinterpret as full-range and
    /// then render the limited 16-235 data without expansion ("hitam pekat")
    /// — the official GeForce Now recordings carry no colour tags at all and
    /// render correctly. The box type is renamed `colr` → `free` (a standard,
    /// universally-ignored MP4 box) so sizes/offsets stay untouched.
    #[test]
    fn strip_mp4_colr_boxes_neutralizes_colour_metadata_without_shifting_structure() {
        // Synthetic faststart MP4 skeleton: moov (with a video trak whose
        // stsd holds an avc1 sample entry containing the colr box + avcC, and
        // an stco with sample offsets into a later mdat) followed by mdat.
        fn box_of(box_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
            let mut b = Vec::new();
            b.extend_from_slice(&((payload.len() as u32 + 8u32).to_be_bytes()));
            b.extend_from_slice(box_type);
            b.extend_from_slice(payload);
            b
        }
        let avc1_payload_start = vec![0u8; 78]; // VisualSampleEntry fixed fields
        let colr = box_of(b"colr", &[b'n', b'c', b'l', b'c', 0, 1, 0, 1, 0, 1]);
        let avcc = box_of(b"avcC", &[1u8; 8]);
        let mut avc1_payload = avc1_payload_start;
        avc1_payload.extend_from_slice(&colr);
        avc1_payload.extend_from_slice(&avcc);
        let avc1 = box_of(b"avc1", &avc1_payload);
        let mut entry_payload = Vec::new();
        entry_payload.extend_from_slice(&1u32.to_be_bytes()); // entry_count
        entry_payload.extend_from_slice(&avc1);
        let stsd = box_of(b"stsd", &entry_payload);
        let mut stco_payload = vec![0u8; 4]; // version+flags
        stco_payload.extend_from_slice(&2u32.to_be_bytes()); // entry_count
        stco_payload.extend_from_slice(&1000u32.to_be_bytes());
        stco_payload.extend_from_slice(&2000u32.to_be_bytes());
        let stco = box_of(b"stco", &stco_payload);
        let mut stbl_payload = Vec::new();
        stbl_payload.extend_from_slice(&stsd);
        stbl_payload.extend_from_slice(&stco);
        let stbl = box_of(b"stbl", &stbl_payload);
        let mut minf_payload = Vec::new();
        minf_payload.extend_from_slice(&stbl);
        let minf = box_of(b"minf", &minf_payload);
        let mut mdia_payload = Vec::new();
        mdia_payload.extend_from_slice(&minf);
        let mdia = box_of(b"mdia", &mdia_payload);
        let mut trak_payload = Vec::new();
        trak_payload.extend_from_slice(&mdia);
        let trak = box_of(b"trak", &trak_payload);
        let mut moov_payload = Vec::new();
        moov_payload.extend_from_slice(&trak);
        let moov = box_of(b"moov", &moov_payload);
        let mdat = box_of(b"mdat", &[0u8; 64]);

        let mut data = Vec::new();
        data.extend_from_slice(&moov);
        data.extend_from_slice(&mdat);
        assert!(
            data.windows(4).any(|w| w == b"colr"),
            "fixture must contain colr"
        );
        let stco_offset = data.windows(4).position(|w| w == b"stco").unwrap();
        let stco_entry_a = stco_offset + 12; // version/flags + count
        let stco_entry_b = stco_entry_a + 4;

        let count = strip_mp4_colr_boxes(&mut data);
        assert_eq!(count, 1, "exactly one colr box must be neutralized");
        assert!(
            !data.windows(4).any(|w| w == b"colr"),
            "colr must be gone after stripping"
        );
        assert!(
            data.windows(4).any(|w| w == b"free"),
            "the neutralized box must remain as a free box"
        );
        // Structure untouched: stco entry offsets and box layout unchanged.
        let new_stco_offset = data.windows(4).position(|w| w == b"stco").unwrap();
        assert_eq!(new_stco_offset, stco_offset, "stco box must not move");
        assert_eq!(
            &data[stco_entry_a..stco_entry_a + 4],
            &1000u32.to_be_bytes(),
            "stco sample offsets must be untouched"
        );
        assert_eq!(
            &data[stco_entry_b..stco_entry_b + 4],
            &2000u32.to_be_bytes(),
            "stco sample offsets must be untouched"
        );
        assert_eq!(
            data.len(),
            moov.len() + mdat.len(),
            "file length must not change"
        );
    }

    /// Regression guard for the 19:56 field failure pattern (the remux
    /// recording branch killed the whole video flow when its elements were
    /// hot-plugged into a PLAYING pipeline without sync_state_with_parent).
    /// The screenshot grab branch has the same hot-plug shape — built on first
    /// use, into an already-PLAYING pipeline, off the post-decode tap tee — so
    /// it must (1) sync every element to PLAYING BEFORE linking into the tee
    /// and (2) keep the main chain flowing after the hot-plug. This test
    /// calls the REAL production function and asserts both.
    #[test]
    fn screenshot_grab_hotplugged_into_playing_pipeline_keeps_main_chain_flowing() {
        gst::init().expect("gstreamer init");
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::{Duration, Instant};

        // Main chain: a deterministic push source → tap tee → main sink.
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
        let tee = gst::ElementFactory::make("tee").build().expect("tee");
        let main_queue = gst::ElementFactory::make("queue")
            .build()
            .expect("main queue");
        main_queue.set_property("max-size-buffers", 100u32);
        let main_sink = gst::ElementFactory::make("fakesink")
            .build()
            .expect("main sink");
        main_sink.set_property("sync", false);
        main_sink.set_property("async", false);

        let pipeline = gst::Pipeline::new();
        for element in [&src, &src_caps, &tee, &main_queue, &main_sink] {
            pipeline.add(element).expect("add main chain");
        }
        src.link(&src_caps).expect("src -> src_caps");
        src_caps.link(&tee).expect("src_caps -> tee");
        tee.link(&main_queue).expect("tee -> main_queue");
        main_queue
            .link(&main_sink)
            .expect("main_queue -> main_sink");

        let main_count = Arc::new(AtomicUsize::new(0));
        let count = main_count.clone();
        let main_sink_pad = main_sink.static_pad("sink").expect("main sink pad");
        main_sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            count.fetch_add(1, Ordering::SeqCst);
            gst::PadProbeReturn::Ok
        });

        // Baseline: the main chain flows before the hot-plug.
        pipeline.set_state(gst::State::Playing).expect("playing");
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && main_count.load(Ordering::SeqCst) < 30 {
            std::thread::sleep(Duration::from_millis(20));
        }
        let baseline = main_count.load(Ordering::SeqCst);
        eprintln!("DIAG screenshot baseline main_count={baseline}");
        assert!(
            baseline >= 30,
            "main chain never flowed even before the screenshot hot-plug (received {baseline})"
        );

        // The real production hot-plug: build the screenshot grab branch off
        // the (already PLAYING) tap tee.
        let grab =
            insert_screenshot_grab_branch(&pipeline, &tee, RtpVideoApi::Software, false, &None)
                .expect("insert screenshot grab branch into PLAYING pipeline");
        for element in [&grab.valve, &grab.appsink] {
            eprintln!(
                "DIAG screenshot branch {:?} state={:?}",
                element.name(),
                element.current_state()
            );
            assert!(
                element.current_state() == gst::State::Playing,
                "screenshot branch element {:?} did not reach PLAYING after hot-plug into a PLAYING pipeline",
                element.name()
            );
        }

        // The main chain must keep flowing after the hot-plug (the tee must
        // not error on the closed-valve branch pad).
        let after = main_count.load(Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && main_count.load(Ordering::SeqCst) < after + 30 {
            std::thread::sleep(Duration::from_millis(20));
        }
        let final_count = main_count.load(Ordering::SeqCst);
        eprintln!(
            "DIAG screenshot hot-plug kept main chain flowing: baseline={baseline} after_plug={after} final={final_count}"
        );
        assert!(
            final_count >= after + 30,
            "main chain stalled after hot-plugging the screenshot grab branch (baseline={baseline} after={after} final={final_count}) — the branch pad error killed the flow"
        );

        let _ = pipeline.set_state(gst::State::Null);
    }

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

    /// The FULL→LIMITED Y-plane scaler must genuinely RESCALE mid-tones
    /// (128 → 126), not merely clip extremes to [16,235] like GStreamer's
    /// videoconvert / the old RGB round-trip did — clipped-full data is what
    /// the field players expand to "hitam pekat" (crushed blacks + blown
    /// highlights) no matter what colour tags the file carries. The probe
    /// tests in gstreamer_liveness verify the scaler is actually wired into
    /// the branch with real pipeline buffers; this test pins the exact
    /// LUT values.
    #[test]
    fn full_to_limited_lut_rescales_midtones() {
        let lut = full_to_limited_lut();
        assert_eq!(lut[0], 16, "black 0 must map to limited 16");
        assert_eq!(
            lut[128], 126,
            "mid-tone 128 must RESCALE to 126 (clip-only converters leave it at 128 — the field bug)"
        );
        assert_eq!(lut[255], 235, "white 255 must map to limited 235");
        // Spot-check the full curve is a genuine linear rescale, not a clip:
        // 64 → 16 + (64*219+127)/255 = 71; 192 → 16 + (192*219+127)/255 = 180.
        assert_eq!(lut[64], 71, "64 must rescale to 71");
        assert_eq!(lut[192], 181, "192 must rescale to 181");
        // Monotonic: never decreases (floor rounding makes some steps flat,
        // but a true rescale never steps backwards — a clip would flatten
        // 235..255 at 235).
        for pair in lut.windows(2) {
            assert!(pair[1] >= pair[0], "LUT must be monotonic non-decreasing");
        }
    }

    /// The hardware encoder must be preferred over software when the runtime
    /// ships it AND it can initialize (a working D3D12 video-encode adapter)
    /// — that is the whole CPU-offload point of the recording branch — and
    /// the picker must never fail when only software encoders exist.
    #[test]
    fn pick_h264_encoder_prefers_hardware_with_software_fallback() {
        gst::init().expect("gstreamer init");
        let (factory, _encoder) =
            pick_h264_encoder(15_000).expect("at least one H.264 encoder must be available");
        // On any platform the picker must return a real encoder, preferring
        // hardware when one can initialize and falling back to software
        // otherwise — never an error while an encoder exists.
        assert!(
            is_hardware_h264_factory(&factory)
                || matches!(factory.as_str(), "x264enc" | "openh264enc"),
            "picker returned an unexpected factory: {factory}"
        );
    }

    /// Configuring the hardware encoder must set the properties that exist in
    /// the bundled runtime (bitrate/gop-size/rate-control) and silently skip
    /// ones it lacks (e.g. `profile` is caps-only on d3d12h264enc — setting it
    /// must not panic).
    #[test]
    fn configure_d3d12_encoder_sets_supported_properties_only() {
        gst::init().expect("gstreamer init");
        let Some(element) = gst::ElementFactory::make("d3d12h264enc").build().ok() else {
            return; // runtime without the hardware encoder — nothing to configure
        };
        configure_h264_encoder(&element, "d3d12h264enc", 15_000);
        let bitrate: u32 = element.property("bitrate");
        assert_eq!(bitrate, 15_000, "d3d12h264enc bitrate must follow the session cap");
        let gop: u32 = element.property("gop-size");
        assert_eq!(gop, 60, "d3d12h264enc gop-size must be 60");        // The whole point of configure_h264_encoder: it must not panic on
        // unsupported properties (reaching here proves `profile` was skipped).
    }

    /// End-to-end offline remux on a REAL field H.265 elementary stream
    /// (testdata/gfn_field.h265 — a GFN capture): the pass-through remux must
    /// PRESERVE the received bitstream (no decode/re-encode — zero GPU/CPU,
    /// exact server quality, the official GeForce Now approach), producing a
    /// playable MP4 whose video sample entry is the RECEIVED codec (hvc1),
    /// and the colour metadata (colr nclx BT.709 full-range — the stream's
    /// SPS VUI says video_full_range_flag=1) must be tagged into the file
    /// without a data transform.
    #[test]
    fn remux_h265_field_es_preserves_bitstream_and_tags_colr() {
        gst::init().expect("gstreamer init");
        let es_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("gfn_field.h265");
        assert!(es_path.exists(), "missing testdata/gfn_field.h265");
        let out_path = std::env::temp_dir().join(format!(
            "opennow-remux-h265-test-{}.mp4",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&out_path);
        let size = remux_pass_through_recording(Some(&es_path), None, &out_path, "H265", 60, &None)
            .expect("field H265 ES must remux offline");
        assert!(size > 100_000, "remux output suspiciously small: {size}");
        let mut bytes = std::fs::read(&out_path).expect("read remux output");
        // Must be a seekable MP4 with an mdat.
        assert!(bytes.len() >= 8 && &bytes[4..8] == b"ftyp", "must be ftyp MP4");
        assert!(bytes.windows(4).any(|w| w == b"mdat"), "must contain mdat");
        // The video sample entry must be the RECEIVED codec (hvc1/hev1 for
        // H.265), NOT avc1 — the pass-through remux must never re-encode.
        let has_h264_entry = bytes.windows(4).any(|w| w == b"avc1");
        let has_h265_entry = bytes.windows(4).any(|w| w == b"hvc1" || w == b"hev1");
        assert!(
            has_h265_entry && !has_h264_entry,
            "pass-through remux must keep the received H.265 (h265={has_h265_entry}, h264={has_h264_entry})"
        );
        // Colour metadata tagging: injecting the colr box must land inside
        // the video sample entry, grow the file by exactly the box size, keep
        // moov's size covering the gap to mdat, leave the mdat payload
        // (the tail of the faststart file) untouched, and be idempotent on a
        // second call.
        let len_before = bytes.len();
        let tail = bytes[bytes.len().saturating_sub(64)..].to_vec();
        assert_eq!(inject_mp4_colr_box(&mut bytes), 1, "colr must be injected");
        assert_eq!(bytes.len(), len_before + 20, "injection adds exactly the 20-byte colr box");
        let colr: [u8; 20] = [
            0x00, 0x00, 0x00, 0x14, b'c', b'o', b'l', b'r', b'n', b'c', b'l', b'x', 0x00, 0x01, 0x00,
            0x01, 0x00, 0x01, 0x01, 0x00,
        ];
        assert!(
            bytes.windows(20).any(|w| w == colr),
            "colr nclx BT.709 full-range box must be present after injection"
        );
        let top = mp4_box_children(&bytes, 0, bytes.len());
        let moov = mp4_find_child(&top, b"moov").expect("moov");
        let mdat = mp4_find_child(&top, b"mdat").expect("mdat");
        assert_eq!(
            moov.content_offset + moov.size,
            mdat.content_offset,
            "moov size must still cover the gap to mdat after the injection"
        );
        assert_eq!(
            &bytes[bytes.len() - tail.len()..],
            tail.as_slice(),
            "mdat payload must be untouched by the injection"
        );
        assert_eq!(
            inject_mp4_colr_box(&mut bytes),
            0,
            "second injection must be a no-op (colr already present)"
        );
        let _ = std::fs::remove_file(&out_path);
    }

    /// Deterministic unit test for the colr injection on a SYNTHETIC
    /// faststart MP4 (moov first, then mdat): the box lands in the video
    /// sample entry, all ancestor sizes grow by 20, the stco chunk offsets
    /// are bumped by 20, and an already-tagged file is left untouched.
    #[test]
    fn inject_mp4_colr_box_tags_synthetic_faststart_mp4() {
        fn box4(fourcc: &[u8; 4], payload: Vec<u8>) -> Vec<u8> {
            let mut out = Vec::new();
            out.extend_from_slice(&((8 + payload.len()) as u32).to_be_bytes());
            out.extend_from_slice(fourcc);
            out.extend_from_slice(&payload);
            out
        }
        let avc1_payload = {
            let mut p = Vec::new();
            p.extend_from_slice(&[0u8; 6]); // VisualSampleEntry reserved
            p.extend_from_slice(&1u16.to_be_bytes()); // data_reference_index
            p.extend_from_slice(&box4(b"avcC", vec![1, 2, 3, 4]));
            p
        };
        let stsd_payload = {
            let mut p = Vec::new();
            p.extend_from_slice(&0u32.to_be_bytes()); // version + flags
            p.extend_from_slice(&1u32.to_be_bytes()); // entry_count
            p.extend_from_slice(&box4(b"avc1", avc1_payload));
            p
        };
        let stco_payload = {
            let mut p = Vec::new();
            p.extend_from_slice(&0u32.to_be_bytes()); // version + flags
            p.extend_from_slice(&2u32.to_be_bytes()); // entry_count
            p.extend_from_slice(&1000u32.to_be_bytes());
            p.extend_from_slice(&2000u32.to_be_bytes());
            p
        };
        let stbl = box4(
            b"stbl",
            {
                let mut p = box4(b"stsd", stsd_payload);
                p.extend_from_slice(&box4(b"stco", stco_payload));
                p
            },
        );
        let moov = box4(b"moov", box4(b"trak", box4(b"mdia", box4(b"minf", stbl))));
        let mut mp4 = moov;
        mp4.extend_from_slice(&box4(b"mdat", vec![0xAB; 64]));
        let len_before = mp4.len();
        assert_eq!(inject_mp4_colr_box(&mut mp4), 1);
        assert_eq!(mp4.len(), len_before + 20);
        let colr: [u8; 20] = [
            0x00, 0x00, 0x00, 0x14, b'c', b'o', b'l', b'r', b'n', b'c', b'l', b'x', 0x00, 0x01, 0x00,
            0x01, 0x00, 0x01, 0x01, 0x00,
        ];
        assert!(
            mp4.windows(20).any(|w| w == colr),
            "colr nclx BT.709 full-range box must be present"
        );
        // moov must still exactly cover the gap to mdat (ancestor sizes bumped).
        let top = mp4_box_children(&mp4, 0, mp4.len());
        let moov = mp4_find_child(&top, b"moov").expect("moov");
        let mdat = mp4_find_child(&top, b"mdat").expect("mdat");
        assert_eq!(moov.content_offset + moov.size, mdat.content_offset);
        // stco chunk offsets must be bumped by 20 so mdat stays reachable.
        let moov_children =
            mp4_box_children(&mp4, moov.content_offset, moov.content_offset + moov.size);
        let trak = mp4_find_child(&moov_children, b"trak").expect("trak");
        let trak_children =
            mp4_box_children(&mp4, trak.content_offset, trak.content_offset + trak.size);
        let mdia = mp4_find_child(&trak_children, b"mdia").expect("mdia");
        let mdia_children =
            mp4_box_children(&mp4, mdia.content_offset, mdia.content_offset + mdia.size);
        let minf = mp4_find_child(&mdia_children, b"minf").expect("minf");
        let minf_children =
            mp4_box_children(&mp4, minf.content_offset, minf.content_offset + minf.size);
        let stbl = mp4_find_child(&minf_children, b"stbl").expect("stbl");
        let stbl_children = mp4_box_children(&mp4, stbl.content_offset, stbl.content_offset + stbl.size);
        let stco = mp4_find_child(&stbl_children, b"stco").expect("stco");
        let table = &mp4[stco.content_offset + 4..];
        let count = u32::from_be_bytes([table[0], table[1], table[2], table[3]]) as usize;
        assert_eq!(count, 2);
        let first = u32::from_be_bytes([table[4], table[5], table[6], table[7]]);
        let second = u32::from_be_bytes([table[8], table[9], table[10], table[11]]);
        assert_eq!(first, 1020, "stco entry 0 must be bumped by 20");
        assert_eq!(second, 2020, "stco entry 1 must be bumped by 20");
        // mdat payload untouched and the second call is a no-op.
        assert!(mp4.windows(64).any(|w| w.iter().all(|&b| b == 0xAB)));
        assert_eq!(inject_mp4_colr_box(&mut mp4), 0);
    }

    /// The finalized MP4 must be delivered to Electron as MULTIPLE
    /// fixed-size base64 chunks, not one giant line: Electron base64-decodes
    /// each stdout line synchronously, so a whole-file single line (~94 MB
    /// base64 for a 70 MB recording) freezes the UI and crashes the app
    /// before the .tmp is written (the field's 0-byte .tmp + "not
    /// responding"). Verify a 14 MB payload becomes many chunks and that
    /// concatenating them reproduces the original bytes exactly.
    #[test]
    fn recording_chunks_split_large_files_and_reassemble() {
        use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
        use std::sync::mpsc;

        let payload: Vec<u8> = (0..(3 * RECORDING_CHUNK_BYTES + 12345))
            .map(|i| (i % 251) as u8)
            .collect();
        let (tx, rx) = mpsc::channel::<Event>();
        send_recording_chunks(&tx, &payload);
        let mut chunks = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let Event::RecordingChunk { chunk_base64 } = event {
                chunks.push(
                    BASE64_STANDARD
                        .decode(&chunk_base64)
                        .expect("valid base64 chunk"),
                );
            }
        }
        assert!(
            chunks.len() >= 3,
            "large payload must be split into multiple chunks, got {}",
            chunks.len()
        );
        let reassembled: Vec<u8> = chunks.concat();
        assert_eq!(
            reassembled, payload,
            "concatenated chunks must reproduce the file byte-for-byte"
        );
    }

    /// The ENCODED-mode remux ladder must follow the MEASURED decoded-frame
    /// cadence of the recording window, not the static negotiated fps: a 60 fps
    /// negotiated stream that delivers 30 unique frames per second (rate
    /// adaptation) must record real-time (33 ms per frame), never 2×-fast
    /// (16.7 ms per frame).
    #[test]
    fn measured_frame_duration_tracks_actual_delivery_rate() {
        // 30 fps delivery inside a 60 fps negotiated session: 100 frames
        // spanning 3.3 s → ~33.3 ms per frame.
        assert_eq!(
            measured_recording_frame_duration_ns(100, 0, 3_300_000_000, 60),
            33_333_333
        );
        // Full-rate delivery (60 fps with content duplicates): 100 frames
        // spanning 1.65 s → ~16.7 ms — unchanged from the negotiated cadence.
        assert_eq!(
            measured_recording_frame_duration_ns(100, 0, 1_650_000_000, 60),
            16_666_666
        );
        // A 25 fps delivery in a 60 fps session stays real-time too.
        assert_eq!(
            measured_recording_frame_duration_ns(100, 0, 3_960_000_000, 60),
            40_000_000
        );
    }

    #[test]
    fn measured_frame_duration_falls_back_on_implausible_input() {
        // Too few frames to measure → negotiated cadence.
        assert_eq!(measured_recording_frame_duration_ns(1, 0, 33_000_000, 60), 16_666_666);
        // PTS went backwards (stream restart / reset) → negotiated cadence.
        assert_eq!(measured_recording_frame_duration_ns(100, 5_000_000_000, 100_000_000, 60), 16_666_666);
        // Same-timestamp duplication (each frame sent twice with the same
        // RTP timestamp: deltas [0, 16.7, 0, 16.7, …] → average ≈ half the
        // negotiated interval) → negotiated cadence: the frame count still
        // matches the delivery rate there, so 60 fps is correct, not 2×.
        assert_eq!(
            measured_recording_frame_duration_ns(120, 0, 900_000_000, 60),
            16_666_666
        );
        // Absurdly slow cadence (> 4× negotiated, or below 1 fps) → negotiated.
        assert_eq!(
            measured_recording_frame_duration_ns(100, 0, 10_000_000_000, 60),
            16_666_666
        );
        // Degenerate negotiated fps (the fps=0 state bug) still falls back to
        // 60 fps, never 1 s-per-frame — but a plausible measured cadence wins
        // over the degenerate fallback.
        assert_eq!(measured_recording_frame_duration_ns(0, 0, 0, 0), 16_666_666);
        assert_eq!(
            measured_recording_frame_duration_ns(100, 0, 3_300_000_000, 0),
            33_333_333
        );
        // Integer-division remainder: 100 frames over 3.3 s divides exactly;
        // odd spans round down per frame (≤ 1 frame total drift).
        assert_eq!(
            measured_recording_frame_duration_ns(100, 0, 3_333_333_337, 60),
            33_670_033
        );
    }
}
