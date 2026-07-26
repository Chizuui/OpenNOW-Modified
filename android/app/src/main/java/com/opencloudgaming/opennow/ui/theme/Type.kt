package com.opencloudgaming.opennow.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import com.opencloudgaming.opennow.R

/**
 * Inter Variable, SIL Open Font License 1.1 (see `app/licenses/Inter-OFL-1.1.txt`).
 *
 * Chosen over Roboto for three reasons that matter to this app specifically: it ships tabular
 * figures and a slashed zero (the UI is full of numeric readouts), its tall x-height and open
 * apertures read far better at TV viewing distance, and one variable file covers every weight.
 *
 * `minSdk` is 23 but variable-font axes need API 26. On 23–25 the file loads at its default
 * instance and Compose applies synthetic bolding — an acceptable degradation on a shrinking
 * slice of devices.
 */
@OptIn(ExperimentalTextApi::class)
private fun interWeight(weight: FontWeight) = Font(
    resId = R.font.inter_variable,
    weight = weight,
    variationSettings = FontVariation.Settings(FontVariation.weight(weight.weight)),
)

val Inter = FontFamily(
    interWeight(FontWeight.Normal),
    interWeight(FontWeight.Medium),
    interWeight(FontWeight.SemiBold),
    interWeight(FontWeight.Bold),
    interWeight(FontWeight.ExtraBold),
)

/**
 * Weight and tracking are baked into each style, so call sites stop appending
 * `fontWeight = FontWeight.ExtraBold` by hand — which is how the previous UI ended up with the
 * same visual role rendered at three different weights on three different screens.
 */
val OpenNowTypography = Typography().run {
    copy(
        displayLarge = displayLarge.copy(
            fontFamily = Inter, fontWeight = FontWeight.ExtraBold,
            fontSize = 44.sp, lineHeight = 50.sp, letterSpacing = (-0.02).em,
        ),
        displayMedium = displayMedium.copy(
            fontFamily = Inter, fontWeight = FontWeight.ExtraBold,
            fontSize = 36.sp, lineHeight = 42.sp, letterSpacing = (-0.02).em,
        ),
        displaySmall = displaySmall.copy(
            fontFamily = Inter, fontWeight = FontWeight.ExtraBold,
            fontSize = 30.sp, lineHeight = 36.sp, letterSpacing = (-0.018).em,
        ),
        headlineLarge = headlineLarge.copy(
            fontFamily = Inter, fontWeight = FontWeight.Bold,
            fontSize = 27.sp, lineHeight = 33.sp, letterSpacing = (-0.014).em,
        ),
        headlineMedium = headlineMedium.copy(
            fontFamily = Inter, fontWeight = FontWeight.Bold,
            fontSize = 24.sp, lineHeight = 30.sp, letterSpacing = (-0.012).em,
        ),
        headlineSmall = headlineSmall.copy(
            fontFamily = Inter, fontWeight = FontWeight.Bold,
            fontSize = 22.sp, lineHeight = 28.sp, letterSpacing = (-0.011).em,
        ),
        titleLarge = titleLarge.copy(
            fontFamily = Inter, fontWeight = FontWeight.Bold,
            fontSize = 19.sp, lineHeight = 25.sp, letterSpacing = (-0.008).em,
        ),
        titleMedium = titleMedium.copy(
            fontFamily = Inter, fontWeight = FontWeight.SemiBold,
            fontSize = 16.sp, lineHeight = 22.sp, letterSpacing = (-0.004).em,
        ),
        titleSmall = titleSmall.copy(
            fontFamily = Inter, fontWeight = FontWeight.SemiBold,
            fontSize = 14.sp, lineHeight = 20.sp,
        ),
        bodyLarge = bodyLarge.copy(
            fontFamily = Inter, fontWeight = FontWeight.Normal,
            fontSize = 16.sp, lineHeight = 24.sp,
        ),
        bodyMedium = bodyMedium.copy(
            fontFamily = Inter, fontWeight = FontWeight.Normal,
            fontSize = 14.sp, lineHeight = 20.sp, letterSpacing = 0.007.em,
        ),
        bodySmall = bodySmall.copy(
            fontFamily = Inter, fontWeight = FontWeight.Normal,
            fontSize = 12.sp, lineHeight = 17.sp, letterSpacing = 0.01.em,
        ),
        labelLarge = labelLarge.copy(
            fontFamily = Inter, fontWeight = FontWeight.SemiBold,
            fontSize = 13.sp, lineHeight = 16.sp, letterSpacing = 0.02.em,
        ),
        labelMedium = labelMedium.copy(
            fontFamily = Inter, fontWeight = FontWeight.Medium,
            fontSize = 12.sp, lineHeight = 15.sp, letterSpacing = 0.025.em,
        ),
        labelSmall = labelSmall.copy(
            fontFamily = Inter, fontWeight = FontWeight.Medium,
            fontSize = 11.sp, lineHeight = 14.sp, letterSpacing = 0.04.em,
        ),
    )
}

/**
 * Tabular, slashed-zero figures for anything that updates in place — queue position, FPS, bitrate,
 * latency, slider values. Proportional digits make those readouts visibly jitter on every tick.
 */
val OpenNowNumericStyle = TextStyle(
    fontFamily = Inter,
    fontFeatureSettings = "tnum, zero",
)

/** Applies tabular figures while keeping whatever size/weight the caller already chose. */
fun TextStyle.numeric(): TextStyle = copy(fontFeatureSettings = "tnum, zero")
