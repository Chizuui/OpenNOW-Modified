package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class GoofySceneTimingTest {
    @Test
    fun countsEachImpactOnceAndStartsFreshAtZero() {
        for (time in listOf(-1L, 0L, 3_300L, GOOFY_CATCH_TIME_MILLIS - 1)) {
            assertEquals(0L, goofyCatchCount(time))
        }
        for (time in listOf(GOOFY_CATCH_TIME_MILLIS, 4_000L, 7_999L, 8_000L, 11_649L)) {
            assertEquals(1L, goofyCatchCount(time))
        }
        assertEquals(2L, goofyCatchCount(11_650L))
        // A long frame spanning several cycles counts catches without repeated increment side effects.
        assertEquals(101L, goofyCatchCount(GOOFY_SCENE_DURATION_MILLIS * 100 + GOOFY_CATCH_TIME_MILLIS))
        assertEquals(0L, goofyCatchCount(0L))
    }

    @Test
    fun peekerSpeaksBeforeTheBoltAndDisappearsAfterImpact() {
        assertEquals(0f, goofySceneCue(0).peekerReveal, 0f)
        assertEquals(0.5f, goofySceneCue(350).peekerReveal, 0f)
        assertFalse(goofySceneCue(699).showBugMessage)
        assertTrue(goofySceneCue(700).showBugMessage)
        assertNull(goofySceneCue(3_299).lightningProgress)
        assertTrue(goofySceneCue(3_299).companionAiming)
        assertEquals(0f, goofySceneCue(3_300).lightningProgress!!, 0f)
        assertEquals(1f, goofySceneCue(3_600).peekerReveal, 0f)
        assertEquals(0.5f, goofySceneCue(3_600).lightningProgress!!, 0f)
        assertFalse(goofySceneCue(3_650).showBugMessage)
        assertEquals(0.5f, goofySceneCue(3_825).peekerReveal, 0f)
        assertEquals(0f, goofySceneCue(4_000).peekerReveal, 0f)
        assertNull(goofySceneCue(4_000).lightningProgress)
        assertFalse(goofySceneCue(4_000).companionAiming)
    }

    @Test
    fun repeatedCyclesCannotLeaveAStalePeekerOrLightningBolt() {
        for (time in 0L until GOOFY_SCENE_DURATION_MILLIS step 40L) {
            assertEquals(goofySceneCue(time), goofySceneCue(time + GOOFY_SCENE_DURATION_MILLIS * 100))
            assertTrue(goofySceneCue(time).peekerReveal in 0f..1f)
            if (time >= 4_000L) {
                assertFalse(goofySceneCue(time).showBugMessage)
                assertNull(goofySceneCue(time).lightningProgress)
            }
        }
        assertEquals(goofySceneCue(0), goofySceneCue(-100))
    }

    @Test
    fun bubblesStayInsidePortraitLandscapeAndSmallViewportsWithTailsPointingAtTheSpeaker() {
        for ((width, height) in listOf(360f to 780f, 780f to 360f, 120f to 100f)) {
            val bubbleWidth = minOf(width, 220f)
            val bubbleHeight = 64f // Includes multiline or enlarged text.
            for (x in listOf(-10f, 0f, width / 2f, width, width + 10f)) {
                for (y in listOf(0f, height / 2f, height)) {
                    val bubble = mascotBubblePlacement(width, height, bubbleWidth, bubbleHeight, x, y, 22.5f)
                    assertTrue(bubble.x >= 0f && bubble.x + bubbleWidth <= width)
                    assertTrue(bubble.y >= 0f && bubble.y + bubbleHeight <= height)
                    assertEquals((x - bubble.x).coerceIn(16f, bubbleWidth - 16f), bubble.tailX, 0.001f)
                    assertEquals(y >= bubbleHeight + 34.5f, bubble.above)
                }
            }
        }
    }
}
