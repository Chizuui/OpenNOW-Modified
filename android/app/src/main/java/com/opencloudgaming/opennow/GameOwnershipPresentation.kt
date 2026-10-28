package com.opencloudgaming.opennow

/** Ownership labels are independent of the real game objects passed back to launch callbacks. */
internal fun presentedOwnedStores(game: GameInfo, demoMode: Boolean): List<String> =
    if (demoMode) availableStoreLabels(game) else libraryStoreDisplayNames(game)

internal fun availableStoreLabels(game: GameInfo): List<String> =
    displayStoresForVariants(game.variants)
        .filterNot { it.equals("Unknown", ignoreCase = true) || it.equals("None", ignoreCase = true) }
        .ifEmpty { game.availableStores.flatMap { gameStoreDisplayName(it).split(" / ") } }
        .map(String::trim)
        .filter { it.isNotBlank() && !it.equals("none", ignoreCase = true) && !it.equals("Unknown", ignoreCase = true) }
        .distinctBy(::normalizeGameStore)

