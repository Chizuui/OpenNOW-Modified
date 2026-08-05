package com.opencloudgaming.opennow.screens

import android.os.Build
import android.provider.Settings
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import com.opencloudgaming.opennow.AppSettings
import com.opencloudgaming.opennow.UiAccent
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowTypography
import com.opencloudgaming.opennow.ui.theme.OpenNowShapes

private val Green = OpenNowPalette.AccentDefault
private val Background = OpenNowPalette.Background
private val Panel = OpenNowPalette.Panel
private val PanelAlt = OpenNowPalette.PanelAlt
private val TextPrimary = OpenNowPalette.TextPrimary
private val TextMuted = OpenNowPalette.TextMuted

val UiAccent.color: androidx.compose.ui.graphics.Color
    get() = when (this) {
        UiAccent.OpenNow -> OpenNowPalette.AccentDefault
        UiAccent.Pixel -> OpenNowPalette.AccentPixel
        UiAccent.HotPink -> OpenNowPalette.AccentHotPink
        UiAccent.Lime -> OpenNowPalette.AccentLime
        UiAccent.Coral -> OpenNowPalette.AccentCoral
        UiAccent.Violet -> OpenNowPalette.AccentViolet
    }

@Composable
fun OpenNowTheme(settings: AppSettings, content: @Composable () -> Unit) {
    val context = LocalContext.current
    val accent = settings.uiAccent.color
    val fallbackScheme = darkColorScheme(
        primary = accent,
        onPrimary = OpenNowPalette.OnAccent,
        background = Background,
        surface = Panel,
        surfaceVariant = PanelAlt,
        onBackground = TextPrimary,
        onSurface = TextPrimary,
        onSurfaceVariant = TextMuted,
        errorContainer = OpenNowPalette.ErrorContainer,
        onErrorContainer = OpenNowPalette.OnErrorContainer,
    )
    val colorScheme = if (settings.dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        dynamicDarkColorScheme(context).copy(
            primary = accent,
            onPrimary = OpenNowPalette.OnAccent,
            secondary = accent,
            tertiary = Green,
            errorContainer = OpenNowPalette.ErrorContainer,
            onErrorContainer = OpenNowPalette.OnErrorContainer,
        )
    } else {
        fallbackScheme
    }
    val reduceMotion = remember(settings.controllerBackgroundAnimations, context) {
        val systemScale = runCatching {
            Settings.Global.getFloat(
                context.contentResolver,
                Settings.Global.ANIMATOR_DURATION_SCALE,
                1f,
            )
        }.getOrDefault(1f)
        systemScale == 0f || !settings.controllerBackgroundAnimations
    }
    CompositionLocalProvider(LocalReduceMotion provides reduceMotion) {
        MaterialTheme(
            colorScheme = colorScheme,
            typography = OpenNowTypography,
            shapes = OpenNowShapes,
            content = content,
        )
    }
}
