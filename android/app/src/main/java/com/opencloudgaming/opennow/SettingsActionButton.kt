package com.opencloudgaming.opennow

import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import com.opencloudgaming.opennow.ui.controls.ControlNavigationRow

/** One action presentation for settings panels and document-backed backup actions. */
@Composable
internal fun SettingsActionButton(label: String, onClick: () -> Unit, modifier: Modifier = Modifier,
    enabled: Boolean = true) {
    if (LocalSpaceUiPresentation.current) {
        ControlNavigationRow(label, onClick, modifier, enabled = enabled)
    } else {
        OutlinedButton(onClick = onClick, enabled = enabled, modifier = modifier) {
            Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}
