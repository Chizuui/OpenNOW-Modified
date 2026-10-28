package com.opencloudgaming.opennow

import android.os.SystemClock
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.receiveAsFlow
import kotlin.math.abs

internal val LocalSpaceFastNavigation = compositionLocalOf { false }

/** Detect both fast swipes and repeated focus changes, including tiles already on screen. */
@Composable
internal fun rememberSpaceFastNavigation(list: LazyListState, selectedId: String?, enabled: Boolean): Boolean {
    var fast by remember { mutableStateOf(false) }
    val activity = remember { Channel<Unit>(Channel.CONFLATED) }
    val pixelsPerSecond = with(LocalDensity.current) { 1100.dp.toPx() }
    LaunchedEffect(enabled) {
        fast = false
        if (enabled) activity.receiveAsFlow().collectLatest {
            fast = true
            delay(280)
            fast = false
        }
    }
    LaunchedEffect(list, enabled, pixelsPerSecond) {
        if (!enabled) return@LaunchedEffect
        var previousPosition: Int? = null
        var previousTime = 0L
        snapshotFlow {
            val width = list.layoutInfo.visibleItemsInfo.firstOrNull()?.size ?: 0
            list.firstVisibleItemIndex * width + list.firstVisibleItemScrollOffset
        }.collect { position ->
            val now = SystemClock.uptimeMillis()
            val elapsed = now - previousTime
            val previous = previousPosition
            if (previous != null && elapsed in 1..200 && abs(position - previous) * 1000f / elapsed > pixelsPerSecond) {
                activity.trySend(Unit)
            }
            previousPosition = position
            previousTime = now
        }
    }
    val cadence = remember(enabled) { SpaceSelectionCadence() }
    LaunchedEffect(selectedId, enabled) {
        if (enabled && selectedId != null && cadence.changed(selectedId, SystemClock.uptimeMillis())) activity.trySend(Unit)
    }
    return enabled && fast
}

internal class SpaceSelectionCadence {
    private var previousId: String? = null
    private var previousTime = 0L
    fun changed(id: String, now: Long): Boolean {
        if (id == previousId) return false
        val rapid = previousId != null && now - previousTime in 0..180
        previousId = id
        previousTime = now
        return rapid
    }
}
