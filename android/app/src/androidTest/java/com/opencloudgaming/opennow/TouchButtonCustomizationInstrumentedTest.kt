package com.opencloudgaming.opennow

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.compose.ui.semantics.SemanticsActions
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class TouchButtonCustomizationInstrumentedTest {
    @get:Rule val compose = createComposeRule()

    @Test fun editorSavesOneButtonAndCancelDiscardsTheDraft() {
        var touch by mutableStateOf(AndroidTouchSettings())
        compose.setContent { MaterialTheme { TouchButtonAppearanceEditor(touch) { touch = it } } }
        compose.onNodeWithText("Customize buttons").performClick()
        compose.onNodeWithText("RT").performScrollTo().performClick()
        compose.onNode(hasSetTextAction()).performTextInput("Shoot")
        compose.onNodeWithContentDescription("Shoot").performScrollTo().performClick()
        compose.onNodeWithText("Save").performClick()
        compose.runOnIdle {
            assertEquals(TouchButtonAppearance("Shoot", "Shoot"), touch.buttonAppearances["RT"])
            assertFalse(touch.buttonAppearances.containsKey("LT"))
        }
        compose.onNodeWithText("RT · Shoot").performScrollTo().performClick()
        compose.onNode(hasSetTextAction()).performTextReplacement("Discard")
        compose.onNodeWithText("Cancel").performClick()
        compose.runOnIdle { assertEquals("Shoot", touch.buttonAppearances["RT"]?.label) }
        compose.onNodeWithText("RT · Shoot").performScrollTo().performClick()
        compose.onNodeWithText("Reset this button").performScrollTo().performClick()
        compose.onNodeWithText("Save").performClick()
        compose.runOnIdle { assertFalse(touch.buttonAppearances.containsKey("RT")) }
    }

    @Test fun individualSizePreviewsSavesAndCancelDiscardsIt() {
        var touch by mutableStateOf(AndroidTouchSettings())
        compose.setContent { MaterialTheme { TouchButtonAppearanceEditor(touch) { touch = it } } }
        compose.onNodeWithText("Customize buttons").performClick()
        compose.onNodeWithText("A").performClick()
        compose.onNodeWithContentDescription("Button size").performScrollTo()
            .performSemanticsAction(SemanticsActions.SetProgress) { it(1.5f) }
        compose.onNodeWithText("Button size: 150%").assertExists()
        compose.onNodeWithText("Save").performClick()
        compose.runOnIdle {
            assertEquals(1.5f, touch.buttonAppearances.getValue("A").sizeScale, 0f)
            assertFalse(touch.buttonAppearances.containsKey("B"))
        }
        compose.onNodeWithText("A").performClick()
        compose.onNodeWithContentDescription("Button size").performScrollTo()
            .performSemanticsAction(SemanticsActions.SetProgress) { it(0.5f) }
        compose.onNodeWithText("Cancel").performClick()
        compose.runOnIdle { assertEquals(1.5f, touch.buttonAppearances.getValue("A").sizeScale, 0f) }
    }

    @Test fun buttonTapOpensEditorWhileDragOnlyMovesAndSizedBoundsFollowTheFace() {
        var selected by mutableStateOf<String?>(null)
        var offset by mutableStateOf(TouchOffset())
        var touch by mutableStateOf(AndroidTouchSettings())
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(
                    LocalTouchButtonEdit provides { selected = it },
                    LocalTouchButtonAppearances provides touch.buttonAppearances,
                ) {
                    Box(Modifier.fillMaxSize()) {
                        TouchControlGroup("editable", true, offset.x.dp, offset.y.dp,
                            { x, y -> offset = TouchOffset(x, y) }) {
                            Box(Modifier.testTag("control").editTouchButtonOnTap("A")) {
                                TouchCapFace("A", false, 64.dp)
                            }
                        }
                    }
                }
                selected?.let { button ->
                    TouchButtonAppearanceDialog(button, touch, { touch = it }, { selected = null })
                }
            }
        }
        val originalWidth = compose.onNodeWithTag("control").fetchSemanticsNode().boundsInRoot.width
        // Tap near the drag label too: decoration must not swallow button editing.
        compose.onNodeWithTag("control").performTouchInput { click(Offset(center.x, 12f)) }
        compose.onNodeWithContentDescription("Button size").performScrollTo()
            .performSemanticsAction(SemanticsActions.SetProgress) { it(1.5f) }
        compose.onNodeWithText("Save").performClick()
        val sizedWidth = compose.onNodeWithTag("control").fetchSemanticsNode().boundsInRoot.width
        assertEquals(originalWidth * 1.5f, sizedWidth, 1f)
        compose.onNodeWithTag("control").performTouchInput {
            swipe(center, center + Offset(180f, 100f), durationMillis = 600)
        }
        compose.runOnIdle { assertNull(selected); assertTrue(offset.x > 0f) }
        compose.onNodeWithTag("control").performTouchInput { click() }
        compose.onNodeWithContentDescription("Button size").assertExists()
    }

    @Test fun keyboardButtonCanBeResizedThenRemovedFromItsEditor() {
        val button = newKeyboardOverlayButton("letter_e")!!
        var touch by mutableStateOf(AndroidTouchSettings().withKeyboardButtonAdded(button))
        compose.setContent { MaterialTheme { TouchButtonAppearanceEditor(touch) { touch = it } } }
        compose.onNodeWithText("Customize buttons").performClick()
        compose.onNodeWithText("E").performScrollTo().performClick()
        compose.onNodeWithContentDescription("Button size").performScrollTo()
            .performSemanticsAction(SemanticsActions.SetProgress) { it(0.5f) }
        compose.onNodeWithText("Save").performClick()
        compose.runOnIdle { assertEquals(0.5f, touch.buttonAppearances.getValue(button.appearanceKey()).sizeScale, 0f) }
        compose.onNodeWithText("E").performScrollTo().performClick()
        compose.onNodeWithText("Remove button").performScrollTo().performClick()
        compose.runOnIdle {
            assertFalse(touch.keyboardButtons.any { it.id == button.id })
            assertFalse(touch.buttonAppearances.containsKey(button.appearanceKey()))
        }
    }

    @Test fun stickTapEditsDragMovesAndRemoveHidesIt() {
        var visible by mutableStateOf(true)
        var editing by mutableStateOf(false)
        var scale by mutableStateOf(1f)
        var offset by mutableStateOf(TouchOffset())
        compose.setContent {
            MaterialTheme {
                Box(Modifier.fillMaxSize()) {
                    if (visible) TouchControlGroup("WASD", true, offset.x.dp, offset.y.dp,
                        { x, y -> offset = TouchOffset(x, y) }, onEdit = { editing = true }) {
                        Box(Modifier.size(100.dp * scale).testTag("stick")) { Text("WASD") }
                    }
                }
                if (editing) TouchStickSizeDialog(scale, { scale = it },
                    onRemove = { visible = false; editing = false }, onDismiss = { editing = false })
            }
        }
        compose.onNodeWithTag("stick", useUnmergedTree = true).performTouchInput { swipe(center, center + Offset(100f, 60f), 600) }
        compose.runOnIdle { assertFalse(editing); assertTrue(offset.x > 0f) }
        compose.onNodeWithTag("stick", useUnmergedTree = true).performTouchInput { click() }
        compose.onNode(SemanticsMatcher.keyIsDefined(SemanticsActions.SetProgress)).performSemanticsAction(SemanticsActions.SetProgress) { it(1.5f) }
        compose.onNodeWithText("Save").performClick()
        compose.runOnIdle { assertEquals(1.5f, scale, 0f) }
        compose.onNodeWithTag("stick", useUnmergedTree = true).performTouchInput { click() }
        compose.onNodeWithText("Remove button").performClick()
        compose.onNodeWithTag("stick", useUnmergedTree = true).assertDoesNotExist()
    }

    @Test fun aControlCanBeDraggedAcrossTheScreenAndBack() {
        var offset by mutableStateOf(TouchOffset())
        compose.setContent {
            CompositionLocalProvider(LocalDensity provides Density(1f)) {
                MaterialTheme {
                    Box(Modifier.fillMaxSize().testTag("viewport")) {
                        TouchControlGroup("test-button", true, offset.x.dp, offset.y.dp,
                            { x, y -> offset = TouchOffset(x, y) }) {
                            Box(Modifier.size(80.dp).testTag("control")) { Text("LT") }
                        }
                    }
                }
            }
        }
        val width = compose.onNodeWithTag("viewport").fetchSemanticsNode().boundsInRoot.width
        compose.onNodeWithTag("control").performTouchInput {
            swipe(center, Offset(width - 100f, center.y), durationMillis = 600)
        }
        compose.runOnIdle { assertTrue("Cross-screen offset was ${offset.x}", offset.x > 280f) }
        val position = compose.onNodeWithTag("control").fetchSemanticsNode().boundsInRoot.left
        compose.onNodeWithTag("control").performTouchInput {
            swipe(center, Offset(40f - position, center.y), durationMillis = 600)
        }
        compose.runOnIdle { assertTrue("Return offset was ${offset.x}", offset.x < 100f) }
    }
}
