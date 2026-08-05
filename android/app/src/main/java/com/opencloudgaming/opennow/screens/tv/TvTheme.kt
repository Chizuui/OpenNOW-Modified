package com.opencloudgaming.opennow.screens.tv

import androidx.compose.material3.Typography
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.opencloudgaming.opennow.ui.theme.OpenNowTypography

/**
 * Design tokens for the TV experience, ported from the Google TV Design Kit
 * (see `docs/figma-design-kits-analysis.md` for the source extraction).
 *
 * The TV language differs from the phone/tablet M3 experience in three deliberate ways:
 *  1. **Typography is 1.5–2× larger** — the user sits metres away. Titles land at 32sp,
 *     subtitles at 24sp and actions at 22sp (vs 19/16/13 on phone).
 *  2. **Focus is explicit** — a blue selection ring + scale is the TV equivalent of a hover state.
 *  3. **Surfaces are darker** — bright video on OLED TVs reads best against near-black chrome.
 */
object TvPalette {
    /** Primary selection/focus blue, from the TV kit's tonal palette. */
    val FocusPrimary = Color(0xFF0B57D0)

    /** Deeper blue for pressed/active accents. */
    val FocusPrimaryDeep = Color(0xFF00639B)

    /** Elevated surface tone for focused rows (dark theme). */
    val SurfaceElevated = Color(0xFF282A2C)

    /** Darkest surface — keyboard, backdrops. */
    val SurfaceDark = Color(0xFF131314)

    /** Light accent tints from the kit — active indicators, highlights. */
    val AccentTint = Color(0xFFC2E7FF)
    val AccentTintBright = Color(0xFF7FCFFF)

    /** Light-theme surface (used only if a light TV theme is ever requested). */
    val SurfaceLight = Color(0xFFF2F2F2)
}

/**
 * TV type scale, per the kit's "Android + Web" typography page. Sizes are intentionally large;
 * weight stays conservative (Medium/Normal) because big text already carries hierarchy on a TV.
 *
 * Font family is deliberately not set here — components inherit the app's Inter variable font
 * from `MaterialTheme`, keeping one typeface across phone and TV.
 */
object TvTypography {
    /** Hero titles — the featured carousel. */
    val Display = TextStyle(fontSize = 40.sp, lineHeight = 46.sp, fontWeight = FontWeight.ExtraBold)

    /** List & card titles. */
    val Title = TextStyle(fontSize = 32.sp, lineHeight = 38.sp, fontWeight = FontWeight.SemiBold)

    /** Secondary lines under a title. */
    val Subtitle = TextStyle(fontSize = 24.sp, lineHeight = 30.sp, fontWeight = FontWeight.Normal)

    /** Button labels & inline actions. */
    val Action = TextStyle(fontSize = 22.sp, lineHeight = 28.sp, fontWeight = FontWeight.Medium)

    /** Supporting body copy. */
    val Body = TextStyle(fontSize = 20.sp, lineHeight = 26.sp, fontWeight = FontWeight.Normal)

    /** Small caps-style meta labels. */
    val Overline = TextStyle(fontSize = 22.sp, lineHeight = 26.sp, fontWeight = FontWeight.Medium)
}

/**
 * The phone `Typography` re-sized for TV, mapped onto the same Material 3 roles so every existing
 * `MaterialTheme.typography.*` call site scales up with **zero edits**.
 *
 * Sizes follow the TV Design Kit: titles land at 32sp, subtitles/body at 24sp, actions at 22sp.
 * Font family, weights and letter-spacing are inherited from [OpenNowTypography] so the type stays
 * on-brand (Inter variable) — only size and line height change.
 *
 * The smallest roles (bodySmall/labelSmall/labelMedium) grow only modestly: they back dense,
 * fixed-height UI (chips, status readouts, captions) that must not overflow when scaled.
 *
 * Applied in `OpenNowTheme` when the device is a TV profile, covering game details, settings,
 * dialogs, the store, and the stream chrome in one pass.
 */
val TvTypographyScheme = OpenNowTypography.copy(
    displayLarge = OpenNowTypography.displayLarge.copy(fontSize = 56.sp, lineHeight = 62.sp),
    displayMedium = OpenNowTypography.displayMedium.copy(fontSize = 48.sp, lineHeight = 54.sp),
    displaySmall = OpenNowTypography.displaySmall.copy(fontSize = 40.sp, lineHeight = 46.sp),
    headlineLarge = OpenNowTypography.headlineLarge.copy(fontSize = 36.sp, lineHeight = 42.sp),
    headlineMedium = OpenNowTypography.headlineMedium.copy(fontSize = 32.sp, lineHeight = 38.sp),
    headlineSmall = OpenNowTypography.headlineSmall.copy(fontSize = 28.sp, lineHeight = 34.sp),
    titleLarge = OpenNowTypography.titleLarge.copy(fontSize = 32.sp, lineHeight = 38.sp),
    titleMedium = OpenNowTypography.titleMedium.copy(fontSize = 26.sp, lineHeight = 32.sp),
    titleSmall = OpenNowTypography.titleSmall.copy(fontSize = 22.sp, lineHeight = 28.sp),
    bodyLarge = OpenNowTypography.bodyLarge.copy(fontSize = 24.sp, lineHeight = 30.sp),
    bodyMedium = OpenNowTypography.bodyMedium.copy(fontSize = 20.sp, lineHeight = 26.sp),
    bodySmall = OpenNowTypography.bodySmall.copy(fontSize = 14.sp, lineHeight = 20.sp),
    labelLarge = OpenNowTypography.labelLarge.copy(fontSize = 22.sp, lineHeight = 28.sp),
    labelMedium = OpenNowTypography.labelMedium.copy(fontSize = 16.sp, lineHeight = 22.sp),
    labelSmall = OpenNowTypography.labelSmall.copy(fontSize = 13.sp, lineHeight = 17.sp),
)
