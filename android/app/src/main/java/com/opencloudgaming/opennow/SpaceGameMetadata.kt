package com.opencloudgaming.opennow

import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.OpenInNew
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** Open, two-column facts over the game artwork, sharing all standard metadata and actions. */
@Composable
internal fun SpaceGameMetadata(game: GameInfo) {
    val genresLabel = stringResource(R.string.space_ui_genres)
    val facts = remember(game, genresLabel) {
        buildList {
            val genres = game.genres.map(::formatGameMetadataLabel).filterNot(::isNoisyGameTag).distinct().joinToString(", ")
            if (genres.isNotBlank()) add(GameDetailRow(genresLabel, genres))
            addAll(gameDetailRows(game))
        }
    }
    Column(Modifier.fillMaxWidth().testTag("space-hub-metadata-grid").focusGroup()) {
        facts.chunked(2).forEach { pair ->
            Box(Modifier.fillMaxWidth().height(1.dp).background(SpaceWhite.copy(alpha = .14f)))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(28.dp)) {
                pair.forEach { fact ->
                    Column(Modifier.weight(1f).testTag("space-hub-detail-${fact.label}")
                        .spaceReadableBlock(focusable = false, contentPadding = 0.dp)
                        .gameDetailInteractions(fact).padding(vertical = 20.dp, horizontal = 4.dp),
                        verticalArrangement = Arrangement.spacedBy(14.dp)) {
                        Text(fact.label, color = SpaceMuted, fontSize = 13.sp)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text(fact.value, color = SpaceWhite, fontSize = 16.sp, lineHeight = 25.sp,
                                modifier = Modifier.weight(1f))
                            if (fact.actionUrl != null) Icon(Icons.AutoMirrored.Outlined.OpenInNew,
                                "Open ${fact.value} store page", tint = SpaceWhite, modifier = Modifier.size(18.dp))
                        }
                    }
                }
                if (pair.size == 1) Spacer(Modifier.weight(1f))
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(SpaceWhite.copy(alpha = .14f)))
    }
}
