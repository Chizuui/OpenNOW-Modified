package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class GameDetailsMetadataTest {
    @Test fun sharedFactsIncludeAvailablePlayerInformationAndFilterNoisyFeatures() {
        val game = GameInfo("game", title = "Game", publisherName = "Studio", genres = listOf("ACTION"),
            playabilityState = "AVAILABLE", membershipTierLabel = "Ultimate", contentRatings = listOf("ESRB T"),
            featureLabels = listOf("ACTION", "SINGLE_PLAYER", "SINGLE-PLAYER", "NVIDIA", "SKU_BASED_TAG"),
            variants = listOf(GameVariant("steam", "STEAM", supportedControls = listOf("GAMEPAD", "KEYBOARD_MOUSE", "GAMEPAD"))))
        val facts = gameDetailRows(game).associate { it.label to it.value }
        assertEquals("Studio", facts["Publisher"])
        assertEquals("Available", facts["Status"])
        assertEquals("Controller, Mouse and keyboard", facts["Controls"])
        assertEquals("Single-player", facts["Features"])
        assertEquals("Ultimate", facts["Membership"])
        assertEquals("ESRB T", facts["Rating"])
        assertEquals("Steam", facts["Stores"])
        assertEquals("game", facts["App ID"])
        assertTrue(gameDetailRows(game).any { it.label == "App ID" })
        assertEquals(listOf("App ID"), gameDetailRows(GameInfo("game", title = "Game")).map { it.label })
    }

    @Test fun variantStoresRemainVisibleWhenNoStoreLinksOrStoreSummaryWereProvided() {
        val game = GameInfo("game", title = "Game", variants = listOf(
            GameVariant("unknown", "Unknown"), GameVariant("steam", "STEAM"), GameVariant("xbox", "XBOX")))
        assertEquals(listOf(GameStoreDetail("Steam, Xbox", null)), gameStoreDetails(game))
    }
}
