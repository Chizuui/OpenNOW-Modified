package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class ControllerDiagnosticsTest {
    @Test
    fun highRateMotionCountsEveryObservationButFormatsOncePerSecond() {
        var now = 0L
        var formats = 0
        val buffer = ControllerDiagnosticsBuffer { now }
        repeat(10_000) {
            buffer.sample("controller.motion.0", history = true) { formats++; "raw=sample-$now" }
            now++
        }
        val snapshot = buffer.capture()
        assertEquals(10, formats)
        assertEquals(10, snapshot.recent.size)
        val motion = snapshot.retained.first { it.first == "controller.motion.0" }.second
        assertEquals(10_000L, motion.count)
        assertEquals(9_999L, motion.lastSeenMs)
        assertEquals("raw=sample-9000", motion.message)
    }

    @Test
    fun differentButtonsAndReleaseEdgesWithinSameMillisecondSurvive() {
        val buffer = ControllerDiagnosticsBuffer { 100L }
        for (key in listOf(98, 103, 188, 0)) {
            for (action in listOf(0, 1)) {
                buffer.sample("controller.raw-key.34.$key.309.$action", history = true) {
                    "inputId=34 key=$key rawScan=309 action=$action handled=false"
                }
            }
        }
        val snapshot = buffer.capture()
        assertEquals(8, snapshot.recent.size)
        assertEquals(8, snapshot.retained.count { it.first.startsWith("controller.raw-key.") })
        assertTrue(snapshot.format().contains("key=98 rawScan=309 action=1 handled=false"))
    }

    @Test
    fun quickNeutralReturnAndDifferentControllersHaveIndependentSamples() {
        var now = 100L
        val buffer = ControllerDiagnosticsBuffer { now }
        buffer.sample("controller.motion.0.active", history = true) { "right=32767,0" }
        now = 110L
        buffer.sample("controller.motion.1.active", history = true) { "right=0,-32767" }
        now = 120L
        buffer.sample("controller.motion.0.neutral", history = true) { "right=0,0" }
        assertEquals(listOf("right=32767,0", "right=0,-32767", "right=0,0"), buffer.capture().recent.map { it.message })
    }

    @Test
    fun changingKeyFloodCannotCauseUnboundedFormattingOrHistory() {
        var now = 0L
        var formats = 0
        val buffer = ControllerDiagnosticsBuffer { now }
        repeat(10) { second ->
            now = second * 1_000L
            repeat(10_000) { index ->
                buffer.sample("controller.raw-key.$second.$index", history = true) {
                    formats++
                    "sample"
                }
            }
        }
        assertEquals(320, formats)
        val snapshot = buffer.capture()
        assertEquals(64, snapshot.recent.size)
        assertEquals(129, snapshot.retained.size) // 128 samples + capture policy/counter
        assertTrue(snapshot.format().contains("rateLimitedDetails=99680"))
    }

    @Test
    fun capturedEvidenceIsImmutableAndDetailedMessagesAreBounded() {
        var now = 0L
        val buffer = ControllerDiagnosticsBuffer { now }
        buffer.sample("controller.capabilities.34") { "x".repeat(10_000) }
        val before = buffer.capture()
        assertEquals(4_096, before.retained.first().second.message.length)
        now = 1_000L
        buffer.sample("controller.capabilities.34") { "changed" }
        assertEquals(4_096, before.retained.first().second.message.length)
        assertEquals("changed", buffer.capture().retained.first().second.message)
    }

    @Test
    fun exportedMappingEvidenceSurvivesRedactionAndParserSerialization() {
        val buffer = ControllerDiagnosticsBuffer { 100L }
        buffer.sample("controller.raw-key.34.98.306.0", history = true) {
            "inputId=34 slot=0 key=98 keyName=KEYCODE_BUTTON_C rawScan=306 action=0 " +
                "vendor=0x45e product=0x2e0 controllerName=Xbox Wireless Controller " +
                "baseMask=null mappedMask=null gamepadRoute=true handled=false " +
                "AXIS_Z=-1.0 AXIS_RZ=-1.0 AXIS_LTRIGGER=0.0"
        }
        val exported = sanitizeDiagnosticExport(diagnosticParserBlock(buffer.capture().toJson(DiagnosticTimeAnchor(1_000, 100))))
        for (expected in listOf("inputId=34", "rawScan=306", "key=98", "vendor=0x45e", "product=0x2e0",
            "controllerName=Xbox Wireless Controller", "AXIS_Z=-1.0", "handled=false")) {
            assertTrue("Missing $expected", exported.contains(expected))
        }
        OpenNowJson.parseToJsonElement(exported.substringAfter("<parser>\n").substringBefore("\n</parser>"))
    }
}
