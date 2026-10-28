package com.opencloudgaming.opennow

import android.os.SystemClock
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.isActive

@Composable
internal fun UselessMascotOverlay(settings: AppSettings, allowed: Boolean) {
    val activity = LocalMascotActivity.current ?: return
    val overlayInstance = remember { Any() }
    DisposableEffect(activity, overlayInstance) {
        activity.attachOverlay(overlayInstance)
        onDispose { activity.detachOverlay(overlayInstance) }
    }
    if (activity.overlayOwner !== overlayInstance) return
    val enabled = settings.uselessMascotEnabled && allowed
    val foreground = activity.resumed && activity.focused
    val delayMillis = normalizeMascotDelaySeconds(settings.uselessMascotDelaySeconds) * 1_000L
    DisposableEffect(activity, enabled) {
        activity.enabled = enabled
        activity.reset()
        onDispose {
            activity.enabled = false
            activity.reset()
        }
    }
    LaunchedEffect(activity, enabled, foreground, delayMillis) {
        activity.recordInput()
        if (!enabled || !foreground) return@LaunchedEffect
        while (isActive) {
            val remaining = delayMillis - (SystemClock.uptimeMillis() - activity.lastInputMillis)
            if (remaining > 0L || activity.inputHeld) {
                delay(remaining.coerceAtLeast(100L))
            } else {
                activity.visible = true
                // Sleep until an actual input dismisses it; no polling or idle animation loop.
                snapshotFlow { activity.visible }.first { !it }
            }
        }
    }
    if (enabled && foreground && activity.visible) {
        GoofyScreensaverScene()
    }
}
