package com.opencloudgaming.opennow

import android.graphics.Movie
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp

@Composable
internal fun rememberGoofyMovie(): Movie? {
    val context = LocalContext.current
    return remember(context) {
        context.resources.openRawResource(R.raw.goofy_screensaver).use(Movie::decodeStream)
            ?.takeIf { it.width() > 0 && it.height() > 0 }
    }
}

/** The scene clock controls the GIF too: peek, hold while speaking, then disappear on impact. */
@Composable
internal fun GoofyEdgeAnimation(movie: Movie, frame: GoofyEdgeFrame, edge: GoofyScreenEdge, reveal: Float) {
    val hiddenOffset = (1f - reveal) * frame.height
    val x = frame.centerX + when (edge) {
        GoofyScreenEdge.Left -> -hiddenOffset
        GoofyScreenEdge.Right -> hiddenOffset
        else -> 0f
    }
    val y = frame.centerY + when (edge) {
        GoofyScreenEdge.Top -> -hiddenOffset
        GoofyScreenEdge.Bottom -> hiddenOffset
        else -> 0f
    }
    // Keep the fading layer sprite-sized, rather than allocating a translucent full-screen layer.
    Canvas(Modifier.size(frame.width.dp, frame.height.dp).graphicsLayer {
        translationX = (x - frame.width / 2f).dp.toPx()
        translationY = (y - frame.height / 2f).dp.toPx()
        rotationZ = frame.rotationDegrees
        alpha = reveal
    }) {
        val canvas = drawContext.canvas.nativeCanvas
        val saved = canvas.save()
        canvas.scale(size.width / movie.width(), size.height / movie.height())
        movie.setTime((reveal * 1_200).toInt().coerceAtMost((movie.duration() - 1).coerceAtLeast(0)))
        movie.draw(canvas, 0f, 0f)
        canvas.restoreToCount(saved)
    }
}
