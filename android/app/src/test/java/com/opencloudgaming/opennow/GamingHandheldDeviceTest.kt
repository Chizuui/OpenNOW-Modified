package com.opencloudgaming.opennow

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class GamingHandheldDeviceTest {
    @Test fun requiresKnownVendorAndHandheldModelTogether() {
        val handhelds = listOf(
            "AYN" to "Odin 2 Portal", "AYN" to "Odin2 Mini", "AYN" to "Odin Pro",
            "AYN" to "Thor", "AYN" to "AYN Thor Max",
            "Moorechip" to "Retroid Pocket 5", "Retroid Pocket" to "RP4Pro",
            "Retroid" to "Retroid Pocket Mini", "Retroid" to "Retroid Pocket Flip 2",
            "Anbernic" to "RG556", "AYANEO" to "Pocket S2", "AYANEO" to "Pocket DS",
            "Powkiddy" to "X18S", "abxylute" to "One", "GPD" to "XD Plus",
            "Razer" to "Razer Edge 5G", "Logitech" to "GR0006",
        )
        for ((vendor, model) in handhelds) {
            assertTrue("$vendor $model", isGamingHandheldDevice(vendor, null, model, null, null))
            assertTrue("brand/product: $vendor $model", isGamingHandheldDevice(null, vendor, null, null, model))
            assertFalse("model alone: $model", isGamingHandheldDevice("Unknown", null, model, null, null))
        }
    }

    @Test fun phonesTabletsComputersAndAmbiguousNamesKeepOrdinaryUi() {
        val otherDevices = listOf(
            listOf("Google", "google", "Pixel 10 Pro", "mustang", "mustang"),
            listOf("Samsung", "samsung", "SM-S938W", "odin", "odin"),
            listOf("Samsung", "samsung", "Galaxy Tab", "retroid", "retroid"),
            listOf("Razer", "razer", "Razer Phone 2", "aura", "aura"),
            listOf("GPD", "GPD", "Pocket 3", "pocket", "pocket"),
            listOf("AYANEO", "AYANEO", "Gaming Pad", "tablet", "tablet"),
            listOf("AYN", "AYN", "Unknown", "generic", "generic"),
            listOf("NotAYN", "RetroidCase", "Odin 2", "RP5", "RG556"),
            listOf("Google", "google", "sdk_gphone64_arm64", "emu64a", "odin"),
            listOf(null, null, null, null, null),
        )
        for (identity in otherDevices) {
            assertFalse(identity.toString(), isGamingHandheldDevice(identity[0], identity[1], identity[2], identity[3], identity[4]))
        }
    }

    @Test fun matchingIgnoresCaseAndSeparatorsButNeverJoinsDifferentFields() {
        assertTrue(isGamingHandheldDevice(" aYn ", null, "ODIN-2_PORTAL", null, null))
        assertFalse(isGamingHandheldDevice("Razer", null, "Phone", "edge_case", null))
        assertFalse(isGamingHandheldDevice("Logitech", null, "G", "Cloud", null))
        assertFalse(isGamingHandheldDevice("Retroid", null, "Pocket Phone", null, null))
    }
}
