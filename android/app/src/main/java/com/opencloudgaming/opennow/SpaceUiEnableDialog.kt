package com.opencloudgaming.opennow

import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource

@Composable
internal fun SpaceUiEnableDialog(onConfirm: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.space_ui_enable_title)) },
        text = { Text(stringResource(R.string.space_ui_enable_warning)) },
        confirmButton = {
            Button(onClick = onConfirm) { Text(stringResource(R.string.space_ui_enable_action)) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.action_cancel)) }
        },
    )
}
