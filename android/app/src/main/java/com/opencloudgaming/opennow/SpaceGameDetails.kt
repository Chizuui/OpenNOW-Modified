package com.opencloudgaming.opennow

import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.roundToInt

/** Five-line expandable description with controller scrolling. */
@Composable
internal fun SpaceGameAbout(game: GameInfo, narrow: Boolean, descriptionRequester: FocusRequester,
    previousRequester: FocusRequester) {
    val description = remember(game) { gameDescriptionForDetails(game) }
        ?: stringResource(R.string.catalog_no_description)
    var expanded by rememberSaveable(game.id, description) { mutableStateOf(false) }
    var canExpand by remember(description) { mutableStateOf(false) }
    val position = remember(game.id) { arrayOfNulls<LayoutCoordinates>(1) }
    val scroll = LocalSpaceDetailsScroll.current
    LaunchedEffect(expanded) {
        if (expanded) {
            // Align the newly measured text before focusing it; no delayed scroll can override input.
            withFrameNanos { }
            val text = position[0]?.takeIf { it.isAttached }
            val viewport = scroll?.viewport?.takeIf { it.isAttached }
            if (text != null && viewport != null && scroll != null) {
                val delta = text.localToRoot(Offset.Zero).y - viewport.localToRoot(Offset.Zero).y
                scroll.state.scrollTo((scroll.state.value + delta.roundToInt()).coerceIn(0, scroll.state.maxValue))
            }
            requestFocusWithRetry(descriptionRequester, initialDelayMs = 0)
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(stringResource(R.string.space_ui_about_game), color = SpaceWhite, fontSize = 22.sp, fontWeight = FontWeight.Light,
            modifier = Modifier.testTag("space-hub-about"))
        Text(description, color = SpaceMuted, fontSize = 15.sp, lineHeight = 25.sp,
            maxLines = if (expanded) Int.MAX_VALUE else 5, overflow = TextOverflow.Ellipsis,
            onTextLayout = { if (!expanded) canExpand = it.hasVisualOverflow },
            modifier = Modifier.fillMaxWidth(if (narrow) 1f else .84f).testTag("space-hub-description")
                .focusProperties { up = previousRequester }
                .onGloballyPositioned { position[0] = it }.spaceReadableBlock(descriptionRequester, contentPadding = 0.dp, highlight = false))
        if (canExpand || expanded) SpaceControl(stringResource(if (expanded) R.string.space_ui_show_less else R.string.space_ui_show_more),
            Modifier.testTag("space-hub-description-toggle"), rectangular = true, onClick = { expanded = !expanded })
    }
}

@Composable
internal fun SpaceGameDetails(game: GameInfo, defaultVariantId: String?) {
    Column(Modifier.fillMaxWidth().testTag("space-hub-details"), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        OwnershipStatusRow(game, compact = true)
        MembershipUpgradeNotice(game)
        GameLastPlayed(game)
        Text(stringResource(R.string.space_ui_game_details), color = SpaceWhite, fontSize = 22.sp, fontWeight = FontWeight.Light)
        SpaceGameMetadata(game)
        LaunchOptionsList(game, defaultVariantId, compact = false)
    }
}
