package com.opencloudgaming.opennow

import kotlin.random.Random

internal const val GOOFY_ANIMATION_SIZE_DP = 45f

internal enum class GoofyScreenEdge(val rotationDegrees: Float) {
    Bottom(0f),
    Left(90f),
    Top(180f),
    Right(270f),
}

internal data class GoofyEdgePlacement(val edge: GoofyScreenEdge, val fraction: Float)

internal fun nextGoofyEdgePlacement(
    previous: GoofyEdgePlacement? = null,
    random: Random = Random.Default,
): GoofyEdgePlacement {
    val edges = GoofyScreenEdge.entries
    val index = if (previous == null) random.nextInt(edges.size) else {
        (previous.edge.ordinal + 1 + random.nextInt(edges.size - 1)) % edges.size
    }
    return GoofyEdgePlacement(edges[index], random.nextFloat())
}

internal data class GoofyEdgeFrame(
    val width: Float,
    val height: Float,
    val centerX: Float,
    val centerY: Float,
    val rotationDegrees: Float,
)

/** The GIF rises from its bottom edge. Rotate that base onto the chosen screen edge. */
internal fun GoofyEdgePlacement.frame(
    viewportWidth: Float,
    viewportHeight: Float,
    sourceWidth: Float,
    sourceHeight: Float,
): GoofyEdgeFrame {
    val screenWidth = viewportWidth.coerceAtLeast(0f)
    val screenHeight = viewportHeight.coerceAtLeast(0f)
    val horizontal = edge == GoofyScreenEdge.Bottom || edge == GoofyScreenEdge.Top
    val along = if (horizontal) screenWidth else screenHeight
    val depth = if (horizontal) screenHeight else screenWidth
    val scale = if (sourceWidth > 0f && sourceHeight > 0f) {
        minOf(
            GOOFY_ANIMATION_SIZE_DP / maxOf(sourceWidth, sourceHeight),
            along / sourceWidth,
            depth / sourceHeight,
        )
    } else 0f
    val width = sourceWidth.coerceAtLeast(0f) * scale
    val height = sourceHeight.coerceAtLeast(0f) * scale
    val position = width / 2f + (along - width).coerceAtLeast(0f) * fraction.coerceIn(0f, 1f)
    val centerX = when (edge) {
        GoofyScreenEdge.Bottom, GoofyScreenEdge.Top -> position
        GoofyScreenEdge.Left -> height / 2f
        GoofyScreenEdge.Right -> screenWidth - height / 2f
    }
    val centerY = when (edge) {
        GoofyScreenEdge.Left, GoofyScreenEdge.Right -> position
        GoofyScreenEdge.Top -> height / 2f
        GoofyScreenEdge.Bottom -> screenHeight - height / 2f
    }
    return GoofyEdgeFrame(width, height, centerX, centerY, edge.rotationDegrees)
}
