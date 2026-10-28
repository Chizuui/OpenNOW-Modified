package com.opencloudgaming.opennow

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PrintedWasteQueuePolicyTest {
    private val now = 1_793_190_000_000L

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
