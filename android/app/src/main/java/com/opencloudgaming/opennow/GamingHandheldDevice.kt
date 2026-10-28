package com.opencloudgaming.opennow

import android.os.Build
import java.util.Locale

/**
 * Build identity shared by handheld defaults and haptics. Require both a known vendor and a
 * handheld model family; a controller, screen size, or a firmware codename alone is insufficient.
 * Unknown identities deliberately keep the ordinary UI. This is best-effort device recognition,
 * not attestation: custom firmware may omit or replace these fields.
 */
internal fun isGamingHandheldDevice(
    manufacturer: String?,
    brand: String?,
    model: String?,
    device: String?,
    product: String?,
): Boolean {
    val vendors = listOf(manufacturer, brand).map(::handheldIdentity)
    val models = listOf(model, device, product).map(::handheldIdentity)
    return handheldFamilies.any { family ->
        vendors.any { it in family.vendors } && models.any(family.model::matches)
    }
}

private fun handheldIdentity(value: String?): String = value.orEmpty().lowercase(Locale.US)
    .replace(handheldSeparators, "")

private val handheldSeparators = Regex("[^a-z0-9]")

private data class HandheldFamily(val vendors: Set<String>, val model: Regex)

private val handheldFamilies = listOf(
    HandheldFamily(setOf("ayn"), Regex("(?:ayn)?(?:odin[0-9]*(?:lite|base|pro|max|mini|portal|ultra)*|thor(?:lite|base|pro|max)?)")),
    HandheldFamily(setOf("retroid", "retroidpocket", "moorechip", "goretroid"),
        Regex("(?:(?:retroid)?pocket|rp)(?:[0-9]+[sp]?|mini[0-9]*|flip[0-9]*|classic|g[0-9]+)(?:pro|plus|max)?")),
    HandheldFamily(setOf("anbernic"), Regex("(?:anbernic)?rg[0-9]+[a-z]*")),
    HandheldFamily(setOf("ayaneo"), Regex("(?:ayaneo)?pocket(?:air|s[0-9]*|evo|micro|dmg|ace|ds|vert)(?:pro|plus|mini|classic)?")),
    HandheldFamily(setOf("powkiddy"), Regex("(?:powkiddy)?(?:x[0-9]+[a-z]*|rgb[0-9]+[a-z]*)")),
    HandheldFamily(setOf("abxylute"), Regex("(?:abxylute)?(?:one|onepro)")),
    HandheldFamily(setOf("gpd"), Regex("(?:gpd)?(?:xd|xp)(?:plus)?")),
    HandheldFamily(setOf("razer"), Regex("(?:razer)?edge(?:wifi|5g)?")),
    HandheldFamily(setOf("logitech"), Regex("(?:logitech)?(?:gcloud|gr0006)")),
)

internal fun isGamingHandheldDevice(): Boolean = isGamingHandheldDevice(
    manufacturer = Build.MANUFACTURER,
    brand = Build.BRAND,
    model = Build.MODEL,
    device = Build.DEVICE,
    product = Build.PRODUCT,
)
