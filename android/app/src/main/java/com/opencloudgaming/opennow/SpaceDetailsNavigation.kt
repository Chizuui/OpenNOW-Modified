package com.opencloudgaming.opennow

import androidx.compose.foundation.MutatePriority
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.*
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

internal class SpaceDetailsScroll(val state: ScrollState, val scope: CoroutineScope) {
    var viewport: LayoutCoordinates? = null

    fun moveBy(delta: Float) {
        scope.launch { state.scroll(MutatePriority.UserInput) { scrollBy(delta) } }
    }

    fun returnToTop() {
        scope.launch { state.scroll(MutatePriority.UserInput) { scrollBy(-state.value.toFloat()) } }
    }

    /** Also handles keys from the fixed header, after its normal focus traversal declines them. */
    fun handleVerticalKey(event: KeyEvent, step: Float): Boolean {
        if (event.type != KeyEventType.KeyDown) return false
        val delta = when {
            event.key == Key.DirectionDown && state.canScrollForward -> step
            event.key == Key.DirectionUp && state.canScrollBackward -> -step
            else -> return false
        }
        moveBy(delta)
        return true
    }
}
internal val LocalSpaceDetailsScroll = staticCompositionLocalOf<SpaceDetailsScroll?> { null }

/** Returning to an opening action reveals the whole hero, not just the action at its foot. */
@Composable
internal fun Modifier.spaceDetailsHeroAction(): Modifier {
    val owner = LocalSpaceDetailsScroll.current
    return onFocusChanged { if (it.isFocused) owner?.returnToTop() }
        .onPreviewKeyEvent { event ->
            if (event.type == KeyEventType.KeyDown && event.key == Key.DirectionUp && owner?.state?.canScrollBackward == true) {
                owner.returnToTop()
                true
            } else false
        }
}
private class ReadablePosition { var coordinates: LayoutCoordinates? = null }
private fun LayoutCoordinates.unclippedRect(): Rect {
    val origin = localToRoot(Offset.Zero)
    return Rect(origin.x, origin.y, origin.x + size.width, origin.y + size.height)
}

/** Focusable facts participate in D-pad traversal. Long facts scroll before focus moves past them. */
@Composable
internal fun Modifier.spaceReadableBlock(requester: FocusRequester? = null, focusable: Boolean = true,
    contentPadding: Dp = 12.dp, highlight: Boolean = true): Modifier {
    var focused by remember { mutableStateOf(false) }
    val position = remember { ReadablePosition() }
    val owner = LocalSpaceDetailsScroll.current
    val manager = LocalFocusManager.current
    val step = with(LocalDensity.current) { 112.dp.toPx() }
    val shape = RoundedCornerShape(4.dp)
    return then(if (requester == null) Modifier else Modifier.focusRequester(requester))
        .onGloballyPositioned { position.coordinates = it }
        .onFocusChanged { focused = it.isFocused }
        .background(if (highlight && focused) SpaceWhite.copy(alpha = .055f) else Color.Transparent, shape)
        .border(1.dp, if (highlight && focused) SpaceIce.copy(alpha = .4f) else Color.Transparent, shape)
        .onPreviewKeyEvent { event ->
            val content = position.coordinates?.takeIf { it.isAttached }?.unclippedRect()
            val viewport = owner?.viewport?.takeIf { it.isAttached }?.unclippedRect()
            val delta = if (event.type == KeyEventType.KeyDown && content != null && viewport != null) when {
                event.key == Key.DirectionDown && content.bottom > viewport.bottom + 1f && owner?.state?.canScrollForward == true ->
                    minOf(step, content.bottom - viewport.bottom)
                event.key == Key.DirectionUp && content.top < viewport.top - 1f && owner?.state?.canScrollBackward == true ->
                    -minOf(step, viewport.top - content.top)
                else -> 0f
            } else 0f
            if (delta != 0f && owner != null) {
                owner.moveBy(delta)
                true
            } else handleDpadFocusMove(event, manager)
        }
        .then(if (focusable) Modifier.focusable() else Modifier)
        .padding(contentPadding)
}
