package com.opencloudgaming.opennow

import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

internal fun isNvidiaCloudMatchHost(host: String): Boolean =
    host == "cloudmatchbeta.nvidiagrid.net" ||
        host.endsWith(".cloudmatchbeta.nvidiagrid.net") ||
        host == "cloudmatch.nvidiagrid.net" ||
        host.endsWith(".cloudmatch.nvidiagrid.net")

/** A direct seat IP is not provider identity; keep its existing transport interpretation. */
internal fun isAllianceCloudMatchBase(streamingBaseUrl: String?): Boolean {
    val host = streamingBaseUrl?.toHttpUrlOrNull()?.host ?: return false
    return !isNvidiaCloudMatchHost(host) && host.contains('.') && ':' !in host &&
        host.any(Char::isLetter)
}

/**
 * YES seats only answer ICE/DTLS on the usage-2 mapping: the WebRTC offer is rewritten to it
 * and connects, while the usage-17 mapping never authenticates for native NVST.
 */
internal fun isYesCloudMatchBase(streamingBaseUrl: String?): Boolean {
    val host = streamingBaseUrl?.toHttpUrlOrNull()?.host ?: return false
    return host == "yes.geforcenow.nvidiagrid.net" || host.endsWith(".yes.geforcenow.nvidiagrid.net")
}

/** YES allocates a native RTSPS-only seat for the alliance identity; WebRTC needs the browser identity. */
internal fun requiresNativeAllianceCloudMatchTransport(streamingBaseUrl: String?): Boolean =
    isYesCloudMatchBase(streamingBaseUrl)
