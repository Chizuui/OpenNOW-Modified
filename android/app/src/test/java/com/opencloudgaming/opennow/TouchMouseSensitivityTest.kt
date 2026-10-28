package com.opencloudgaming.opennow

import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import org.junit.Assert.*
import org.junit.Test

class TouchMouseSensitivityTest {
    @Test fun oldSettingsKeepTheirSavedMouseSensitivity() {
        val settings = OpenNowJson.decodeFromString<AppSettings>(
            """{"stream":{"mouseSensitivity":1.75}}""",
        ).normalizedForAndroid()
        assertNull(settings.stream.touchMouseSensitivity)
        assertEquals(1.75f, settings.stream.effectiveTouchMouseSensitivity(), 0f)
    }

    @Test fun touchTuningSurvivesSavingAndDoesNotChangePhysicalMouseTuning() {
        val original = AppSettings(stream = StreamSettings(mouseSensitivity = 0.75f, touchMouseSensitivity = 4.5f))
        val restored = OpenNowJson.decodeFromString<AppSettings>(OpenNowJson.encodeToString(original)).normalizedForAndroid()
        assertEquals(4.5f, restored.stream.effectiveTouchMouseSensitivity(), 0f)
        assertEquals(0.75f, restored.stream.mouseSensitivity, 0f)
    }

    @Test fun persistedInvalidValuesCannotPoisonTouchMotion() {
        for ((saved, expected) in listOf(Float.NaN to 1.5f, Float.POSITIVE_INFINITY to 1.5f, -2f to 0.1f, 99f to 5f)) {
            val settings = AppSettings(stream = StreamSettings(mouseSensitivity = 1.5f, touchMouseSensitivity = saved))
                .normalizedForAndroid().stream
            assertEquals(expected, settings.effectiveTouchMouseSensitivity(), 0f)
        }
    }

    @Test fun sameFingerTravelProducesProportionalCursorMotionIncludingFineAim() {
        for ((sensitivity, expected) in listOf(0.1f to 2, 1f to 20, 5f to 100)) {
            val settings = StreamSettings(touchMouseSensitivity = sensitivity)
            val accumulator = MouseMotionAccumulator()
            val moves = mutableListOf<MouseMotionDelta>()
            repeat(100) { index ->
                accumulator.add(0.2f, -0.2f, index.toLong(), settings.effectiveTouchMouseSensitivity(), settings.mouseAcceleration)
                    ?.let(moves::add)
            }
            accumulator.add(0f, 0f, 101L, settings.effectiveTouchMouseSensitivity(), settings.mouseAcceleration, force = true)
                ?.let(moves::add)
            assertEquals(expected, moves.sumOf { it.dx })
            assertEquals(-expected, moves.sumOf { it.dy })
        }
    }
}
