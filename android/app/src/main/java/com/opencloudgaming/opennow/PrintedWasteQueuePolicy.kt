package com.opencloudgaming.opennow

import java.net.URI

internal const val PRINTEDWASTE_MAX_QUEUE_AGE_SECONDS = 30 * 60L

/** PrintedWaste's "Last Updated" is Unix seconds, not the time we fetched the response. */
internal fun isPrintedWasteQueueFresh(lastUpdatedEpochSeconds: Long, nowEpochMs: Long = System.currentTimeMillis()): Boolean {
    val nowSeconds = nowEpochMs / 1000
    return lastUpdatedEpochSeconds > 0 && lastUpdatedEpochSeconds <= nowSeconds &&
        lastUpdatedEpochSeconds >= nowSeconds - PRINTEDWASTE_MAX_QUEUE_AGE_SECONDS
}

/** One policy for recommendations, visible choices, pings, and final launch validation. */
internal fun selectablePrintedWasteRegionalUrl(
    zoneId: String,
    zone: PrintedWasteZone,
    mapping: Map<String, PrintedWasteServerMappingEntry>,
    regions: List<StreamRegion>,
    nowEpochMs: Long = System.currentTimeMillis(),
): String? {
    if (mapping[zoneId]?.nuked == true || !isPrintedWasteQueueFresh(zone.LastUpdated, nowEpochMs)) return null
    return printedWasteRegionalUrl(zoneId, mapping, regions)
}

internal fun shouldUsePrintedWasteQueue(
    hideServerSelector: Boolean,
    providerCode: String,
    providerStreamingServiceUrl: String,
    effectiveStreamingBaseUrl: String,
    membershipTier: String?,
): Boolean {
    if (hideServerSelector) return false
    if (!providerCode.equals("NVIDIA", ignoreCase = true)) return false
    if (!membershipTier.isNullOrBlank() && !membershipTier.equals("FREE", ignoreCase = true)) return false
    return !isAllianceStreamingServiceUrl(providerStreamingServiceUrl) &&
        !isAllianceStreamingServiceUrl(effectiveStreamingBaseUrl)
}

private fun isAllianceStreamingServiceUrl(streamingServiceUrl: String): Boolean {
    val host = runCatching { URI(streamingServiceUrl).host.orEmpty() }.getOrDefault("")
    return host.isNotBlank() && !host.endsWith(".nvidiagrid.net", ignoreCase = true)
}
