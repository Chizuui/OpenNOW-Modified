package com.opencloudgaming.opennow.ui.adaptive

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * Material 3 window size classes, the single source of truth for adaptive breakpoints in this app.
 *
 * Official M3 breakpoints (see developer.android.com/develop/ui/compose/layouts/adaptive):
 *  - Width:  Compact < 600dp | Medium 600–839dp | Expanded ≥ 840dp
 *  - Height: Compact < 480dp | Medium 480–899dp | Expanded ≥ 900dp
 */
enum class WindowWidthSizeClass { Compact, Medium, Expanded }

/** Material 3 window height size classes. */
enum class WindowHeightSizeClass { Compact, Medium, Expanded }

/**
 * The window's width and height size classes. Everything else in the app should derive its
 * adaptive decisions from this rather than comparing raw dp values, so every screen responds to
 * the same breakpoints.
 */
data class WindowSizeClass(
    val width: WindowWidthSizeClass,
    val height: WindowHeightSizeClass,
) {
    /**
     * A handheld (phone) window: at least one axis is compact. A portrait phone has a compact
     * width; a landscape phone has a compact height (the short dimension), which is why this
     * checks both axes rather than only the width class.
     *
     * Note the height threshold is the M3 compact height of 480dp, so windows whose smaller
     * dimension lands in the 480–599dp band (unusual multi-window sizes, e.g. 700x550 landscape)
     * are no longer treated as phones the way the old `minOf(width, height) < 600dp` check did.
     * Realistic phones are 360–450dp tall in landscape, so they are unaffected.
     */
    val isPhone: Boolean
        get() = width == WindowWidthSizeClass.Compact || height == WindowHeightSizeClass.Compact
}

val WindowWidthSizeClass.isAtLeastMedium: Boolean get() = this != WindowWidthSizeClass.Compact

// ── Official M3 breakpoints (dp) ───────────────────────────────────────────
private const val WINDOW_COMPACT_MAX_WIDTH_DP = 600
private const val WINDOW_MEDIUM_MAX_WIDTH_DP = 840
private const val WINDOW_COMPACT_MAX_HEIGHT_DP = 480
private const val WINDOW_MEDIUM_MAX_HEIGHT_DP = 900

fun windowWidthSizeClassOf(widthDp: Float): WindowWidthSizeClass = when {
    widthDp < WINDOW_COMPACT_MAX_WIDTH_DP -> WindowWidthSizeClass.Compact
    widthDp < WINDOW_MEDIUM_MAX_WIDTH_DP -> WindowWidthSizeClass.Medium
    else -> WindowWidthSizeClass.Expanded
}

fun windowWidthSizeClassOf(width: Dp): WindowWidthSizeClass = windowWidthSizeClassOf(width.value)

fun windowHeightSizeClassOf(heightDp: Float): WindowHeightSizeClass = when {
    heightDp < WINDOW_COMPACT_MAX_HEIGHT_DP -> WindowHeightSizeClass.Compact
    heightDp < WINDOW_MEDIUM_MAX_HEIGHT_DP -> WindowHeightSizeClass.Medium
    else -> WindowHeightSizeClass.Expanded
}

fun windowSizeClassOf(widthDp: Float, heightDp: Float): WindowSizeClass =
    WindowSizeClass(windowWidthSizeClassOf(widthDp), windowHeightSizeClassOf(heightDp))

fun windowSizeClassOf(width: Dp, height: Dp): WindowSizeClass =
    windowSizeClassOf(width.value, height.value)

/** The current window's size class, recomputed when the window resizes (rotation, multi-window). */
@Composable
fun rememberWindowSizeClass(): WindowSizeClass {
    val windowInfo = LocalWindowInfo.current
    val density = LocalDensity.current
    val size = windowInfo.containerSize
    return remember(size, density) {
        with(density) {
            windowSizeClassOf(size.width.toDp(), size.height.toDp())
        }
    }
}

// ── App-level breakpoints (custom values between the M3 classes) ────────────
// Kept here next to the M3 classes so every screen reads the same numbers instead of sprinkling
// raw dp values around the codebase.

/** Content switches to a wide side-by-side layout at this width (two-pane details, landscape login). */
val WIDE_CONTENT_MIN_WIDTH = 720.dp

/** The tablet two-pane recommendation pane grows to 360dp at this width. */
val TWO_PANE_WIDE_PANE_MIN_WIDTH = 900.dp

/**
 * The shared "compact content" boundary: below this width content stacks into compact layouts
 * (2-column metric grids, compact zone rows, phone-only device login), and at/above it layouts
 * expand (3-column grids, side-by-side device login).
 */
val CONTENT_COMPACT_MAX_WIDTH = 520.dp
