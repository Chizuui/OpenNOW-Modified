package com.opencloudgaming.opennow

import android.content.pm.ActivityInfo
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.test.espresso.Espresso.closeSoftKeyboard
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test

/** Exercises the shared app-settings/stream flow in a compact landscape window. */
class OnScreenControlsSettingsInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Before fun landscape() {
        compose.activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
        compose.waitUntil(10_000) {
            compose.activity.resources.configuration.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
        }
    }

    private fun show(
        initial: AndroidTouchSettings = AndroidTouchSettings(),
        fontScale: Float = 1f,
        onChange: (AndroidTouchSettings) -> Unit = {},
        onModeChange: ((Boolean) -> Unit)? = null,
        visible: Boolean? = null,
        onVisibilityChange: ((Boolean) -> Unit)? = null,
        onEdit: (() -> Unit)? = null,
    ) {
        var touch by mutableStateOf(initial)
        var effectiveVisible by mutableStateOf(visible)
        compose.setContent {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density, fontScale)) {
                OpenNowTheme(AppSettings(), physicalControllerConnected = false) {
                    androidx.compose.material3.Surface(Modifier.fillMaxSize()) {
                    OnScreenControlsSettings(
                        touch, emptyList(), false,
                        onChange = { touch = it; onChange(it) }, onPresetsChange = {}, onBack = {},
                        controlsVisible = effectiveVisible ?: touch.enabled,
                        onVisibilityChange = { enabled ->
                            if (visible != null) effectiveVisible = enabled
                            if (onVisibilityChange != null) onVisibilityChange(enabled)
                            else touch = touch.copy(enabled = enabled)
                        },
                        onModeChange = { enabled ->
                            touch = touch.copy(keyboardModeEnabled = enabled)
                            onModeChange?.invoke(enabled)
                        },
                        onEditLayout = onEdit,
                        modifier = Modifier.fillMaxSize(),
                    )
                    }
                }
            }
        }
    }

    private fun capture(name: String) {
        val bitmap = compose.onRoot().captureToImage().asAndroidBitmap()
        val folder = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext.getExternalFilesDir(null)!!
        java.io.FileOutputStream(java.io.File(folder, name)).use {
            bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it)
        }
    }

    @Test fun bothControlTypesAreDiscoverableWithoutOpeningAdvancedSettings() {
        show()
        compose.onNodeWithText("Gamepad buttons").assertIsDisplayed()
        compose.onNodeWithText("Keyboard & mouse buttons").assertIsDisplayed()
        capture("on-screen-controls.png")
        compose.onNodeWithText("Keyboard & mouse buttons").performClick()
        compose.onNodeWithText("Add button").assertIsDisplayed()
        compose.onNodeWithText("WASD movement stick").performScrollTo().assertIsDisplayed()
    }

    @Test fun keyboardButtonCanBeAddedResizedAndRemovedFromSettingsWithLargeText() {
        var saved = AndroidTouchSettings()
        show(fontScale = 1.5f, onChange = { saved = it })
        compose.onNodeWithText("Keyboard & mouse buttons").performScrollTo().performClick()
        compose.onNodeWithText("Add button").performScrollTo().performClick()
        compose.onNode(hasSetTextAction()).performTextInput("space")
        closeSoftKeyboard()
        compose.onNodeWithText("Space", substring = false).performClick()
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText("Space", substring = false))
        compose.onNodeWithText("Space", substring = false).performClick()
        compose.onNodeWithContentDescription("Button size").performScrollTo()
            .performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.SetProgress) { it(1.5f) }
        compose.onNodeWithText("Save", substring = false).assertIsDisplayed().performClick()
        val added = saved.keyboardButtons.single { it.actionId == "space" }
        assertTrue(saved.keyboardModeEnabled)
        assertEquals(1.5f, saved.buttonAppearances.getValue(added.appearanceKey()).sizeScale, 0f)
        compose.onNodeWithText("Space", substring = false).performClick()
        compose.onNodeWithText("Remove button").performScrollTo().performClick()
        compose.runOnIdle {
            assertFalse(saved.keyboardButtons.any { it.id == added.id })
            assertFalse(saved.buttonAppearances.containsKey(added.appearanceKey()))
            assertEquals(AndroidTouchSettings().offsets, saved.offsets)
        }
    }

    @Test fun liveVisibilityAndModeUseHostCallbacksAndLayoutEditingRemainsReachable() {
        var visibility: Boolean? = null
        var mode: Boolean? = null
        var edited = false
        // Persisted enabled can differ from effective visibility after physical-controller suppression.
        show(visible = false, onVisibilityChange = { visibility = it }, onModeChange = { mode = it }, onEdit = { edited = true })
        compose.onNodeWithText("Show on-screen controls").performClick()
        compose.runOnIdle { assertEquals(true, visibility) }
        compose.onNodeWithText("Keyboard & mouse buttons").performClick()
        compose.runOnIdle { assertEquals(true, mode) }
        compose.onNodeWithText("Move buttons on screen").performScrollTo().performClick()
        compose.runOnIdle { assertTrue(edited) }
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithText("Gamepad buttons").performScrollTo().performClick()
        compose.runOnIdle { assertEquals(false, mode) }
        compose.onNodeWithText("Visible controls").assertIsDisplayed()
        compose.onNodeWithText("Programmable buttons").assertIsDisplayed()
    }
    @Test fun compactStreamMenuKeepsControlsAudioAndStatusBarVisible() {
        var settings by mutableStateOf(AppSettings())
        compose.setContent {
            OpenNowTheme(AppSettings(), physicalControllerConnected = false) {
                androidx.compose.foundation.layout.Box(Modifier.fillMaxSize(), contentAlignment = androidx.compose.ui.Alignment.Center) {
                    StreamControlsPanel(
                        gameTitle = "Test game",
                        status = null,
                        settings = settings,
                        tvProfile = false,
                        touchControlsVisible = settings.androidTouch.enabled,
                        builtInGameTouchSupported = false,
                        nativeTouchActive = false,
                        keyboardOverlayActive = settings.androidTouch.keyboardModeEnabled,
                        gyroscopeAvailable = false,
                        controllerMouseAssistEnabled = false,
                        controllerMouseEmulationEnabled = false,
                        showSessionTimer = true,
                        sessionTimerLimit = SmartSessionLimit("Free", 1, SessionTimerMode.Countdown),
                        sessionStartedAtMs = 0L,
                        sessionNowMs = 0L,
                        audioMuted = false,
                        microphoneRequested = false,
                        microphonePermissionGranted = false,
                        microphoneEnabled = false,
                        statsVisible = false,
                        statusBarDragging = false,
                        receivedBitrateKbps = null,
                        recordingPhase = null,
                        touchLayoutEditing = false,
                        bugReportSubmission = BugReportSubmissionState(),
                        bugReportVersionCheck = AndroidBugReportVersionCheckState(),
                        update = AndroidUpdateState(),
                        bugReportPreflightProvider = { error("Not used by this test") },
                        onAudioToggle = { },
                        onMicrophoneToggle = { },
                        onStatsToggle = { },
                        onStatusBarDraggingToggle = { },
                        onStatsStyleCycle = { },
                        onStatsPositionCycle = { },
                        onStatsMetricsChange = { _ -> },
                        onStatsBackgroundEnabledChange = { _ -> },
                        onStatsBackgroundOpacityChange = { _ -> },
                        onKeyboardButtonToggle = { },
                        onVibrationToggle = { },
                        onTouchLayoutEditingToggle = { },
                        onKeyboardOpen = { },
                        onRecordingToggle = { },
                        onEsc = { },
                        onEnter = { },
                        onBackspace = { },
                        onSteamMenuOpen = { },
                        onControllerMouseAssistToggle = { },
                        onControllerMouseEmulationToggle = { },
                        onExit = { },
                        onKeyboardModeToggle = { enabled -> settings = settings.copy(androidTouch = settings.androidTouch.copy(keyboardModeEnabled = enabled)) },
                        onTouchControlsToggle = { },
                        onMousePadToggle = { },
                        onMouseDirectClickToggle = { },
                        onSharpeningToggle = { },
                        onSharpeningAmountChange = { _ -> },
                        onMaxBitrateChange = { _ -> },
                        onStretchToFitToggle = { },
                        onMouseSensitivityChange = { _ -> },
                        onMouseScrollSensitivityChange = { _ -> },
                        onPhysicalStickDeadZoneChange = { _ -> },
                        onTouchSettingsChange = { settings = settings.copy(androidTouch = it) },
                        onTouchPresetsChange = { _ -> },
                        onBugReportSubmit = { _, _, _, _, _ -> },
                        onBugReportReset = { },
                        onBugReportVersionCheck = { },
                        onOpenUpdate = { },
                        onButtonTone = { },
                        onClose = {},
                    )
                }
            }
        }
        capture("stream-controls.png")
        compose.onNodeWithText("On-screen controls").assertIsDisplayed()
        compose.onNodeWithText("Audio").assertIsDisplayed()
        compose.onNodeWithText("Status bar").assertIsDisplayed()
        compose.onNodeWithText("On-screen controls").performClick()
        compose.onNodeWithText("Keyboard & mouse buttons").assertIsDisplayed().performClick()
        compose.onNodeWithText("Add button").assertIsDisplayed()
    }

    @Test fun streamRowTextKeepsReadableContrastUnderALightHostTheme() {
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(
                    com.opencloudgaming.opennow.ui.controls.LocalControlRowStyle provides
                        com.opencloudgaming.opennow.ui.controls.ControlRowStyle.stream(),
                ) {
                    androidx.compose.material3.Surface(color = com.opencloudgaming.opennow.ui.theme.OpenNowPalette.PanelOverVideo) {
                        com.opencloudgaming.opennow.ui.controls.ControlNavigationRow("On-screen controls", {})
                    }
                }
            }
        }
        val pixels = compose.onNodeWithText("On-screen controls").captureToImage().toPixelMap()
        var lightest = 0f
        var darkest = 1f
        for (y in 0 until pixels.height) for (x in 0 until pixels.width) {
            val luminance = pixels[x, y].luminance()
            lightest = maxOf(lightest, luminance)
            darkest = minOf(darkest, luminance)
        }
        val contrast = (lightest + 0.05f) / (darkest + 0.05f)
        assertTrue("Stream label contrast was $contrast:1", contrast >= 4.5f)
    }

}
