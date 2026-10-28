package com.opencloudgaming.opennow

import kotlinx.serialization.encodeToString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidDemoModeTest {
    private val unowned = GameInfo(
        id = "catalog-game",
        title = "Catalog game",
        variants = listOf(GameVariant("steam-variant", "STEAM", libraryStatus = "NOT_OWNED")),
    )

    @Test
    fun demoPreferenceDefaultsOffAndSurvivesSettingsSerialization() {
        assertFalse(OpenNowJson.decodeFromString<AppSettings>("{}").demoMode)
        val settings = AppSettings(developerOptionsUnlocked = true, demoMode = true)
        assertEquals(settings, OpenNowJson.decodeFromString<AppSettings>(OpenNowJson.encodeToString(settings)))
        assertFalse(settings.lockingDeveloperOptions().demoMode)
    }

    @Test
    fun maskingReplacesProfileAndConnectionDetailsWithoutChangingRealState() {
        val session = AuthSession(
            provider = defaultProvider(),
            tokens = AuthTokens("real-access-token", expiresAt = Long.MAX_VALUE),
            user = AuthUser("real-id", "Private Name", "private@example.org", "https://example.org/avatar"),
        )
        val state = OpenNowUiState(
            settings = AppSettings(demoMode = true),
            authSession = session,
            savedAccounts = listOf(SavedAccount("another-id", "Other Private Name")),
            accountConnectors = listOf(AccountConnector(
                "STEAM", "Steam", userIdentifier = "private-store-id", userDisplayName = "Private Steam Name",
                expiresInSeconds = 100, syncedGameCount = 27, syncState = "FAILED", syncDate = "2026-10-18",
            )),
            loadingAccountConnectors = true,
            connectorActionStore = "STEAM",
        )

        val demo = state.forDemoPresentation(demoLibraryGames(state))

        assertEquals("OpenNOW Demo", demo.authSession?.user?.displayName)
        assertEquals("player@example.com", demo.authSession?.user?.email)
        assertNull(demo.authSession?.user?.avatarUrl)
        assertSame(session.tokens, demo.authSession?.tokens)
        assertEquals(listOf("OpenNOW Demo"), demo.savedAccounts.map { it.displayName })
        assertFalse(demo.loadingAccountConnectors)
        assertNull(demo.connectorActionStore)
        assertTrue(demo.accountConnectors.size >= 6)
        demo.accountConnectors.forEach {
            assertTrue(it.isLinked)
            assertEquals("OpenNOW Demo", it.userDisplayName)
            assertNull(it.userIdentifier)
            assertNull(it.syncedGameCount)
            assertNull(it.syncState)
            assertNull(it.syncDate)
            assertNull(it.expiresInSeconds)
        }
        assertEquals("Private Name", state.authSession?.user?.displayName)
        assertEquals(27, state.accountConnectors.single().syncedGameCount)
        val disabled = state.copy(settings = state.settings.copy(demoMode = false))
        assertSame(disabled, disabled.forDemoPresentation(emptyList()))
    }

    @Test
    fun demoLibraryIncludesEveryLoadedFeedWithoutChangingLaunchOwnership() {
        val libraryGame = GameInfo("owned", title = "Owned", isInLibrary = true)
        val newlyAdded = GameInfo("new", title = "New")
        val fallbackGame = GameInfo("fallback", title = "Fallback")
        val state = OpenNowUiState(
            settings = AppSettings(demoMode = true),
            games = listOf(unowned),
            newlyAddedGames = listOf(newlyAdded),
            libraryGames = listOf(libraryGame, unowned),
            catalogResult = CatalogBrowseResult(listOf(unowned, fallbackGame)),
            selectedGame = unowned,
        )

        val demo = state.forDemoPresentation(demoLibraryGames(state))

        assertEquals(setOf("catalog-game", "owned", "new", "fallback"), demo.libraryGames.map { it.id }.toSet())
        assertEquals(4, demo.libraryGames.size)
        val gameForCallback = demo.libraryGames.first { it.id == unowned.id }
        assertSame(unowned, gameForCallback)
        assertSame(unowned, demo.selectedGame)
        assertFalse(isGameInLibrary(gameForCallback))
        assertFalse(shouldLaunchWithAccountLinked(gameForCallback, gameForCallback.variants.single()))
        assertTrue(shouldMarkVariantOwnedBeforeLaunch(gameForCallback.variants.single()))
        assertEquals(listOf(libraryGame, unowned), state.libraryGames)
    }

    @Test
    fun demoOwnershipLabelsAndLibraryFiltersIncludeUnownedStores() {
        val game = unowned.copy(variants = unowned.variants + GameVariant("epic-variant", "EPIC"))
        assertTrue(presentedOwnedStores(game, demoMode = false).isEmpty())
        assertEquals(listOf("Steam", "Epic"), presentedOwnedStores(game, demoMode = true))
        val filters = libraryStoreFilterOptions(listOf(game), demoMode = true)
        assertEquals(setOf("STEAM", "EPIC"), filters.map { it.rawId }.toSet())
        filters.forEach {
            assertTrue(gameMatchesLibraryFilters(game, listOf(it.id), demoMode = true))
            assertFalse(gameMatchesLibraryFilters(game, listOf(it.id)))
        }
        assertEquals(listOf("Steam", "Epic"), presentedOwnedStores(
            GameInfo("no-variants", title = "Fallback", availableStores = listOf("STEAM,EPIC")),
            demoMode = true,
        ))
    }

    @Test
    fun signedOutDemoDoesNotFabricateASession() {
        val demo = OpenNowUiState(settings = AppSettings(demoMode = true)).forDemoPresentation(emptyList())
        assertNull(demo.authSession)
        assertTrue(demo.savedAccounts.isEmpty())
    }
}
