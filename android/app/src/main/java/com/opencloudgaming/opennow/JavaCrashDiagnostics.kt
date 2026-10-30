package com.opencloudgaming.opennow

import java.io.File
import java.io.FileOutputStream
import java.util.Collections
import java.util.IdentityHashMap
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** Independent of periodic exports: even a crash on the first stream frame leaves evidence. */
internal class JavaCrashDiagnostics(private val directory: File) {
    private val file = File(directory, "last-java-crash.json")
    private val writing = AtomicBoolean(false)
    @Volatile var streamContext: String = "No stream UI active"
    private var previous: JsonObject? = null

    fun loadPrevious() {
        previous = runCatching {
            if (!file.isFile || file.length() > MAX_CRASH_CHARACTERS * 6L) null
            else OpenNowJson.parseToJsonElement(file.readText()) as? JsonObject
        }.getOrNull()
    }

    fun previousCrash(): JsonObject? = previous

    fun handler(
        delegate: Thread.UncaughtExceptionHandler,
        version: String,
        build: Int,
    ): Thread.UncaughtExceptionHandler = Thread.UncaughtExceptionHandler { thread, error ->
        try {
            // No diagnostic-buffer locks, coroutines, HTTP, or full export on a crashing thread.
            if (writing.compareAndSet(false, true)) {
                runCatching { save(thread.name, error, version, build) }
            }
        } finally {
            // Preserve Android's crash reporting and normal process termination on write failure.
            delegate.uncaughtException(thread, error)
        }
    }

    private fun save(thread: String, error: Throwable, version: String, build: Int) {
        val timestamp = System.currentTimeMillis()
        val crash = buildJsonObject {
            put("scope", "previous_process_uncaught_java_exception")
            put("timestampEpochMs", timestamp)
            put("timestamp", diagnosticTimestamp(timestamp))
            put("version", version)
            put("build", build)
            put("thread", thread.take(128))
            put("streamContext", streamContext.take(512))
            put("stackTrace", boundedJavaCrashTrace(error))
        }
        val sanitized = sanitizeDiagnosticParserJson(crash) as JsonObject
        // Redaction can expand short values; enforce the schema bound after sanitizing too.
        val text = JsonObject(sanitized + ("stackTrace" to JsonPrimitive(
            (sanitized.getValue("stackTrace") as JsonPrimitive).content.take(MAX_CRASH_CHARACTERS),
        ))).toString()
        directory.mkdirs()
        val staged = File(directory, "last-java-crash.json.stage")
        FileOutputStream(staged).use { output ->
            output.write(text.toByteArray(Charsets.UTF_8))
            output.fd.sync()
        }
        check(staged.renameTo(file)) { "Could not retain Java crash" }
    }

    private companion object {
        const val MAX_CRASH_CHARACTERS = 32_768
    }
}

/** Bound work as well as output, including cyclic cause chains and huge exception messages. */
internal fun boundedJavaCrashTrace(error: Throwable): String = buildString {
    val seen = Collections.newSetFromMap(IdentityHashMap<Throwable, Boolean>())
    val pending = java.util.ArrayDeque<Pair<String, Throwable>>()
    pending.add("" to error)
    var remaining = 8
    while (pending.isNotEmpty() && remaining-- > 0 && length < 30_000) {
        val (label, current) = pending.removeFirst()
        if (!seen.add(current)) continue
        append(label).append(current.javaClass.name).append(": ").appendLine(current.message?.take(2_000).orEmpty())
        current.stackTrace.take(96).forEach { append("\tat ").appendLine(it.toString().take(512)) }
        current.cause?.let { pending.addLast("Caused by: " to it) }
        current.suppressed.take(4).forEach { pending.addLast("Suppressed: " to it) }
    }
}.take(32_768)
