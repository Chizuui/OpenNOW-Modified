package com.opencloudgaming.opennow.ui.theme

import androidx.compose.ui.unit.dp

/**
 * Tonal-elevation tokens, so depth stops being an assortment of inline `tonalElevation = 6.dp` /
 * `8.dp` / `10.dp` values chosen per call site.
 *
 * These map to Material 3 surface-tint levels: a higher value tints the surface further toward the
 * primary hue, which is how M3 signals that one surface floats above another. Before this, most
 * surfaces were flat colored `Box`es — depth was hand-painted with palette tokens and borders
 * rather than the tonal system. The Expressive pass leans on these instead.
 */
object OpenNowElevation {
    /** Content that sits flat on its parent — no tint. */
    val flat = 0.dp

    /** A card at rest in a grid: just enough tint to read as a distinct object, not the page. */
    val low = 1.dp

    /** Grouping surfaces — settings section cards, resting list containers. */
    val md = 3.dp

    /** Floating chrome over content: control panels, stats pills, guide callouts. */
    val high = 6.dp

    /** A focused/lifted card, or a modal surface above a scrim. */
    val focus = 8.dp
}
