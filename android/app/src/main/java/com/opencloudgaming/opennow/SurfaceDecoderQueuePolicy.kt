package com.opencloudgaming.opennow

/** Codec priming and WebRTC catch-up arrive in bursts, not at the negotiated frame interval. */
internal class SurfaceDecoderQueuePolicy(fps: Int) {
    private val maximumFrames = maxOf(32, (fps.coerceAtLeast(1) + 1) / 2)
    private var firstOutputAtMs: Long? = null

    // Called under the decoder lock, alongside queue checks and codec resets.
    fun outputProduced(nowMs: Long) {
        if (firstOutputAtMs == null) firstOutputAtMs = nowMs
    }

    fun reset() { firstOutputAtMs = null }

    fun failure(inFlightFrames: Int, queuedBytes: Long, oldestAgeMs: Long, nowMs: Long): String? {
        val firstOutput = firstOutputAtMs
        val priming = firstOutput == null
        // Startup frames already carry codec initialization latency. Give them one bounded
        // active-timeout window to drain after the first output, rather than failing immediately.
        // Later output must not keep extending this grace period and hide an aging queue.
        val stalledAgeMs = if (firstOutput == null) oldestAgeMs
            else minOf(oldestAgeMs, (nowMs - firstOutput).coerceAtLeast(0))
        return when {
            queuedBytes > MAX_QUEUED_BYTES -> "surface queue memory limit bytes=$queuedBytes"
            inFlightFrames >= maximumFrames -> "surface queue frame limit frames=$inFlightFrames limit=$maximumFrames"
            inFlightFrames > 0 && stalledAgeMs > if (priming) STARTUP_TIMEOUT_MS else ACTIVE_TIMEOUT_MS ->
                "surface queue stalled ageMs=$oldestAgeMs priming=$priming frames=$inFlightFrames"
            else -> null
        }
    }

    private companion object {
        const val MAX_QUEUED_BYTES = 16L * 1024 * 1024
        const val STARTUP_TIMEOUT_MS = 1000L
        const val ACTIVE_TIMEOUT_MS = 250L
    }
}
