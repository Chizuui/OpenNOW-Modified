package com.opencloudgaming.opennow

import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.*
import org.junit.Test

class RecordingAudioMixerTest {
    private val origin = 10_000_000_000L
    private val blockNs = 10_000_000L
    private fun pcm(frames: Int = 480, channels: Int = 1, sample: Int): ByteBuffer =
        ByteBuffer.allocate(frames * channels * 2).order(ByteOrder.LITTLE_ENDIAN).apply {
            repeat(frames * channels) { putShort(sample.toShort()) }
            flip()
        }
    private fun samples(block: RecordingAudioMixer.Block) =
        ByteBuffer.wrap(block.bytes).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer().let { buffer ->
            ShortArray(buffer.remaining()).also { buffer.get(it) }
        }

    @Test fun mixesSimultaneousInputsIntoOneStereoBlockWithoutChangingDuration() {
        val mixer = RecordingAudioMixer(origin)
        mixer.queue(RecordingAudioMixer.Source.Game, pcm(channels = 2, sample = 1000), 48_000, 2, origin)
        mixer.queue(RecordingAudioMixer.Source.Microphone, pcm(sample = 2000), 48_000, 1, origin)
        val block = mixer.poll(origin + blockNs)!!
        assertEquals(origin, block.ptsNs)
        assertEquals(960, samples(block).size)
        assertTrue(samples(block).all { it == 3000.toShort() })
        assertNull(mixer.poll(origin + blockNs))
    }

    @Test fun microphoneAloneWorksWhenGameAudioNeverArrives() {
        val mixer = RecordingAudioMixer(origin)
        mixer.queue(RecordingAudioMixer.Source.Microphone, pcm(sample = -1200), 48_000, 1, origin)
        assertTrue(samples(mixer.poll(origin + blockNs)!!).all { it == (-1200).toShort() })
        assertTrue(samples(mixer.poll(origin + blockNs * 2)!!).all { it == 0.toShort() })
    }

    @Test fun clipsBothSignsInsteadOfWrappingPcm() {
        val mixer = RecordingAudioMixer(origin)
        for (sample in listOf(30_000, -30_000)) {
            val pts = origin + if (sample > 0) 0 else blockNs
            RecordingAudioMixer.Source.entries.forEach { source ->
                mixer.queue(source, pcm(sample = sample), 48_000, 1, pts)
            }
            val expected = if (sample > 0) Short.MAX_VALUE else Short.MIN_VALUE
            assertTrue(samples(mixer.poll(pts + blockNs)!!).all { it == expected })
        }
    }

    @Test fun resamples44100HzMicrophoneToRecordingClock() {
        val mixer = RecordingAudioMixer(origin)
        mixer.queue(RecordingAudioMixer.Source.Microphone, pcm(frames = 441, sample = 2345), 44_100, 1, origin)
        assertTrue(samples(mixer.poll(origin + blockNs)!!).all { it == 2345.toShort() })
        assertNull(mixer.poll(origin + blockNs))
    }

    @Test fun callbackJitterDoesNotCreateGapsButLongPausesDo() {
        val mixer = RecordingAudioMixer(origin)
        mixer.queue(RecordingAudioMixer.Source.Microphone, pcm(sample = 100), 48_000, 1, origin)
        mixer.queue(RecordingAudioMixer.Source.Microphone, pcm(sample = 200), 48_000, 1, origin + blockNs + 2_000_000L)
        assertTrue(samples(mixer.poll(origin + blockNs)!!).all { it == 100.toShort() })
        assertTrue(samples(mixer.poll(origin + blockNs * 2)!!).all { it == 200.toShort() })
        mixer.queue(RecordingAudioMixer.Source.Microphone, pcm(sample = 300), 48_000, 1, origin + 100_000_000L)
        repeat(8) { assertTrue(samples(mixer.poll(origin + 100_000_000L)!!).all { it == 0.toShort() }) }
        assertTrue(samples(mixer.poll(origin + 110_000_000L)!!).all { it == 300.toShort() })
    }

    @Test fun malformedAndLatePacketsDoNotCorruptOrDelayOutput() {
        val mixer = RecordingAudioMixer(origin)
        mixer.queue(RecordingAudioMixer.Source.Game, ByteBuffer.wrap(byteArrayOf(1)), 48_000, 2, origin)
        mixer.queue(RecordingAudioMixer.Source.Game, pcm(sample = 100), 0, 1, origin)
        assertTrue(samples(mixer.poll(origin + blockNs)!!).all { it == 0.toShort() })
        mixer.queue(RecordingAudioMixer.Source.Microphone, pcm(sample = 200), 48_000, 1, origin)
        assertTrue(samples(mixer.poll(origin + blockNs * 2)!!).all { it == 0.toShort() })
    }

    @Test fun fallingBehindDropsBoundedBacklogWithoutReplayingOldSpeech() {
        val mixer = RecordingAudioMixer(origin)
        mixer.queue(RecordingAudioMixer.Source.Microphone, pcm(sample = 300), 48_000, 1, origin)
        val block = mixer.poll(origin + 10_000_000_000L)!!
        assertEquals(origin + 9_000_000_000L, block.ptsNs)
        assertTrue(samples(block).all { it == 0.toShort() })
    }
}
