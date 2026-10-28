package com.opencloudgaming.opennow

import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent

/** Call only inside a sampled diagnostic supplier, never for every input packet. */
internal object ControllerInputDiagnostics {
    private val buttonCodes = (
        (KeyEvent.KEYCODE_BUTTON_A..KeyEvent.KEYCODE_BUTTON_MODE) +
            (KeyEvent.KEYCODE_BUTTON_1..KeyEvent.KEYCODE_BUTTON_16) +
            listOf(KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_DOWN,
                KeyEvent.KEYCODE_DPAD_LEFT, KeyEvent.KEYCODE_DPAD_RIGHT,
                KeyEvent.KEYCODE_DPAD_CENTER, KeyEvent.KEYCODE_BACK, KeyEvent.KEYCODE_MENU)
        ).toIntArray()

    fun capabilities(device: InputDevice, slot: Int?): String {
        val supported = device.hasKeys(*buttonCodes)
        return buildString {
            // inputId is an ephemeral Android input handle, not an account/device identifier.
            append("controller capabilities inputId=${device.id} slot=${slot ?: -1} ")
            append("controllerName=${device.name.take(96).replace('\n', ' ').replace('\r', ' ')} ")
            append("vendor=0x${device.vendorId.toString(16)} product=0x${device.productId.toString(16)} ")
            append("sources=0x${device.sources.toString(16)} keyboardType=${device.keyboardType} ")
            append("buttons=[")
            buttonCodes.forEachIndexed { index, code ->
                if (supported[index]) append("${KeyEvent.keyCodeToString(code)}($code),")
            }
            append("] ranges=[")
            device.motionRanges.take(32).forEach { range ->
                append("${MotionEvent.axisToString(range.axis)}(${range.axis}):source=0x${range.source.toString(16)},")
                append("min=${range.min},max=${range.max},flat=${range.flat},fuzz=${range.fuzz},resolution=${range.resolution};")
            }
            append("] rangeCount=${device.motionRanges.size}")
        }
    }

    fun key(event: KeyEvent): String =
        "inputId=${event.deviceId} key=${event.keyCode} keyName=${KeyEvent.keyCodeToString(event.keyCode)} " +
            "rawScan=${event.scanCode} action=${event.action} source=0x${event.source.toString(16)} " +
            "flags=0x${event.flags.toString(16)} repeat=${event.repeatCount}"

    fun axes(event: MotionEvent): String = buildString {
        for (axis in intArrayOf(MotionEvent.AXIS_X, MotionEvent.AXIS_Y, MotionEvent.AXIS_Z,
            MotionEvent.AXIS_RZ, MotionEvent.AXIS_RX, MotionEvent.AXIS_RY,
            MotionEvent.AXIS_HAT_X, MotionEvent.AXIS_HAT_Y, MotionEvent.AXIS_LTRIGGER,
            MotionEvent.AXIS_RTRIGGER, MotionEvent.AXIS_BRAKE, MotionEvent.AXIS_GAS)) {
            append("${MotionEvent.axisToString(axis)}=${event.getAxisValue(axis)} ")
        }
    }
}
