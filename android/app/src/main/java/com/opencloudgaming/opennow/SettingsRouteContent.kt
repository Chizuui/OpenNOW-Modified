package com.opencloudgaming.opennow

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.ui.theme.OpenNowMotion
import kotlin.math.roundToInt

/**
 * Slides only the incoming Settings route using layout placement.
 *
 * The outgoing route is discarded immediately, so two control-heavy Settings trees are never
 * measured or drawn together. Unlike the former alpha/translation implementation, this does not
 * create a page-sized RenderNode animation layer.
 */
@Composable
internal fun <T> SettingsRouteContent(
    targetState: T,
    reduceMotion: Boolean,
    depth: (T) -> Int,
    modifier: Modifier = Modifier,
    animateInitial: Boolean = false,
    content: @Composable (T) -> Unit,
) {
    var initialized by remember { mutableStateOf(animateInitial) }
    var previousDepth by remember { mutableStateOf(depth(targetState)) }
    var direction by remember { mutableStateOf(1f) }
    val animateEntrance = initialized && !reduceMotion
    val progress = remember(targetState, reduceMotion) {
        Animatable(if (animateEntrance) 0f else 1f)
    }
    val slideDistancePx = with(LocalDensity.current) { 28.dp.toPx() }

    LaunchedEffect(targetState, reduceMotion) {
        val nextDepth = depth(targetState)
        direction = if (nextDepth < previousDepth) -1f else 1f
        previousDepth = nextDepth
        if (!initialized) {
            initialized = true
        } else if (!reduceMotion) {
            progress.animateTo(
                targetValue = 1f,
                animationSpec = tween(
                    durationMillis = OpenNowMotion.DurationStandard,
                    easing = OpenNowMotion.EasingEmphasizedDecel,
                ),
            )
        }
    }

    Box(
        modifier
            .fillMaxWidth()
            .offset {
                IntOffset(
                    x = (direction * slideDistancePx * (1f - progress.value)).roundToInt(),
                    y = 0,
                )
            },
    ) {
        content(targetState)
    }
}

