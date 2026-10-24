package com.opencloudgaming.opennow

import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import android.view.KeyEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class KeyboardOverlayCatalogTest {
    @Test
    fun heldModifiersRemainActiveWhenDirectionKeysChange() {
        val diagonal = setOf(
            KeyEvent.KEYCODE_W,
            KeyEvent.KEYCODE_A,
            KeyEvent.KEYCODE_SHIFT_LEFT,
            KeyEvent.KEYCODE_CTRL_LEFT,
        )
        val straight = diagonal - KeyEvent.KEYCODE_A

        assertEquals(0x03, keyboardModifierMaskForKeys(diagonal))
        assertEquals(0x03, keyboardModifierMaskForKeys(straight))
        assertEquals(0x02, keyboardModifierMaskForKeys(straight - KeyEvent.KEYCODE_SHIFT_LEFT))
    }

    @Test
    fun catalogCoversLettersDigitsFunctionKeysAndEveryOptionIsEncodable() {
        val actions = keyboardButtonCatalog.map(KeyboardButtonOption::id).toSet()
        ('a'..'z').forEach { assertTrue("Missing letter $it", "letter_$it" in actions) }
        (0..9).forEach { assertTrue("Missing digit $it", "digit_$it" in actions) }
        (1..12).forEach { assertTrue("Missing F$it", "function_f$it" in actions) }
        listOf(
            "page_up", "page_down", "tab", "space", "escape", "backspace", "delete",
            "insert", "home", "end", "arrow_up", "arrow_down", "arrow_left", "arrow_right",
            "shift_left", "shift_right", "ctrl_left", "ctrl_right", "alt_left", "alt_right",
            "meta_left", "meta_right", "print_screen", "pause_break", "mouse_left", "mouse_middle", "mouse_right",
        ).forEach { assertTrue("Missing $it", it in actions) }

        val unencodable = keyboardButtonCatalog.mapNotNull { option ->
            option.keyCode?.takeIf { keyCode ->
                InputEncoder.mapKeyboardPayload(keyCode = keyCode, unicode = 0, scanCode = 0, timestampUs = 0L) == null
            }?.let { option.id }
        }
        assertTrue("Catalog actions without an encoder mapping: $unencodable", unencodable.isEmpty())
        assertEquals(setOf(1, 2, 3), keyboardButtonCatalog.mapNotNull(KeyboardButtonOption::mouseButton).toSet())
    }

    @Test
    fun userCanAddTheSameActionMoreThanOnceAndRemoveOneWithoutLeavingStyleOrOffsetBehind() {
        val first = newKeyboardOverlayButton("letter_w")!!
        val second = newKeyboardOverlayButton("letter_w")!!
        assertTrue(first.id != second.id)

        val touch = AndroidTouchSettings()
            .withKeyboardButtonAdded(first)
            .withKeyboardButtonAdded(second)
            .withButtonAppearance(first.appearanceKey(), TouchButtonAppearance(label = "Sprint"))
            .withOffset("${first.positionKey()}_landscape", 28f, -40f)
        assertEquals(9, touch.keyboardButtons.size)
        assertEquals(7, touch.keyboardButtons.first { it.id == first.id }.layoutIndex)
        assertEquals(8, touch.keyboardButtons.first { it.id == second.id }.layoutIndex)

        val restored = OpenNowJson.decodeFromString<AndroidTouchSettings>(OpenNowJson.encodeToString(touch))
        assertEquals(touch.keyboardButtons, restored.keyboardButtons)
        assertEquals("Sprint", restored.buttonAppearances.getValue(first.appearanceKey()).label)
        assertEquals(TouchOffset(28f, -40f), restored.offsets["${first.positionKey()}_landscape"])

        val remaining = restored.withKeyboardButtonRemoved(first.id)
        assertFalse(remaining.keyboardButtons.any { it.id == first.id })
        assertTrue(remaining.keyboardButtons.any { it.id == second.id })
        assertFalse(remaining.buttonAppearances.containsKey(first.appearanceKey()))
        assertFalse(remaining.offsets.containsKey("${first.positionKey()}_landscape"))
        assertEquals(8, remaining.keyboardButtons.first { it.id == second.id }.layoutIndex)
        val third = newKeyboardOverlayButton("letter_e")!!
        assertEquals(9, remaining.withKeyboardButtonAdded(third).keyboardButtons.first { it.id == third.id }.layoutIndex)
    }

    @Test
    fun removingOneButtonKeepsOffsetsForIdsThatShareItsPrefix() {
        val settings = AndroidTouchSettings(
            keyboardButtons = defaultKeyboardOverlayButtons() + listOf(
                KeyboardOverlayButton("custom", "letter_a"),
                KeyboardOverlayButton("custom_more", "letter_b"),
            ),
            offsets = mapOf(
                "keyboard_button_custom_landscape" to TouchOffset(1f, 2f),
                "keyboard_button_custom_more_landscape" to TouchOffset(3f, 4f),
            ),
        )

        val remaining = settings.withKeyboardButtonRemoved("custom")
        assertFalse(remaining.offsets.containsKey("keyboard_button_custom_landscape"))
        assertEquals(TouchOffset(3f, 4f), remaining.offsets["keyboard_button_custom_more_landscape"])
    }

    @Test
    fun normalizationKeepsOnlyKnownActionsAndActiveButtonAppearances() {
        val valid = newKeyboardOverlayButton("page_down")!!
        val settings = AndroidTouchSettings(
            keyboardButtons = defaultKeyboardOverlayButtons() + listOf(
                valid,
                KeyboardOverlayButton("duplicate", "letter_a"),
                KeyboardOverlayButton("duplicate", "letter_b"),
                KeyboardOverlayButton("unknown_action", "not_a_key"),
                KeyboardOverlayButton("bad/id", "letter_c"),
            ),
            buttonAppearances = mapOf(
                valid.appearanceKey() to TouchButtonAppearance(label = "Next page"),
                "keyboard_custom_orphan" to TouchButtonAppearance(label = "Orphan"),
            ),
            offsets = mapOf(
                "${valid.positionKey()}_portrait" to TouchOffset(5f, 8f),
                "keyboard_button_removed_user_portrait" to TouchOffset(9f, 9f),
            ),
        ).normalizedTouchControls()

        assertEquals(9, settings.keyboardButtons.size)
        assertEquals("Next page", settings.buttonAppearances.getValue(valid.appearanceKey()).label)
        assertFalse(settings.buttonAppearances.containsKey("keyboard_custom_orphan"))
        assertTrue(settings.offsets.containsKey("${valid.positionKey()}_portrait"))
        assertFalse(settings.offsets.containsKey("keyboard_button_removed_user_portrait"))
    }

    @Test
    fun defaultLayoutSlotsAreSeparateAndLegacyGroupTranslationIsPreserved() {
        val base = (0..9).map { index -> keyboardButtonDefaultOffset(index, spacingDp = 55f) }
        assertEquals(base.size, base.toSet().size)
        val moved = keyboardButtonDefaultOffset(0, spacingDp = 55f, migratedGroupOffset = TouchOffset(20f, 12f))
        assertEquals(TouchOffset(-90f, -98f), moved)
        assertEquals(TouchOffset(), keyboardButtonDefaultOffset(-1, spacingDp = 55f))
    }

    @Test
    fun oldSettingsGetTheOriginalSevenButtonsAndInvalidPickerValuesAreRejected() {
        val restored = OpenNowJson.decodeFromString<AndroidTouchSettings>("{}")
        assertFalse(restored.keyboardModeEnabled)
        assertEquals(7, restored.keyboardButtons.size)
        assertNull(newKeyboardOverlayButton("not_a_catalog_action"))
        assertNotNull(newKeyboardOverlayButton("function_f12"))
    }

    @Test
    fun highSlotsStartAtTheCenterAfterSettingsReload() {
        // High slots without manual offsets used to extend above a short phone viewport.
        val touch = AndroidTouchSettings(keyboardButtons = (19..29).map {
            KeyboardOverlayButton("captured_$it", "digit_6", layoutIndex = it)
        })
        val restored = OpenNowJson.decodeFromString<AndroidTouchSettings>(OpenNowJson.encodeToString(touch))
        val size = 48f
        val spacing = size + 7f
        val width = 1524f / 1.875f - 28f
        val height = 720f / 1.875f - 16f
        assertTrue(keyboardButtonDefaultOffset(19, spacing).y < -height)
        restored.keyboardButtons.forEach {
            val offset = keyboardButtonViewportOffset(it.layoutIndex, spacing, size, width, height)
            assertEquals(width / 2f, width - size / 2f + offset.x, 0.001f)
            assertEquals(height / 2f, height - size / 2f + offset.y, 0.001f)
        }
    }

    @Test
    fun addingUpToTheLimitKeepsEveryNewKeyboardOrMouseButtonCentered() {
        var touch = AndroidTouchSettings()
        repeat(MAX_KEYBOARD_OVERLAY_BUTTONS - touch.keyboardButtons.size) { index ->
            val button = newKeyboardOverlayButton(if (index % 2 == 0) "mouse_left" else "letter_w")!!
            touch = touch.withKeyboardButtonAdded(button)
            val slot = touch.keyboardButtons.last().layoutIndex
            listOf(900f to 400f, 400f to 900f).forEach { (width, height) ->
                listOf(24f, 48f, 96f, 144f).forEach { size ->
                    val offset = keyboardButtonViewportOffset(slot, size + 7f, size, width, height)
                    assertEquals(TouchOffset(-(width - size) / 2f, -(height - size) / 2f), offset)
                }
            }
        }
        assertEquals(MAX_KEYBOARD_OVERLAY_BUTTONS, touch.keyboardButtons.size)
    }

    @Test
    fun newButtonsStillStartAtTheCenterAfterRemovingDefaultButtons() {
        val remainingDefaults = defaultKeyboardOverlayButtons().take(1)
        val button = newKeyboardOverlayButton("letter_e")!!
        val touch = AndroidTouchSettings(keyboardButtons = remainingDefaults).withKeyboardButtonAdded(button)
        val added = touch.keyboardButtons.last()
        assertEquals(7, added.layoutIndex)
        assertEquals(TouchOffset(-426f, -176f),
            keyboardButtonViewportOffset(added.layoutIndex, 55f, 48f, 900f, 400f))
        assertEquals(added.layoutIndex, touch.normalizedTouchControls().keyboardButtons.last().layoutIndex)
    }

    @Test
    fun automaticPlacementPreservesOriginalSevenButtonsAndBoundsOversizedControls() {
        (0..6).forEach { slot ->
            assertEquals(keyboardButtonDefaultOffset(slot, 55f),
                keyboardButtonViewportOffset(slot, 55f, 48f, 900f, 400f))
        }
        listOf(440f to 230f, 230f to 440f, 60f to 60f).forEach { (width, height) ->
            (0 until MAX_KEYBOARD_OVERLAY_BUTTONS).forEach { slot ->
                val offset = keyboardButtonViewportOffset(slot, 103f, 96f, width, height)
                assertTrue(offset.x in -(width - 96f).coerceAtLeast(0f)..0f)
                assertTrue(offset.y in -(height - 96f).coerceAtLeast(0f)..0f)
            }
        }
        assertEquals(TouchOffset(-50f, -100f),
            keyboardButtonViewportOffset(6, 55f, 48f, 900f, 400f, TouchOffset(-50f, -100f)))
    }

    @Test
    fun savedPositionsSurviveReloadAndAreClampedWhenTheViewportShrinks() {
        val button = newKeyboardOverlayButton("mouse_right")!!
        val touch = AndroidTouchSettings().withKeyboardButtonAdded(button)
            .withOffset("${button.positionKey()}_landscape", -500f, -200f)
            .withOffset("${button.positionKey()}_portrait", -100f, -600f)
        val restored = OpenNowJson.decodeFromString<AndroidTouchSettings>(OpenNowJson.encodeToString(touch))
        val slot = restored.keyboardButtons.last().layoutIndex
        val landscape = restored.offsets.getValue("${button.positionKey()}_landscape")
        val portrait = restored.offsets.getValue("${button.positionKey()}_portrait")
        assertEquals(landscape,
            keyboardButtonViewportOffset(slot, 55f, 48f, 900f, 400f, storedOffset = landscape))
        assertEquals(portrait,
            keyboardButtonViewportOffset(slot, 55f, 48f, 400f, 900f, storedOffset = portrait))
        assertEquals(TouchOffset(-252f, -152f),
            keyboardButtonViewportOffset(slot, 55f, 48f, 300f, 200f, storedOffset = landscape))
        assertEquals(TouchOffset(),
            keyboardButtonViewportOffset(slot, 55f, 48f, 900f, 400f, storedOffset = TouchOffset(100f, 100f)))
    }

    @Test
    fun customKeyboardButtonListHasNoExtraButtonSlotLimit() {
        val manyButtons = AndroidTouchSettings(
            keyboardButtons = defaultKeyboardOverlayButtons() + (0 until 100).map { index ->
                KeyboardOverlayButton("custom_$index", "letter_${'a' + (index % 26)}")
            },
        ).normalizedTouchControls()
        assertEquals(107, manyButtons.keyboardButtons.size)
    }

    @Test
    fun normalizationBoundsUntrustedButtonLists() {
        val raw = (0..300).map { index -> KeyboardOverlayButton("custom_$index", "letter_a") }
        val normalized = AndroidTouchSettings(keyboardButtons = raw).normalizedTouchControls()
        assertEquals(MAX_KEYBOARD_OVERLAY_BUTTONS, normalized.keyboardButtons.size)
        assertEquals(normalized, normalized.withKeyboardButtonAdded(newKeyboardOverlayButton("letter_b")!!))
    }

    @Test
    fun wasdStickReleasesAtCenterAndRejectsInvalidCoordinates() {
        assertEquals(setOf(KeyEvent.KEYCODE_W, KeyEvent.KEYCODE_D), keyboardStickKeyCodes(0.8f, -0.8f))
        assertEquals(emptySet<Int>(), keyboardStickKeyCodes(0.2f, -0.2f))
        assertEquals(emptySet<Int>(), keyboardStickKeyCodes(Float.NaN, 0f))
    }
}
