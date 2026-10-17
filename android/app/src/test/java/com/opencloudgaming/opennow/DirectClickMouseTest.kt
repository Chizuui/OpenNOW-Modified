package com.opencloudgaming.opennow

import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.*
import org.junit.Test

class DirectClickMouseTest {
    private class Host(protocolVersion: Int = 3) {
        val encoder = InputEncoder().apply { setProtocolVersion(protocolVersion) }
        val packets = mutableListOf<ByteArray>()
        var position = 1733 to 941
        var pressed = false
        var acceptPosition = true
        var acceptButton = true

        fun sendPosition(value: AbsoluteMousePosition): Boolean {
            if (!acceptPosition) return false
            val packet = encoder.encodeMouseAbsolute(value.x, value.y, value.width, value.height)
            packets.add(packet)
            val offset = if (packet[0] == 0x23.toByte()) 12 else 0
            val le = ByteBuffer.wrap(packet).order(ByteOrder.LITTLE_ENDIAN)
            val be = ByteBuffer.wrap(packet).order(ByteOrder.BIG_ENDIAN)
            assertEquals(InputEncoder.INPUT_MOUSE_ABS, le.getInt(offset))
            position = (be.getShort(offset + 4).toInt() and 0xffff) to
                (be.getShort(offset + 6).toInt() and 0xffff)
            assertEquals(value.width, be.getShort(offset + 10).toInt() and 0xffff)
            assertEquals(value.height, be.getShort(offset + 12).toInt() and 0xffff)
            return true
        }

        fun setButton(value: Boolean): Boolean {
            if (!acceptButton) return false
            pressed = value
            packets.add(encoder.encodeMouseButton(
                if (value) InputEncoder.INPUT_MOUSE_BUTTON_DOWN else InputEncoder.INPUT_MOUSE_BUTTON_UP,
                1,
            ))
            return true
        }

        fun packetTypes(): List<Int> = packets.map { packet ->
            val offset = if (packet[0] == 0x23.toByte()) {
                if (packet[9] == 0x21.toByte()) 12 else 10
            } else 0
            ByteBuffer.wrap(packet).order(ByteOrder.LITTLE_ENDIAN).getInt(offset)
        }
    }

    @Test fun tapPositionsBeforePressingEvenWhenTheGameMovesTheCursorBetweenTaps() {
        for (protocol in listOf(2, 3)) {
            val mouse = DirectClickMouse()
            val host = Host(protocol)
            val target = directClickMousePosition(StreamPoint(100f, 200f), 1920, 1080)

            assertTrue(mouse.press(target, host::sendPosition, host::setButton))
            assertEquals(100 to 200, host.position)
            mouse.release(host::setButton)
            host.position = 1890 to 1020 // The client cannot observe a game-side cursor warp.
            assertTrue(mouse.press(target, host::sendPosition, host::setButton))
            assertEquals(100 to 200, host.position)
            mouse.release(host::setButton)

            assertEquals(listOf(5, 8, 9, 5, 8, 9), host.packetTypes())
            assertFalse(host.pressed)
        }
    }

    @Test fun draggingUsesTheNewPositionAfterAResolutionChangeWithoutCursorDrift() {
        val mouse = DirectClickMouse()
        val host = Host()
        assertTrue(mouse.press(directClickMousePosition(StreamPoint(500f, 400f), 1920, 1080), host::sendPosition, host::setButton))
        repeat(500) { step ->
            val targetX = 100f + step * 0.37f
            assertTrue(mouse.move(directClickMousePosition(StreamPoint(targetX, 600f), 1280, 720), host::sendPosition))
            assertEquals(targetX, host.position.first.toFloat(), 0.5f)
        }
        mouse.release(host::setButton)
        assertEquals(600, host.position.second)
        assertFalse(mouse.isPressed)
        assertFalse(host.pressed)
    }

    @Test fun failedPositionSendDoesNotClickAtTheOldCursorAndTheNextTapCanRecover() {
        val mouse = DirectClickMouse()
        val host = Host().apply { acceptPosition = false }
        val target = directClickMousePosition(StreamPoint(100f, 200f), 1920, 1080)
        assertFalse(mouse.press(target, host::sendPosition, host::setButton))
        mouse.release(host::setButton)
        assertTrue(host.packets.isEmpty())
        assertFalse(mouse.isPressed)

        host.acceptPosition = true
        assertTrue(mouse.press(target, host::sendPosition, host::setButton))
        assertEquals(100 to 200, host.position)
    }

    @Test fun failedButtonSendLeavesNoDragOrSpuriousRelease() {
        val mouse = DirectClickMouse()
        val host = Host().apply { acceptButton = false }
        val target = directClickMousePosition(StreamPoint(100f, 200f), 1920, 1080)
        assertFalse(mouse.press(target, host::sendPosition, host::setButton))
        assertFalse(mouse.move(target, host::sendPosition))
        mouse.release(host::setButton)
        assertEquals(listOf(5), host.packetTypes())
        assertFalse(mouse.isPressed)
    }

    @Test fun cancelOrLifecycleResetReleasesTheHeldButtonOnlyOnce() {
        val mouse = DirectClickMouse()
        val host = Host()
        val target = directClickMousePosition(StreamPoint(100f, 200f), 1920, 1080)
        assertTrue(mouse.press(target, host::sendPosition, host::setButton))
        assertFalse(mouse.press(target, host::sendPosition, host::setButton))
        mouse.release(host::setButton)
        mouse.release(host::setButton)
        assertFalse(mouse.move(target, host::sendPosition))
        assertEquals(listOf(5, 8, 9), host.packetTypes())
        assertFalse(host.pressed)
    }

    @Test fun cursorOnlyPositioningMovesWithoutChangingAnAssignedButtonsHold() {
        val mouse = DirectClickMouse()
        val host = Host()
        host.pressed = true // An independent overlay button owns this hold.
        val target = directClickMousePosition(StreamPoint(100f, 200f), 1920, 1080)
        assertTrue(mouse.press(target, host::sendPosition, host::setButton, clickEnabled = false))
        assertFalse(mouse.isPressed)
        assertTrue(mouse.move(directClickMousePosition(StreamPoint(800f, 600f), 1920, 1080), host::sendPosition))
        mouse.release(host::setButton)
        mouse.release(host::setButton)
        assertFalse(mouse.move(target, host::sendPosition))
        assertEquals(listOf(5, 5), host.packetTypes())
        assertTrue(host.pressed)
        assertEquals(800 to 600, host.position)
    }

    @Test fun pillarboxingAndViewResizeProduceTheSameHostPositionAndExtent() {
        val full = streamPointForTouch(750f, 270f, 2460, 1080, 1920, 1080, false, 0f)
        val resized = streamPointForTouch(375f, 135f, 1230, 540, 1920, 1080, false, 0f)
        val position = directClickMousePosition(full, 1920, 1080)
        assertEquals(AbsoluteMousePosition(480, 270, 1920, 1080), position)
        assertEquals(position, directClickMousePosition(resized, 1920, 1080))
    }

    @Test fun targetClampingUsesTheUnsignedAbsoluteExtentAndRejectsInvalidCoordinates() {
        assertEquals(AbsoluteMousePosition(0, 1080, 1920, 1080), directClickMousePosition(StreamPoint(-10f, 5000f), 1920, 1080))
        assertEquals(AbsoluteMousePosition(65535, 65535, 65535, 65535), directClickMousePosition(StreamPoint(Float.MAX_VALUE, Float.MAX_VALUE), 65535, 65535))
        assertNull(directClickMousePosition(StreamPoint(Float.NaN, 200f), 1920, 1080))
        assertNull(directClickMousePosition(StreamPoint(100f, Float.POSITIVE_INFINITY), 1920, 1080))
        assertNull(directClickMousePosition(StreamPoint(100f, 200f), 0, 1080))
        assertNull(directClickMousePosition(StreamPoint(100f, 200f), 1920, 0))
        assertNull(directClickMousePosition(StreamPoint(100f, 200f), 65536, 1080))
    }
}
