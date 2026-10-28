package com.opencloudgaming.opennow

import android.os.Build
import android.view.View
import android.view.Window
import android.view.WindowInsets
import android.view.WindowInsetsController
import androidx.core.view.WindowCompat

/** The window owns visibility; stream input, pointer capture and keep-awake remain independent. */
internal fun applyOpenNowSystemBars(window: Window, immersive: Boolean) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.insetsController?.let { controller ->
            if (immersive) {
                controller.systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                controller.hide(WindowInsets.Type.statusBars() or WindowInsets.Type.navigationBars())
            } else {
                controller.show(WindowInsets.Type.statusBars() or WindowInsets.Type.navigationBars())
            }
        }
    } else {
        @Suppress("DEPRECATION")
        window.decorView.systemUiVisibility = if (immersive) {
            View.SYSTEM_UI_FLAG_FULLSCREEN or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
                View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
                View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_LAYOUT_STABLE
        } else 0
    }
}

/** Space UI stays landscape even while stream settings are open. */
internal fun openNowRequestedOrientation(defaultOrientation: Int, streamLocked: Boolean,
    streamPanelExpanded: Boolean, spaceEnabled: Boolean): Int = when {
    spaceEnabled -> android.content.pm.ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
    streamPanelExpanded -> android.content.pm.ActivityInfo.SCREEN_ORIENTATION_FULL_SENSOR
    streamLocked -> android.content.pm.ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
    else -> defaultOrientation
}
