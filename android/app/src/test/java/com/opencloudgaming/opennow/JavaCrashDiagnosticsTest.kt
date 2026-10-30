package com.opencloudgaming.opennow

import java.nio.file.Files
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.*
import org.junit.Test

class JavaCrashDiagnosticsTest {
    @Test fun fastCrashSurvivesRestartWithoutAPeriodicSnapshot() {
        val directory = Files.createTempDirectory("java-crash").toFile()
        try {
            val store = JavaCrashDiagnostics(directory)
            store.loadPrevious()
            assertNull(store.previousCrash())
            store.streamContext = "streamReady=true guide=true"
            val error = IllegalArgumentException("invalid signaling shape", IllegalStateException("cause"))
            var delegated = false
            val handler = store.handler(Thread.UncaughtExceptionHandler { thread, received ->
                assertSame(Thread.currentThread(), thread)
                assertSame(error, received)
                delegated = true
            }, "2.2.1", 177)
            handler.uncaughtException(Thread.currentThread(), error)
            assertTrue(delegated)
            val nextRun = JavaCrashDiagnostics(directory).apply { loadPrevious() }
            val retained = requireNotNull(nextRun.previousCrash())
            assertEquals("177", retained["build"]?.jsonPrimitive?.content)
            assertEquals("streamReady=true guide=true", retained["streamContext"]?.jsonPrimitive?.content)
            val trace = retained["stackTrace"]!!.jsonPrimitive.content
            assertTrue(trace.contains("IllegalArgumentException: invalid signaling shape"))
            assertTrue(trace.contains("Caused by: java.lang.IllegalStateException: cause"))
            nextRun.loadPrevious()
            assertEquals(retained, nextRun.previousCrash())
        } finally { directory.deleteRecursively() }
    }

    @Test fun failedDiskWriteStillDelegatesTheOriginalException() {
        val file = Files.createTempFile("crash-not-a-directory", ".txt").toFile()
        try {
            val error = RuntimeException("original")
            var delegated: Throwable? = null
            JavaCrashDiagnostics(file).handler(Thread.UncaughtExceptionHandler { _, received -> delegated = received }, "test", 1)
                .uncaughtException(Thread.currentThread(), error)
            assertSame(error, delegated)
        } finally { file.delete() }
    }

    @Test fun stackCaptureBoundsHugeMessagesAndIncludesSuppressedExceptions() {
        val error = RuntimeException("x".repeat(100_000))
        error.addSuppressed(IllegalStateException("suppressed evidence"))
        val trace = boundedJavaCrashTrace(error)
        assertTrue(trace.length <= 32_768)
        assertTrue(trace.contains("suppressed evidence"))
    }

    @Test fun storedExceptionTextIsSanitized() {
        val directory = Files.createTempDirectory("java-crash-redaction").toFile()
        try {
            val error = RuntimeException("Bearer secret-token user@example.com").apply {
                stackTrace = Array(96) { StackTraceElement("Example", "callback", "0.0.0.0 ".repeat(60), 1) }
            }
            JavaCrashDiagnostics(directory).handler(Thread.UncaughtExceptionHandler { _, _ -> }, "test", 1)
                .uncaughtException(Thread.currentThread(), error)
            val crash = requireNotNull(JavaCrashDiagnostics(directory).apply { loadPrevious() }.previousCrash())
            assertFalse(crash.toString().contains("secret-token"))
            assertFalse(crash.toString().contains("user@example.com"))
            assertTrue(crash.getValue("stackTrace").jsonPrimitive.content.length <= 32_768)
        } finally { directory.deleteRecursively() }
    }
}
