package com.opencloudgaming.opennow

internal const val MIN_TOUCH_MOUSE_SENSITIVITY = 0.1f
internal const val MAX_TOUCH_MOUSE_SENSITIVITY = 5f

/** Keep physical mouse tuning separate, with the old shared value as the migration fallback. */
internal fun StreamSettings.effectiveTouchMouseSensitivity(): Float =
    (touchMouseSensitivity?.takeIf { it.isFinite() } ?: mouseSensitivity)
        .let { if (it.isFinite()) it.coerceIn(MIN_TOUCH_MOUSE_SENSITIVITY, MAX_TOUCH_MOUSE_SENSITIVITY) else 1f }
