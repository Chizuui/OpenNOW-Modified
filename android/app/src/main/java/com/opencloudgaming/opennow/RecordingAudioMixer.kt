package com.opencloudgaming.opennow

import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.abs
import kotlin.math.roundToInt

/** Bounded PCM mixer on a 48 kHz stereo timeline. Missing inputs become silence, never a stall. */
internal class RecordingAudioMixer(private val originNs: Long) {
    companion object {
        const val SAMPLE_RATE = 48_000
        const val BLOCK_FRAMES = 480
        const val INPUT_DELAY_NS = 60_000_000L
        private const val CAPACITY = SAMPLE_RATE
    }

    enum class Source { Game, Microphone }
    data class Block(val bytes: ByteArray, val ptsNs: Long)

    private class Input {
        val tags = LongArray(CAPACITY) { -1L }
        val samples = ShortArray(CAPACITY * 2)
        var nextFrame: Long? = null
    }
    private val inputs = Array(2) { Input() }
    private var readFrame = 0L

    /** The timestamp denotes the beginning of this packet, in elapsedRealtimeNanos. */
    @Synchronized
    fun queue(source: Source, pcm: ByteBuffer, sampleRate: Int, channels: Int, ptsNs: Long) {
        if (sampleRate !in 8_000..96_000 || channels !in 1..2) return
        val data = pcm.duplicate().order(ByteOrder.LITTLE_ENDIAN)
        if (data.remaining() % (channels * 2) != 0) return
        val sourceFrames = data.remaining() / (channels * 2)
        if (sourceFrames == 0 || sourceFrames > sampleRate) return
        val input = inputs[source.ordinal]
        val arrivalFrame = ((ptsNs - originNs) * SAMPLE_RATE / 1_000_000_000L).coerceAtLeast(0L)
        // Use sample count between packets to remove callback scheduling jitter. Re-anchor after
        // a real gap or capture restart; never shift old speech into a later point in the video.
        val expected = input.nextFrame
        val firstFrame = if (expected == null || abs(arrivalFrame - expected) > SAMPLE_RATE / 20) arrivalFrame else expected
        val frames = (sourceFrames.toLong() * SAMPLE_RATE / sampleRate).toInt()
        val base = data.position()
        for (frame in 0 until frames) {
            val timelineFrame = firstFrame + frame
            if (timelineFrame < readFrame || timelineFrame >= readFrame + CAPACITY) continue
            val slot = (timelineFrame % CAPACITY).toInt()
            val sourcePosition = frame.toDouble() * sampleRate / SAMPLE_RATE
            val low = sourcePosition.toInt().coerceAtMost(sourceFrames - 1)
            val high = (low + 1).coerceAtMost(sourceFrames - 1)
            val fraction = sourcePosition - low
            for (channel in 0..1) {
                val sourceChannel = channel.coerceAtMost(channels - 1)
                val a = data.getShort(base + (low * channels + sourceChannel) * 2).toInt()
                val b = data.getShort(base + (high * channels + sourceChannel) * 2).toInt()
                input.samples[slot * 2 + channel] = (a + (b - a) * fraction).roundToInt().toShort()
            }
            input.tags[slot] = timelineFrame
        }
        input.nextFrame = firstFrame + frames
    }

    /** The worker reads at most a fixed number of blocks each tick, including during shutdown. */
    @Synchronized
    fun poll(throughNs: Long): Block? {
        val availableFrames = ((throughNs - originNs).coerceAtLeast(0L) * SAMPLE_RATE / 1_000_000_000L)
        if (readFrame + BLOCK_FRAMES > availableFrames) return null
        // If the encoder falls a long way behind, drop old audio rather than retaining an
        // unbounded backlog of silence or replaying overwritten ring slots.
        if (availableFrames - readFrame > CAPACITY) readFrame = availableFrames - CAPACITY
        val ptsNs = originNs + readFrame * 1_000_000_000L / SAMPLE_RATE
        val output = ByteBuffer.allocate(BLOCK_FRAMES * 4).order(ByteOrder.LITTLE_ENDIAN)
        repeat(BLOCK_FRAMES) {
            val slot = (readFrame % CAPACITY).toInt()
            for (channel in 0..1) {
                var sum = 0
                inputs.forEach { input ->
                    if (input.tags[slot] == readFrame) sum += input.samples[slot * 2 + channel].toInt()
                }
                output.putShort(sum.coerceIn(Short.MIN_VALUE.toInt(), Short.MAX_VALUE.toInt()).toShort())
            }
            inputs.forEach { input -> if (input.tags[slot] == readFrame) input.tags[slot] = -1L }
            readFrame++
        }
        return Block(output.array(), ptsNs)
    }
}
