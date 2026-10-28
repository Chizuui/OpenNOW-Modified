package com.opencloudgaming.opennow

import android.content.Intent
import android.net.Uri
import android.widget.Toast
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.AnnotatedString

/** Identical store links and copy behavior across both game-details presentations. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun Modifier.gameDetailInteractions(row: GameDetailRow): Modifier {
    val clipboard = LocalClipboardManager.current
    val context = LocalContext.current
    return combinedClickable(
        role = if (row.actionUrl != null) Role.Button else null,
        onClick = {
            row.actionUrl?.let { url ->
                runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) }
                    .onFailure { Toast.makeText(context, context.getString(R.string.error_open_store_page), Toast.LENGTH_SHORT).show() }
            }
        },
        onLongClick = {
            clipboard.setText(AnnotatedString(row.copyValue))
            Toast.makeText(context, "${row.label} copied", Toast.LENGTH_SHORT).show()
        },
    )
}
