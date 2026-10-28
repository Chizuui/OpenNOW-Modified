package com.opencloudgaming.opennow

import java.util.Locale

/** Shared catalog facts for the standard details view and the Space overview. */
internal data class GameDetailRow(
    val label: String,
    val value: String,
    val copyValue: String = value,
    val actionUrl: String? = null,
)

internal data class GameStoreDetail(
    val label: String,
    val url: String?,
)

internal fun validExternalStoreUrl(rawUrl: String?): String? {
    val value = rawUrl?.trim()?.takeIf { it.isNotBlank() } ?: return null
    val uri = runCatching { java.net.URI(value) }.getOrNull() ?: return null
    return value.takeIf {
        uri.scheme.equals("https", ignoreCase = true) && !uri.host.isNullOrBlank()
    }
}

internal fun gameStoreDetails(game: GameInfo): List<GameStoreDetail> {
    val variantDetails = launchableGameVariants(game.variants)
        .map { variant ->
            GameStoreDetail(
                label = gameStoreDisplayName(variant.store),
                url = validExternalStoreUrl(variant.storeUrl),
            )
        }
        .filter { it.label.isNotBlank() && !it.label.equals("Unknown", ignoreCase = true) && !it.label.equals("None", ignoreCase = true) }
        .distinctBy { normalizeGameStore(it.label) }
    if (variantDetails.none { it.url != null }) {
        return game.availableStores
            .map(::gameStoreDisplayName)
            .filterNot { it.equals("Unknown", ignoreCase = true) || it.equals("None", ignoreCase = true) }
            .distinctBy(::normalizeGameStore)
            .ifEmpty { variantDetails.map { it.label } }
            .takeIf { it.isNotEmpty() }
            ?.let { stores -> listOf(GameStoreDetail(stores.joinToString(", "), null)) }
            .orEmpty()
    }

    val variantStoreKeys = variantDetails.mapTo(mutableSetOf()) { normalizeGameStore(it.label) }
    val fallbackDetails = game.availableStores
        .map(::gameStoreDisplayName)
        .filterNot { it.equals("Unknown", ignoreCase = true) || it.equals("None", ignoreCase = true) }
        .filter { normalizeGameStore(it) !in variantStoreKeys }
        .distinctBy(::normalizeGameStore)
        .map { GameStoreDetail(it, null) }
    return variantDetails + fallbackDetails
}

internal fun gameDetailRows(game: GameInfo): List<GameDetailRow> = buildList {
    addAll(
        listOfNotNull(
            game.playabilityState?.takeIf { it.isNotBlank() }?.let { GameDetailRow("Status", formatGameMetadataLabel(it)) },
            game.publisherName?.takeIf { it.isNotBlank() }?.let { GameDetailRow("Publisher", it) },
            supportedControlLabels(game).takeIf { it.isNotEmpty() }?.joinToString(", ")?.let { GameDetailRow("Controls", it) },
            game.featureLabels
                .map(::formatGameMetadataLabel)
                .filterNot(::isNoisyGameTag)
                .filterNot { feature -> game.genres.any { genre -> feature.equals(formatGameMetadataLabel(genre), ignoreCase = true) } }
                .distinctBy { it.lowercase(Locale.US) }
                .take(8)
                .takeIf { it.isNotEmpty() }
                ?.joinToString(", ")
                ?.let { GameDetailRow("Features", it) },
            game.membershipTierLabel?.takeIf { it.isNotBlank() }?.let { GameDetailRow("Membership", formatGameMetadataLabel(it)) },
            game.contentRatings.takeIf { it.isNotEmpty() }?.joinToString(", ")?.let { GameDetailRow("Rating", it) },
        ),
    )
    val stores = gameStoreDetails(game)
    addAll(stores.map { store ->
        GameDetailRow(
            label = if (store.url == null && stores.size == 1) "Stores" else "Store",
            value = store.label,
            actionUrl = store.url,
        )
    })
    gameAppIdForDetails(game)?.let { add(GameDetailRow("App ID", it)) }
}

internal fun supportedControlLabels(game: GameInfo): List<String> =
    game.variants
        .flatMap { it.supportedControls }
        .map(::formatGameMetadataLabel)
        .filter(String::isNotBlank)
        .distinctBy { it.lowercase(Locale.US) }

private fun gameAppIdForDetails(game: GameInfo): String? =
    game.launchAppId?.takeIf { it.isNotBlank() }
        ?: game.variants.firstNotNullOfOrNull { variant -> variant.id.takeIf { it.isNotBlank() && it.all(Char::isDigit) } }
        ?: game.uuid?.takeIf { it.isNotBlank() }
        ?: game.id.takeIf { it.isNotBlank() }

internal fun isNoisyGameTag(label: String): Boolean {
    val normalized = label.trim().lowercase(Locale.US)
    return normalized.isBlank() ||
        normalized == "unknown" ||
        normalized == "gfn" ||
        normalized == "nvidia" ||
        normalized.contains("sku based tag") ||
        normalized.contains("catalog")
}

