package com.opencloudgaming.opennow

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.foundation.focusGroup
import androidx.compose.ui.input.key.*
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

internal val LocalSpaceSettingsEntry = staticCompositionLocalOf<FocusRequester?> { null }

internal data class SpaceSettingsSection(val id: String, val title: String, val summary: String, val icon: ImageVector)

/** Presentation only. The existing settings owner still supplies every control and its action. */
@Composable
internal fun SpaceSettingsFrame(sections: List<SpaceSettingsSection>, selectedId: String, onSelect: (String) -> Unit,
    content: @Composable () -> Unit) {
    val section = sections.firstOrNull { it.id == selectedId } ?: sections.first()
    val scroll = rememberScrollState()
    val entry = LocalSpaceSettingsEntry.current ?: remember { FocusRequester() }
    LaunchedEffect(selectedId) { scroll.scrollTo(0) }
    CompositionLocalProvider(LocalSpaceAnimatedFocusOutline provides false) {
    BoxWithConstraints(Modifier.fillMaxSize().padding(horizontal = 8.dp, vertical = 16.dp).testTag("space-settings")) {
        val wide = maxWidth >= 600.dp
        val rail: @Composable () -> Unit = {
            if (wide) LazyColumn(contentPadding = PaddingValues(horizontal = 8.dp, vertical = 14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                items(sections, key = { it.id }) { item ->
                    SpaceControl(item.title, rectangular = true, active = item.id == selectedId, modifier = Modifier.fillMaxWidth().onPreviewKeyEvent { event ->
                        if (event.key == Key.DirectionRight && event.type == KeyEventType.KeyDown) entry.requestFocus() else false
                    }, onClick = { onSelect(item.id) }) {
                        Icon(item.icon, null, Modifier.size(18.dp))
                        Text(item.title, color = SpaceWhite, modifier = Modifier.padding(start = 12.dp).weight(1f), style = MaterialTheme.typography.bodyMedium)
                    }
                }
            } else LazyRow(contentPadding = PaddingValues(horizontal = 8.dp, vertical = 14.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                items(sections, key = { it.id }) { item ->
                    SpaceControl(item.title, rectangular = true, active = item.id == selectedId, modifier = Modifier.onPreviewKeyEvent { event ->
                        if (event.key == Key.DirectionDown && event.type == KeyEventType.KeyDown) entry.requestFocus() else false
                    }, onClick = { onSelect(item.id) })
                }
            }
        }
        val controls: @Composable () -> Unit = {
            Column(Modifier.fillMaxSize().focusRequester(entry).focusGroup().spaceArrival(selectedId).verticalScroll(scroll).padding(horizontal = if (wide) 20.dp else 12.dp).padding(top = 20.dp, bottom = 48.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
                Text(section.title, color = SpaceWhite, style = MaterialTheme.typography.headlineLarge, fontWeight = FontWeight.Light)
                content()
            }
        }
        if (wide) Row(Modifier.fillMaxSize(), horizontalArrangement = Arrangement.spacedBy(24.dp)) {
            Box(Modifier.weight(.28f)) { rail() }
            Box(Modifier.weight(.72f)) { controls() }
        } else Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            rail()
            Box(Modifier.weight(1f)) { controls() }
        }
    }
    }
}
