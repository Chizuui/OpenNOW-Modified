package com.opencloudgaming.opennow

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf

internal val LocalDemoMode = staticCompositionLocalOf { false }

@Composable
internal fun rememberDemoPresentation(state: OpenNowUiState): OpenNowUiState {
    // Session/queue updates must not rebuild the combined catalog on every state emission.
    val library = remember(
        state.settings.demoMode, state.games, state.catalogResult.games,
        state.newlyAddedGames, state.libraryGames,
    ) {
        if (state.settings.demoMode) demoLibraryGames(state) else emptyList()
    }
    return remember(state, library) { state.forDemoPresentation(library) }
}
