package com.opencloudgaming.opennow.ui.theme

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.SpringSpec
import androidx.compose.animation.core.spring
import androidx.compose.runtime.staticCompositionLocalOf

/**
 * Duration and easing tokens, replacing the assorted `tween(1_100)` / `tween(900)` / `tween(820)`
 * values that were picked independently across the UI.
 */
object OpenNowMotion {
    /** Press, toggle, ripple — anything that must feel instantaneous. */
    const val DurationFast = 120

    /** Focus, hover, chip and tab changes. */
    const val DurationStandard = 260

    /** Sheets and page transitions, where the movement itself carries meaning. */
    const val DurationEmphasized = 420

    val EasingStandard = CubicBezierEasing(0.2f, 0f, 0f, 1f)
    val EasingEmphasizedDecel = CubicBezierEasing(0.05f, 0.7f, 0.1f, 1f)
    val EasingEmphasizedAccel = CubicBezierEasing(0.3f, 0f, 0.8f, 0.15f)

    /**
     * M3 Expressive springs. Expressive motion trades fixed-duration tweens for physical springs so
     * that scale/offset changes overshoot slightly and settle — the difference that reads as
     * "responsive" rather than "animated". Used only when `expressiveUi` is on and motion isn't
     * reduced.
     *
     * [SpringSpatial] moves things (card lift, offset); it's allowed a little bounce.
     * [SpringEffect] changes non-spatial values (colour, alpha, elevation); no bounce, so tints
     * don't wobble.
     */
    val SpringSpatial: SpringSpec<Float> = spring(
        dampingRatio = Spring.DampingRatioMediumBouncy,
        stiffness = Spring.StiffnessMediumLow,
    )

    val SpringEffect: SpringSpec<Float> = spring(
        dampingRatio = Spring.DampingRatioNoBouncy,
        stiffness = Spring.StiffnessMedium,
    )
}

/**
 * True when the user has turned animations off system-wide, or disabled background animations in
 * app settings. Infinite transitions (shimmer, focus pulse, carousel auto-advance) must check this
 * — an animation that never ends is the one that actually hurts.
 */
val LocalReduceMotion = staticCompositionLocalOf { false }

/**
 * True when the M3 Expressive visual treatment is on (`settings.expressiveUi`). Provided at the
 * theme root so deep composables — settings section cards, control rows — can opt into tonal
 * elevation and expressive shape without every caller threading the flag through as a parameter.
 * Defaults to `false` so anything read outside the theme (previews, tests) stays on the calm look.
 */
val LocalExpressiveUi = staticCompositionLocalOf { false }
