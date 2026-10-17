package com.opencloudgaming.opennow

import android.view.MotionEvent
import org.junit.Assert.*
import org.junit.Test

class TouchMouseStateInstrumentedTest {
    private class Output : TouchMouseOutput {
        override val settings = StreamSettings(mouseAcceleration = 0)
        val moves = mutableListOf<Pair<Int, Int>>()
        val positions = mutableListOf<AbsoluteMousePosition>()
        val buttons = mutableListOf<Boolean>()
        var taps = 0
        var rightTaps = 0
        var scroll = 0
        override fun setTouchMouseButton(pressed: Boolean): Boolean { buttons += pressed; return true }
        override fun setDirectClickMouseButton(pressed: Boolean) = setTouchMouseButton(pressed)
        override fun sendDirectClickMousePosition(position: AbsoluteMousePosition): Boolean { positions += position; return true }
        override fun sendRawMouseMove(dx: Int, dy: Int): Boolean { moves += dx to dy; return true }
        override fun sendTouchMouseClick() { taps++ }
        override fun sendTouchMouseRightClick() { rightTaps++ }
        override fun sendTouchMouseWheel(delta: Int) { scroll += delta }
    }

    private fun TouchMouseState.event(
        output: Output,
        action: Int,
        time: Long,
        x: Float = 100f,
        y: Float = 100f,
        buttonsEnabled: Boolean = false,
        direct: Boolean = false,
    ) {
        val event = MotionEvent.obtain(time, time, action, x, y, 0)
        try {
            handle(event, true, output, emptySet(), directClick = direct, buttonsEnabled = buttonsEnabled,
                width = 1920, height = 1080, decodedStreamWidth = 1920, decodedStreamHeight = 1080)
        } finally { event.recycle() }
    }

    @Test fun keyboardMouseTapsAndRepeatedDragsOnlyMoveTheCursor() {
        val output = Output()
        val state = TouchMouseState()
        // A tap followed immediately by a drag used to arm double-tap dragging.
        state.event(output, MotionEvent.ACTION_DOWN, 1000)
        state.event(output, MotionEvent.ACTION_UP, 1050)
        state.event(output, MotionEvent.ACTION_DOWN, 1100)
        state.event(output, MotionEvent.ACTION_MOVE, 1150, x = 140f)
        state.event(output, MotionEvent.ACTION_UP, 1200, x = 145f)
        repeat(10) { i ->
            val time = 1300L + i * 100L
            state.event(output, MotionEvent.ACTION_DOWN, time)
            state.event(output, MotionEvent.ACTION_MOVE, time + 20, x = 130f)
            state.event(output, MotionEvent.ACTION_UP, time + 40, x = 135f)
        }
        state.reset(output)
        assertTrue(output.moves.any { it.first != 0 })
        assertTrue(output.buttons.isEmpty())
        assertEquals(0, output.taps)
        assertEquals(0, output.rightTaps)
    }

    @Test fun keyboardMouseDirectPositioningNeverPressesOrReleasesAButton() {
        val output = Output()
        val state = TouchMouseState()
        state.event(output, MotionEvent.ACTION_DOWN, 1000, direct = true)
        state.event(output, MotionEvent.ACTION_MOVE, 1100, x = 700f, direct = true)
        state.event(output, MotionEvent.ACTION_UP, 1200, x = 800f, direct = true)
        state.event(output, MotionEvent.ACTION_DOWN, 1300, direct = true)
        state.event(output, MotionEvent.ACTION_CANCEL, 1400, direct = true)
        state.reset(output)
        assertEquals(listOf(100, 700, 800, 100), output.positions.map { it.x })
        assertTrue(output.buttons.isEmpty())
        assertEquals(0, output.taps)
    }

    @Test fun keyboardMouseTwoFingerScrollKeepsClicksDisabled() {
        val output = Output()
        val state = TouchMouseState()
        state.event(output, MotionEvent.ACTION_DOWN, 1000)
        fun twoFingerEvent(action: Int, time: Long, secondY: Float) {
            val properties = Array(2) { id -> MotionEvent.PointerProperties().apply {
                this.id = id; toolType = MotionEvent.TOOL_TYPE_FINGER
            } }
            val coordinates = Array(2) { id -> MotionEvent.PointerCoords().apply {
                x = 100f + id * 50f; y = if (id == 0) 100f else secondY
                pressure = 1f; size = 1f
            } }
            val event = MotionEvent.obtain(1000, time, action, 2, properties, coordinates,
                0, 0, 1f, 1f, 0, 0, android.view.InputDevice.SOURCE_TOUCHSCREEN, 0)
            try { state.handle(event, true, output, emptySet(), buttonsEnabled = false) }
            finally { event.recycle() }
        }
        twoFingerEvent(MotionEvent.ACTION_POINTER_DOWN or (1 shl MotionEvent.ACTION_POINTER_INDEX_SHIFT), 1050, 100f)
        twoFingerEvent(MotionEvent.ACTION_MOVE, 1100, 240f)
        twoFingerEvent(MotionEvent.ACTION_POINTER_UP or (1 shl MotionEvent.ACTION_POINTER_INDEX_SHIFT), 1150, 240f)
        state.event(output, MotionEvent.ACTION_UP, 1200)
        assertTrue(output.scroll != 0)
        assertTrue(output.buttons.isEmpty())
        assertEquals(0, output.taps)
        assertEquals(0, output.rightTaps)
    }

    @Test fun switchingToCursorOnlyReleasesAnExistingGestureAndRestoresNormalClicksLater() {
        val output = Output()
        val state = TouchMouseState()
        state.event(output, MotionEvent.ACTION_DOWN, 1000, buttonsEnabled = true, direct = true)
        assertEquals(listOf(true), output.buttons)
        state.event(output, MotionEvent.ACTION_MOVE, 1100, buttonsEnabled = false, direct = true)
        state.event(output, MotionEvent.ACTION_UP, 1200, buttonsEnabled = false, direct = true)
        assertEquals(listOf(true, false), output.buttons)
        state.event(output, MotionEvent.ACTION_DOWN, 1300, buttonsEnabled = true)
        state.event(output, MotionEvent.ACTION_UP, 1350, buttonsEnabled = true)
        assertEquals(1, output.taps)
        state.event(output, MotionEvent.ACTION_DOWN, 1400, buttonsEnabled = true)
        state.event(output, MotionEvent.ACTION_MOVE, 1450, x = 150f, buttonsEnabled = true)
        state.event(output, MotionEvent.ACTION_CANCEL, 1500, buttonsEnabled = true)
        assertEquals(listOf(true, false, true, false), output.buttons)
    }
}
