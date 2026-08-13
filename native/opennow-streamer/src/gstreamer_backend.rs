use crate::backend::{
    normalize_bitrate_kbps, prepare_native_offer, prepared_offer_events,
    update_context_bitrate_limit, web_rtc_media_connection_info, BackendReply,
    NativeStreamerBackend,
};
use crate::gstreamer_config::{
    resolve_d3d_fullscreen_sink, resolve_present_max_fps, use_internal_renderer,
    NATIVE_D3D_FULLSCREEN_ENV, NATIVE_PRESENT_MAX_FPS_ENV, PRESENT_LIMITER_AUTO_SENTINEL,
    PRESENT_LIMITER_STREAM_SENTINEL, PRESENT_LIMITER_VRR_SENTINEL,
};
use crate::gstreamer_pipeline::{
    current_platform_label, init_gstreamer, native_video_backend_capabilities, GstreamerPipeline,
};
use crate::gstreamer_platform::{clear_native_shortcut_bindings, set_native_shortcut_bindings};
use crate::protocol::{
    missing_field, CommandEnvelope, Event, IceCandidatePayload, NativeRenderSurface,
    NativeStreamerCapabilities, NativeStreamerSessionContext, NativeVideoBackendCapability,
    Response, SendAnswerRequest, PROTOCOL_VERSION,
};
use gstreamer as gst;
use crate::sdp::{
    build_nvst_sdp_for_answer, ensure_audio_red_in_answer, extract_negotiated_video_codec,
    munge_answer_sdp, restore_h265_fmtp_params, rewrite_answer_video_bitrate,
    rewrite_ice_candidate_endpoint, NvstParams,
};
use std::sync::{mpsc, Arc};
use std::sync::mpsc::Sender;

pub(crate) fn send_log(event_sender: &Option<Sender<Event>>, level: &'static str, message: String) {
    if let Some(event_sender) = event_sender {
        let _ = event_sender.send(Event::Log { level, message });
    } else {
        eprintln!("[NativeStreamer] {message}");
    }
}

#[derive(Debug)]
pub struct GstreamerBackend {
    active_context: Option<NativeStreamerSessionContext>,
    pending_remote_ice: Vec<IceCandidatePayload>,
    pipeline: Option<Arc<GstreamerPipeline>>,
    event_sender: Option<Sender<Event>>,
    remote_description_set: bool,
    render_surface: Option<NativeRenderSurface>,
    recording_worker: RecordingWorkerHandle,
    /// The NVST params of the last negotiated offer, kept so a mid-session
    /// max-bitrate change can rebuild the nvstSdp with the new
    /// `vqos.bw.maximumBitrateKbps` and re-send the answer to the server
    /// (the "request a re-offer" path) without waiting for a reconnect.
    last_nvst_params: Option<NvstParams>,
    /// The final munged answer SDP of the last negotiation (b=AS + H265 fmtp
    /// + RED audio applied), re-sent unchanged (except its b=AS video line)
    /// when the bitrate cap changes mid-session.
    last_answer_sdp: Option<String>,
}

/// Recording work (valve open/close, branch rebuild, drain + EOS finalize)
/// runs on a dedicated worker thread instead of the command loop. A recording
/// stop can block for the whole drain/flush budget (up to ~11 s) and — when
/// the branch is wedged — forever inside a GStreamer call; running it inline
/// froze input, surface, and bitrate commands behind it (the field "record
/// affects input / input-paused timeouts" bug: every `input-paused` and
/// `surface` request timed out while stop-recording held the loop). With the
/// worker, the command loop replies immediately and the app stays fully
/// responsive while the recording finalizes (or fails) in the background.
/// Commands are FIFO on one thread, so start/stop ordering is preserved and
/// the `recording-finished` event (emitted inside the pipeline, after the
/// muxer EOS) still lands strictly after every `recording-chunk`.
#[derive(Debug)]
struct RecordingWorkerHandle {
    tx: Sender<RecordingCommand>,
    join: Option<std::thread::JoinHandle<()>>,
}

#[derive(Debug)]
enum RecordingCommand {
    Start {
        pipeline: Arc<GstreamerPipeline>,
        event_sender: Option<Sender<Event>>,
    },
    Stop {
        pipeline: Arc<GstreamerPipeline>,
        finalize: bool,
        event_sender: Option<Sender<Event>>,
    },
    Shutdown,
}

impl RecordingWorkerHandle {
    fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<RecordingCommand>();
        let join = std::thread::Builder::new()
            .name("recording-worker".to_owned())
            .spawn(move || {
                for command in rx {
                    match command {
                        RecordingCommand::Start {
                            pipeline,
                            event_sender,
                        } => {
                            if let Err(message) = pipeline.start_recording() {
                                if let Some(sender) = &event_sender {
                                    let _ = sender.send(Event::Error {
                                        code: "recording-start-failed".to_owned(),
                                        message,
                                    });
                                }
                            }
                        }
                        RecordingCommand::Stop {
                            pipeline,
                            finalize,
                            event_sender,
                        } => {
                            if let Err(message) = pipeline.stop_recording(finalize) {
                                if let Some(sender) = &event_sender {
                                    let _ = sender.send(Event::Error {
                                        code: "recording-stop-failed".to_owned(),
                                        message,
                                    });
                                }
                            }
                        }
                        RecordingCommand::Shutdown => break,
                    }
                }
            })
            .expect("recording worker thread spawn");
        Self {
            tx,
            join: Some(join),
        }
    }

    /// Stop the worker and wait (bounded) for it to drain pending recording
    /// work. A wedged GStreamer call can keep the thread alive past the
    /// budget; the JoinHandle is then dropped (thread detached) rather than
    /// blocking session teardown on it.
    fn shutdown(&mut self) {
        let _ = self.tx.send(RecordingCommand::Shutdown);
        if let Some(join) = self.join.take() {
            let deadline =
                std::time::Instant::now() + std::time::Duration::from_millis(3_000);
            while !join.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
}

impl GstreamerBackend {
    pub fn new(event_sender: Option<Sender<Event>>) -> Self {
        Self {
            active_context: None,
            pending_remote_ice: Vec::new(),
            pipeline: None,
            event_sender,
            remote_description_set: false,
            render_surface: None,
            recording_worker: RecordingWorkerHandle::spawn(),
            last_nvst_params: None,
            last_answer_sdp: None,
        }
    }

    fn replay_pending_remote_ice(&mut self) -> Vec<Event> {
        let candidates = std::mem::take(&mut self.pending_remote_ice);
        let Some(pipeline) = self
            .pipeline
            .as_mut()
            .and_then(|arc| Arc::get_mut(arc))
        else {
            self.pending_remote_ice = candidates;
            return Vec::new();
        };

        let mut events = Vec::new();
        for candidate in candidates {
            if let Err(message) = pipeline.add_remote_ice(&candidate) {
                events.push(Event::Error {
                    code: "remote-ice-failed".to_owned(),
                    message,
                });
            }
        }
        events
    }

    fn rewrite_remote_ice_candidate(&self, candidate: IceCandidatePayload) -> IceCandidatePayload {
        let Some(context) = self.active_context.as_ref() else {
            return candidate;
        };
        let Some(media_connection_info) = web_rtc_media_connection_info(context) else {
            return candidate;
        };
        let (rewritten, changed) = rewrite_ice_candidate_endpoint(
            &candidate.candidate,
            &media_connection_info.ip,
            media_connection_info.port,
        );
        if changed {
            send_log(
                &self.event_sender,
                "info",
                format!(
                    "Rewrote remote ICE candidate endpoint to GFN media connection hint {}:{}.",
                    media_connection_info.ip, media_connection_info.port
                ),
            );
            IceCandidatePayload {
                candidate: rewritten,
                ..candidate
            }
        } else {
            candidate
        }
    }
}

impl NativeStreamerBackend for GstreamerBackend {
    fn capabilities(&self) -> NativeStreamerCapabilities {
        NativeStreamerCapabilities {
            protocol_version: PROTOCOL_VERSION,
            backend: "gstreamer",
            requested_backend: None,
            fallback_reason: None,
            supports_offer_answer: true,
            supports_remote_ice: true,
            supports_local_ice: true,
            supports_input: true,
            video_backends: match init_gstreamer() {
                Ok(()) => native_video_backend_capabilities(),
                Err(error) => vec![NativeVideoBackendCapability {
                    backend: "gstreamer".to_owned(),
                    platform: current_platform_label().to_owned(),
                    codecs: Vec::new(),
                    zero_copy_modes: Vec::new(),
                    sink: None,
                    available: false,
                    reason: Some(error),
                }],
            },
        }
    }

    fn start(&mut self, command: CommandEnvelope) -> BackendReply {
        let id = command.id;
        let Some(context) = command.context else {
            return BackendReply::response(missing_field(&id, "context"));
        };

        let session_id = context.session.session_id.clone();
        let pipeline =
            match GstreamerPipeline::build(self.event_sender.clone(), &context.session.ice_servers)
            {
                Ok(pipeline) => Arc::new(pipeline),
                Err(message) => {
                    return BackendReply {
                        events: vec![Event::Error {
                            code: "gstreamer-start-failed".to_owned(),
                            message: message.clone(),
                        }],
                        response: Some(Response::Error {
                            id: Some(id),
                            code: "gstreamer-start-failed".to_owned(),
                            message,
                        }),
                        should_continue: true,
                    };
                }
            };

        if let Some(mut old_pipeline) = self.pipeline.take() {
            // No recording command may hold a clone of the old pipeline at
            // this point: `stop` (session end) shuts the worker down before
            // tearing down, and the worker drops each per-command Arc when the
            // command finishes. When a wedged recording worker still holds a
            // clone (should not happen — it is drained before `stop`), skip
            // the explicit teardown; the process exits right after session
            // end and GStreamer cleans up on drop.
            match Arc::get_mut(&mut old_pipeline) {
                Some(old_pipeline) => {
                    if let Err(message) = old_pipeline.stop() {
                        eprintln!("[NativeStreamer] {message}");
                    }
                }
                None => eprintln!(
                    "[NativeStreamer] Recording worker still holds the previous pipeline; skipping explicit teardown."
                ),
            }
        }

        // The sink-native RawInput mouse path scales deltas with the same
        // sensitivity / acceleration the renderer applies to the addon and DOM
        // pointer-lock paths, so stacked capture feels identical to the
        // configured mouse settings instead of raw HID counts.
        crate::gstreamer_input::set_native_mouse_settings(
            context.settings.mouse_sensitivity,
            context.settings.mouse_acceleration_percent,
        );
        crate::gstreamer_input::set_native_sink_input_capture_enabled(
            context.settings.native_sink_input_capture,
        );
        set_native_shortcut_bindings(&context.shortcuts);
        self.active_context = Some(context);
        self.pending_remote_ice.clear();
        self.remote_description_set = false;
        let webrtc_name = pipeline.webrtc_name();
        self.pipeline = Some(pipeline);

        let mut events = vec![Event::Status {
            status: "ready",
            message: Some(format!(
                "GStreamer backend selected for session {session_id}; {} pipeline is ready.",
                webrtc_name
            )),
        }];

        if let Some(nvst) = self
            .active_context
            .as_ref()
            .and_then(|ctx| ctx.nvst_video.clone())
        {
            let fallback_codec = self
                .active_context
                .as_ref()
                .map(|ctx| ctx.settings.codec.as_str().to_owned())
                .unwrap_or_else(|| "H265".to_owned());
            let requested_fps = self.active_context.as_ref().map(|ctx| ctx.settings.fps);
            let d3d_fullscreen = resolve_d3d_fullscreen_sink(
                self.active_context
                    .as_ref()
                    .map(|ctx| ctx.settings.enable_cloud_gsync)
                    .unwrap_or(false),
            );
            let cloud_gsync_enabled = self
                .active_context
                .as_ref()
                .map(|ctx| ctx.settings.enable_cloud_gsync)
                .unwrap_or(false);
            let present_max_fps = resolve_present_max_fps(cloud_gsync_enabled);
            if let Some(pipeline) = self
                .pipeline
                .as_mut()
                .and_then(|arc| Arc::get_mut(arc))
            {
                pipeline.set_present_max_fps(present_max_fps);
                pipeline.set_d3d_fullscreen_sink(d3d_fullscreen);
                if let Some(ctx) = self.active_context.as_ref() {
                    let bitrate_kbps = ctx.settings.max_bitrate_mbps.saturating_mul(1000);
                    pipeline.configure_stats(ctx, bitrate_kbps);
                    pipeline.set_record_bitrate_kbps(bitrate_kbps);
                }
                match pipeline.attach_nvst_video(
                    nvst,
                    &fallback_codec,
                    requested_fps.filter(|fps| *fps > 0),
                    d3d_fullscreen,
                ) {
                    Ok(()) => events.push(Event::Log {
                        level: "info",
                        message: "NVST classic UDP video receive scaffold attached (hybrid WebRTC input)."
                            .to_owned(),
                    }),
                    Err(message) => {
                        events.push(Event::Error {
                            code: "nvst-video-attach-failed".to_owned(),
                            message: message.clone(),
                        });
                        return BackendReply {
                            events,
                            response: Some(Response::Error {
                                id: Some(id),
                                code: "nvst-video-attach-failed".to_owned(),
                                message,
                            }),
                            should_continue: true,
                        };
                    }
                }
            }
        }

        if let (Some(surface), Some(pipeline)) =
            (self.render_surface.clone(), self.pipeline.as_ref())
        {
            pipeline.update_render_surface(surface);
        }

        BackendReply {
            events,
            response: Some(Response::Ok { id }),
            should_continue: true,
        }
    }

    fn handle_offer(&mut self, command: CommandEnvelope) -> BackendReply {
        let id = command.id.clone();
        let Some(context) = command.context else {
            return BackendReply::response(missing_field(&id, "context"));
        };
        let Some(offer_sdp) = command.sdp else {
            return BackendReply::response(missing_field(&id, "sdp"));
        };

        let prepared = match prepare_native_offer(&context, &offer_sdp) {
            Ok(prepared) => prepared,
            Err(error) => return BackendReply::response(error.into_response(id)),
        };

        let mut events = prepared_offer_events(&prepared);
        let parsed_offer = match GstreamerPipeline::parse_offer_sdp(&prepared.gstreamer_offer_sdp) {
            Ok(offer) => offer,
            Err(message) => {
                return BackendReply {
                    events,
                    response: Some(Response::Error {
                        id: Some(id),
                        code: "invalid-remote-sdp".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        };

        let Some(pipeline) = self
            .pipeline
            .as_mut()
            .and_then(|arc| Arc::get_mut(arc))
        else {
            return BackendReply {
                events,
                response: Some(Response::Error {
                    id: Some(id),
                    code: "gstreamer-not-started".to_owned(),
                    message: "GStreamer pipeline is not started.".to_owned(),
                }),
                should_continue: true,
            };
        };

        let present_max_fps = resolve_present_max_fps(context.settings.enable_cloud_gsync);
        // Internal child-surface mode never uses exclusive D3D fullscreen present.
        let d3d_fullscreen_sink = resolve_d3d_fullscreen_sink(context.settings.enable_cloud_gsync);
        set_native_shortcut_bindings(&context.shortcuts);
        pipeline.set_present_max_fps(present_max_fps);
        pipeline.set_d3d_fullscreen_sink(d3d_fullscreen_sink);
        pipeline.configure_stats(&context, prepared.nvst_params.max_bitrate_kbps);
        pipeline.set_record_bitrate_kbps(prepared.nvst_params.max_bitrate_kbps);
        if present_max_fps > 0
            && present_max_fps != PRESENT_LIMITER_AUTO_SENTINEL
            && present_max_fps != PRESENT_LIMITER_VRR_SENTINEL
            && present_max_fps != PRESENT_LIMITER_STREAM_SENTINEL
        {
            events.push(Event::Log {
                level: "info",
                message: format!(
                    "Native present limiter enabled at {present_max_fps} fps for {} fps stream; set {NATIVE_PRESENT_MAX_FPS_ENV}=0 to disable.",
                    context.settings.fps
                ),
            });
        } else if present_max_fps == PRESENT_LIMITER_AUTO_SENTINEL {
            events.push(Event::Log {
                level: "info",
                message: format!(
                    "Native present limiter auto mode for {} fps stream (D3D11 caps to display Hz when stream fps exceeds it); set {NATIVE_PRESENT_MAX_FPS_ENV}=0 to disable.",
                    context.settings.fps
                ),
            });
        } else if present_max_fps == PRESENT_LIMITER_VRR_SENTINEL {
            events.push(Event::Log {
                level: "info",
                message: format!(
                    "Native VRR present limiter auto mode for {} fps stream (caps below the display refresh ceiling when needed).",
                    context.settings.fps
                ),
            });
        } else if present_max_fps == PRESENT_LIMITER_STREAM_SENTINEL {
            events.push(Event::Log {
                level: "info",
                message: format!(
                    "Native present limiter paced to the {} fps stream (default: renders network jitter bursts at real-time instead of blinking); set {NATIVE_PRESENT_MAX_FPS_ENV}=0 to disable.",
                    context.settings.fps
                ),
            });
        } else {
            events.push(Event::Log {
                level: "info",
                message: "Native present limiter disabled for uncapped VSync-off presentation."
                    .to_owned(),
            });
        }
        if d3d_fullscreen_sink {
            events.push(Event::Log {
                level: "info",
                message: format!(
                    "Native D3D fullscreen presentation is enabled for Cloud G-Sync/VRR; set {NATIVE_D3D_FULLSCREEN_ENV}=0 to disable."
                ),
            });
        } else if use_internal_renderer() {
            events.push(Event::Log {
                level: "info",
                message: "Native Internal renderer keeps exclusive D3D fullscreen off (child HWND present; sync=false, depth-1 post-decode queue)."
                    .to_owned(),
            });
        }

        let answer_sdp = match pipeline.negotiate_answer(
            parsed_offer,
            (prepared.gstreamer_ice_pwd_replacements > 0)
                .then_some(&prepared.nvst_params.credentials),
            prepared.nvst_params.partial_reliable_threshold_ms,
            context.settings.microphone_enabled,
        ) {
            Ok(answer_sdp) => {
                let munged = munge_answer_sdp(&answer_sdp, prepared.nvst_params.max_bitrate_kbps);
                // The offer handed to webrtcbin had H265 fmtp parameters
                // stripped (rtph265depay rejects them in the receive caps); put
                // them back on the answer sent to GFN so the server sees the
                // fmtp it expects in the answer.
                let munged = if let Some(h265_fmtp) = prepared.h265_fmtp_params.as_deref() {
                    restore_h265_fmtp_params(&munged, h265_fmtp)
                } else {
                    munged
                };
                // webrtcbin drops the server's RED audio redundancy payload
                // from its answer (no auto rtpreddepay at answer time);
                // re-advertise it so the server sends the redundant Opus copy
                // — but only when the bundled runtime can unwrap it.
                ensure_audio_red_in_answer(
                    &munged,
                    &prepared.fixed_offer_sdp,
                    gst::ElementFactory::find("rtpreddepay").is_some(),
                )
            }
            Err(message) => {
                return BackendReply {
                    events,
                    response: Some(Response::Error {
                        id: Some(id),
                        code: "gstreamer-negotiation-failed".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        };

        if let Some(negotiated_codec) = extract_negotiated_video_codec(&answer_sdp) {
            // The liveness startup watchdog decides an AV1 zero-frame startup
            // against the codec the server actually sends, so feed it the
            // negotiated codec (may differ from the requested one).
            pipeline.set_negotiated_video_codec(negotiated_codec.as_str());
            if negotiated_codec != prepared.nvst_params.codec {
                events.push(Event::Log {
                    level: "warn",
                    message: format!(
                        "Negotiated video codec is {} while requested codec was {}; building NVST SDP for the negotiated codec to avoid server/client codec mismatch.",
                        negotiated_codec.as_str(),
                        prepared.nvst_params.codec.as_str(),
                    ),
                });
            } else {
                events.push(Event::Log {
                    level: "debug",
                    message: format!(
                        "Negotiated video codec confirmed as {}.",
                        negotiated_codec.as_str()
                    ),
                });
            }
        }

        self.remote_description_set = true;
        events.extend(self.replay_pending_remote_ice());

        events.push(Event::Log {
            level: "info",
            message:
                "GStreamer created a local WebRTC answer and replayed queued remote ICE candidates."
                    .to_owned(),
        });

        // Diagnostic: dump the answer's video payload types and codec mappings
        // so a `not-negotiated` H265/AV1 receive failure can be matched against
        // the payload types the server actually sends.
        {
            let video_section = answer_sdp
                .lines()
                .skip_while(|line| !line.starts_with("m=video"))
                .take_while(|line| !line.starts_with("m=") || line.starts_with("m=video"));
            let video_mline = video_section
                .clone()
                .find(|line| line.starts_with("m=video"))
                .unwrap_or_default()
                .to_owned();
            let video_rtpmaps = video_section
                .filter(|line| line.starts_with("a=rtpmap:"))
                .collect::<Vec<_>>()
                .join(" | ");
            events.push(Event::Log {
                level: "debug",
                message: format!("Answer video payload: {video_mline}; rtpmap: {video_rtpmaps}"),
            });
        }

        let nvst_sdp = match build_nvst_sdp_for_answer(&prepared.nvst_params, &answer_sdp) {
            Ok(nvst_sdp) => nvst_sdp,
            Err(message) => {
                return BackendReply {
                    events,
                    response: Some(Response::Error {
                        id: Some(id),
                        code: "invalid-local-answer-sdp".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        };

        events.push(Event::Log {
            level: "debug",
            message: "Built native NVST SDP from the local WebRTC answer transport credentials."
                .to_owned(),
        });

        // Remember the negotiated params + final answer so a mid-session max
        // bitrate change can rebuild the nvstSdp with the new cap and re-send
        // the answer to the server (the "request a re-offer" path) instead of
        // waiting for the next natural offer/reconnect.
        self.last_nvst_params = Some(prepared.nvst_params.clone());
        self.last_answer_sdp = Some(answer_sdp.clone());

        BackendReply {
            events,
            response: Some(Response::Answer {
                id,
                answer: SendAnswerRequest {
                    sdp: answer_sdp,
                    nvst_sdp: Some(nvst_sdp),
                },
            }),
            should_continue: true,
        }
    }

    fn add_remote_ice(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(candidate) = command.candidate else {
            return BackendReply::response(missing_field(&command.id, "candidate"));
        };
        let candidate = self.rewrite_remote_ice_candidate(candidate);

        if self.remote_description_set {
            if let Some(pipeline) = self
                .pipeline
                .as_mut()
                .and_then(|arc| Arc::get_mut(arc))
            {
                if let Err(message) = pipeline.add_remote_ice(&candidate) {
                    return BackendReply::response(Response::Error {
                        id: Some(command.id),
                        code: "remote-ice-failed".to_owned(),
                        message,
                    });
                }
            } else {
                self.pending_remote_ice.push(candidate);
            }
        } else {
            self.pending_remote_ice.push(candidate);
        }
        BackendReply::response(Response::Ok { id: command.id })
    }

    fn send_input(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(packet) = command.input else {
            return BackendReply::continue_without_response();
        };

        let Ok(payload) = packet.payload_bytes() else {
            return BackendReply::continue_without_response();
        };

        if payload.is_empty() || payload.len() > 4096 {
            return BackendReply::continue_without_response();
        }

        if let Some(pipeline) = self.pipeline.as_ref() {
            let _ = pipeline.send_input_packet(&payload, packet.partially_reliable);
        }

        BackendReply::continue_without_response()
    }

    fn set_input_paused(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(paused) = command.paused else {
            return BackendReply::response(missing_field(&command.id, "paused"));
        };

        if let Some(pipeline) = self.pipeline.as_ref() {
            pipeline.set_input_paused(paused);
        }

        BackendReply::response(Response::Ok { id: command.id })
    }

    fn update_render_surface(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(surface) = command.surface else {
            return BackendReply::response(missing_field(&command.id, "surface"));
        };

        self.render_surface = Some(surface.clone());
        if let Some(pipeline) = self.pipeline.as_ref() {
            pipeline.update_render_surface(surface);
        }

        BackendReply::response(Response::Ok { id: command.id })
    }

    fn update_bitrate_limit(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(max_bitrate_kbps) = command.max_bitrate_kbps else {
            return BackendReply::response(missing_field(&command.id, "maxBitrateKbps"));
        };

        let max_bitrate_kbps = normalize_bitrate_kbps(max_bitrate_kbps);
        update_context_bitrate_limit(&mut self.active_context, max_bitrate_kbps);

        let mut message = format!(
            "Updated native bitrate limit to {max_bitrate_kbps} Kbps. The active GFN server bitrate cap is negotiated in NVST SDP and will apply on the next native offer/reconnect."
        );
        if let Some(context) = self.active_context.as_ref() {
            if let Some(pipeline) = self.pipeline.as_ref() {
                pipeline.configure_stats(context, max_bitrate_kbps);
                // Recording bitrate follows the negotiated cap too, so a
                // mid-session bitrate change also applies to the NEXT
                // recording (its branch is rebuilt fresh on each start).
                pipeline.set_record_bitrate_kbps(max_bitrate_kbps);
                message = format!("Updated native bitrate limit to {max_bitrate_kbps} Kbps.");
            }
        }

        // Mid-session re-offer: rebuild the nvstSdp with the new cap and hand
        // back a re-answer the Electron main pushes to the server immediately
        // (the same channel used for the session-start answer). The server
        // reads `vqos.bw.maximumBitrateKbps` from nvstSdp in answer messages;
        // whether it honors a mid-session re-send varies by server build, and
        // the next natural offer/reconnect still applies it either way. The
        // answer SDP itself is unchanged apart from its video b=AS line.
        let mut events = Vec::new();
        let mut response = Response::Ok { id: command.id.clone() };
        if let (Some(nvst_params), Some(answer_sdp)) =
            (&self.last_nvst_params, &self.last_answer_sdp)
        {
            let updated_params = NvstParams {
                max_bitrate_kbps,
                ..nvst_params.clone()
            };
            match build_nvst_sdp_for_answer(&updated_params, answer_sdp) {
                Ok(nvst_sdp) => {
                    message = format!(
                        "Updated native bitrate limit to {max_bitrate_kbps} Kbps and requested a mid-session server re-offer (re-sending the answer with the new vqos.bw.maximumBitrateKbps)."
                    );
                    response = Response::Answer {
                        id: command.id,
                        answer: SendAnswerRequest {
                            sdp: rewrite_answer_video_bitrate(answer_sdp, max_bitrate_kbps),
                            nvst_sdp: Some(nvst_sdp),
                        },
                    };
                }
                Err(error) => {
                    events.push(Event::Log {
                        level: "warn",
                        message: format!(
                            "Updated native bitrate limit to {max_bitrate_kbps} Kbps but could not rebuild the mid-session nvstSdp: {error}"
                        ),
                    });
                }
            }
        }
        events.push(Event::Log {
            level: "info",
            message,
        });

        BackendReply {
            events,
            response: Some(response),
            should_continue: true,
        }
    }

    fn update_shortcuts(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(shortcuts) = command.shortcuts else {
            return BackendReply::response(missing_field(&command.id, "shortcuts"));
        };
        set_native_shortcut_bindings(&shortcuts);
        if let Some(context) = self.active_context.as_mut() {
            context.shortcuts = shortcuts;
        }
        BackendReply::response(Response::Ok { id: command.id })
    }

    fn set_microphone_enabled(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(enabled) = command.microphone_enabled else {
            return BackendReply::response(missing_field(&command.id, "microphoneEnabled"));
        };

        if let Some(context) = self.active_context.as_mut() {
            context.settings.microphone_enabled = enabled;
        }
        if let Some(pipeline) = self.pipeline.as_ref() {
            pipeline.set_microphone_enabled(enabled);
        }
        let attached = self
            .pipeline
            .as_ref()
            .is_some_and(|pipeline| pipeline.mic_attached());
        send_log(
            &self.event_sender,
            "info",
            format!(
                "Native microphone {}{}",
                if enabled { "unmuted" } else { "muted" },
                if attached {
                    ""
                } else {
                    " (no mic pipeline attached — session started without microphone)"
                }
            ),
        );
        BackendReply::response(Response::Ok { id: command.id })
    }

    fn take_screenshot(&mut self, command: CommandEnvelope) -> BackendReply {
        let id = command.id;
        let Some(pipeline) = self.pipeline.as_ref() else {
            return BackendReply::response(Response::Error {
                id: Some(id),
                code: "gstreamer-not-started".to_owned(),
                message: "GStreamer pipeline is not started.".to_owned(),
            });
        };

        match pipeline.capture_screenshot() {
            Ok(screenshot) => BackendReply {
                events: vec![Event::Screenshot { screenshot }],
                response: Some(Response::Ok { id }),
                should_continue: true,
            },
            Err(message) => BackendReply {
                events: vec![Event::Log {
                    level: "warn",
                    message: format!("Native screenshot failed: {message}"),
                }],
                response: Some(Response::Error {
                    id: Some(id),
                    code: "screenshot-failed".to_owned(),
                    message,
                }),
                should_continue: true,
            },
        }
    }

    fn start_recording(&mut self, command: CommandEnvelope) -> BackendReply {
        let id = command.id;
        let Some(pipeline) = self.pipeline.clone() else {
            return BackendReply::response(Response::Error {
                id: Some(id),
                code: "gstreamer-not-started".to_owned(),
                message: "GStreamer pipeline is not started.".to_owned(),
            });
        };
        let event_sender = self.event_sender.clone();
        if self
            .recording_worker
            .tx
            .send(RecordingCommand::Start {
                pipeline,
                event_sender,
            })
            .is_err()
        {
            return BackendReply::response(Response::Error {
                id: Some(id),
                code: "recording-worker-unavailable".to_owned(),
                message: "Recording worker is not available.".to_owned(),
            });
        }
        // Fire-and-forget: the valve open (or the spent-branch rebuild) runs
        // on the worker, so the command loop never blocks and input/surface/
        // bitrate commands keep flowing while recording starts.
        BackendReply::response(Response::Ok { id })
    }

    fn stop_recording(&mut self, command: CommandEnvelope) -> BackendReply {
        let id = command.id;
        let finalize = command.finalize.unwrap_or(true);
        let Some(pipeline) = self.pipeline.clone() else {
            return BackendReply::response(Response::Error {
                id: Some(id),
                code: "gstreamer-not-started".to_owned(),
                message: "GStreamer pipeline is not started.".to_owned(),
            });
        };
        let event_sender = self.event_sender.clone();
        if self
            .recording_worker
            .tx
            .send(RecordingCommand::Stop {
                pipeline,
                finalize,
                event_sender,
            })
            .is_err()
        {
            return BackendReply::response(Response::Error {
                id: Some(id),
                code: "recording-worker-unavailable".to_owned(),
                message: "Recording worker is not available.".to_owned(),
            });
        }
        // Fire-and-forget: drain + EOS finalize runs on the worker and the
        // `recording-finished` event still arrives strictly after the last
        // `recording-chunk` (both flow through the same FIFO event channel,
        // and the worker sends it only after the muxer EOS). The command loop
        // stays responsive — input never blocks behind a slow recording stop.
        BackendReply::response(Response::Ok { id })
    }

    fn send_data_channel_message(&mut self, command: CommandEnvelope) -> BackendReply {
        let id = command.id;
        let Some(label) = command.label.as_deref() else {
            return BackendReply::response(Response::Error {
                id: Some(id),
                code: "missing-field".to_owned(),
                message: "data-channel-message requires a label.".to_owned(),
            });
        };
        let Some(payload_base64) = command.payload_base64.as_deref() else {
            return BackendReply::response(Response::Error {
                id: Some(id),
                code: "missing-field".to_owned(),
                message: "data-channel-message requires a payloadBase64.".to_owned(),
            });
        };
        match crate::gstreamer_input::send_remote_data_channel_message(label, payload_base64) {
            Ok(()) => BackendReply::response(Response::Ok { id }),
            Err(message) => BackendReply::response(Response::Error {
                id: Some(id),
                code: "data-channel-send-failed".to_owned(),
                message,
            }),
        }
    }

    fn stop(&mut self, command: CommandEnvelope) -> BackendReply {
        self.active_context = None;
        self.pending_remote_ice.clear();
        self.remote_description_set = false;
        clear_native_shortcut_bindings();
        // Drain the recording worker FIRST so no per-command Arc clone of the
        // pipeline is still alive; then the backend's own Arc is the only one
        // left and the pipeline can be torn down explicitly.
        self.recording_worker.shutdown();
        if let Some(mut pipeline) = self.pipeline.take() {
            match Arc::get_mut(&mut pipeline) {
                Some(pipeline) => {
                    if let Err(message) = pipeline.stop() {
                        return BackendReply {
                            events: vec![Event::Error {
                                code: "gstreamer-stop-failed".to_owned(),
                                message: message.clone(),
                            }],
                            response: Some(Response::Error {
                                id: Some(command.id),
                                code: "gstreamer-stop-failed".to_owned(),
                                message,
                            }),
                            should_continue: true,
                        };
                    }
                }
                None => send_log(
                    &self.event_sender,
                    "warn",
                    "Recording worker still holds the pipeline after shutdown; skipping explicit teardown (process exits)."
                        .to_owned(),
                ),
            }
        }
        let message = command
            .reason
            .unwrap_or_else(|| "stop requested".to_owned());
        BackendReply::stop(command.id, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gstreamer_config::PRESENT_LIMITER_AUTO_SENTINEL;
    use crate::gstreamer_input::parse_input_handshake_version;
    use crate::gstreamer_liveness::{
        caps_framerate_summary, sink_stats_summary, VideoStallAction, VideoStallTracker,
    };
    use crate::gstreamer_pipeline::{
        align_windows_vulkan_download_factory, backend_runs_on_platform,
        configure_stats_overlay_element, default_rtp_video_api_priority, effective_present_max_fps,
        format_video_chain_selection, init_gstreamer, post_decode_caps_for,
        preferred_rtp_video_apis_for, resolve_gstreamer_stun_server, rtp_video_chain_definition,
        DISPLAY_NV12_FULL_RANGE_CAPS, RtpVideoApi, RtpVideoChainRole,
    };
    use crate::gstreamer_transitions::resolve_queue_mode;
    use crate::protocol::{IceServer, NativeQueueMode, StreamSettings, VideoCodec};
    use crate::sdp::IceCredentials;
    use gst::prelude::*;
    use gstreamer as gst;
    use gstreamer_webrtc as gst_webrtc;

    #[test]
    fn builds_and_stops_webrtc_pipeline() {
        let mut pipeline = GstreamerPipeline::build(None, &[]).expect("GStreamer webrtcbin pipeline");
        assert_eq!(pipeline.webrtc.name(), "opennow-webrtcbin");
        assert_eq!(
            pipeline.webrtc.property::<String>("stun-server"),
            "stun://stun2.l.google.com:19302"
        );
        pipeline.stop().expect("pipeline stops");
    }

    #[test]
    fn configures_session_stun_server_for_gstreamer() {
        let servers = vec![
            IceServer {
                urls: vec!["turn:relay.example.test:3478".to_owned()],
                username: None,
                credential: None,
            },
            IceServer {
                urls: vec!["stun:192.0.2.10:19302".to_owned()],
                username: None,
                credential: None,
            },
        ];

        assert_eq!(
            resolve_gstreamer_stun_server(&servers),
            "stun://192.0.2.10:19302"
        );
        let mut pipeline =
            GstreamerPipeline::build(None, &servers).expect("GStreamer webrtcbin pipeline");
        assert_eq!(
            pipeline.webrtc.property::<String>("stun-server"),
            "stun://192.0.2.10:19302"
        );
        pipeline.stop().expect("pipeline stops");
    }

    #[test]
    fn configures_dwrite_stats_overlay_without_type_panics() {
        gst::init().expect("gstreamer init");
        let Some(overlay) = gst::ElementFactory::make("dwritetextoverlay").build().ok() else {
            return;
        };

        configure_stats_overlay_element(&overlay);
        overlay.set_property("text", "OpenNOW native stats");
    }

    #[test]
    fn parses_basic_remote_offer_sdp() {
        let sdp = "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\nc=IN IP4 127.0.0.1\r\na=mid:0\r\na=sctp-port:5000\r\n";
        let parsed = GstreamerPipeline::parse_offer_sdp(sdp).expect("valid SDP");
        assert_eq!(parsed.medias_len(), 1);
    }

    #[test]
    fn defers_gfn_uuid_ice_password_until_actual_ice_stream_exists() {
        let mut pipeline =
            GstreamerPipeline::build(None, &[]).expect("GStreamer webrtcbin pipeline");
        let credentials = IceCredentials {
            ufrag: "2efecf37".to_owned(),
            pwd: "26b335b8-6cb2-4c18-96d0-963e5e586c9a".to_owned(),
            fingerprint: String::new(),
        };

        pipeline.original_remote_ice_credentials = Some(credentials);
        assert!(!pipeline
            .try_restore_original_remote_ice_credentials("without negotiated streams")
            .expect("remote ICE credential restoration can be deferred"));
        pipeline.stop().expect("pipeline stops");
    }

    #[test]
    fn remote_ice_credential_restore_after_remote_description_does_not_probe_fake_streams() {
        let mut pipeline =
            GstreamerPipeline::build(None, &[]).expect("GStreamer webrtcbin pipeline");
        let sdp = concat!(
            "v=0\r\n",
            "o=- 4373647202393833435 2 IN IP4 127.0.0.1\r\n",
            "s=-\r\n",
            "t=0 0\r\n",
            "a=group:BUNDLE 0 1 2 3\r\n",
            "a=ice-options:trickle\r\n",
            "a=ice-lite\r\n",
            "m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:0\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=rtcp-mux\r\n",
            "a=rtpmap:111 OPUS/48000/2\r\n",
            "m=video 9 UDP/TLS/RTP/SAVPF 96\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:1\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=rtcp-mux\r\n",
            "a=rtpmap:96 H264/90000\r\n",
            "m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:2\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=sctp-port:5000\r\n",
            "m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:3\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=sctp-port:5000\r\n",
        );
        let offer_sdp = GstreamerPipeline::parse_offer_sdp(sdp).expect("valid SDP");
        let offer =
            gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Offer, offer_sdp);
        pipeline
            .pipeline
            .set_state(gst::State::Playing)
            .expect("pipeline plays");
        pipeline
            .set_description("set-remote-description", &offer)
            .expect("remote description");

        let credentials = IceCredentials {
            ufrag: "2efecf37".to_owned(),
            pwd: "26b335b8-99a8-4ffa-b9aa-f38ddad1e2b4".to_owned(),
            fingerprint: String::new(),
        };
        pipeline.original_remote_ice_credentials = Some(credentials);
        pipeline
            .try_restore_original_remote_ice_credentials("after remote description")
            .expect("remote ICE credential restoration does not fail without actual streams");
        pipeline.stop().expect("pipeline stops");
    }

    #[test]
    fn reports_offer_answer_and_local_ice_capabilities() {
        let backend = GstreamerBackend::new(None);
        let capabilities = backend.capabilities();
        assert!(capabilities.supports_offer_answer);
        assert!(capabilities.supports_local_ice);
        assert!(capabilities.supports_input);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn bundles_av1_rtp_depayloading() {
        init_gstreamer().expect("GStreamer initializes");
        assert!(gst::ElementFactory::find("rtpav1depay").is_some());
    }

    #[test]
    fn parses_input_handshake_versions() {
        assert_eq!(
            parse_input_handshake_version(&[0x0e, 0x02, 0x03, 0x00]),
            Some(3)
        );
        assert_eq!(parse_input_handshake_version(&[0x0e, 0x02]), Some(2));
        assert_eq!(parse_input_handshake_version(&[0x0e, 0x03]), Some(0x030e));
        assert_eq!(parse_input_handshake_version(&[0x01, 0x02, 0x03]), None);
        assert_eq!(parse_input_handshake_version(&[0x0e]), None);
    }

    #[test]
    fn maps_rtp_video_codecs_to_explicit_gpu_decode_chains() {
        let h265 =
            rtp_video_chain_definition("H265", RtpVideoApi::D3D11).expect("H265 D3D11 chain");
        // GStreamer 1.29.x `rtph265depay` rejects receive-pad caps carrying the
        // H265 fmtp fields (`profile-id`/`level-id`/`tier-flag`); the chain
        // strips them with a capsfilter ahead of the depayloader.
        assert_eq!(h265[0].factory, "capsfilter");
        assert_eq!(h265[0].role, RtpVideoChainRole::ReceiveCapsFilter);
        assert_eq!(
            h265[0].caps.as_deref(),
            Some("application/x-rtp, encoding-name=H265")
        );
        assert_eq!(h265[1].factory, "rtph265depay");
        assert_eq!(h265[4].factory, "d3d11h265dec");
        // AV1/H265 take the safe present path: download + videoconvert to
        // system NV12 instead of zero-copy D3DMemory presentation.
        assert_eq!(h265[5].factory, "d3d11download");
        assert_eq!(h265[6].factory, "videoconvert");
        assert_eq!(h265[7].role, RtpVideoChainRole::PostDecodeCapsFilter);
        assert_eq!(h265[7].caps.as_deref(), Some(DISPLAY_NV12_FULL_RANGE_CAPS));
        assert_eq!(h265[8].factory, "dwritetextoverlay");
        assert_eq!(h265[10].factory, "d3d11videosink");

        let h264 =
            rtp_video_chain_definition("h264", RtpVideoApi::D3D12).expect("H264 D3D12 chain");
        assert_eq!(h264[0].factory, "rtph264depay");
        assert_eq!(h264[3].factory, "d3d12h264dec");
        // D3D12 H264 uses the safe system-memory present path: the field
        // regression showed zero-copy decoding stop after the first frame
        // while encoded RTP continued to arrive.
        assert_eq!(h264[4].factory, "d3d12download");
        assert_eq!(h264[5].factory, "videoconvert");
        assert_eq!(h264[6].role, RtpVideoChainRole::PostDecodeCapsFilter);
        assert_eq!(h264[6].caps.as_deref(), Some(DISPLAY_NV12_FULL_RANGE_CAPS));
        assert_eq!(h264[7].factory, "dwritetextoverlay");
        assert_eq!(h264[9].factory, "d3d12videosink");
        assert!(!h264
            .iter()
            .any(|spec| spec.role == RtpVideoChainRole::ReceiveCapsFilter));

        let av1 = rtp_video_chain_definition("AV1", RtpVideoApi::D3D11).expect("AV1 D3D11 chain");
        // The stock rtpav1depay produced zero frames on some GFN AV1 streams;
        // AV1 chains now use the custom lenient gfnav1depay.
        assert_eq!(av1[0].factory, "gfnav1depay");
        assert_eq!(av1[3].factory, "d3d11av1dec");
        assert_eq!(av1[4].factory, "d3d11download");
        assert_eq!(av1[5].factory, "videoconvert");
        assert_eq!(av1[6].role, RtpVideoChainRole::PostDecodeCapsFilter);
        assert_eq!(av1[6].caps.as_deref(), Some(DISPLAY_NV12_FULL_RANGE_CAPS));
        assert_eq!(av1[7].factory, "dwritetextoverlay");
        assert_eq!(av1[9].factory, "d3d11videosink");
        assert!(!av1
            .iter()
            .any(|spec| spec.role == RtpVideoChainRole::ReceiveCapsFilter));
    }

    #[test]
    fn does_not_force_d3d_memory_caps_by_default() {
        let d3d11 =
            rtp_video_chain_definition("H265", RtpVideoApi::D3D11).expect("H265 D3D11 chain");
        let d3d12 =
            rtp_video_chain_definition("H264", RtpVideoApi::D3D12).expect("H264 D3D12 chain");

        // No capsfilter demands a D3D memory type without zero-copy being
        // requested; the AV1/H265 safe-present capsfilter is system NV12.
        assert!(!d3d11.iter().any(|spec| spec
            .caps
            .as_deref()
            .is_some_and(|caps| caps.contains("memory:"))));
        assert!(!d3d12.iter().any(|spec| spec
            .caps
            .as_deref()
            .is_some_and(|caps| caps.contains("memory:"))));
    }

    #[test]
    fn post_decode_caps_stay_memory_only_with_safe_present_for_ten_bit_codecs() {
        // post_decode_caps_for no longer forces format=NV12: the D3D DXVA
        // decoders refuse to convert 10-bit→8-bit internally (not-negotiated
        // for a 10-bit stream), so the caps stay memory-only and the safe
        // present path does the conversion with download + videoconvert.
        let d3d11_av1 =
            post_decode_caps_for(RtpVideoApi::D3D11, "AV1", "video/x-raw(memory:D3D11Memory)");
        assert_eq!(d3d11_av1, "video/x-raw(memory:D3D11Memory)");
        let d3d12_h265 = post_decode_caps_for(
            RtpVideoApi::D3D12,
            "H265",
            "video/x-raw(memory:D3D12Memory)",
        );
        assert_eq!(d3d12_h265, "video/x-raw(memory:D3D12Memory)");
        let d3d11_h264 = post_decode_caps_for(
            RtpVideoApi::D3D11,
            "H264",
            "video/x-raw(memory:D3D11Memory)",
        );
        assert_eq!(d3d11_h264, "video/x-raw(memory:D3D11Memory)");

        // The chain routes every D3D codec through download + videoconvert to
        // system NV12 instead of presenting the D3D texture zero-copy.
        let chain = rtp_video_chain_definition("AV1", RtpVideoApi::D3D12).expect("AV1 D3D12 chain");
        assert!(chain.iter().any(|spec| spec.factory == "d3d12download"));
        assert!(chain.iter().any(|spec| spec.factory == "videoconvert"));
        assert!(chain.iter().any(|spec| {
            spec.role == RtpVideoChainRole::PostDecodeCapsFilter
                && spec.caps.as_deref() == Some(DISPLAY_NV12_FULL_RANGE_CAPS)
        }));

        // H264 on D3D12 uses the same safe system-memory path. This guards
        // against reintroducing the zero-copy stall seen in the field.
        let h264 =
            rtp_video_chain_definition("H264", RtpVideoApi::D3D12).expect("H264 D3D12 chain");
        assert!(h264.iter().any(|spec| spec.factory == "d3d12download"));
        assert!(h264.iter().any(|spec| spec.factory == "videoconvert"));
        assert!(h264.iter().any(|spec| {
            spec.role == RtpVideoChainRole::PostDecodeCapsFilter
                && spec.caps.as_deref() == Some(DISPLAY_NV12_FULL_RANGE_CAPS)
        }));

        // D3D11 H264 uses the SAME safe present chain — zero-copy D3DMemory
        // presented gray/pink garbage on some GPU/driver combos, so it is no
        // longer eligible for zero-copy.
        let d3d11_h264 =
            rtp_video_chain_definition("H264", RtpVideoApi::D3D11).expect("H264 D3D11 chain");
        assert!(d3d11_h264
            .iter()
            .any(|spec| spec.factory == "d3d11download"));
        assert!(d3d11_h264.iter().any(|spec| {
            spec.role == RtpVideoChainRole::PostDecodeCapsFilter
                && spec.caps.as_deref() == Some(DISPLAY_NV12_FULL_RANGE_CAPS)
        }));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn software_av1_decoder_override_removes_d3d_download_stage() {
        // Simulate OPENNOW_NATIVE_AV1_DECODER=dav1d on a D3D12 backend: the
        // decoder replacement happens in rtp_video_chain_specs, then
        // align_windows_vulkan_download_factory must drop the D3D download
        // stage (dav1ddec outputs system memory) while keeping the
        // videoconvert + NV12 capsfilter so the D3D sink still receives
        // 8-bit NV12 it can upload.
        let mut specs =
            rtp_video_chain_definition("AV1", RtpVideoApi::D3D12).expect("AV1 D3D12 chain");
        for spec in &mut specs {
            if spec.role == RtpVideoChainRole::Decoder {
                spec.factory = "dav1ddec";
            }
        }
        align_windows_vulkan_download_factory(&mut specs, "dav1ddec");

        assert!(!specs
            .iter()
            .any(|spec| spec.factory == "d3d12download" || spec.factory == "d3d11download"));
        assert_eq!(
            specs
                .iter()
                .find(|spec| spec.role == RtpVideoChainRole::Decoder)
                .map(|spec| spec.factory),
            Some("dav1ddec")
        );
        assert!(specs.iter().any(|spec| spec.factory == "videoconvert"));
        assert!(specs.iter().any(|spec| {
            spec.role == RtpVideoChainRole::PostDecodeCapsFilter
                && spec.caps.as_deref() == Some(DISPLAY_NV12_FULL_RANGE_CAPS)
        }));
        assert_eq!(
            specs.last().map(|spec| spec.factory),
            Some("d3d12videosink")
        );
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn software_h265_decoder_override_keeps_convert_stage() {
        let mut specs =
            rtp_video_chain_definition("H265", RtpVideoApi::D3D11).expect("H265 D3D11 chain");
        for spec in &mut specs {
            if spec.role == RtpVideoChainRole::Decoder {
                spec.factory = "avdec_h265";
            }
        }
        align_windows_vulkan_download_factory(&mut specs, "avdec_h265");

        assert!(!specs
            .iter()
            .any(|spec| spec.factory == "d3d11download" || spec.factory == "d3d12download"));
        assert!(specs.iter().any(|spec| spec.factory == "videoconvert"));
        assert_eq!(
            specs
                .iter()
                .find(|spec| spec.role == RtpVideoChainRole::Decoder)
                .map(|spec| spec.factory),
            Some("avdec_h265")
        );
        assert_eq!(
            specs.last().map(|spec| spec.factory),
            Some("d3d11videosink")
        );
    }

    #[test]
    fn maps_cross_platform_video_paths_to_expected_decoders() {
        // Chains carry a pre-decode queue at varying positions, so assert on the
        // Decoder role instead of a hard-coded index (which drifted when the
        // pre-decode queue was added and left this test silently broken until it
        // ran with the gstreamer feature enabled).
        let vt =
            rtp_video_chain_definition("H264", RtpVideoApi::VideoToolbox).expect("VideoToolbox");
        assert_eq!(vt[3].factory, "vtdec_hw");
        assert!(vt.iter().any(|spec| spec.factory == "videoconvert"));
        assert_eq!(vt.last().map(|spec| spec.factory), Some("glimagesink"));
        assert!(!vt.iter().any(|spec| spec.factory == "capsfilter"));

        let vaapi = rtp_video_chain_definition("AV1", RtpVideoApi::Vaapi).expect("VAAPI AV1");
        assert_eq!(vaapi[3].factory, "vaav1dec");
        assert!(vaapi.iter().any(|spec| spec.factory == "videoconvert"));
        assert_eq!(vaapi.last().map(|spec| spec.factory), Some("glimagesink"));

        let nvdec = rtp_video_chain_definition("AV1", RtpVideoApi::Nvdec).expect("NVDEC AV1");
        assert_eq!(nvdec[3].factory, "nvav1dec");
        assert!(nvdec.iter().any(|spec| spec.factory == "videoconvert"));
        assert_eq!(nvdec.last().map(|spec| spec.factory), Some("glimagesink"));

        let v4l2 = rtp_video_chain_definition("H265", RtpVideoApi::V4L2).expect("V4L2 H265");
        assert_eq!(
            v4l2.iter()
                .find(|spec| spec.role == RtpVideoChainRole::Decoder)
                .map(|spec| spec.factory.as_ref()),
            Some("v4l2slh265dec")
        );
        assert!(!v4l2.iter().any(|spec| spec.factory == "videoconvert"));

        let v4l2_av1 = rtp_video_chain_definition("AV1", RtpVideoApi::V4L2).expect("V4L2 AV1");
        assert_eq!(
            v4l2_av1
                .iter()
                .find(|spec| spec.role == RtpVideoChainRole::Decoder)
                .map(|spec| spec.factory.as_ref()),
            Some("v4l2slav1dec")
        );

        let vulkan = rtp_video_chain_definition("H265", RtpVideoApi::Vulkan).expect("Vulkan H265");
        #[cfg(target_os = "windows")]
        {
            assert_eq!(
                vulkan
                    .iter()
                    .find(|spec| spec.role == RtpVideoChainRole::Decoder)
                    .map(|spec| spec.factory.as_ref()),
                Some("d3d12h265dec")
            );
            assert!(!vulkan.iter().any(|spec| spec.factory == "vulkanh265dec"));
            // Default Internal renderer: Electron cannot composite vulkansink.
            assert_eq!(
                vulkan.last().map(|spec| spec.factory),
                Some("d3d12videosink")
            );
            assert!(vulkan.iter().any(|spec| {
                spec.role == RtpVideoChainRole::StatsOverlay && spec.factory == "dwritetextoverlay"
            }));
            assert!(!vulkan.iter().any(|spec| spec.factory == "vulkanupload"));
        }
        #[cfg(not(target_os = "windows"))]
        {
            assert_eq!(
                vulkan
                    .iter()
                    .find(|spec| spec.role == RtpVideoChainRole::Decoder)
                    .map(|spec| spec.factory.as_ref()),
                Some("vulkanh265dec")
            );
            assert!(vulkan
                .iter()
                .any(|spec| spec.factory == "vulkancolorconvert"));
            assert_eq!(vulkan.last().map(|spec| spec.factory), Some("vulkansink"));
        }
        let vulkan_av1 =
            rtp_video_chain_definition("AV1", RtpVideoApi::Vulkan).expect("Vulkan AV1");
        #[cfg(target_os = "windows")]
        assert_eq!(
            vulkan_av1
                .iter()
                .find(|spec| spec.role == RtpVideoChainRole::Decoder)
                .map(|spec| spec.factory.as_ref()),
            Some("d3d12av1dec")
        );
        #[cfg(not(target_os = "windows"))]
        assert_eq!(
            vulkan_av1
                .iter()
                .find(|spec| spec.role == RtpVideoChainRole::Decoder)
                .map(|spec| spec.factory.as_ref()),
            Some("vulkanav1dec")
        );

        let software =
            rtp_video_chain_definition("H264", RtpVideoApi::Software).expect("software H264");
        assert_eq!(
            software
                .iter()
                .find(|spec| spec.role == RtpVideoChainRole::Decoder)
                .map(|spec| spec.factory.as_ref()),
            Some("avdec_h264")
        );
        assert!(software.iter().any(|spec| spec.factory == "videoconvert"));
        assert_eq!(
            software.last().map(|spec| spec.factory),
            Some("autovideosink")
        );
    }

    #[test]
    fn exposes_vulkan_on_windows_and_linux_only() {
        assert!(backend_runs_on_platform(RtpVideoApi::Vulkan, "windows"));
        assert!(backend_runs_on_platform(RtpVideoApi::Vulkan, "linux"));
        assert!(!backend_runs_on_platform(RtpVideoApi::Vulkan, "macos"));
        assert!(!backend_runs_on_platform(RtpVideoApi::Vulkan, "other"));
    }

    #[test]
    fn explicit_linux_backend_selection_does_not_fall_back() {
        assert_eq!(
            preferred_rtp_video_apis_for("nvdec", Some(120)),
            vec![RtpVideoApi::Nvdec]
        );
        assert_eq!(
            preferred_rtp_video_apis_for("vaapi", Some(120)),
            vec![RtpVideoApi::Vaapi]
        );
        assert_eq!(
            preferred_rtp_video_apis_for("v4l2", Some(120)),
            vec![RtpVideoApi::V4L2]
        );
        assert_eq!(
            preferred_rtp_video_apis_for("vulkan", Some(240)),
            vec![RtpVideoApi::Vulkan]
        );
        assert_eq!(
            preferred_rtp_video_apis_for("vk", Some(120)),
            vec![RtpVideoApi::Vulkan]
        );
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_default_video_api_prefers_d3d12_for_high_fps() {
        assert_eq!(
            default_rtp_video_api_priority(Some(240)),
            vec![
                RtpVideoApi::D3D12,
                RtpVideoApi::D3D11,
                RtpVideoApi::Software
            ]
        );
        assert_eq!(
            default_rtp_video_api_priority(Some(120)),
            vec![
                RtpVideoApi::D3D11,
                RtpVideoApi::D3D12,
                RtpVideoApi::Software
            ]
        );
    }

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    fn linux_arm64_prefers_v4l2_for_raspberry_pi_and_arm_devices() {
        assert_eq!(
            default_rtp_video_api_priority(Some(60)),
            vec![
                RtpVideoApi::V4L2,
                RtpVideoApi::Nvdec,
                RtpVideoApi::Vaapi,
                RtpVideoApi::Vulkan,
                RtpVideoApi::Software,
            ]
        );
    }

    #[test]
    #[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
    fn linux_desktop_prefers_vendor_decoders_before_generic_paths() {
        assert_eq!(
            default_rtp_video_api_priority(Some(120)),
            vec![
                RtpVideoApi::Nvdec,
                RtpVideoApi::Vaapi,
                RtpVideoApi::Vulkan,
                RtpVideoApi::V4L2,
                RtpVideoApi::Software,
            ]
        );
    }

    #[test]
    fn automatic_present_limiter_targets_d3d_present_paths() {
        assert_eq!(
            effective_present_max_fps(
                PRESENT_LIMITER_AUTO_SENTINEL,
                Some(240),
                RtpVideoApi::D3D11,
                Some(165)
            ),
            165
        );
        assert_eq!(
            effective_present_max_fps(
                PRESENT_LIMITER_AUTO_SENTINEL,
                Some(240),
                RtpVideoApi::D3D12,
                Some(165)
            ),
            165
        );
        assert_eq!(
            effective_present_max_fps(144, Some(240), RtpVideoApi::D3D12, Some(165)),
            144
        );
        assert_eq!(
            effective_present_max_fps(0, Some(240), RtpVideoApi::D3D11, Some(165)),
            0
        );
        // Default (non-G-Sync) policy paces to the stream fps on every path,
        // regardless of display Hz: it thins jitter bursts back to real-time.
        assert_eq!(
            effective_present_max_fps(
                PRESENT_LIMITER_STREAM_SENTINEL,
                Some(60),
                RtpVideoApi::D3D12,
                Some(60)
            ),
            60
        );
        assert_eq!(
            effective_present_max_fps(
                PRESENT_LIMITER_STREAM_SENTINEL,
                Some(60),
                RtpVideoApi::D3D12,
                Some(144)
            ),
            60
        );
        assert_eq!(
            effective_present_max_fps(
                PRESENT_LIMITER_STREAM_SENTINEL,
                None,
                RtpVideoApi::Software,
                Some(60)
            ),
            0
        );
        assert_eq!(
            effective_present_max_fps(
                PRESENT_LIMITER_VRR_SENTINEL,
                Some(240),
                RtpVideoApi::D3D11,
                Some(165)
            ),
            162
        );
        assert_eq!(
            effective_present_max_fps(
                PRESENT_LIMITER_VRR_SENTINEL,
                Some(120),
                RtpVideoApi::D3D11,
                Some(165)
            ),
            0
        );
    }

    #[test]
    fn formats_selected_video_chain_diagnostics() {
        let specs =
            rtp_video_chain_definition("H264", RtpVideoApi::Software).expect("software H264");
        let message = format_video_chain_selection("H264", RtpVideoApi::Software, &specs);

        assert!(message.contains("backend=software"));
        assert!(message.contains("decoder=avdec_h264"));
        assert!(message.contains("converter=videoconvert"));
        assert!(message.contains("memory=system-memory"));
    }

    #[test]
    fn extracts_caps_framerate_summary() {
        let caps = "video/x-raw(memory:D3D11Memory), format=(string)NV12, framerate=(fraction)240/1; zeroCopyD3D11=true";
        assert_eq!(caps_framerate_summary(caps).as_deref(), Some("240/1"));
        assert_eq!(caps_framerate_summary("video/x-raw").as_deref(), None);
    }

    #[test]
    fn video_stall_tracker_waits_until_threshold() {
        let mut tracker = VideoStallTracker::default();

        assert_eq!(tracker.evaluate(2_499, 0), VideoStallAction::None);
    }

    #[test]
    fn video_stall_tracker_progresses_recovery_attempts() {
        let mut tracker = VideoStallTracker::default();

        assert_eq!(
            tracker.evaluate(2_500, 0),
            VideoStallAction::RequestKeyframe {
                attempt: 1,
                stall_ms: 2_500,
            },
        );
        assert_eq!(tracker.evaluate(3_000, 0), VideoStallAction::None);
        assert_eq!(
            tracker.evaluate(5_000, 0),
            VideoStallAction::RequestKeyframe {
                attempt: 2,
                stall_ms: 5_000,
            },
        );
        assert_eq!(
            tracker.evaluate(8_000, 0),
            VideoStallAction::Resync {
                attempt: 3,
                stall_ms: 8_000,
            },
        );
        assert_eq!(
            tracker.evaluate(12_000, 0),
            VideoStallAction::PartialFlush {
                attempt: 4,
                stall_ms: 12_000,
            },
        );
        assert_eq!(
            tracker.evaluate(16_000, 0),
            VideoStallAction::CompleteFlush {
                attempt: 5,
                stall_ms: 16_000,
            },
        );
        assert_eq!(
            tracker.evaluate(20_000, 0),
            VideoStallAction::Fatal {
                attempt: 6,
                stall_ms: 20_000,
            },
        );
    }

    #[test]
    fn video_stall_tracker_resets_after_recovery() {
        let mut tracker = VideoStallTracker::default();

        assert_eq!(
            tracker.evaluate(2_500, 0),
            VideoStallAction::RequestKeyframe {
                attempt: 1,
                stall_ms: 2_500,
            },
        );
        assert_eq!(
            tracker.evaluate(2_600, 2_600),
            VideoStallAction::Recovered { stall_ms: 2_600 },
        );
        assert_eq!(tracker.evaluate(3_000, 2_600), VideoStallAction::None);
        assert_eq!(
            tracker.evaluate(5_100, 2_600),
            VideoStallAction::RequestKeyframe {
                attempt: 1,
                stall_ms: 2_500,
            },
        );
    }

    #[test]
    fn resolve_queue_mode_prefers_adaptive_for_240_fps_and_vrr_for_cloud_gsync() {
        let adaptive = resolve_queue_mode(&StreamSettings {
            resolution: "2560x1440".to_owned(),
            fps: 240,
            max_bitrate_mbps: 75,
            codec: VideoCodec::H265,
            color_quality: crate::protocol::ColorQuality::TenBit420,
            enable_cloud_gsync: false,
            fallback_codec: None,
            native_transition_diagnostics: None,
            mouse_sensitivity: 1.0,
            mouse_acceleration_percent: 1.0,
            native_sink_input_capture: false,
            microphone_enabled: false,
        });
        assert_eq!(adaptive, NativeQueueMode::Adaptive);

        let vrr = resolve_queue_mode(&StreamSettings {
            resolution: "2560x1440".to_owned(),
            fps: 120,
            max_bitrate_mbps: 75,
            codec: VideoCodec::H265,
            color_quality: crate::protocol::ColorQuality::TenBit420,
            enable_cloud_gsync: true,
            fallback_codec: None,
            native_transition_diagnostics: None,
            mouse_sensitivity: 1.0,
            mouse_acceleration_percent: 1.0,
            native_sink_input_capture: false,
            microphone_enabled: false,
        });
        assert_eq!(vrr, NativeQueueMode::Vrr);
    }

    #[test]
    fn reports_missing_sink_stats_as_unavailable() {
        gst::init().expect("gstreamer init");
        let sink = gst::ElementFactory::make("fakesink")
            .build()
            .expect("fakesink");
        assert_eq!(
            sink_stats_summary(&sink),
            "sinkStats rendered=0 dropped=0 averageRate=0.0"
        );
    }
}
