package com.opencloudgaming.opennow

import androidx.activity.compose.BackHandler
import androidx.annotation.StringRes
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import kotlinx.coroutines.delay
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.ui.controls.*

private val LocalTouchSettingsLivePreview = staticCompositionLocalOf { false }

private enum class TouchSettingsPage(@StringRes val title: Int) {
    Home(R.string.controls_title),
    Gamepad(R.string.controls_gamepad),
    Keyboard(R.string.controls_keyboard),
    Visibility(R.string.settings_touch_visible_controls),
    ExtraButtons(R.string.settings_touch_extra_buttons),
    Appearance(R.string.controls_appearance),
    Sizing(R.string.controls_sizing),
    Sticks(R.string.controls_sticks),
}

/** One owner for the settings and live-stream customization flows. The host owns persistence
 * and live input-mode transitions; this screen never creates a stream client or sends input. */
@Composable
internal fun OnScreenControlsSettings(
    touch: AndroidTouchSettings,
    presets: List<TouchControlPreset>,
    gyroscopeAvailable: Boolean,
    onChange: (AndroidTouchSettings) -> Unit,
    onPresetsChange: (List<TouchControlPreset>) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    livePreview: Boolean = false,
    controlsVisible: Boolean = touch.enabled,
    onVisibilityChange: (Boolean) -> Unit = { onChange(touch.copy(enabled = it)) },
    onModeChange: (Boolean) -> Unit = { onChange(touch.copy(keyboardModeEnabled = it)) },
    onEditLayout: (() -> Unit)? = null,
    onClose: () -> Unit = onBack,
    headerActions: @Composable RowScope.() -> Unit = {},
    additionalSettings: @Composable () -> Unit = {},
    notice: @Composable () -> Unit = {},
) {
    var page by rememberSaveable { mutableStateOf(TouchSettingsPage.Home) }
    var pickerOpen by rememberSaveable { mutableStateOf(false) }
    var editingButtonId by rememberSaveable { mutableStateOf<String?>(null) }
    val focusManager = LocalFocusManager.current
    val backFocusRequester = remember { FocusRequester() }
    val controllerNavigation = LocalControlRowStyle.current?.focusable ?: LocalSettingsControllerNavigationEnabled.current
    LaunchedEffect(page, controllerNavigation) {
        if (controllerNavigation) {
            delay(80)
            runCatching { backFocusRequester.requestFocus() }
        }
    }
    val goBack = { if (page == TouchSettingsPage.Home) onBack() else page = when (page) {
        TouchSettingsPage.Visibility, TouchSettingsPage.ExtraButtons -> TouchSettingsPage.Gamepad
        else -> TouchSettingsPage.Home
    } }
    BackHandler { goBack() }
    CompositionLocalProvider(LocalTouchSettingsLivePreview provides livePreview) {
    Column(modifier.fillMaxSize()) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = goBack, modifier = Modifier.focusRequester(backFocusRequester)) {
                Icon(painterResource(R.drawable.ic_arrow_back), stringResource(R.string.action_back))
            }
            Text(stringResource(page.title), Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
            headerActions()
            TextButton(onClick = onClose) { Text(stringResource(R.string.stream_panel_done)) }
        }
        // Each destination starts at its first row.
        key(page) {
            LazyColumn(
                Modifier.weight(1f).fillMaxWidth()
                    .onPreviewKeyEvent { handleVerticalDpadFocusMove(it, focusManager) },
                state = rememberLazyListState(),
                contentPadding = PaddingValues(12.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                when (page) {
                    TouchSettingsPage.Home -> {
                        item {
                            ControlSwitchRow(stringResource(R.string.controls_show), controlsVisible, onVisibilityChange)
                        }
                        item {
                            ControlNavigationRow(
                                stringResource(R.string.controls_gamepad),
                                value = stringResource(if (touch.keyboardModeEnabled) R.string.controls_gamepad_hint else R.string.controls_selected),
                                onClick = { onModeChange(false); page = TouchSettingsPage.Gamepad },
                            )
                        }
                        item {
                            ControlNavigationRow(
                                stringResource(R.string.controls_keyboard),
                                value = stringResource(if (touch.keyboardModeEnabled) R.string.controls_selected else R.string.controls_keyboard_hint),
                                onClick = { onModeChange(true); page = TouchSettingsPage.Keyboard },
                            )
                        }
                        if (onEditLayout != null) item {
                            ControlActionRow(
                                stringResource(R.string.controls_edit_layout), stringResource(R.string.controls_open), onEditLayout,
                                enabled = controlsVisible,
                                value = if (controlsVisible) null else stringResource(R.string.controls_show_to_move),
                            )
                        }
                        item {
                            ControlNavigationRow(stringResource(R.string.controls_appearance), onClick = { page = TouchSettingsPage.Appearance })
                        }
                        item {
                            ControlNavigationRow(stringResource(R.string.controls_sizing), onClick = { page = TouchSettingsPage.Sizing })
                        }
                        item {
                            ControlNavigationRow(stringResource(R.string.controls_sticks), onClick = { page = TouchSettingsPage.Sticks })
                        }
                        item { TouchControlPresetEditor(touch, presets, onChange, onPresetsChange) }
                        item { additionalSettings() }
                    }
                    TouchSettingsPage.Keyboard -> {
                        item { Text(stringResource(R.string.controls_keyboard_help), style = MaterialTheme.typography.bodyMedium) }
                        item {
                            ControlActionRow(
                                stringResource(R.string.keyboard_overlay_add_button), stringResource(R.string.controls_add),
                                onClick = { pickerOpen = true }, enabled = touch.keyboardButtons.size < MAX_KEYBOARD_OVERLAY_BUTTONS,
                            )
                        }
                        if (onEditLayout != null) item {
                            ControlActionRow(
                                stringResource(R.string.controls_edit_layout), stringResource(R.string.controls_open), onEditLayout,
                                enabled = controlsVisible,
                                value = if (controlsVisible) null else stringResource(R.string.controls_show_to_move),
                            )
                        } else item { Text(stringResource(R.string.controls_position_in_stream), style = MaterialTheme.typography.bodySmall) }
                        item {
                            ControlSwitchRow(
                                stringResource(R.string.controls_wasd_stick), touch.isControlVisible(TouchControlGroup.LeftStick),
                                { onChange(touch.withControlVisible(TouchControlGroup.LeftStick, it)) },
                            )
                        }
                        items(touch.keyboardButtons, key = { it.id }) { button ->
                            ControlNavigationRow(
                                label = keyboardButtonOption(button.actionId)?.label ?: button.actionId,
                                value = stringResource(R.string.controls_edit_button_hint),
                                onClick = { editingButtonId = button.id },
                            )
                        }
                    }
                    TouchSettingsPage.Gamepad -> {
                        item { notice() }
                        if (onEditLayout != null) item {
                            ControlActionRow(
                                stringResource(R.string.controls_edit_layout), stringResource(R.string.controls_open), onEditLayout,
                                enabled = controlsVisible,
                                value = if (controlsVisible) null else stringResource(R.string.controls_show_to_move),
                            )
                        }
                        item { ControlNavigationRow(stringResource(R.string.settings_touch_visible_controls), onClick = { page = TouchSettingsPage.Visibility }) }
                        item { ControlNavigationRow(stringResource(R.string.settings_touch_extra_buttons), onClick = { page = TouchSettingsPage.ExtraButtons }) }
                        item { TouchButtonAppearanceEditor(touch, onChange) }
                    }
                    TouchSettingsPage.Visibility -> item {
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { GamepadVisibilitySettings(touch, onChange) }
                    }
                    TouchSettingsPage.ExtraButtons -> item {
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { GamepadExtraButtonsSettings(touch, onChange) }
                    }
                    TouchSettingsPage.Appearance -> item {
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            if (!touch.keyboardModeEnabled) AppearanceSettings(touch, onChange)
                            TouchButtonAppearanceEditor(touch, onChange)
                            TouchSettingsSlider(stringResource(R.string.stream_panel_opacity), touch.opacity, 0f, 1f, 0.05f) {
                                onChange(touch.copy(opacity = it))
                            }
                        }
                    }
                    TouchSettingsPage.Sizing -> item {
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text(stringResource(R.string.controls_position_in_stream), style = MaterialTheme.typography.bodySmall)
                            ControlActionRow(stringResource(R.string.stream_panel_reset_layout), stringResource(R.string.action_reset),
                                onClick = { onChange(touch.withResetOffsets()) })
                            SizingSettings(touch, onChange)
                        }
                    }
                    TouchSettingsPage.Sticks -> item {
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { SticksSettings(touch, onChange, gyroscopeAvailable) }
                    }
                }
            }
        }
    }
    } // Live-preview policy
    if (pickerOpen) KeyboardButtonPickerDialog(
        onDismiss = { pickerOpen = false },
        onSelect = { actionId ->
            newKeyboardOverlayButton(actionId)?.let { onChange(touch.withKeyboardButtonAdded(it)) }
            pickerOpen = false
        },
    )
    val editingButton = touch.keyboardButtons.firstOrNull { it.id == editingButtonId }
    if (editingButton != null) TouchButtonAppearanceDialog(
        editingButton.appearanceKey(), touch, onChange,
        onDismiss = { editingButtonId = null },
        onRemove = { onChange(touch.withKeyboardButtonRemoved(editingButton.id)); editingButtonId = null },
    )
}

@Composable
private fun AppearanceSettings(touch: AndroidTouchSettings, onChange: (AndroidTouchSettings) -> Unit) {
    val touchStyleOptions = TouchControllerStyle.entries.map { style ->
        SettingsChoiceOption(style.name, touchControllerStyleLabel(style))
    }
    ChoiceOptionRow(
        stringResource(R.string.settings_touch_skin),
        touchStyleOptions,
        touch.touchControllerStyle.name,
        description = stringResource(R.string.settings_touch_skin_desc),
    ) { styleName ->
        val style = TouchControllerStyle.valueOf(styleName)
        onChange(touch.copy(touchControllerStyle = style))
    }
    // The skins differ by shape now, which a list of names cannot show.
    TouchControllerSkinPreview(
        style = touch.touchControllerStyle,
        tint = touch.touchSkinTint,
        opacity = touch.opacity,
        showLabels = touch.touchButtonLabels,
        modifier = Modifier.padding(vertical = 8.dp),
    )
    val touchTintOptions = TOUCH_SKIN_TINTS.map { SettingsChoiceOption(it.id, it.label) }
    ChoiceOptionRow(
        stringResource(R.string.settings_touch_skin_tint),
        touchTintOptions,
        touchSkinTintId(touch.touchSkinTint),
        description = stringResource(R.string.settings_touch_skin_tint_desc),
    ) { tintId ->
        onChange(touch.copy(touchSkinTint = touchSkinTintForId(tintId)))
    }
    SettingSwitch(
        label = stringResource(R.string.settings_touch_button_labels),
        checked = touch.touchButtonLabels,
    ) { enabled ->
        onChange(touch.copy(touchButtonLabels = enabled))
    }

}

@Composable
private fun GamepadVisibilitySettings(touch: AndroidTouchSettings, onChange: (AndroidTouchSettings) -> Unit) {
    Text(
        text = stringResource(R.string.settings_touch_visible_controls),
        color = LocalContentColor.current,
        style = MaterialTheme.typography.titleSmall,
    )
    Text(
        text = stringResource(R.string.settings_touch_visible_controls_desc),
        color = controlRowStyle().supportingColor,
        style = MaterialTheme.typography.bodySmall,
    )
    TouchControlGroup.entries.forEach { group ->
        SettingSwitch(
            label = stringResource(touchControlGroupLabelRes(group)),
            checked = touch.isControlVisible(group),
        ) { visible ->
            onChange(touch.withControlVisible(group, visible))
        }
    }
}

@Composable
private fun GamepadExtraButtonsSettings(touch: AndroidTouchSettings, onChange: (AndroidTouchSettings) -> Unit) {
    Text(
        text = stringResource(R.string.settings_touch_extra_buttons),
        color = LocalContentColor.current,
        style = MaterialTheme.typography.titleSmall,
    )
    Text(
        text = stringResource(R.string.settings_touch_extra_buttons_desc),
        color = controlRowStyle().supportingColor,
        style = MaterialTheme.typography.bodySmall,
    )
    val extraButtonOptions = TouchExtraButtonAction.entries.map { action ->
        SettingsChoiceOption(action.name, touchExtraButtonActionLabel(action))
    }
    repeat(TOUCH_EXTRA_BUTTON_COUNT) { index ->
        val customCombo = extraButtonComboKey(index) in touch.extraButtonCombos
        val customValue = "custom-combo-$index"
        val options = if (customCombo) {
            listOf(
                SettingsChoiceOption(
                    customValue,
                    stringResource(
                        R.string.touch_extra_combo_custom_value,
                        touchExtraButtonComboLabel(touch.extraButtonCombo(index)),
                    ),
                ),
            ) + extraButtonOptions
        } else {
            extraButtonOptions
        }
        ChoiceOptionRow(
            label = stringResource(R.string.settings_touch_extra_button, index + 1),
            options = options,
            selectedValue = if (customCombo) customValue else touch.extraButtonAction(index).name,
        ) { actionName ->
            if (actionName == customValue) return@ChoiceOptionRow
            val action = TouchExtraButtonAction.valueOf(actionName)
            onChange(touch.withExtraButtonAction(index, action))
        }
    }
    TouchExtraButtonComboEditor(touch, onChange)
    TouchSettingsSlider(
        stringResource(R.string.settings_touch_extra_button_size),
        touch.extraButtonScale,
        0.6f,
        1.6f,
        0.05f,
    ) { value ->
        onChange(touch.copy(extraButtonScale = value))
    }

}

@Composable
private fun SticksSettings(touch: AndroidTouchSettings, onChange: (AndroidTouchSettings) -> Unit, gyroscopeAvailable: Boolean) {
    if (!touch.keyboardModeEnabled) {
        val touchAimOptions = listOf(
            SettingsChoiceOption(TouchAimMode.LockJoystick.name, stringResource(R.string.stream_joysticks_lock_joystick)),
            SettingsChoiceOption(TouchAimMode.LockZone.name, stringResource(R.string.stream_joysticks_lock_zone)),
        )
        ChoiceOptionRow(stringResource(R.string.stream_joysticks_aim_mode), touchAimOptions, touch.aimMode.name) { modeName ->
            val mode = TouchAimMode.valueOf(modeName)
            onChange(touch.copy(aimMode = mode))
        }
        if (touch.aimMode == TouchAimMode.LockZone) {
            TouchSettingsSlider(
                stringResource(R.string.stream_joysticks_aim_zone_scale),
                touch.aimZoneScale,
                0.5f,
                1.5f,
                0.05f,
            ) { value ->
                onChange(touch.copy(aimZoneScale = value))
            }
            TouchSettingsSlider(
                stringResource(R.string.stream_joysticks_aim_zone_sensitivity),
                touch.aimZoneSensitivity,
                0.25f,
                3f,
                0.05f,
            ) { value ->
                onChange(touch.copy(aimZoneSensitivity = value))
            }
        }
    }
    val joystickModeOptions = listOf(
        SettingsChoiceOption(TouchJoystickMode.Fixed.name, stringResource(R.string.stream_panel_joystick_fixed)),
        SettingsChoiceOption(TouchJoystickMode.Dynamic.name, stringResource(R.string.stream_panel_joystick_dynamic)),
    )
    ChoiceOptionRow(stringResource(R.string.controls_joystick_placement), joystickModeOptions, touch.joystickMode.name) { modeName ->
        val mode = TouchJoystickMode.valueOf(modeName)
        onChange(touch.copy(joystickMode = mode))
    }
    TouchSettingsSlider(stringResource(R.string.stream_joysticks_dead_zone), touch.joystickDeadZone, 0f, 0.3f, 0.01f) { value ->
        onChange(touch.copy(joystickDeadZone = value))
    }
    if (!touch.keyboardModeEnabled) {
        SettingSwitch(
            label = stringResource(R.string.settings_touch_gyro),
            checked = touch.gyroscopeEnabled && gyroscopeAvailable,
            enabled = gyroscopeAvailable,
            description = stringResource(
                if (gyroscopeAvailable) R.string.settings_touch_gyro_desc
                else R.string.settings_touch_gyro_unavailable,
            ),
        ) { enabled ->
            onChange(touch.copy(gyroscopeEnabled = enabled))
        }
        if (touch.gyroscopeEnabled && gyroscopeAvailable) {
            TouchSettingsSlider(stringResource(R.string.settings_touch_gyro_sensitivity), touch.gyroscopeSensitivity, 0.25f, 3f, 0.05f) { value ->
                onChange(touch.copy(gyroscopeSensitivity = value))
            }
            TouchSettingsSlider(stringResource(R.string.settings_touch_gyro_dead_zone), touch.gyroscopeDeadZone, 0f, 0.2f, 0.005f) { value ->
                onChange(touch.copy(gyroscopeDeadZone = value))
            }
            TouchSettingsSlider(stringResource(R.string.settings_touch_gyro_smoothing), touch.gyroscopeSmoothing, 0f, 0.9f, 0.05f) { value ->
                onChange(touch.copy(gyroscopeSmoothing = value))
            }
            SettingSwitch(
                stringResource(R.string.settings_touch_gyro_invert_horizontal),
                touch.gyroscopeInvertHorizontal,
            ) { enabled ->
                onChange(touch.copy(gyroscopeInvertHorizontal = enabled))
            }
            SettingSwitch(
                stringResource(R.string.settings_touch_gyro_invert_vertical),
                touch.gyroscopeInvertVertical,
            ) { enabled ->
                onChange(touch.copy(gyroscopeInvertVertical = enabled))
            }
        }
    }

}

@Composable
private fun SizingSettings(touch: AndroidTouchSettings, onChange: (AndroidTouchSettings) -> Unit) {
    TouchSettingsSlider(stringResource(R.string.stream_panel_layout_scale), touch.scale, 0.6f, 1.4f, 0.05f) { value -> onChange(touch.copy(scale = value)) }
    TouchSettingsSlider(stringResource(R.string.stream_panel_button_size), touch.buttonScale, 0.65f, 1.5f, 0.05f) { value -> onChange(touch.copy(buttonScale = value)) }
    TouchSettingsSlider(stringResource(R.string.stream_joysticks_stick_size), touch.stickScale, 0.65f, 1.5f, 0.05f) { value -> onChange(touch.copy(stickScale = value)) }
    if (!touch.keyboardModeEnabled) TouchSettingsSlider(stringResource(R.string.settings_touch_face_size), touch.faceButtonScale, 0.6f, 1.5f, 0.05f) { value -> onChange(touch.copy(faceButtonScale = value)) }
    if (!touch.keyboardModeEnabled) TouchSettingsSlider(stringResource(R.string.settings_touch_dpad_size), touch.dpadScale, 0.6f, 1.5f, 0.05f) { value -> onChange(touch.copy(dpadScale = value)) }
    if (!touch.keyboardModeEnabled) TouchSettingsSlider(stringResource(R.string.settings_touch_shoulders_size), touch.shoulderButtonScale, 0.6f, 1.5f, 0.05f) { value -> onChange(touch.copy(shoulderButtonScale = value)) }
    if (!touch.keyboardModeEnabled) TouchSettingsSlider(stringResource(R.string.settings_touch_center_size), touch.centerButtonScale, 0.6f, 1.5f, 0.05f) { value -> onChange(touch.copy(centerButtonScale = value)) }
    TouchSettingsSlider(stringResource(R.string.settings_touch_left_stick_size), touch.leftStickScale, 0.6f, 1.5f, 0.05f) { value -> onChange(touch.copy(leftStickScale = value)) }
    if (!touch.keyboardModeEnabled) TouchSettingsSlider(stringResource(R.string.settings_touch_right_stick_size), touch.rightStickScale, 0.6f, 1.5f, 0.05f) { value -> onChange(touch.copy(rightStickScale = value)) }
    if (!touch.keyboardModeEnabled) TouchSettingsSlider(stringResource(R.string.settings_touch_stick_knob_size), touch.stickKnobScale, 0.28f, 0.72f, 0.02f) { value -> onChange(touch.copy(stickKnobScale = value)) }
    TouchSettingsSlider(stringResource(R.string.stream_panel_edge_padding), touch.edgePaddingDp, 0f, 72f, 1f, unit = "dp") { value -> onChange(touch.copy(edgePaddingDp = value)) }
    TouchSettingsSlider(stringResource(R.string.stream_panel_bottom_padding), touch.bottomPaddingDp, 0f, 120f, 1f, unit = "dp") { value -> onChange(touch.copy(bottomPaddingDp = value)) }
    if (!touch.keyboardModeEnabled) TouchSettingsSlider(stringResource(R.string.controls_left_horizontal), touch.leftOffsetXDp, -220f, 220f, 2f, unit = "dp") { value -> onChange(touch.copy(leftOffsetXDp = value)) }
    if (!touch.keyboardModeEnabled) TouchSettingsSlider(stringResource(R.string.stream_panel_left_position), touch.leftOffsetYDp, -160f, 160f, 2f, unit = "dp") { value -> onChange(touch.copy(leftOffsetYDp = value)) }
    if (!touch.keyboardModeEnabled) TouchSettingsSlider(stringResource(R.string.controls_right_horizontal), touch.rightOffsetXDp, -220f, 220f, 2f, unit = "dp") { value -> onChange(touch.copy(rightOffsetXDp = value)) }
    if (!touch.keyboardModeEnabled) TouchSettingsSlider(stringResource(R.string.stream_panel_right_position), touch.rightOffsetYDp, -160f, 160f, 2f, unit = "dp") { value -> onChange(touch.copy(rightOffsetYDp = value)) }

}

/** Settings commit on release; the stream also previews changes against the game while dragging. */
@Composable
private fun TouchSettingsSlider(
    label: String,
    value: Float,
    min: Float,
    max: Float,
    step: Float,
    unit: String? = null,
    onChange: (Float) -> Unit,
) {
    val preview = LocalTouchSettingsLivePreview.current
    ControlSliderRow(
        label = label, value = value, min = min, max = max, step = step,
        unit = unit, onChange = onChange,
        onChangePreview = onChange.takeIf { preview },
    )
}
