package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SurfacePresentationClockTest {
    @Test fun webRtcPacedFramesDoNotGainAnotherFrameOfPresentationDelay() {
        for (fps in listOf(30, 60, 120, 240, 360)) {
            val clock = SurfacePresentationClock(fps, SurfacePresentationMode.Immediate)
            val interval = 1_000_000_000L / fps
            val now = 40_000_000_000_000L
            val media = 100_000_000_000L
            assertEquals(now, clock.next(media, now))
            assertEquals(now + interval + 4_000_000,
                clock.next(media + interval, now + interval + 4_000_000))
            assertEquals(now + 2 * interval - 4_000_000,
                clock.next(media + 2 * interval, now + 2 * interval - 4_000_000))
        }
    }

    @Test fun webRtcCatchupPauseAndResetNeverBuildAnotherPresentationQueue() {
        val clock = SurfacePresentationClock(60, SurfacePresentationMode.Immediate)
        val interval = 1_000_000_000L / 60
        for (frame in 0..100) {
            val now = 1000L + frame * 100_000L
            assertEquals(now, clock.next(frame * interval, now))
        }
        assertEquals(1_000_000_000L, clock.next(0, 1_000_000_000L))
        clock.reset()
        assertEquals(2_000_000_000L, clock.next(100, 2_000_000_000L))
    }

    @Test fun callbackJitterDoesNotChangeMediaCadence() {
        val clock = SurfacePresentationClock(60)
        val interval = 1_000_000_000L / 60
        val now = 40_000_000_000_000L
        val media = 100_000_000_000L
        val first = clock.next(media, now)
        val second = clock.next(media + interval, now + interval + 4_000_000)
        val third = clock.next(media + 2 * interval, now + 2 * interval - 4_000_000)
        assertEquals(interval, second - first)
        assertEquals(interval, third - second)
    }

    @Test fun pauseAndBackwardTimestampReanchorWithoutAccumulatedDelay() {
        val clock = SurfacePresentationClock(60)
        val interval = 1_000_000_000L / 60
        clock.next(0, 0)
        assertEquals(1_000_000_000L + interval, clock.next(interval, 1_000_000_000L))
        assertEquals(1_010_000_000L + interval, clock.next(0, 1_010_000_000L))
        clock.reset()
        assertEquals(2_000_000_000L + interval, clock.next(100, 2_000_000_000L))
    }

    @Test fun catchupCannotQueueUnboundedFutureFrames() {
        val clock = SurfacePresentationClock(60)
        val interval = 1_000_000_000L / 60
        for (frame in 0..500) {
            val target = clock.next(frame * interval, 1000)
            assertTrue(target >= 1000)
            assertTrue(target <= 1000 + 2 * interval)
        }
    }

    @Test fun catchupBurstNeverSchedulesANewerFrameBeforeAnOlderFrame() {
        val clock = SurfacePresentationClock(60)
        val interval = 1_000_000_000L / 60
        var previous = 0L
        for (frame in 0..100) {
            val now = 1000L + frame * 100_000L
            val target = clock.next(frame * interval, now)
            assertTrue("frame $frame: $target before $previous", target >= previous)
            assertTrue(target <= now + 2 * interval)
            previous = target
        }
    }

    @Test fun highFpsRetainsItsOwnSchedulingInterval() {
        for (fps in listOf(120, 240, 360)) {
            val clock = SurfacePresentationClock(fps)
            val interval = 1_000_000_000L / fps
            val first = clock.next(100, 1000)
            assertEquals(1000 + interval, first)
            assertEquals(interval, clock.next(100 + interval, 1000 + interval) - first)
        }
    }
}
