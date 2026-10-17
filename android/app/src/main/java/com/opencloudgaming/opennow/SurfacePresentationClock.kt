package com.opencloudgaming.opennow

/** Maps the media timeline to SurfaceView's monotonic clock without accumulating latency. */
internal class SurfacePresentationClock(fps: Int) {
    private val frameNs = 1_000_000_000L / fps.coerceAtLeast(1)
    private var mediaAnchorNs = 0L
    private var localAnchorNs = 0L
    private var lastMediaNs: Long? = null

    fun next(mediaNs: Long, nowNs: Long): Long {
        val previous = lastMediaNs
        val target = localAnchorNs + (mediaNs - mediaAnchorNs)
        // One frame of headroom absorbs callback jitter. A pause or a backward timestamp
        // starts a fresh timeline rather than building a queue.
        val reset = previous == null || mediaNs <= previous || target < nowNs
        lastMediaNs = mediaNs
        if (reset) {
            mediaAnchorNs = mediaNs
            localAnchorNs = nowNs + frameNs
            return localAnchorNs
        }
        if (target - nowNs > 2 * frameNs) {
            // A catch-up burst must not move presentation backwards from two frames ahead
            // to one frame ahead. Clamp the newest frame to the same bounded horizon,
            // allowing the Surface to replace stale scheduled output without sawtooth pacing.
            mediaAnchorNs = mediaNs
            localAnchorNs = nowNs + 2 * frameNs
            return localAnchorNs
        }
        return target
    }

    fun reset() { lastMediaNs = null }
}
