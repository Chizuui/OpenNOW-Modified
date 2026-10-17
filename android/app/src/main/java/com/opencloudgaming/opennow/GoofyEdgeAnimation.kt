package com.opencloudgaming.opennow

import android.graphics.Movie
import android.os.SystemClock
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive

@Composable
internal fun GoofyEdgeAnimation() {
    val context = LocalContext.current
    val movie = remember(context) {
        context.resources.openRawResource(R.raw.goofy_screensaver).use(Movie::decodeStream)
    } ?: return
    if (movie.width() <= 0 || movie.height() <= 0) return
    var placement by remember(movie) { mutableStateOf(nextGoofyEdgePlacement()) }
    var frameTimeMillis by remember(movie) { mutableIntStateOf(0) }
    LaunchedEffect(movie) {
        val durationMillis = movie.duration().takeIf { it > 0 } ?: 5_400
        val startMillis = SystemClock.uptimeMillis()
        var lastLoop = 0L
        while (isActive) {
            val elapsed = SystemClock.uptimeMillis() - startMillis
            val loop = elapsed / durationMillis
            if (loop > lastLoop) {
                placement = nextGoofyEdgePlacement(placement)
                lastLoop = loop
            }
            frameTimeMillis = (elapsed % durationMillis).toInt()
            delay(40L)
        }
    }
    BoxWithConstraints(Modifier.fillMaxSize().clipToBounds().clearAndSetSemantics {}) {
        val frame = remember(placement, maxWidth, maxHeight, movie) {
            placement.frame(maxWidth.value, maxHeight.value, movie.width().toFloat(), movie.height().toFloat())
        }
        Canvas(Modifier.fillMaxSize()) {
            val canvas = drawContext.canvas.nativeCanvas
            val saved = canvas.save()
            canvas.translate(frame.centerX.dp.toPx(), frame.centerY.dp.toPx())
            canvas.rotate(frame.rotationDegrees)
            canvas.scale(frame.width.dp.toPx() / movie.width(), frame.height.dp.toPx() / movie.height())
            canvas.translate(-movie.width() / 2f, -movie.height() / 2f)
            movie.setTime(frameTimeMillis)
            movie.draw(canvas, 0f, 0f)
            canvas.restoreToCount(saved)
        }
    }
}
