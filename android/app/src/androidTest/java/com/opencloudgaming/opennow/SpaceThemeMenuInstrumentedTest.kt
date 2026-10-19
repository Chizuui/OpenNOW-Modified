package com.opencloudgaming.opennow

import androidx.activity.ComponentActivity
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class SpaceThemeMenuInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun iconMenuSwitchesThemesBothWaysAndDismissesAfterEachChoice() {
        var enabled by mutableStateOf(false)
        val changes = mutableListOf<Boolean>()
        compose.setContent {
            MaterialTheme {
                SpaceUiTheme(enabled = enabled) {
                    val focus = remember { FocusRequester() }
                    TopBarProfileMenu(OpenNowUiState(settings = AppSettings(experimentalSpaceUi = enabled)),
                        focusRequester = focus, nextFocusRequester = null,
                        onOpenSettings = {}, onOpenLocalApps = {},
                        onSpaceUiChange = { enabled = it; changes.add(it) })
                }
            }
        }
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val menu = context.getString(R.string.profile_menu_description)
        val theme = context.getString(R.string.space_ui_name)
        compose.onNodeWithContentDescription(menu).performClick()
        compose.onNodeWithText(theme).assertIsOff().performClick()
        compose.onNodeWithText(theme).assertDoesNotExist()
        compose.runOnIdle { assertEquals(listOf(true), changes) }
        compose.onNodeWithContentDescription(menu).performClick()
        compose.onNodeWithText(theme).assertIsOn().performClick()
        compose.onNodeWithText(theme).assertDoesNotExist()
        compose.runOnIdle { assertEquals(listOf(true, false), changes) }
    }
}
