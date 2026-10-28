package com.opencloudgaming.opennow

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material3.Icon
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics

@Composable
internal fun SpaceBackButton(modifier: Modifier = Modifier, onClick: () -> Unit) {
    SpaceIconButton(Icons.AutoMirrored.Outlined.ArrowBack, stringResource(R.string.space_ui_back), modifier, onClick = onClick)
}

@Composable
internal fun SpaceIconButton(icon: ImageVector, label: String, modifier: Modifier = Modifier,
    active: Boolean = false, size: Dp = 48.dp, onClick: () -> Unit) {
    var focused by remember { mutableStateOf(false) }
    val interaction = rememberSpaceInteraction(focused, cornerRadius = size / 2)
    val focusManager = LocalFocusManager.current
    Box(modifier.size(size).then(interaction.modifier)
        .onFocusChanged { focused = it.isFocused }
        .hoverable(interaction.source)
        .clip(CircleShape).background(SpaceWhite)
        .onPreviewKeyEvent { handleDpadFocusMove(it, focusManager) }
        .semantics { selected = active }
        .clickable(interactionSource = interaction.source, indication = null, onClick = onClick),
        contentAlignment = Alignment.Center) {
        Icon(icon, label,
            tint = SpaceInk, modifier = Modifier.size(24.dp))
    }
}
