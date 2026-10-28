package com.opencloudgaming.opennow

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import kotlin.math.ceil

@Composable
internal fun SpaceDiscoverCatalog(games: List<GameInfo>, selectedId: String?, gridState: LazyGridState,
    selectedRequester: FocusRequester, compact: Boolean, showPremiumMarker: Boolean, modifier: Modifier, onFocused: (String) -> Unit,
    onSelectGame: (GameInfo) -> Unit, onBack: () -> Unit, catalogControls: @Composable () -> Unit) {
    val reduced = LocalReduceMotion.current
    Column(modifier.fillMaxWidth().spaceArrival("discover-catalog")) {
        Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(16.dp),
            verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
            SpaceBackButton(Modifier.testTag("space-catalog-back"), onClick = onBack)
            Text(stringResource(R.string.library_count, games.size), color = SpaceMuted, fontSize = 13.sp)
            Spacer(Modifier.weight(1f))
            catalogControls()
        }
        BoxWithConstraints(Modifier.weight(1f).fillMaxWidth()) {
            // Fit a complete cover and its focus frame even on short landscape phones.
            val slotWidth = minOf(if (compact) 148.dp else 184.dp,
                ((maxHeight - 40.dp).coerceAtLeast(80.dp) * (628f / 888f)) + 24.dp)
            val columns = ceil(maxWidth / slotWidth).toInt().coerceAtLeast(1)
            LazyVerticalGrid(columns = GridCells.Fixed(columns), state = gridState,
                modifier = Modifier.fillMaxSize().testTag("space-discover-catalog"), contentPadding = PaddingValues(4.dp)) {
                items(games, key = { it.id }) { game ->
                    SpaceGameTile(game, selectedId == game.id, compact, reduced, showPremiumMarker,
                        modifier = Modifier.fillMaxWidth().then(if (selectedId == game.id) Modifier.focusRequester(selectedRequester) else Modifier),
                        onFocused = { onFocused(game.id) }, onClick = { onSelectGame(game) }, portraitArtwork = true)
                }
            }
        }
    }
}

/** Only intentional vertical touch scrolling expands the rail; focus scrolling does not change modes. */
@Composable
internal fun Modifier.expandSpaceCatalogOnSwipe(enabled: Boolean, onExpand: () -> Unit): Modifier {
    val threshold = with(LocalDensity.current) { 48.dp.toPx() }
    val expand by rememberUpdatedState(onExpand)
    val connection = remember(enabled, threshold) {
        object : NestedScrollConnection {
            var distance = 0f
            override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                if (!enabled || source != NestedScrollSource.UserInput) return Offset.Zero
                distance = if (available.y < 0f) distance - available.y else 0f
                if (distance < threshold) return Offset.Zero
                distance = 0f
                expand()
                return Offset(0f, available.y)
            }
        }
    }
    return nestedScroll(connection)
}
