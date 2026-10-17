package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class CodecBufferProgressTest {
    @Test fun exposesTextureDropsEvenWhenCodecProducesAllSixtyFrames() {
        val progress = CodecBufferProgress()
        repeat(60) {
            progress.inputQueued()
            progress.outputDequeued(codecConfig = false)
            progress.outputReleased(render = it < 36)
        }
        assertEquals("inputBuffers=60 outputBuffers=60 codecConfigBuffers=0 " +
            "textureSubmissions=36 discardedOutputBuffers=24", progress.report(0))
        assertNull(progress.report(4_999))
        assertNotNull(progress.report(5_000))
    }

    @Test fun configurationOutputIsDistinguishedAndTeardownReportsBeforeInterval() {
        val progress = CodecBufferProgress()
        progress.outputDequeued(codecConfig = true)
        progress.outputReleased(render = false)
        progress.report(0)
        progress.inputQueued()
        progress.outputDequeued(codecConfig = false)
        progress.outputReleased(render = true)
        assertEquals("inputBuffers=1 outputBuffers=2 codecConfigBuffers=1 " +
            "textureSubmissions=1 discardedOutputBuffers=1", progress.report(100, force = true))
    }

    @Test fun separateInputAndOutputWorkersDoNotLoseCounts() {
        val progress = CodecBufferProgress()
        val input = Thread { repeat(10_000) { progress.inputQueued() } }
        val output = Thread { repeat(10_000) {
            progress.outputDequeued(codecConfig = false)
            progress.outputReleased(render = true)
        } }
        input.start()
        output.start()
        input.join()
        output.join()
        assertEquals("inputBuffers=10000 outputBuffers=10000 codecConfigBuffers=0 " +
            "textureSubmissions=10000 discardedOutputBuffers=0", progress.report(0))
    }
}
