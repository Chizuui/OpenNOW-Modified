package com.opencloudgaming.opennow

import android.content.pm.ActivityInfo
import androidx.activity.ComponentActivity
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.espresso.Espresso.closeSoftKeyboard
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test

/** Run in a short landscape window, e.g. 1312x720 at 320 or 400dpi. */
class TouchEditorCompactScreenInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Before fun landscapeWindow() {
        compose.activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
        compose.waitUntil(10_000) {
            compose.activity.resources.configuration.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
        }
    }

    private fun assertFullyOnScreen(node: SemanticsNodeInteraction) {
        node.assertIsDisplayed()
        val semantics = node.fetchSemanticsNode()
        val bounds = Rect(semantics.positionInWindow, androidx.compose.ui.geometry.Size(
            semantics.size.width.toFloat(), semantics.size.height.toFloat(),
        ))
        val screen = InstrumentationRegistry.getInstrumentation().targetContext.resources.displayMetrics
        assertTrue("Outside ${screen.widthPixels}x${screen.heightPixels}: $bounds",
            bounds.left >= 0 && bounds.top >= 0 && bounds.right <= screen.widthPixels && bounds.bottom <= screen.heightPixels)
        assertEquals("Clipped vertically: $bounds", bounds.height, semantics.boundsInWindow.height, 1f)
    }

    @Test fun keyboardAppearanceOptionsScrollWhileSaveAndCancelStayVisible() {
        val button = newKeyboardOverlayButton("mouse_middle")!!
        var touch by mutableStateOf(AndroidTouchSettings().withKeyboardButtonAdded(button))
        var open by mutableStateOf(true)
        compose.setContent {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density, 1.5f)) {
                MaterialTheme {
                    if (open) TouchButtonAppearanceDialog(button.appearanceKey(), touch, { touch = it },
                        onDismiss = { open = false }, wideLayout = true,
                        onRemove = { touch = touch.withKeyboardButtonRemoved(button.id) })
                }
            }
        }
        assertFullyOnScreen(compose.onNodeWithText("Save"))
        assertFullyOnScreen(compose.onNodeWithText("Cancel"))
        compose.onNodeWithText("Reset this button").performScrollTo().assertIsDisplayed()
        compose.onNodeWithContentDescription("Button size").performScrollTo()
            .performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.SetProgress) { it(1.5f) }
        // The preview grows above the slider, so bring the slider back into view after resizing.
        compose.onNodeWithContentDescription("Button size").performScrollTo()
        assertFullyOnScreen(compose.onNodeWithContentDescription("Button size"))
        compose.onNodeWithText("Remove button").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Save").performClick()
        compose.runOnIdle {
            assertFalse(open)
            assertEquals(1.5f, touch.buttonAppearances.getValue(button.appearanceKey()).sizeScale, 0f)
        }
    }

    @Test fun stickEditorCanSaveAndRemoveWithLargeText() {
        var removed = false
        compose.setContent {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density, 1.5f)) {
                MaterialTheme { TouchStickSizeDialog(1f, {}, { removed = true }, {}) }
            }
        }
        assertFullyOnScreen(compose.onNodeWithText("Save"))
        assertFullyOnScreen(compose.onNodeWithText("Cancel"))
        compose.onNodeWithText("Remove button").performScrollTo()
        assertFullyOnScreen(compose.onNodeWithText("Remove button"))
        compose.onNodeWithText("Remove button").performClick()
        compose.runOnIdle { assertTrue(removed) }
    }

    @Test fun keyPickerScrollsAndSearchesInTheAvailableWindow() {
        var selected: String? = null
        compose.setContent {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density, 1.5f)) {
                MaterialTheme { KeyboardButtonPickerDialog({}, { selected = it }) }
            }
        }
        assertFullyOnScreen(compose.onNodeWithText("Close"))
        compose.onNode(hasScrollToIndexAction()).assertHeightIsAtLeast(48.dp)
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText("F12"))
        assertFullyOnScreen(compose.onNodeWithText("F12"))
        compose.onNodeWithText("F12").performClick()
        compose.runOnIdle { assertEquals("function_f12", selected) }
        compose.onNode(hasSetTextAction()).performTextInput("middle")
        assertFullyOnScreen(compose.onNodeWithText("Close"))
        closeSoftKeyboard()
        compose.onNodeWithText("Mouse middle button").performClick()
        compose.runOnIdle { assertEquals("mouse_middle", selected) }
    }
}
