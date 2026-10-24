package com.opencloudgaming.opennow

import android.content.Context
import android.graphics.Bitmap
import android.os.Handler
import android.os.Looper
import android.view.PixelCopy
import android.view.SurfaceView
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlin.coroutines.resumeWithException
import kotlin.math.sqrt

/** Bound one-shot allocation to 32 MiB, preserving the surface's aspect ratio. */
internal fun screenshotSize(width: Int, height: Int): Pair<Int, Int> {
    require(width > 0 && height > 0)
    val scale = minOf(1.0, sqrt(8_388_608.0 / (width.toDouble() * height)))
    return (width * scale).toInt().coerceAtLeast(1) to (height * scale).toInt().coerceAtLeast(1)
}

/** The callback owns the bitmap until PixelCopy finishes, even if the caller is cancelled. */
internal suspend fun copyStreamScreenshot(source: SurfaceView, isCurrent: () -> Boolean): Bitmap {
    check(Looper.myLooper() == Looper.getMainLooper())
    check(source.holder.surface.isValid && isCurrent()) { "Video surface unavailable" }
    val bounds = source.holder.surfaceFrame
    val (width, height) = screenshotSize(bounds.width(), bounds.height())
    var allocated: Bitmap? = null
    try {
        // Allocating/clearing a large bitmap must not stall input dispatch on the UI thread.
        withContext(Dispatchers.Default) {
            allocated = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        }
        val bitmap = checkNotNull(allocated)
        return suspendCancellableCoroutine { continuation ->
            allocated = null // Ownership passes to the PixelCopy completion callback.
            try {
                check(isCurrent()) { "Video surface changed" }
                PixelCopy.request(source, bitmap, { result ->
                    if (result == PixelCopy.SUCCESS && isCurrent()) {
                        continuation.resume(bitmap) { _, abandonedBitmap, _ -> abandonedBitmap.recycle() }
                    } else {
                        bitmap.recycle()
                        continuation.resumeWithException(IllegalStateException("Video copy failed: $result"))
                    }
                }, Handler(Looper.getMainLooper()))
            } catch (error: Exception) {
                bitmap.recycle()
                continuation.resumeWithException(error)
            }
        }
    } finally {
        // Covers cancellation while the allocation worker hands its bitmap back to the UI.
        allocated?.recycle()
    }
}

/** The caller retains bitmap ownership. Failed or cancelled writes leave no empty document. */
internal suspend fun saveStreamScreenshot(context: Context, folder: String, fileName: String, bitmap: Bitmap) =
    withContext(Dispatchers.IO) {
        val uri = StreamCaptureDestination.createFile(context, folder, fileName, "image/png")
        var saved = false
        try {
            ensureActive()
            requireNotNull(context.contentResolver.openOutputStream(uri, "w")) { "Cannot open screenshot" }.use {
                check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)) { "Cannot encode screenshot" }
            }
            ensureActive()
            saved = true
        } finally {
            if (!saved) runCatching { context.contentResolver.delete(uri, null, null) }
        }
    }
