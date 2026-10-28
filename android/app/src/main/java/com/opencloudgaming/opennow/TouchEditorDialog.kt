package com.opencloudgaming.opennow

import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ProvideTextStyle
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties

/** Reserve space for actions first; editors scroll within the remaining window height. */
@Composable
internal fun TouchEditorDialog(
    title: String,
    onDismiss: () -> Unit,
    wideLayout: Boolean = false,
    actions: @Composable () -> Unit,
    content: @Composable BoxWithConstraintsScope.() -> Unit,
) {
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        BoxWithConstraints(
            Modifier.widthIn(max = if (wideLayout) 960.dp else 560.dp).fillMaxWidth(),
            contentAlignment = Alignment.Center,
        ) {
            val compact = maxHeight < 480.dp
            Surface(
                modifier = Modifier.padding(if (compact) 8.dp else 12.dp),
                shape = MaterialTheme.shapes.extraLarge,
                tonalElevation = 6.dp,
            ) {
                Column(Modifier.padding(if (compact) 8.dp else 24.dp)) {
                    Text(
                        title,
                        style = if (compact) MaterialTheme.typography.titleMedium else MaterialTheme.typography.headlineSmall,
                        maxLines = if (compact) 1 else 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Spacer(Modifier.height(if (compact) 8.dp else 16.dp))
                    ProvideTextStyle(MaterialTheme.typography.bodyMedium) {
                        BoxWithConstraints(Modifier.weight(1f, fill = false).fillMaxWidth(), content = content)
                    }
                    Spacer(Modifier.height(8.dp))
                    FlowRow(
                        Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
                        content = { actions() },
                    )
                }
            }
        }
    }
}
