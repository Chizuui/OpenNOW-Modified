package com.opencloudgaming.opennow

import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test
import java.io.File

class GoofyScreensaverInstrumentedTest {
    @get:Rule val compose = createComposeRule()

    @Test
    fun avatarPlaysTheSceneAndAnyKeyDismissesThePreview() {
        compose.mainClock.autoAdvance = false
        compose.setContent { MaterialTheme { DeveloperPanel() } }
        compose.onNodeWithContentDescription("Play goofy screensaver").performClick()
        compose.mainClock.advanceTimeByFrame()
        compose.waitForIdle()
        compose.mainClock.advanceTimeBy(1_800)
        compose.mainClock.advanceTimeByFrame()
        compose.waitForIdle()
        compose.onNodeWithText("I got a bug", useUnmergedTree = true).assertExists()
        compose.onNodeWithText("× 0", useUnmergedTree = true).assertExists()
        capture("goofy-peeker-speaking.png")
        compose.mainClock.advanceTimeBy(1_800)
        compose.mainClock.advanceTimeByFrame()
        compose.waitForIdle()
        capture("goofy-snowman-strike.png")
        compose.mainClock.advanceTimeBy(700)
        compose.mainClock.advanceTimeByFrame()
        compose.waitForIdle()
        compose.onNodeWithText("I got a bug", useUnmergedTree = true).assertDoesNotExist()
        compose.onNodeWithText("× 1", useUnmergedTree = true).assertExists()
        capture("goofy-after-impact.png")
        compose.mainClock.advanceTimeBy(8_000)
        compose.mainClock.advanceTimeByFrame()
        compose.waitForIdle()
        compose.onNodeWithText("× 2", useUnmergedTree = true).assertExists()
        compose.onNode(isDialog()).performKeyInput { pressKey(Key.DirectionRight) }
        compose.mainClock.advanceTimeByFrame()
        compose.onNode(isDialog()).assertDoesNotExist()
        compose.onNodeWithContentDescription("Play goofy screensaver").assertExists()
        compose.onNodeWithContentDescription("Play goofy screensaver").performClick()
        compose.mainClock.advanceTimeByFrame()
        compose.waitForIdle()
        compose.onNodeWithText("× 0", useUnmergedTree = true).assertExists()
    }

    @Test
    fun tappingClosesThePreview() {
        compose.mainClock.autoAdvance = false
        compose.setContent { MaterialTheme { DeveloperPanel() } }
        compose.onNodeWithContentDescription("Play goofy screensaver").performClick()
        compose.mainClock.advanceTimeByFrame()
        compose.onNode(isDialog()).performTouchInput { click(center) }
        compose.mainClock.advanceTimeByFrame()
        compose.onNode(isDialog()).assertDoesNotExist()
    }

    private fun capture(name: String) {
        val bitmap = compose.onNode(isDialog()).captureToImage().asAndroidBitmap()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        File(context.getExternalFilesDir(null), name).outputStream().use {
            bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it)
        }
    }
}
