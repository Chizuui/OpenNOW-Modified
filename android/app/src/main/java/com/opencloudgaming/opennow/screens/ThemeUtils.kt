package com.opencloudgaming.opennow.screens

import com.opencloudgaming.opennow.UiAccent
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette

/**
 * Accent swatch shared across the screens package (accent picker, login, settings panels).
 *
 * Note: the app-wide `OpenNowTheme` composable lives in `OpenNowScreens.kt`
 * (package `com.opencloudgaming.opennow`), where it can select the TV typography scale.
 * This file intentionally only holds the extension below; the old duplicate `OpenNowTheme`
 * was removed to avoid a second, diverging theme definition.
 */
val UiAccent.color: androidx.compose.ui.graphics.Color
    get() = when (this) {
        UiAccent.OpenNow -> OpenNowPalette.AccentDefault
        UiAccent.Pixel -> OpenNowPalette.AccentPixel
        UiAccent.HotPink -> OpenNowPalette.AccentHotPink
        UiAccent.Lime -> OpenNowPalette.AccentLime
        UiAccent.Coral -> OpenNowPalette.AccentCoral
        UiAccent.Violet -> OpenNowPalette.AccentViolet
    }
