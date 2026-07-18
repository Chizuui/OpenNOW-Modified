package com.opencloudgaming.opennow

import android.util.Log
import android.view.Surface

/**
 * Kotlin/JNI bridge to the GStreamer WebRTC native pipeline.
 *
 * This class is intentionally thin — all heavy lifting lives in the C++ side
 * ([opennow_native.cpp]).  Kotlin code only:
 *  - calls the native functions to manage the pipeline lifecycle, and
 *  - receives async event callbacks via [onGstEvent] (called from the GLib
 *    main-loop thread through JNI).
 *
 * All public methods are safe to call from any thread.  Callbacks are
 * dispatched to [eventHandler] from the GLib thread; callers must post to the
 * main thread themselves if they need to touch the UI.
 */
class NativeStreamerBridge(
    /** Called whenever the native pipeline emits an event or error. */
    private val eventHandler: (tag: String, data: String) -> Unit,
) {
    companion object {
        private const val TAG = "NativeStreamerBridge"

        private val libraryLoaded: Boolean = runCatching {
            System.loadLibrary("opennow_native")
            true
        }.getOrElse { ex ->
            Log.e(TAG, "Failed to load opennow_native: ${ex.message}")
            false
        }

        /** Returns true if the native library was loaded and GStreamer was compiled in. */
        fun isGStreamerAvailable(): Boolean {
            if (!libraryLoaded) return false
            return runCatching { NativeStreamerBridge(eventHandler = { _, _ -> }).gstIsAvailable() }.getOrDefault(false)
        }
    }

    // ── Native method declarations ───────────────────────────────────────────

    /** Initialises GStreamer (gst_init) and caches JVM / callback references. */
    external fun gstNativeInit(): Boolean

    /**
     * Creates the GStreamer WebRTC pipeline and binds video output to [surface].
     * Must be called after [gstNativeInit].
     */
    external fun gstCreatePipeline(surface: Surface?): Boolean

    /**
     * Feeds the remote SDP offer (received from the signaling server) to the
     * webrtcbin element. The pipeline will create an answer and call back via
     * [onGstEvent] with tag="sdp-answer".
     */
    external fun gstSetRemoteOffer(sdp: String)

    /**
     * Adds a remote ICE candidate received from the signaling server.
     * [sdpMid] is informational; the native side uses [sdpMLineIndex].
     */
    external fun gstAddRemoteIce(candidate: String, sdpMid: String?, sdpMLineIndex: Int)

    /** Stops the pipeline and releases all GStreamer resources. */
    external fun gstDestroyPipeline()

    /** Returns true if GStreamer was compiled into this build. */
    external fun gstIsAvailable(): Boolean

    // ── Lifecycle helpers ────────────────────────────────────────────────────

    /**
     * Convenience: initialise the library and create the pipeline in one call.
     *
     * @return true on success; false if GStreamer is unavailable or a step failed.
     */
    fun initAndCreatePipeline(surface: Surface?): Boolean {
        if (!libraryLoaded) {
            Log.e(TAG, "initAndCreatePipeline: native library not loaded")
            return false
        }
        if (!gstIsAvailable()) {
            Log.e(TAG, "initAndCreatePipeline: GStreamer not compiled in")
            return false
        }
        if (!gstNativeInit()) {
            Log.e(TAG, "initAndCreatePipeline: gstNativeInit failed")
            return false
        }
        val created = gstCreatePipeline(surface)
        if (!created) {
            Log.e(TAG, "initAndCreatePipeline: gstCreatePipeline failed")
        }
        return created
    }

    /** Tear down everything. Safe to call even if the pipeline was never created. */
    fun destroy() {
        if (!libraryLoaded) return
        runCatching { gstDestroyPipeline() }
            .onFailure { Log.e(TAG, "destroy: exception during gstDestroyPipeline: ${it.message}") }
    }

    // ── Callback invoked from C++ (GLib main loop thread) ───────────────────

    /**
     * Called from the GLib main loop thread by the C++ JNI code whenever the
     * native pipeline emits an event.
     *
     * Known tags:
     *  - "status"         — informational status string in [data]
     *  - "sdp-answer"     — [data] is the full SDP answer to send via signaling
     *  - "ice-candidate"  — [data] is "mlineIndex|candidate"
     *  - "error"          — [data] is an error code/description
     */
    @Suppress("unused") // Called from JNI
    fun onGstEvent(tag: String, data: String) {
        Log.d(TAG, "onGstEvent tag=$tag data=${data.take(120)}")
        eventHandler(tag, data)
    }
}
