package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test
import org.webrtc.EncodedImage
import org.webrtc.VideoCodecStatus
import org.webrtc.VideoDecoder
import java.nio.ByteBuffer

class NvstDecoderInputTest {
    @Test fun borrowingUsesTheNativeSliceAndLeavesSourcePositionUntouched() {
        val source = ByteBuffer.allocateDirect(10).apply {
            put(byteArrayOf(0, 1, 2, 3, 4, 5, 6, 7, 8, 9))
            position(2)
            limit(7)
        }
        val frame = NvstDecoderInput(borrowInput = true).frame(source, 123, true)
        try {
            assertTrue(frame.buffer.isDirect)
            assertEquals(5, frame.buffer.remaining())
            source.put(2, 42)
            assertEquals(42.toByte(), frame.buffer.get()) // Shares native storage, no staging copy.
            assertEquals(2, source.position())
            assertEquals(7, source.limit())
            assertEquals(123L, frame.captureTimeNs)
            assertEquals(0, frame.encodedWidth)
            assertEquals(0, frame.encodedHeight)
            assertEquals(EncodedImage.FrameType.VideoFrameKey, frame.frameType)
        } finally { frame.release() }
    }

    @Test fun ownedCopiesSurviveCallerReuseAndAreNotRecycledWhileRetained() {
        val source = ByteBuffer.allocateDirect(4).apply { putInt(42); flip() }
        val input = NvstDecoderInput(borrowInput = false)
        val first = input.frame(source, 1, true)
        first.retain()
        first.release()
        source.putInt(0, 99)
        val second = input.frame(source, 2, false)
        assertEquals(42, first.buffer.getInt(0))
        assertEquals(99, second.buffer.getInt(0))
        assertNotSame(first.buffer, second.buffer)
        first.release()
        val third = input.frame(source, 3, false)
        assertSame(first.buffer, third.buffer)
        second.release()
        third.release()
    }

    @Test fun unknownDecoderAndItsWrappersCannotOptIntoBorrowing() {
        val unknown = object : VideoDecoder {
            override fun initDecode(settings: VideoDecoder.Settings?, callback: VideoDecoder.Callback?) = VideoCodecStatus.OK
            override fun decode(frame: EncodedImage?, info: VideoDecoder.DecodeInfo?) = VideoCodecStatus.OK
            override fun release() = VideoCodecStatus.OK
            override fun getImplementationName() = "c2.mtk.av1.decoder" // A matching name proves no ownership contract.
        }
        val surface = MediaCodecSurfaceVideoDecoder(60, { null }, false) { _, _, _ -> null }
        assertFalse(unknown.supportsBorrowedNvstInput())
        assertFalse(LowLatencyVideoDecoder(unknown, 60, false).supportsBorrowedNvstInput())
        assertTrue(surface.supportsBorrowedNvstInput())
        assertFalse(SurfaceOrTextureVideoDecoder(surface, unknown, { true }, { null }, {}).supportsBorrowedNvstInput())
    }
}
