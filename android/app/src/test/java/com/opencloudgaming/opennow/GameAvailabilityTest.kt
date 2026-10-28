package com.opencloudgaming.opennow

import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test

class GameAvailabilityTest {
    @Test
    fun recognizesProviderMaintenanceAndUpdateStates() {
        for (state in listOf("MAINTENANCE", "offline", " UNDER_MAINTENANCE ")) {
            assertEquals(state, GameAvailability.Maintenance, gameAvailability(state))
        }
        for (state in listOf("PATCHING", "auto_patching", " MANUAL_PATCHING ")) {
            assertEquals(state, GameAvailability.Patching, gameAvailability(state))
        }
        for (state in listOf(null, "", " ", "AVAILABLE", "UNPLAYABLE_DUE_TO_UPGRADE", "UNKNOWN")) {
            assertEquals(state, GameAvailability.Available, gameAvailability(state))
        }
    }

    @Test
    fun providerStateParsingDoesNotDependOnDeviceLanguage() {
        val original = Locale.getDefault()
        try {
            Locale.setDefault(Locale.forLanguageTag("tr-TR"))
            assertEquals(GameAvailability.Patching, gameAvailability("PATCHING"))
            assertEquals(GameAvailability.Maintenance, gameAvailability("MAINTENANCE"))
        } finally {
            Locale.setDefault(original)
        }
    }

    @Test
    fun gameWideStatusTakesPriorityOverStoreStatus() {
        val game = game().copy(playabilityState = "MAINTENANCE")
        assertEquals(GameAvailability.Maintenance, game.availability("steam"))
    }

    @Test
    fun onlyTheChosenStoresStatusAffectsAvailability() {
        val game = game()
        assertEquals(GameAvailability.Patching, game.availability())
        assertEquals(GameAvailability.Available, game.availability("epic"))
        assertEquals(GameAvailability.Patching, game.copy(selectedVariantIndex = 1).availability("steam"))
    }

    @Test
    fun staleDefaultAndInvalidSelectionFallBackToExistingVariant() {
        assertEquals(GameAvailability.Patching, game().availability("removed-store"))
        assertEquals(GameAvailability.Patching, game().copy(selectedVariantIndex = -1).availability())
        assertEquals(GameAvailability.Patching, game().copy(selectedVariantIndex = 20).availability())
        assertEquals(GameAvailability.Available, game().copy(variants = emptyList()).availability())
        assertEquals(
            GameAvailability.Maintenance,
            game().copy(variants = emptyList(), playabilityState = "OFFLINE").availability(),
        )
    }

    private fun game() = GameInfo(
        id = "game",
        title = "Game",
        variants = listOf(
            GameVariant(id = "steam", store = "Steam", gfnStatus = "PATCHING"),
            GameVariant(id = "epic", store = "Epic", gfnStatus = "AVAILABLE"),
        ),
    )
}
