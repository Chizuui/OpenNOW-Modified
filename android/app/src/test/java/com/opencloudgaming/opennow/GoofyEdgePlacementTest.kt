package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.random.Random

class GoofyEdgePlacementTest {
    @Test
    fun everyEdgeFacesInwardAndRestsFlushAgainstTheScreen() {
        val expected = listOf(
            Triple(GoofyScreenEdge.Bottom, 0f, 200f - 45f * 144f / 176f / 2f),
            Triple(GoofyScreenEdge.Left, 90f, 100f),
            Triple(GoofyScreenEdge.Top, 180f, 45f * 144f / 176f / 2f),
            Triple(GoofyScreenEdge.Right, 270f, 100f),
        )
        expected.forEach { (edge, rotation, centerY) ->
            val frame = GoofyEdgePlacement(edge, 0.5f).frame(300f, 200f, 176f, 144f)
            assertEquals(rotation, frame.rotationDegrees, 0f)
            assertEquals(centerY, frame.centerY, 0.001f)
            val centerX = when (edge) {
                GoofyScreenEdge.Left -> frame.height / 2f
                GoofyScreenEdge.Right -> 300f - frame.height / 2f
                else -> 150f
            }
            assertEquals(centerX, frame.centerX, 0.001f)
            assertEquals(176f / 144f, frame.width / frame.height, 0.001f)
        }
    }

    @Test
    fun positionsSpanEachWholeEdgeAndStayInBoundsAfterResizing() {
        for (edge in GoofyScreenEdge.entries) {
            for ((width, height) in listOf(300f to 200f, 200f to 300f, 12f to 8f, 0f to 0f)) {
                for (fraction in listOf(0f, 0.25f, 0.5f, 0.75f, 1f)) {
                    val frame = GoofyEdgePlacement(edge, fraction).frame(width, height, 176f, 144f)
                    val horizontal = edge == GoofyScreenEdge.Bottom || edge == GoofyScreenEdge.Top
                    val halfWidth = (if (horizontal) frame.width else frame.height) / 2f
                    val halfHeight = (if (horizontal) frame.height else frame.width) / 2f
                    assertTrue(frame.centerX - halfWidth >= -0.001f)
                    assertTrue(frame.centerX + halfWidth <= width + 0.001f)
                    assertTrue(frame.centerY - halfHeight >= -0.001f)
                    assertTrue(frame.centerY + halfHeight <= height + 0.001f)
                    val along = if (horizontal) frame.centerX else frame.centerY
                    val length = if (horizontal) width else height
                    assertEquals(frame.width / 2f + (length - frame.width) * fraction, along, 0.001f)
                }
            }
        }
    }

    @Test
    fun eachLoopChoosesADifferentEdgeAndPositionsBeyondCorners() {
        val random = Random(42)
        var placement = nextGoofyEdgePlacement(random = random)
        val seen = mutableSetOf<GoofyScreenEdge>()
        val fractions = mutableListOf<Float>()
        repeat(100) {
            val next = nextGoofyEdgePlacement(placement, random)
            assertNotEquals(placement.edge, next.edge)
            assertTrue(next.fraction >= 0f && next.fraction < 1f)
            seen += next.edge
            fractions += next.fraction
            placement = next
        }
        assertEquals(GoofyScreenEdge.entries.toSet(), seen)
        assertTrue(fractions.any { it > 0.25f && it < 0.75f })
        assertTrue(fractions.any { it < 0.1f })
        assertTrue(fractions.any { it > 0.9f })
    }
}
