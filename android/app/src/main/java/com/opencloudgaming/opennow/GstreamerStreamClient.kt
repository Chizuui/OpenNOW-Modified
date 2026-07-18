package com.opencloudgaming.opennow

import android.content.Context
import android.util.Log
import android.view.Surface
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.webrtc.IceCandidate

/**
 * Experimental GStreamer-based streaming client.
 *
 * Replaces [NativeStreamClient] when [StreamSettings.nativeStreamerEnabled] is `true`.
 *
 * Architecture
 * ─────────────
 *  • Signaling (WebSocket) is handled by the existing [GfnSignalingClient], identical to the
 *    WebRTC path — the signaling server does not know or care which local renderer we use.
 *  • SDP offers and ICE candidates received from the server are forwarded to the native
 *    GStreamer pipeline via [NativeStreamerBridge].
 *  • The GStreamer pipeline generates an SDP answer and local ICE candidates, which are sent
 *    back to the server through the same [GfnSignalingClient].
 *  • Video is rendered directly onto the [Surface] provided via [attachSurface].
 *
 * Input (keyboard / gamepad) is NOT yet implemented in this experimental backend.
 */
class GstreamerStreamClient(
    context: Context,
    private val onState: (String) -> Unit,
    private val onError: (String) -> Unit,
    private val onFirstVideoFrameRendered: () -> Unit = {},
    private val onStreamStopped: () -> Unit = {},
) {
    companion object {
        private const val TAG = "GstreamerStreamClient"
    }

    private val appContext: Context = context.applicationContext
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    private var signaling: GfnSignalingClient? = null
    private var session: SessionInfo? = null
    private var settings: StreamSettings = StreamSettings()
    private var transportGeneration = 0

    private var renderSurface: Surface? = null
    private var released = false

    private val bridge = NativeStreamerBridge { tag, data ->
        handleNativeEvent(tag, data)
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /**
     * Provide the [Surface] that the GStreamer video sink should render into.
     * Call this before or after [start]; the bridge will pass it to the pipeline.
     * Call with `null` to detach (e.g. when SurfaceHolder is destroyed).
     */
    fun attachSurface(surface: Surface?) {
        renderSurface = surface
        Log.d(TAG, "attachSurface: ${if (surface != null) "attached" else "detached"}")
        // Forward to native immediately if pipeline is already up.
        if (!released) {
            runCatching { bridge.gstSetSurface(surface) }
        }
    }

    /**
     * Start the streaming session: initialise GStreamer, build the pipeline, and
     * connect to the signaling server.
     */
    fun start(session: SessionInfo, settings: StreamSettings) {
        if (released) return
        this.session = session
        this.settings = settings
        transportGeneration += 1
        val generation = transportGeneration

        NativeInputDiagnostics.add(
            "GstreamerStreamClient start session=${session.sessionId.take(12)} " +
                "gstAvailable=${NativeStreamerBridge.isGStreamerAvailable()} " +
                "settings=${settings.resolution}/${settings.fps}/${settings.codec}"
        )

        emitState("Connecting signaling")

        scope.launch {
            startPipelineAndSignaling(session, settings, generation)
        }
    }

    /**
     * Stop the session and release the GStreamer pipeline.
     */
    fun stop() {
        transportGeneration += 1
        closeTransport()
        emitState("Stopped")
    }

    /**
     * Release all resources. Must be called when the host Activity/Fragment is destroyed.
     */
    fun release() {
        if (released) return
        released = true
        stop()
        bridge.destroy()
        scope.launch { /* let pending coroutines drain */ delay(200) }.invokeOnCompletion {
            scope.coroutineContext[Job]?.cancel()
        }
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    private suspend fun startPipelineAndSignaling(
        session: SessionInfo,
        settings: StreamSettings,
        generation: Int,
    ) {
        val libLoaded = NativeStreamerBridge.isLibraryLoaded()
        NativeInputDiagnostics.add("GstreamerStreamClient step libLoaded=$libLoaded")
        if (!libLoaded) {
            emitError("GStreamer native library failed to load (opennow_native.so missing or corrupt).")
            return
        }

        val gstAvailable = NativeStreamerBridge.isGStreamerAvailable()
        NativeInputDiagnostics.add("GstreamerStreamClient step gstAvailable=$gstAvailable")
        if (!gstAvailable) {
            emitError("GStreamer is not available in this build (GSTREAMER_ROOT_ANDROID was not set at compile time).")
            return
        }

        emitState("Initialising GStreamer pipeline")
        NativeInputDiagnostics.add("GstreamerStreamClient step calling gstNativeInit")
        val initOk = runCatching { bridge.gstNativeInit() }.getOrElse { ex ->
            Log.e(TAG, "gstNativeInit threw: ${ex.message}")
            NativeInputDiagnostics.add("GstreamerStreamClient step gstNativeInit threw: ${ex.message}")
            false
        }
        NativeInputDiagnostics.add("GstreamerStreamClient step gstNativeInit=$initOk")
        if (!initOk) {
            emitError("GStreamer init failed (gstNativeInit returned false).")
            return
        }

        NativeInputDiagnostics.add("GstreamerStreamClient step calling gstCreatePipeline")
        val pipelineOk = runCatching { bridge.gstCreatePipeline(null) }.getOrElse { ex ->
            Log.e(TAG, "gstCreatePipeline threw: ${ex.message}")
            NativeInputDiagnostics.add("GstreamerStreamClient step gstCreatePipeline threw: ${ex.message}")
            false
        }
        NativeInputDiagnostics.add("GstreamerStreamClient step gstCreatePipeline=$pipelineOk")

        if (!pipelineOk) {
            emitError("Failed to create GStreamer pipeline. Check that GStreamer libs are present.")
            return
        }

        // If surface was already attached before pipeline was ready, set it now.
        renderSurface?.let { bridge.gstSetSurface(it) }

        if (generation != transportGeneration) return

        emitState("Connecting signaling")
        NativeInputDiagnostics.add("GstreamerStreamClient pipeline ready — connecting signaling")

        signaling = GfnSignalingClient(session, settings) { event ->
            if (generation == transportGeneration) handleSignaling(event)
        }.also { it.connect() }
    }

    private fun closeTransport() {
        signaling?.disconnect()
        signaling = null
        runCatching { bridge.gstDestroyPipeline() }
    }

    private var lastOfferSdp: String? = null

    private fun handleSignaling(event: SignalingEvent) {
        when (event) {
            SignalingEvent.Connected -> {
                NativeInputDiagnostics.add("GstreamerStreamClient signaling connected")
                emitState("Waiting for offer")
            }
            is SignalingEvent.Disconnected -> {
                NativeInputDiagnostics.add("GstreamerStreamClient signaling disconnected: ${event.reason}")
                val isTerminated = event.reason.contains("code=1000", ignoreCase = true) ||
                    event.reason.contains("http=410", ignoreCase = true) ||
                    event.reason.contains("http=404", ignoreCase = true)
                if (isTerminated) {
                    stop()
                    scope.launch { onStreamStopped() }
                } else {
                    emitError("Signaling disconnected: ${event.reason}")
                }
            }
            is SignalingEvent.Error -> {
                NativeInputDiagnostics.add("GstreamerStreamClient signaling error: ${event.message}")
                emitError("Signaling error: ${event.message}")
            }
            is SignalingEvent.Log -> NativeInputDiagnostics.add("GstreamerStreamClient sig-log: ${event.message}")
            is SignalingEvent.Offer -> {
                NativeInputDiagnostics.add("GstreamerStreamClient received SDP offer (${event.sdp.length} chars)")
                emitState("Streaming (GStreamer)")
                lastOfferSdp = event.sdp
                bridge.gstSetRemoteOffer(event.sdp)
            }
            is SignalingEvent.RemoteIce -> {
                val c: IceCandidate = event.candidate
                bridge.gstAddRemoteIce(
                    candidate = c.sdp,
                    sdpMid = c.sdpMid,
                    sdpMLineIndex = c.sdpMLineIndex,
                )
            }
        }
    }

    /** Handles callbacks emitted from the C++ GLib main loop thread. */
    private fun handleNativeEvent(tag: String, data: String) {
        Log.d(TAG, "native event tag=$tag data=${data.take(120)}")
        when (tag) {
            "sdp-answer" -> {
                // Forward the GStreamer-generated answer back to the signaling server.
                NativeInputDiagnostics.add("GstreamerStreamClient sending SDP answer via signaling")
                val offer = lastOfferSdp
                if (offer != null) {
                    val nvst = runCatching {
                        SdpTools.buildNvstSdp(offerSdp = offer, settings = settings, localAnswer = data)
                    }.getOrNull()
                    scope.launch { signaling?.sendAnswer(data, nvstSdp = nvst) }
                } else {
                    scope.launch { signaling?.sendAnswer(data, nvstSdp = null) }
                }
            }
            "ice-candidate" -> {
                // data = "mlineIndex|candidateString"
                val parts = data.split("|", limit = 2)
                val mlineIndex = parts.getOrNull(0)?.toIntOrNull() ?: 0
                val candidateSdp = parts.getOrNull(1) ?: return
                val iceCandidate = IceCandidate(/* sdpMid */ "0", mlineIndex, candidateSdp)
                NativeInputDiagnostics.add("GstreamerStreamClient sending local ICE candidate")
                scope.launch { signaling?.sendIceCandidate(iceCandidate) }
            }
            "status" -> when (data) {
                "video-pad-linked" -> {
                    NativeInputDiagnostics.add("GstreamerStreamClient video pad linked — first frame expected")
                    scope.launch { onFirstVideoFrameRendered() }
                }
                "pipeline-destroyed" -> NativeInputDiagnostics.add("GstreamerStreamClient pipeline destroyed")
                else -> NativeInputDiagnostics.add("GstreamerStreamClient status: $data")
            }
            "error" -> {
                NativeInputDiagnostics.add("GstreamerStreamClient native error: $data")
                emitError("GStreamer pipeline error: $data")
            }
        }
    }

    private fun emitState(state: String) {
        scope.launch { onState(state) }
    }

    private fun emitError(message: String) {
        Log.e(TAG, message)
        NativeInputDiagnostics.add("GstreamerStreamClient error: $message")
        scope.launch { onError(message) }
    }
}
