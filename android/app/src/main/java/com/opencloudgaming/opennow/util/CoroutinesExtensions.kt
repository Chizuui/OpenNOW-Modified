package com.opencloudgaming.opennow.util

import android.util.Log
import kotlinx.coroutines.CancellationException

private const val TAG = "CoroutinesExt"

/**
 * Run a suspend block and catch all non-cancellation exceptions.
 * Logs the error and returns null on failure.
 */
suspend fun <T> runCatchingOrNull(block: suspend () -> T): T? {
    return try {
        block()
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        Log.e(TAG, e.message ?: "Unknown error", e)
        null
    }
}

/**
 * Run a suspend block and return a Result.
 * Properly handles CancellationException by re-throwing it.
 */
suspend fun <T> runCatchingSafe(block: suspend () -> T): Result<T> {
    return try {
        Result.success(block())
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        Log.e(TAG, e.message ?: "Unknown error", e)
        Result.failure(e)
    }
}

/**
 * Run a suspend block and catch all non-cancellation exceptions.
 * Invokes the error handler on failure and returns null.
 */
suspend fun <T> runCatchingOrHandle(
    errorHandler: (Exception) -> Unit = { Log.e(TAG, it.message ?: "Unknown error", it) },
    block: suspend () -> T,
): T? {
    return try {
        block()
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        errorHandler(e)
        null
    }
}

/**
 * Execute a block and return a descriptive debug message from any throwable.
 */
fun Throwable.debugMessage(): String {
    return buildString {
        append(javaClass.simpleName)
        message?.let { append(": $it") }
    }
}
