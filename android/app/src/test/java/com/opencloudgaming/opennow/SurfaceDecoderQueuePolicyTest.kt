package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class SurfaceDecoderQueuePolicyTest {
    @Test fun observedEightFrameStartupBurstIsNotDecoderFailure() {
        assertNull(SurfaceDecoderQueuePolicy(60).failure(8, 800_000, 28, nowMs = 28))
    }

    @Test fun activeCatchUpBurstIsAllowedWhileAgeRemainsShort() {
        val policy = SurfaceDecoderQueuePolicy(60)
        policy.outputProduced(0)
        assertNull(policy.failure(16, 2_000_000, 100, nowMs = 1000))
    }

    @Test fun startupAndActiveStallsHaveBoundedGrace() {
        val policy = SurfaceDecoderQueuePolicy(60)
        assertNull(policy.failure(8, 1000, 500, nowMs = 500))
        assertNotNull(policy.failure(8, 1000, 1001, nowMs = 1001))
        policy.outputProduced(1001)
        assertNotNull(policy.failure(8, 1000, 251, nowMs = 1252))
        assertNull(policy.failure(0, 0, 2000, nowMs = 3001))
    }

    @Test fun observedGoogleTvFirstOutputDoesNotImmediatelyTriggerTextureFallback() {
        val policy = SurfaceDecoderQueuePolicy(60)
        assertNull(policy.failure(13, 800_000, 515, nowMs = 1515))
        policy.outputProduced(1515)
        assertNull(policy.failure(13, 800_000, 517, nowMs = 1517))
        assertNull(policy.failure(8, 500_000, 765, nowMs = 1765))
        assertNotNull(policy.failure(8, 500_000, 766, nowMs = 1766))
    }

    @Test fun drainingStartupQueueReturnsToOrdinaryActiveAgeLimit() {
        val policy = SurfaceDecoderQueuePolicy(60)
        policy.outputProduced(1515)
        assertNull(policy.failure(1, 1000, 16, nowMs = 1800))
        assertNotNull(policy.failure(1, 1000, 251, nowMs = 2035))
    }

    @Test fun continuingOutputDoesNotExtendStartupGraceForAnAgingQueue() {
        val policy = SurfaceDecoderQueuePolicy(60)
        policy.outputProduced(1515)
        policy.outputProduced(1765)
        assertNotNull(policy.failure(8, 500_000, 766, nowMs = 1766))
    }

    @Test fun codecResetRestoresStartupGrace() {
        val policy = SurfaceDecoderQueuePolicy(60)
        policy.outputProduced(1000)
        policy.reset()
        assertNull(policy.failure(13, 800_000, 517, nowMs = 2517))
        assertNotNull(policy.failure(13, 800_000, 1001, nowMs = 3001))
        policy.outputProduced(3001)
        assertNull(policy.failure(13, 800_000, 1003, nowMs = 3003))
        assertNotNull(policy.failure(13, 800_000, 1252, nowMs = 3252))
    }

    @Test fun frameCountRemainsBoundedAndScalesWithHighFps() {
        assertNotNull(SurfaceDecoderQueuePolicy(60).failure(32, 1000, 28, nowMs = 28))
        for (fps in listOf(120, 240, 360)) {
            val policy = SurfaceDecoderQueuePolicy(fps)
            assertNull(policy.failure(fps / 2 - 1, 1000, 28, nowMs = 28))
            assertNotNull(policy.failure(fps / 2, 1000, 28, nowMs = 28))
        }
    }

    @Test fun encodedMemoryCannotGrowWithoutLimitAtHighFrameRates() {
        val policy = SurfaceDecoderQueuePolicy(360)
        assertNotNull(policy.failure(10, 16L * 1024 * 1024 + 1, 28, nowMs = 28))
        policy.outputProduced(28)
        assertNotNull(policy.failure(10, 16L * 1024 * 1024 + 1, 28, nowMs = 29))
        assertNotNull(policy.failure(180, 1000, 28, nowMs = 29))
    }
}
