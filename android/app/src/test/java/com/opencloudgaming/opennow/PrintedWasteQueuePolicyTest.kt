package com.opencloudgaming.opennow

import okhttp3.Headers
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PrintedWasteQueuePolicyTest {
    private val now = 1_793_190_000_000L

    @Test fun incidentQueueRemainsSelectableWhenDeviceClockIsAnHourBehindOrAhead() {
        // Values from the 2026-10-30 report: all 69 zones were rejected by device time.
        val deviceTimeMs = 1_793_331_757_792L
        val serverTimeMs = 1_793_335_379_000L
        val fresh = PrintedWasteZone(QueuePosition = 5, LastUpdated = 1_793_335_238L, Region = "EU")
        val retired = fresh.copy(LastUpdated = 1_755_006_995L)
        val mapping = mapOf("NP-FRK-07" to PrintedWasteServerMappingEntry(title = "EU Central"))
        val regions = listOf(StreamRegion("EU Central", "https://eu-central.cloudmatchbeta.nvidiagrid.net/"))
        val headers = Headers.Builder().add("Date", "Fri, 30 Oct 2026 04:42:59 GMT").build()
        assertFalse(isPrintedWasteQueueFresh(fresh.LastUpdated, deviceTimeMs))
        for (wallTimeMs in listOf(deviceTimeMs, serverTimeMs + 3_600_000)) {
            val queue = printedWasteQueueSnapshot(mapOf("NP-FRK-07" to fresh), headers, wallTimeMs, 100)
            val queueTimeMs = queue.nowEpochMs(100)
            assertEquals(serverTimeMs, queueTimeMs)
            assertEquals(regions.single().url, selectablePrintedWasteRegionalUrl("NP-FRK-07", fresh, mapping, regions, queueTimeMs))
            assertFalse(isPrintedWasteQueueFresh(retired.LastUpdated, queueTimeMs))
            assertNull(selectablePrintedWasteRegionalUrl("NP-FRK-07", fresh, mapping.mapValues { it.value.copy(nuked = true) }, regions, queueTimeMs))
            assertFalse(isPrintedWasteQueueFresh(serverTimeMs / 1000 + 1, queueTimeMs))
        }
    }

    @Test fun queueExpiresWithElapsedTimeEvenIfDeviceWallClockChanges() {
        val zone = PrintedWasteZone(QueuePosition = 0, LastUpdated = now / 1000 - 1799, Region = "EU")
        val queue = printedWasteQueueSnapshot(mapOf("NP-FRK-07" to zone), Headers.Builder().build(), now, 100)
        assertTrue(isPrintedWasteQueueFresh(zone.LastUpdated, queue.nowEpochMs(1_100)))
        assertFalse(isPrintedWasteQueueFresh(zone.LastUpdated, queue.nowEpochMs(2_100)))
        assertFalse(isPrintedWasteQueueFresh(zone.LastUpdated, queue.nowEpochMs(3_600_100)))
        assertEquals(now + 3_600_000, queue.nowEpochMs(3_600_100))
    }

    @Test fun missingOrMalformedServerDateUsesReceiptWallTimeThenElapsedTime() {
        for (date in listOf(null, "invalid", "Thu, 01 Jan 1970 00:00:00 GMT")) {
            val headers = Headers.Builder().apply { date?.let { add("Date", it) } }.build()
            val queue = printedWasteQueueSnapshot(emptyMap(), headers, now, 100)
            assertEquals(now + 1000, queue.nowEpochMs(1_100))
        }
    }

    @Test fun cachedResponseAgeCannotReviveExpiredZones() {
        val serverTimeMs = 1_793_335_379_000L
        val zone = PrintedWasteZone(QueuePosition = 0, LastUpdated = serverTimeMs / 1000 - 60, Region = "EU")
        val headers = Headers.Builder().add("Date", "Fri, 30 Oct 2026 04:42:59 GMT").add("Age", "1800").build()
        val queue = printedWasteQueueSnapshot(mapOf("NP-FRK-07" to zone), headers, now, 100)
        assertEquals(serverTimeMs + 1_800_000, queue.nowEpochMs(100))
        assertFalse(isPrintedWasteQueueFresh(zone.LastUpdated, queue.nowEpochMs(100)))
    }

    @Test fun responseContainingOnlyOldZonesStaysExpired() {
        val old = PrintedWasteZone(QueuePosition = 0, LastUpdated = now / 1000 - 1801, Region = "EU")
        val queue = printedWasteQueueSnapshot(mapOf("NP-FRK-07" to old), Headers.Builder().build(), now, 100)
        assertFalse(isPrintedWasteQueueFresh(old.LastUpdated, queue.nowEpochMs(100)))
    }

    @Test fun queueAgeUsesPerServerUnixSecondsWithThirtyMinuteCutoff() {
        assertTrue(isPrintedWasteQueueFresh(now / 1000, now))
        assertTrue(isPrintedWasteQueueFresh(now / 1000 - 1800, now))
        assertFalse(isPrintedWasteQueueFresh(now / 1000 - 1801, now))
        assertFalse(isPrintedWasteQueueFresh(1_757_315_021, now))
        assertFalse(isPrintedWasteQueueFresh(0, now))
        assertFalse(isPrintedWasteQueueFresh(Long.MIN_VALUE, now))
        assertFalse(isPrintedWasteQueueFresh(Long.MAX_VALUE, now))
    }

    @Test fun expiredOrMissingQueueTimestampCannotProduceASelectableRoute() {
        val mapping = mapOf("NP-FRK-07" to PrintedWasteServerMappingEntry(title = "EU Central"))
        val regions = listOf(StreamRegion(name = "EU Central", url = "https://eu-central.cloudmatchbeta.nvidiagrid.net/"))
        val fresh = PrintedWasteZone(QueuePosition = 0, LastUpdated = now / 1000 - 1800, Region = "EU")
        assertEquals(regions.single().url, selectablePrintedWasteRegionalUrl("NP-FRK-07", fresh, mapping, regions, now))
        assertNull(selectablePrintedWasteRegionalUrl("NP-FRK-07", fresh, mapping, regions, now + 1000))
        assertNull(selectablePrintedWasteRegionalUrl("NP-FRK-07", fresh.copy(LastUpdated = 0), mapping, regions, now))
        assertNull(selectablePrintedWasteRegionalUrl("NP-FRK-07", fresh, mapping.mapValues { it.value.copy(nuked = true) }, regions, now))
    }

    @Test
    fun nvidiaFreeAccountsSeeTheQueueSelector() {
        assertTrue(
            shouldUsePrintedWasteQueue(
                hideServerSelector = false,
                providerCode = "NVIDIA",
                providerStreamingServiceUrl = "https://prod.cloudmatchbeta.nvidiagrid.net/",
                effectiveStreamingBaseUrl = "https://np-lax-03.cloudmatchbeta.nvidiagrid.net/",
                membershipTier = "FREE",
            ),
        )
    }

    @Test
    fun allianceAccountsNeverSeeTheQueueSelector() {
        assertFalse(
            shouldUsePrintedWasteQueue(
                hideServerSelector = false,
                providerCode = "YES",
                providerStreamingServiceUrl = "https://yes.geforcenow.nvidiagrid.net/",
                effectiveStreamingBaseUrl = "https://yes.geforcenow.nvidiagrid.net/",
                membershipTier = "FREE",
            ),
        )
    }

    @Test
    fun allianceServerOverrideIsExcludedForNvidiaAccount() {
        assertFalse(
            shouldUsePrintedWasteQueue(
                hideServerSelector = false,
                providerCode = "NVIDIA",
                providerStreamingServiceUrl = "https://prod.cloudmatchbeta.nvidiagrid.net/",
                effectiveStreamingBaseUrl = "https://cloudmatch.alliance.example/",
                membershipTier = "FREE",
            ),
        )
    }
}
