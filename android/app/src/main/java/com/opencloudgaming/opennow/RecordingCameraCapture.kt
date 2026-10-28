package com.opencloudgaming.opennow

import android.content.Context
import org.webrtc.Camera1Enumerator
import org.webrtc.Camera2Enumerator
import org.webrtc.CameraEnumerator
import org.webrtc.CameraVideoCapturer
import org.webrtc.CapturerObserver
import org.webrtc.EglBase
import org.webrtc.SurfaceTextureHelper
import org.webrtc.VideoFrame

/** Recorder-owned camera. Only one small I420 frame is retained, regardless of encoder load. */
internal class RecordingCameraCapture(
    private val context: Context,
    private val sharedContext: EglBase.Context,
    private val onError: (String) -> Unit,
) {
    private val gate = Any()
    private var closed = false
    private var latest: VideoFrame? = null
    private var helper: SurfaceTextureHelper? = null
    private var capturer: CameraVideoCapturer? = null

    fun start() {
        val enumerator: CameraEnumerator = if (Camera2Enumerator.isSupported(context)) Camera2Enumerator(context)
            else Camera1Enumerator(true)
        val names = enumerator.deviceNames
        val name = names.firstOrNull { enumerator.isFrontFacing(it) } ?: names.firstOrNull()
            ?: error("No camera is available")
        val events = object : CameraVideoCapturer.CameraEventsHandler {
            override fun onCameraError(errorDescription: String) = fail(errorDescription)
            override fun onCameraDisconnected() = fail("Camera disconnected")
            override fun onCameraFreezed(errorDescription: String) = fail(errorDescription)
            override fun onCameraOpening(cameraName: String) = Unit
            override fun onFirstFrameAvailable() = Unit
            override fun onCameraClosed() = fail("Camera closed unexpectedly")
        }
        val camera = enumerator.createCapturer(name, events) ?: error("Could not create camera capture")
        capturer = camera
        val textureHelper = SurfaceTextureHelper.create("OpenNOW-recording-camera", sharedContext)
            ?: error("Could not create camera surface")
        helper = textureHelper
        camera.initialize(textureHelper, context, object : CapturerObserver {
            override fun onCapturerStarted(success: Boolean) {
                if (!success) fail("Camera could not start")
            }
            override fun onCapturerStopped() = fail("Camera stopped unexpectedly")
            override fun onFrameCaptured(frame: VideoFrame) {
                // Keeping a camera texture would stop SurfaceTextureHelper from delivering the
                // next frame. Copy this small image to I420 and return the texture immediately.
                val buffer = try { frame.buffer.toI420() } catch (error: Exception) {
                    fail(error.message ?: "Could not read camera frame")
                    return
                } ?: return fail("Could not read camera frame")
                val copy = VideoFrame(buffer, frame.rotation, frame.timestampNs)
                synchronized(gate) {
                    if (closed) copy.release()
                    else {
                        latest?.release()
                        latest = copy
                    }
                }
            }
        })
        camera.startCapture(640, 480, 15)
    }

    fun acquireFrame(): VideoFrame? = synchronized(gate) { latest?.also { it.retain() } }
    fun hasFrame(): Boolean = synchronized(gate) { latest != null }

    private fun fail(message: String) {
        val active = synchronized(gate) { !closed }
        if (active) onError(message)
    }

    /** Runs on the recorder worker; camera disposal never blocks the UI or streaming callbacks. */
    fun stop() {
        synchronized(gate) {
            if (closed) return
            closed = true
        }
        runCatching { capturer?.stopCapture() }
        runCatching { capturer?.dispose() }
        capturer = null
        runCatching { helper?.dispose() }
        helper = null
    }

    fun close() {
        stop()
        synchronized(gate) {
            latest?.release()
            latest = null
        }
    }
}
