package com.opencloudgaming.opennow

import android.content.pm.ActivityInfo
import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import org.junit.Rule
import org.junit.Test
import java.io.File

class TouchOverlayModeInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun freshInstallRendersGamepadAndKeyboardRequiresExplicitSelection() {
        compose.activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
        compose.waitUntil(10_000) {
            compose.activity.resources.configuration.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
        }
        var touch by mutableStateOf(loadAndroidSettings(null, playStoreRelease = false).androidTouch)
        compose.setContent {
            val client = remember {
                WebRtcRuntime.ensureInitialized(compose.activity)
                NativeStreamClient(compose.activity, lowLatencyGameAudio = false, onState = {}, onError = {})
            }
            DisposableEffect(client) { onDispose { client.release() } }
            OpenNowTheme(AppSettings(), physicalControllerConnected = false) {
                Box(Modifier.fillMaxSize().background(Color.Black)) {
                    TouchOverlay(
                        client = client,
                        touch = touch,
                        onButtonTone = {},
                        layoutEditing = false,
                        onSaveAllOffsets = {},
                        onButtonAppearanceChange = { _, _ -> },
                    )
                }
            }
        }

        fun assertGamepad() {
            listOf("A", "B", "X", "Y", "LS", "RS").forEach {
                compose.onNodeWithText(it, substring = false).assertIsDisplayed()
            }
            compose.onNodeWithText("WASD").assertDoesNotExist()
            compose.onNodeWithText("LMB").assertDoesNotExist()
        }
        assertGamepad()
        val screenshot = compose.onRoot().captureToImage().asAndroidBitmap()
        val output = File(compose.activity.getExternalFilesDir(null), "default-gamepad-overlay.png")
        output.outputStream().use { screenshot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }

        compose.runOnIdle { touch = touch.copy(keyboardModeEnabled = true) }
        compose.onNodeWithText("WASD").assertIsDisplayed()
        compose.onNodeWithText("LMB").assertIsDisplayed()
        compose.onNodeWithText("A", substring = false).assertDoesNotExist()

        compose.runOnIdle { touch = touch.copy(keyboardModeEnabled = false) }
        assertGamepad()
    }
}
