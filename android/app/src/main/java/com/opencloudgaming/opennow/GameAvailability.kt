package com.opencloudgaming.opennow

import java.util.Locale

internal enum class GameAvailability {
    Available,
    Maintenance,
    Patching,
}

internal fun gameAvailability(raw: String?): GameAvailability {
    val status = raw.orEmpty().trim().lowercase(Locale.ROOT)
    return when {
        "patch" in status -> GameAvailability.Patching
        "maintenance" in status || "offline" in status -> GameAvailability.Maintenance
        else -> GameAvailability.Available
    }
}

internal fun GameInfo.availability(defaultVariantId: String? = null): GameAvailability {
    val gameStatus = gameAvailability(playabilityState)
    if (gameStatus != GameAvailability.Available) return gameStatus

    // Match the launcher's saved default without letting another store's outage affect this one.
    val variant = defaultVariantId?.let { id -> launchableGameVariants(variants).firstOrNull { it.id == id } }
        ?: variants.getOrNull(selectedVariantIndex)
        ?: variants.firstOrNull()
    return gameAvailability(variant?.gfnStatus)
}
