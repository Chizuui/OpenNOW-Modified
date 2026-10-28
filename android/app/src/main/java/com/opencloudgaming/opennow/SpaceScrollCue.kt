package com.opencloudgaming.opennow

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.KeyboardArrowDown
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion

/** A decorative scroll hint; motion never causes layout or takes controller focus. */
@Composable
internal fun SpaceScrollCue(modifier: Modifier = Modifier) {
    val moving = !LocalReduceMotion.current && LocalWindowInfo.current.isWindowFocused
    val phase = if (moving) {
        rememberInfiniteTransition(label = "space-scroll-cue").animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(tween(1100, easing = LinearEasing)),
            label = "space-scroll-cue-descent",
        )
    } else rememberUpdatedState(0f)
    val travel = with(LocalDensity.current) { 8.dp.toPx() }
    Icon(Icons.Outlined.KeyboardArrowDown, contentDescription = null, tint = SpaceWhite,
        modifier = modifier.size(36.dp).testTag("space-hub-scroll-cue").graphicsLayer {
            translationY = travel * phase.value
            alpha = .85f * (1f - phase.value)
        })
}
