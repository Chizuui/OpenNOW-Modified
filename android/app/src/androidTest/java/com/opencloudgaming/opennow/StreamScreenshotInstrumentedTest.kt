package com.opencloudgaming.opennow

import android.graphics.Color
import android.view.SurfaceView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class StreamScreenshotInstrumentedTest {
    @get:Rule val compose = createComposeRule()

    @Test fun copiesVideoPixelsWithoutComposeOverlay() = runBlocking {
        lateinit var surface: SurfaceView
        compose.setContent {
            Box(Modifier.size(160.dp, 90.dp)) {
                AndroidView(factory = { SurfaceView(it).also { view -> surface = view } }, modifier = Modifier.fillMaxSize())
                Box(Modifier.fillMaxSize().background(androidx.compose.ui.graphics.Color.Red))
            }
        }
        compose.waitUntil(5_000) { surface.holder.surface.isValid && surface.width > 0 }
        compose.runOnIdle {
            val canvas = surface.holder.lockCanvas()
            try { canvas.drawColor(Color.GREEN) } finally { surface.holder.unlockCanvasAndPost(canvas) }
        }
        withTimeout(5_000) {
            withContext(Dispatchers.Main) {
                val bitmap = copyStreamScreenshot(surface) { true }
                try {
                    assertTrue(bitmap.width > 0 && bitmap.height > 0)
                    assertEquals(Color.GREEN, bitmap.getPixel(bitmap.width / 2, bitmap.height / 2))
                } finally { bitmap.recycle() }
            }
        }
    }

    @Test fun detachedVideoSurfaceFailsWithoutReturningAnImage() = runBlocking {
        withContext(Dispatchers.Main) {
            val context = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext
            val failure = runCatching { copyStreamScreenshot(SurfaceView(context)) { true } }.exceptionOrNull()
            assertTrue(failure is IllegalStateException)
        }
    }
}
