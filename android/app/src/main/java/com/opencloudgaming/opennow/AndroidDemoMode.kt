package com.opencloudgaming.opennow

private const val DEMO_PLAYER_NAME = "OpenNOW Demo"
private const val DEMO_PLAYER_EMAIL = "player@example.com"

/** Display fixtures only. These never enter the ViewModel, auth store, or provider requests. */
private val demoConnectors = listOf(
    AccountConnector("STEAM", "Steam"),
    AccountConnector("EPIC", "Epic Games"),
    AccountConnector("XBOX", "Xbox"),
    AccountConnector("UBISOFT", "Ubisoft Connect"),
    AccountConnector("GOG", "GOG"),
    AccountConnector("BATTLENET", "Battle.net"),
)

internal fun demoLibraryGames(state: OpenNowUiState): List<GameInfo> =
    (state.games + state.catalogResult.games + state.newlyAddedGames + state.libraryGames)
        .distinctBy(::gameTrackingKey)

/**
 * Keeps game objects and their real ownership intact, including when UI callbacks launch a game.
 * Ownership badges and filters use the separate demo presentation flag. Account IDs stay stable
 * for UI selection; names, email, avatars and connected-store metadata are replaced.
 */
internal fun OpenNowUiState.forDemoPresentation(demoLibrary: List<GameInfo>): OpenNowUiState {
    if (!settings.demoMode) return this
    val demoSession = authSession?.let { session ->
        session.copy(user = session.user.copy(
            displayName = DEMO_PLAYER_NAME,
            email = DEMO_PLAYER_EMAIL,
            avatarUrl = null,
        ))
    }
    return copy(
        authSession = demoSession,
        savedAccounts = listOfNotNull(demoSession?.let { session ->
            SavedAccount(
                userId = session.user.userId,
                displayName = DEMO_PLAYER_NAME,
                email = DEMO_PLAYER_EMAIL,
                membershipTier = session.user.membershipTier,
                providerCode = session.provider.code,
            )
        }),
        accountConnectors = (demoConnectors + accountConnectors)
            .distinctBy { normalizeGameStore(it.store) }
            .map { connector ->
                AccountConnector(
                    store = connector.store,
                    label = connector.label,
                    userDisplayName = DEMO_PLAYER_NAME,
                )
            },
        loadingAccountConnectors = false,
        connectorActionStore = null,
        libraryGames = demoLibrary,
    )
}

