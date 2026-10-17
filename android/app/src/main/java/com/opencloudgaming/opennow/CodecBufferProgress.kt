package com.opencloudgaming.opennow

import java.util.concurrent.atomic.AtomicLong

/** Codec input and output run on different threads. No locks or per-buffer allocations. */
internal class CodecBufferProgress {
    private val inputs = AtomicLong()
    private val outputs = AtomicLong()
    private val configs = AtomicLong()
    private val textureReleases = AtomicLong()
    private val discarded = AtomicLong()
    private val lastReportAtMs = AtomicLong(Long.MIN_VALUE)

    fun inputQueued() { inputs.incrementAndGet() }
    fun outputDequeued(codecConfig: Boolean) {
        outputs.incrementAndGet()
        if (codecConfig) configs.incrementAndGet()
    }
    fun outputReleased(render: Boolean) {
        if (render) textureReleases.incrementAndGet() else discarded.incrementAndGet()
    }

    fun report(nowMs: Long, force: Boolean = false): String? {
        val previous = lastReportAtMs.get()
        if (!force && previous != Long.MIN_VALUE && nowMs - previous < 5_000) return null
        if (!lastReportAtMs.compareAndSet(previous, nowMs)) return null
        // A rendered release submits to SurfaceTexture; it does not prove display presentation.
        // Discarded buffers can include codec configuration, reported separately.
        return "inputBuffers=${inputs.get()} outputBuffers=${outputs.get()} " +
            "codecConfigBuffers=${configs.get()} textureSubmissions=${textureReleases.get()} " +
            "discardedOutputBuffers=${discarded.get()}"
    }
}
