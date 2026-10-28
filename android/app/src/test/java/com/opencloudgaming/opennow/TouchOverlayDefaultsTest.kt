package com.opencloudgaming.opennow

import kotlinx.coroutines.runBlocking
import kotlinx.serialization.encodeToString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TouchOverlayDefaultsTest {
    @Test
    fun freshAndLegacyInstallsUseBothGamepadSticks() {
        for (playStoreRelease in listOf(false, true)) {
            for (raw in listOf(null, "{}", """{"androidTouch":{}}""")) {
                val touch = loadAndroidSettings(raw, playStoreRelease).androidTouch

                assertTrue(touch.enabled)
                assertFalse(touch.keyboardModeEnabled)
                assertEquals(TouchAimMode.LockJoystick, touch.aimMode)
                assertTrue(touch.isControlVisible(TouchControlGroup.LeftStick))
                assertTrue(touch.isControlVisible(TouchControlGroup.RightStick))
                assertFalse(keyboardOverlayEnabledForStream(touch.keyboardModeEnabled, StreamInputMode.KeyboardMouse))
            }
        }
    }

    @Test
    fun savedKeyboardChoiceAndLaterReturnToGamepadSurviveReloads() {
        for (playStoreRelease in listOf(false, true)) {
            val initial = loadAndroidSettings(null, playStoreRelease)
            val customButton = newKeyboardOverlayButton("space")!!
            var settings = initial.copy(androidTouch = initial.androidTouch.withKeyboardButtonAdded(customButton))

            for (keyboardEnabled in listOf(true, false)) {
                settings = settings.copy(androidTouch = settings.androidTouch.copy(keyboardModeEnabled = keyboardEnabled))
                // Reload repeatedly, including a switch between APK and Play distributions.
                repeat(3) { index ->
                    settings = loadAndroidSettings(OpenNowJson.encodeToString(settings), index % 2 == 0)
                    assertEquals(keyboardEnabled, settings.androidTouch.keyboardModeEnabled)
                    assertEquals(
                        keyboardEnabled,
                        keyboardOverlayEnabledForStream(settings.androidTouch.keyboardModeEnabled, StreamInputMode.KeyboardMouse),
                    )
                    assertTrue(settings.androidTouch.keyboardButtons.any { it.id == customButton.id })
                }
            }
        }
    }

    @Test
    fun launchInputChoiceAndConnectedDevicesDoNotOptIntoKeyboardOverlay() = runBlocking {
        for (nativeTouchAvailable in listOf(false, true)) {
            for (connected in listOf(false, true)) {
                for (rememberedMode in listOf(null) + StreamInputMode.entries) {
                    val settings = loadAndroidSettings(null, playStoreRelease = false)
                    val mode = chooseStreamInputModeAtStart(
                        nativeTouchAvailable = nativeTouchAvailable,
                        keyboardMouseConnected = connected,
                        promptForChoice = false,
                        rememberedMode = rememberedMode,
                    ) { error("No launch prompt was requested") }

                    assertFalse(keyboardOverlayEnabledForStream(settings.androidTouch.keyboardModeEnabled, mode))
                    val reloaded = loadAndroidSettings(OpenNowJson.encodeToString(settings), playStoreRelease = false)
                    assertFalse(reloaded.androidTouch.keyboardModeEnabled)
                }
            }
        }
    }
}
