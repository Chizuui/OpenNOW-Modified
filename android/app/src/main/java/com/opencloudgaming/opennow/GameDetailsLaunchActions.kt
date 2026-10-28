package com.opencloudgaming.opennow

import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalContext

/** One launch gate for every game-details presentation, including the experimental console hub. */
@Composable
internal fun rememberGameDetailsLaunchActions(
    game: GameInfo,
    defaultVariantId: String?,
    onPlay: (GameInfo) -> Unit,
    onChooseStore: (GameInfo) -> Unit,
    onPlayOnTv: (GameInfo) -> Unit,
    onDismiss: () -> Unit,
): (GameDetailsLaunchAction) -> Unit {
    val context = LocalContext.current
    val availability = game.availability(defaultVariantId)
    var pending by remember(game, defaultVariantId) { mutableStateOf<GameDetailsLaunchAction?>(null) }
    fun complete(action: GameDetailsLaunchAction) {
        onDismiss()
        when (action) {
            GameDetailsLaunchAction.Play -> playOrUpgrade(game, context, onPlay)
            GameDetailsLaunchAction.ChooseStore -> onChooseStore(game)
            GameDetailsLaunchAction.PlayOnTv -> onPlayOnTv(game)
        }
    }
    pending?.let { action ->
        GameAvailabilityDialog(availability,
            onContinue = { pending = null; complete(action) },
            onDismiss = { pending = null })
    }
    return { action ->
        if (availability == GameAvailability.Available || game.requiresMembershipUpgrade()) complete(action)
        else pending = action
    }
}
