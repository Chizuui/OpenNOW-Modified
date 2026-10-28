package com.opencloudgaming.opennow

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import com.opencloudgaming.opennow.ui.controls.ControlSliderRow

/** Shared by the K&M editor and both finger-mouse settings surfaces. */
@Composable
internal fun TouchMouseSensitivitySlider(settings: StreamSettings, onChange: (Float) -> Unit) {
    ControlSliderRow(
        label = stringResource(R.string.stream_touch_mouse_sensitivity),
        value = settings.effectiveTouchMouseSensitivity(),
        min = MIN_TOUCH_MOUSE_SENSITIVITY,
        max = MAX_TOUCH_MOUSE_SENSITIVITY,
        step = 0.05f,
        valueFormatter = { "%.2f×".format(it) },
        description = stringResource(R.string.stream_touch_mouse_sensitivity_description),
        onChange = onChange,
    )
}
