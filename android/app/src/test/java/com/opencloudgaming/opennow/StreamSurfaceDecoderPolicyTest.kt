package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class StreamSurfaceDecoderPolicyTest {
    private val sdr = StreamSettings(colorQuality = ColorQuality.EightBit420,
        videoOutput = StreamVideoOutput.MediaCodecSurface)

    @Test fun absentChoiceFollowsDistributionAndExplicitChoiceOverridesIt() {
        val legacy = sdr.copy(videoOutput = StreamVideoOutput.Default)
        assertTrue(shouldPreferDirectSdrSurface(legacy, playStoreRelease = false))
        assertFalse(shouldPreferDirectSdrSurface(legacy, playStoreRelease = true))
        assertTrue(shouldPreferDirectSdrSurface(sdr, playStoreRelease = true))
        assertFalse(shouldPreferDirectSdrSurface(sdr.copy(videoOutput = StreamVideoOutput.WebRtcTexture),
            playStoreRelease = false))
    }

    @Test fun directSurfaceSupportsSdrAtRequestedHighFrameRates() {
        for (fps in listOf(30, 60, 120, 240, 360)) {
            for (codec in VideoCodec.values()) {
                assertTrue("$codec at $fps FPS", shouldPreferDirectSdrSurface(sdr.copy(codec = codec, fps = fps)))
            }
        }
    }

    @Test fun sharpeningKeepsTextureFrames() {
        assertFalse(shouldPreferDirectSdrSurface(sdr.copy(streamSharpeningEnabled = true)))
    }

    @Test fun enabledSharpeningAtZeroStrengthDoesNotRequireTextureFrames() {
        assertTrue(shouldPreferDirectSdrSurface(sdr.copy(streamSharpeningEnabled = true, streamSharpeningAmount = 0f)))
    }

    @Test fun constrainedGoogleTvProfileUsesDirectSurfaceWithoutChangingSavedPreference() {
        val saved = sdr.copy(resolution = "2560x1440", fps = 60, codec = VideoCodec.AV1,
            maxBitrateMbps = 150, streamSharpeningEnabled = true)
        val report = RuntimeCodecReport(capabilities = listOf(CodecCapability(
            codec = VideoCodec.AV1, decoderAvailable = true, encoderAvailable = false,
            hardwareDecoder = true, hardwareEncoder = false, realtimeSafe = true,
            webRtcDecoderAvailable = true, webRtcHardwareDecoderAvailable = true,
            webRtcDecoderName = "c2.mtk.av1.decoder", maxSupportedWidth = 3840, maxSupportedHeight = 2160,
        )), nativeRuntimeSummary = "{}", androidTvProfile = true,
            lowPowerGpuProfile = true, constrainedRuntimeProfile = true)

        val launch = saved.adjustedForDevice(report)
        assertTrue(saved.streamSharpeningEnabled)
        assertFalse(launch.streamSharpeningEnabled)
        assertEquals(saved.resolution, launch.resolution)
        assertEquals(saved.fps, launch.fps)
        assertEquals(saved.codec, launch.codec)
        assertEquals(saved.maxBitrateMbps, launch.maxBitrateMbps)
        assertTrue(shouldPreferDirectSdrSurface(launch))
        assertFalse(shouldPreferDirectSdrSurface(launch.copy(videoOutput = StreamVideoOutput.WebRtcTexture)))
    }

    @Test fun tenBitSdrKeepsEstablishedColorPath() {
        assertFalse(shouldPreferDirectSdrSurface(sdr.copy(colorQuality = ColorQuality.TenBit420)))
        assertFalse(shouldPreferDirectSdrSurface(sdr.copy(colorQuality = ColorQuality.TenBit444)))
    }

    @Test fun hdrMustUseDedicatedHdrConfiguration() {
        assertFalse(shouldPreferDirectSdrSurface(sdr.copy(codec = VideoCodec.H265, hdrEnabled = true)))
    }
}
