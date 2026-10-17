package com.opencloudgaming.opennow

import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonObject

internal const val LIBRARY_APPS_SORT_ORDER =
    "variants.gfn.library.lastPlayedDate:DESC,computedValues.libraryAddedDate:DESC,sortName:ASC"
private const val LIBRARY_PAGE_SIZE = 100
private const val MAX_LIBRARY_GAMES = 20_000
private const val MAX_LIBRARY_BYTES = 32L * 1024 * 1024
private const val MAX_LIBRARY_CURSOR_BYTES = 4096
private const val LIBRARY_PAGE_BYTES = 768L * 1024

internal fun libraryAppsFilter(): JsonObject = buildJsonObject {
    putJsonObject("variants") {
        putJsonObject("gfn") {
            putJsonObject("library") {
                putJsonObject("status") { put("notEquals", "NOT_OWNED") }
            }
        }
    }
}

// Keep the account-library document aligned with native/opennow-core/src/gfn/catalog.rs.
// Details are hydrated when opened; the library needs no per-game metadata requests.
internal fun libraryCatalogQuery(includeAppStore: Boolean): String = """
    query GetLibraryApps(
      ${'$'}vpcId: String!, ${'$'}locale: String!, ${'$'}sortString: String!,
      ${'$'}fetchCount: Int!, ${'$'}cursor: String!, ${'$'}filters: AppFilterFields!
    ) {
      apps(vpcId: ${'$'}vpcId, language: ${'$'}locale, orderBy: ${'$'}sortString, first: ${'$'}fetchCount, after: ${'$'}cursor, filters: ${'$'}filters) {
        numberReturned numberSupported pageInfo { hasNextPage endCursor totalCount }
        items {
          id title developerName publisherName genres supportedControls
          library { favorited }
          images { KEY_ART KEY_IMAGE GAME_BOX_ART TV_BANNER HERO_IMAGE MARQUEE_HERO_IMAGE FEATURE_IMAGE GAME_LOGO SCREENSHOTS }
          variants {
            id ${if (includeAppStore) "appStore" else ""} storeUrl supportedControls paymentModels { __typename } subscriptions
            gfn {
              status
              features {
                __typename
                ... on GfnSubscriptionFeatureValue { key value }
                ... on GfnSubscriptionFeatureValueList { key values }
              }
              library { status selected lastPlayedDate playStatus installed subscription }
              stateDetails { __typename ... on VariantGfnAutoPatchingMetadata { subType startTime endTime historicalEtaMins etaPredictionType } ... on VariantGfnManualPatchingMetadata { subType startTime endTime } ... on VariantGfnMaintenanceMetadata { subType } }
            }
          }
          gfn { playType playabilityState minimumMembershipTierLabel catalogSkuStrings { SKU_BASED_TAG SKU_BASED_PLAYABILITY_TEXT } }
          itemMetadata { campaignIds }
        }
      }
    }
""".trimIndent()

/** A failed traversal never returns a truncated library to the cache or UI. */
internal suspend fun fetchLibraryCatalogApps(
    isCurrentScope: () -> Boolean = { true },
    fetchPage: suspend (count: Int, cursor: String) -> JsonObject,
): List<JsonObject> {
    val appsById = linkedMapOf<String, JsonObject>()
    val seenCursors = mutableSetOf<String>()
    var cursor = ""
    var receivedCount = 0
    var receivedBytes = 0L
    var count = LIBRARY_PAGE_SIZE
    while (true) {
        currentCoroutineContext().ensureActive()
        check(isCurrentScope()) { "The catalog changed. Restart the library refresh." }
        val payload = fetchPage(count, cursor)
        currentCoroutineContext().ensureActive()
        check(isCurrentScope()) { "The catalog changed. Restart the library refresh." }
        check((payload["errors"] as? JsonArray).isNullOrEmpty()) { "Library query returned GraphQL errors" }
        val apps = (payload["data"] as? JsonObject)?.get("apps") as? JsonObject
        val items = apps?.get("items") as? JsonArray ?: error("Library response has no games array")
        val pageInfo = apps["pageInfo"] as? JsonObject ?: error("Library response has no pagination state")
        val hasMore = (pageInfo["hasNextPage"] as? JsonPrimitive)?.takeUnless { it.isString }?.booleanOrNull
            ?: error("Library response has no pagination state")
        val next = (pageInfo["endCursor"] as? JsonPrimitive)?.takeIf { it.isString }?.contentOrNull.orEmpty()
        val pageBytes = items.toString().length.toLong() * 3
        if (pageBytes > LIBRARY_PAGE_BYTES && count > 1) {
            count = (count / 2).coerceAtLeast(1)
            continue // Retry the same cursor with fewer games; never skip part of a page.
        }
        check(pageBytes <= LIBRARY_PAGE_BYTES) { "A library game exceeds the safe page size" }
        if (hasMore) {
            check(next.isNotEmpty() && next.toByteArray(Charsets.UTF_8).size <= MAX_LIBRARY_CURSOR_BYTES && seenCursors.add(next)) {
                "Library pagination did not advance"
            }
        }
        receivedCount += items.size
        receivedBytes += pageBytes
        check(receivedCount <= MAX_LIBRARY_GAMES && receivedBytes <= MAX_LIBRARY_BYTES) {
            "Library exceeds the refresh budget; the saved library has been kept"
        }
        for (item in items) {
            val app = item as? JsonObject ?: error("Library contains an invalid game")
            val id = app.libraryIdentity("id")
            check((app["title"] as? JsonPrimitive)?.contentOrNull?.isNotBlank() == true) {
                "Library game has no title"
            }
            val variants = app["variants"] as? JsonArray ?: error("Library game has no variants")
            check(variants.isNotEmpty() && variants.size <= 128) { "Library game has invalid variants" }
            val mergedVariants = linkedMapOf<String, JsonObject>()
            val existingVariants = (appsById[id]?.get("variants") as? JsonArray).orEmpty()
            for (raw in existingVariants + variants) {
                val variant = raw as? JsonObject ?: error("Library contains an invalid variant")
                mergedVariants[variant.libraryIdentity("id")] = variant
            }
            // The latest page owns each variant's status; title equality is never identity.
            appsById[id] = JsonObject(app + ("variants" to JsonArray(mergedVariants.values.toList())))
        }
        if (!hasMore) return appsById.values.toList()
        cursor = next
        count = LIBRARY_PAGE_SIZE
    }
}

private fun JsonObject.libraryIdentity(key: String): String =
    (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.contentOrNull
        ?.takeIf { it.isNotEmpty() && it.length <= 256 }
        ?: error("Library contains an invalid $key")
