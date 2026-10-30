package com.opencloudgaming.opennow

import android.content.pm.ActivityInfo
import android.content.res.Configuration
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performSemanticsAction
import org.junit.Rule
import org.junit.Test

class StreamFirstLaunchGuideInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun tutorialSurvivesAnimatedEntranceAndOpeningControlsOnCompactLandscape() {
        compose.activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
        compose.waitUntil(10_000) {
            compose.activity.resources.configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
        }
        var step by mutableStateOf(StreamGuideStep.OpenControls)
        var controlsOpen by mutableStateOf(false)
        var skipped = false
        compose.setContent {
            OpenNowTheme(AppSettings(), physicalControllerConnected = false) {
                Box(Modifier.fillMaxSize()) {
                    AnimatedLaunchOverlay {
                        StreamFirstLaunchGuide(
                            step = step,
                            controlsOpen = controlsOpen,
                            touchControlsEnabled = true,
                            controllerConnected = false,
                            streamMenuShortcut = DEFAULT_ANDROID_STREAM_MENU_SHORTCUT,
                            onOpenControls = { controlsOpen = true; step = StreamGuideStep.PressDone },
                            onSkip = { skipped = true },
                        )
                    }
                }
            }
        }
        compose.onNodeWithText("Open the stream menu").assertIsDisplayed()
        // Exercise the tutorial's composition transition independently of scroll hit testing.
        compose.onNodeWithText("Open controls").performSemanticsAction(SemanticsActions.OnClick) { it() }
        compose.onNodeWithText(compose.activity.getString(R.string.stream_guide_press_done)).assertIsDisplayed()
        compose.onNodeWithText(compose.activity.getString(R.string.action_skip))
            .performSemanticsAction(SemanticsActions.OnClick) { it() }
        compose.runOnIdle { check(skipped) }
    }
}
