package com.opencloudgaming.opennow

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class GfnLibraryCatalogTest {
    @Test
    fun libraryTraversesBeyondTheOldTwentyFivePageCeiling() = runBlocking {
        var page = 0
        val apps = fetchLibraryCatalogApps { count, cursor ->
            assertEquals(100, count)
            assertEquals(if (page == 0) "" else "page-$page", cursor)
            val start = page++ * count
            response((start until start + count).map { app("app-$it") }, page < 52, "page-$page")
        }

        assertEquals(52, page)
        assertEquals(5200, apps.size)
        assertEquals("app-5199", apps.last()["id"]?.jsonPrimitive?.content)
    }

    @Test
    fun equalTitlesRemainSeparateWhileDuplicateIdsMergeLatestVariants() = runBlocking {
        var page = 0
        val apps = fetchLibraryCatalogApps { _, _ ->
            when (page++) {
                0 -> response(listOf(app("one", "Same title", "123"), app("two", "Same title")), true, "next")
                else -> response(listOf(app("one", "Same title", "456"), app("one", "Same title", "123", "NOT_OWNED")))
            }
        }

        assertEquals(listOf("one", "two"), apps.map { it["id"]!!.jsonPrimitive.content })
        val variants = apps[0]["variants"]!!.jsonArray
        assertEquals(listOf("123", "456"), variants.map { it.jsonObject["id"]!!.jsonPrimitive.content })
        assertEquals("NOT_OWNED", variants[0].jsonObject["gfn"]!!.jsonObject["library"]!!.jsonObject["status"]!!.jsonPrimitive.content)
    }

    @Test
    fun repeatedAndCyclicCursorsFailInsteadOfReturningPartialLibrary() {
        for (cursors in listOf(listOf("same", "same"), listOf("a", "b", "a"))) {
            var calls = 0
            assertThrows(IllegalStateException::class.java) {
                runBlocking {
                    fetchLibraryCatalogApps { _, _ ->
                        response(listOf(app("app-${calls}")), true, cursors[calls++])
                    }
                }
            }
            assertEquals(cursors.size, calls)
        }
    }

    @Test
    fun missingPaginationItemsOrIdentityCannotClearASavedLibrary() {
        val valid = response(listOf(app("one")))
        val validApps = valid["data"]!!.jsonObject["apps"]!!.jsonObject
        val invalidApps = listOf(
            JsonObject(validApps - "items"),
            JsonObject(validApps - "pageInfo"),
            JsonObject(validApps + ("pageInfo" to buildJsonObject { put("hasNextPage", "false") })),
            JsonObject(validApps + ("pageInfo" to buildJsonObject { put("hasNextPage", true); put("endCursor", JsonNull) })),
            JsonObject(validApps + ("pageInfo" to buildJsonObject { put("hasNextPage", true); put("endCursor", "x".repeat(4097)) })),
            JsonObject(validApps + ("items" to JsonArray(listOf(JsonObject(app("one") - "id"))))),
            JsonObject(validApps + ("items" to JsonArray(listOf(JsonObject(app("one") - "variants"))))),
            JsonObject(validApps + ("items" to JsonArray(listOf(app("one", variantId = ""))))),
        )
        for (apps in invalidApps) {
            var saved = listOf(app("saved"))
            assertThrows(IllegalStateException::class.java) {
                runBlocking {
                    saved = fetchLibraryCatalogApps { _, _ ->
                        buildJsonObject { put("data", buildJsonObject { put("apps", apps) }) }
                    }
                }
            }
            assertEquals("saved", saved.single()["id"]!!.jsonPrimitive.content)
        }
    }

    @Test
    fun emptyFinalPageIsAValidEmptyLibrary() = runBlocking {
        assertTrue(fetchLibraryCatalogApps { _, _ -> response(emptyList()) }.isEmpty())
    }

    @Test
    fun errorsAndCancellationOnLaterPagesPropagateWithoutAReplacementResult() {
        for (cancelled in listOf(false, true)) {
            var calls = 0
            val error = assertThrows(Exception::class.java) {
                runBlocking {
                    fetchLibraryCatalogApps { _, _ ->
                        if (calls++ == 0) response(listOf(app("one")), true, "next")
                        else if (cancelled) throw CancellationException("Cancelled")
                        else throw IllegalStateException("Provider unavailable")
                    }
                }
            }
            assertEquals(2, calls)
            assertEquals(cancelled, error is CancellationException)
        }
        assertThrows(IllegalStateException::class.java) {
            runBlocking {
                fetchLibraryCatalogApps { _, _ ->
                    JsonObject(response(listOf(app("one"))) + ("errors" to JsonArray(listOf(buildJsonObject { put("message", "failed") }))))
                }
            }
        }
    }

    @Test
    fun accountRouteOrRevisionChangeDuringARequestRejectsTheResult() {
        var current = true
        var calls = 0
        assertThrows(IllegalStateException::class.java) {
            runBlocking {
                fetchLibraryCatalogApps(isCurrentScope = { current }) { _, _ ->
                    calls++
                    current = false
                    response(listOf(app("old-account")))
                }
            }
        }
        assertEquals(1, calls)
    }

    @Test
    fun oversizedPagesRetryTheSameCursorWithoutSkippingGames() = runBlocking {
        val requests = mutableListOf<Pair<Int, String>>()
        val apps = fetchLibraryCatalogApps { count, cursor ->
            requests += count to cursor
            val start = cursor.toIntOrNull() ?: 0
            val end = minOf(start + count, 75)
            response((start until end).map { app("app-$it", "x".repeat(5000)) }, end < 75, "$end")
        }

        assertEquals(listOf(100 to "", 50 to "", 100 to "50"), requests)
        assertEquals(75, apps.size)
        assertEquals((0 until 75).map { "app-$it" }, apps.map { it["id"]!!.jsonPrimitive.content })
    }

    @Test
    fun libraryRefreshBudgetFailsExplicitlyInsteadOfSilentlyTruncating() {
        var page = 0
        val error = assertThrows(IllegalStateException::class.java) {
            runBlocking {
                fetchLibraryCatalogApps { count, _ ->
                    val start = page++ * count
                    response((start until start + count).map { app("app-$it") }, true, "page-$page")
                }
            }
        }
        assertTrue(error.message.orEmpty().contains("refresh budget"))
        assertEquals(201, page)
    }

    @Test
    fun documentRequestsUpstreamLibraryMetadataAndSupportsTheExistingStoreEnumFallback() {
        val query = libraryCatalogQuery(true)
        assertTrue(query.contains("query GetLibraryApps("))
        assertTrue(query.contains("library { favorited }"))
        assertTrue(query.contains("playStatus installed subscription"))
        assertTrue(query.contains("genres supportedControls"))
        assertTrue(query.contains("VariantGfnAutoPatchingMetadata"))
        assertTrue(query.contains("appStore"))
        assertFalse(libraryCatalogQuery(false).contains("appStore"))
        assertEquals(query.replace("appStore", ""), libraryCatalogQuery(false))
    }

    private fun app(
        id: String,
        title: String = "Game $id",
        variantId: String = "123",
        status: String = "MANUAL",
    ): JsonObject = buildJsonObject {
        put("id", id)
        put("title", title)
        put("variants", JsonArray(listOf(buildJsonObject {
            put("id", variantId)
            put("appStore", "STEAM")
            put("gfn", buildJsonObject {
                put("library", buildJsonObject { put("status", status); put("selected", true) })
            })
        })))
    }

    private fun response(items: List<JsonObject>, hasMore: Boolean = false, next: String = ""): JsonObject =
        buildJsonObject {
            put("data", buildJsonObject {
                put("apps", buildJsonObject {
                    put("items", JsonArray(items))
                    put("pageInfo", buildJsonObject { put("hasNextPage", hasMore); put("endCursor", next) })
                })
            })
        }
}
