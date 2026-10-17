package com.opencloudgaming.opennow

import kotlin.math.roundToInt

/** Sends each direct touch as a host position, independent of the game's current cursor. */
internal class DirectClickMouse {
    private var active = false
    var isPressed = false
        private set

    fun press(
        position: AbsoluteMousePosition?,
        sendPosition: (AbsoluteMousePosition) -> Boolean,
        setButton: (Boolean) -> Boolean,
        clickEnabled: Boolean = true,
    ): Boolean {
        if (active || position == null || !sendPosition(position)) return false
        isPressed = clickEnabled && setButton(true)
        active = !clickEnabled || isPressed
        return active
    }

    fun move(position: AbsoluteMousePosition?, sendPosition: (AbsoluteMousePosition) -> Boolean): Boolean {
        if (!active || position == null) return false
        return sendPosition(position)
    }

    fun release(setButton: ((Boolean) -> Boolean)?) {
        if (isPressed) setButton?.invoke(false)
        isPressed = false
        active = false
    }
}

/** The absolute packet includes the decoded stream extent; view dimensions never go on the wire. */
internal fun directClickMousePosition(target: StreamPoint, width: Int, height: Int): AbsoluteMousePosition? {
    if (
        width !in 1..65535 || height !in 1..65535 ||
        !target.x.isFinite() || !target.y.isFinite()
    ) return null
    return AbsoluteMousePosition(
        x = target.x.coerceIn(0f, width.toFloat()).roundToInt(),
        y = target.y.coerceIn(0f, height.toFloat()).roundToInt(),
        width = width,
        height = height,
    )
}
