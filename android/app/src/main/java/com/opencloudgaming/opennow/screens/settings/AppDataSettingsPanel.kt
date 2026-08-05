package com.opencloudgaming.opennow.screens.settings

import android.os.BatteryManager
import android.os.Build
import android.os.PowerManager
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.Uri
import android.provider.Settings
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.AppSettings
import com.opencloudgaming.opennow.BuildConfig
import com.opencloudgaming.opennow.OpenNowViewModel
import com.opencloudgaming.opennow.R
import com.opencloudgaming.opennow.StreamStatsPosition
import com.opencloudgaming.opennow.StreamStatsStyle
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import kotlinx.coroutines.delay

private val SettingsText = OpenNowPalette.TextPrimary
private val SettingsTextMuted = OpenNowPalette.TextMuted
private val SettingsPanelAlt = OpenNowPalette.PanelAlt

@Composable
internal fun AppDataSettingsPanel(viewModel: OpenNowViewModel) {
    var clearCacheConfirmOpen by remember { mutableStateOf(false) }
    var resetSettingsConfirmOpen by remember { mutableStateOf(false) }

    if (clearCacheConfirmOpen) {
        AlertDialog(
            onDismissRequest = { clearCacheConfirmOpen = false },
            title = { Text(stringResource(R.string.settings_clear_cache_title)) },
            text = { Text(stringResource(R.string.settings_clear_cache_body)) },
            confirmButton = {
                Button(
                    onClick = {
                        clearCacheConfirmOpen = false
                        viewModel.clearCatalogCache()
                    },
                ) {
                    Text(stringResource(R.string.settings_clear_cache))
                }
            },
            dismissButton = {
                TextButton(onClick = { clearCacheConfirmOpen = false }) {
                    Text(stringResource(R.string.action_cancel))
                }
            },
        )
    }

    if (resetSettingsConfirmOpen) {
        AlertDialog(
            onDismissRequest = { resetSettingsConfirmOpen = false },
            title = { Text(stringResource(R.string.settings_reset_title)) },
            text = { Text(stringResource(R.string.settings_reset_body)) },
            confirmButton = {
                Button(
                    onClick = {
                        resetSettingsConfirmOpen = false
                        viewModel.resetSettings()
                    },
                ) {
                    Text(stringResource(R.string.settings_reset))
                }
            },
            dismissButton = {
                TextButton(onClick = { resetSettingsConfirmOpen = false }) {
                    Text(stringResource(R.string.action_cancel))
                }
            },
        )
    }

    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(
            "Reset tutorial only makes the stream guide appear again. Reset settings is destructive: it clears local app data and relaunches OpenNOW.",
            color = SettingsTextMuted,
            style = MaterialTheme.typography.bodySmall,
        )
        Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                OutlinedButton(onClick = { clearCacheConfirmOpen = true }, modifier = Modifier.weight(1f)) {
                    Text(stringResource(R.string.settings_clear_cache), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                OutlinedButton(onClick = viewModel::resetStreamTutorial, modifier = Modifier.weight(1f)) {
                    Text(stringResource(R.string.settings_reset_tutorial), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            OutlinedButton(onClick = { resetSettingsConfirmOpen = true }, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.settings_reset), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

@Composable
internal fun BatteryOptimizationPanel() {
    val context = LocalContext.current
    var isIgnoring by remember { mutableStateOf(false) }

    LaunchedEffect(Unit) {
        val pm = context.getSystemService(Context.POWER_SERVICE) as? PowerManager
        while (true) {
            isIgnoring = pm?.isIgnoringBatteryOptimizations(context.packageName) == true
            delay(1000L)
        }
    }

    Column(
        verticalArrangement = Arrangement.spacedBy(10.dp),
        modifier = Modifier.fillMaxWidth()
    ) {
        Text(
            text = "Android battery optimization restricts the app's background activity, which can cause connection timeouts or pause GFN queue progress when the app is minimized.",
            style = MaterialTheme.typography.bodyMedium,
            color = SettingsTextMuted
        )
        Spacer(Modifier.height(4.dp))
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = androidx.compose.ui.Alignment.CenterVertically
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = "Background activity",
                    fontWeight = androidx.compose.ui.text.font.FontWeight.Bold,
                    style = MaterialTheme.typography.bodyMedium
                )
                Text(
                    text = if (isIgnoring) "Unlimited (Allowed in background)" else "Optimized (May timeout in background)",
                    color = if (isIgnoring) androidx.compose.ui.graphics.Color(0xff81c784) else androidx.compose.ui.graphics.Color(0xffffb74d),
                    style = MaterialTheme.typography.bodySmall
                )
            }
            if (!isIgnoring) {
                Button(
                    onClick = {
                        val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS).apply {
                            data = Uri.parse("package:${context.packageName}")
                        }
                        try {
                            context.startActivity(intent)
                        } catch (e: Exception) {
                            try {
                                context.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
                            } catch (_: Exception) {}
                        }
                    }
                ) {
                    Text(stringResource(R.string.settings_battery_allow))
                }
            } else {
                OutlinedButton(
                    onClick = {
                        try {
                            context.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
                        } catch (_: Exception) {}
                    }
                ) {
                    Text(stringResource(R.string.settings_battery_settings))
                }
            }
        }
    }
}

@Composable
internal fun DebugLogsPanel(state: com.opencloudgaming.opennow.OpenNowUiState, viewModel: OpenNowViewModel) {
    val context = LocalContext.current
    val clipboard = androidx.compose.ui.platform.LocalClipboardManager.current
    var copied by remember { mutableStateOf(false) }
    var saved by remember { mutableStateOf(false) }
    var saveError by remember { mutableStateOf<String?>(null) }
    var pendingLogText by remember { mutableStateOf("") }
    val saveLauncher = androidx.activity.compose.rememberLauncherForActivityResult(
        androidx.activity.result.contract.ActivityResultContracts.CreateDocument("text/plain")
    ) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        runCatching {
            context.contentResolver.openOutputStream(uri)?.use { output ->
                output.write(pendingLogText.toByteArray(Charsets.UTF_8))
            } ?: error("Could not open log file")
        }.onSuccess {
            saved = true
            saveError = null
        }.onFailure { error ->
            saveError = error.message ?: "Could not save logs"
        }
    }

    Text(
        "Exports launch state, queue state, stream updates, recovery events, settings, codec capabilities, and recent sanitized CloudMatch JSON responses.",
        color = SettingsTextMuted,
    )

    if (state.androidTvProfile) {
        Button(
            onClick = viewModel::uploadDiagnosticShare,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text(stringResource(R.string.settings_logs_upload_qr), maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Text(
            stringResource(R.string.settings_logs_qr_description),
            color = SettingsTextMuted,
            style = MaterialTheme.typography.bodySmall,
        )
    } else {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            Button(
                onClick = {
                    clipboard.setText(AnnotatedString(viewModel.sanitizedDebugLogText()))
                    copied = true
                },
                modifier = Modifier.weight(1f),
            ) {
                Text(if (copied) stringResource(R.string.settings_logs_copied) else stringResource(R.string.settings_logs_copy), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            OutlinedButton(
                onClick = {
                    pendingLogText = viewModel.sanitizedDebugLogText()
                    saved = false
                    saveError = null
                    saveLauncher.launch(viewModel.debugLogFileName())
                },
                modifier = Modifier.weight(1f),
            ) {
                Text(if (saved) stringResource(R.string.settings_logs_exported) else stringResource(R.string.settings_logs_export), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }

    state.error?.let { error ->
        OutlinedButton(
            onClick = {
                clipboard.setText(AnnotatedString(error))
                copied = true
            },
        ) {
            Text(stringResource(R.string.settings_copy_error))
        }
    }
    saveError?.let {
        Text(it, color = androidx.compose.ui.graphics.Color(0xffff9f9f), style = MaterialTheme.typography.bodySmall)
    }
}

@Composable
internal fun AppVersionPanel() {
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(SettingsPanelAlt)
            .padding(horizontal = 14.dp, vertical = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = androidx.compose.ui.Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text("OpenNOW Android", color = SettingsText, fontWeight = androidx.compose.ui.text.font.FontWeight.SemiBold)
            Text("Version ${BuildConfig.VERSION_NAME}", color = SettingsTextMuted, style = MaterialTheme.typography.bodySmall)
        }
        Text("Build ${BuildConfig.VERSION_CODE}", color = SettingsTextMuted, style = MaterialTheme.typography.labelMedium)
    }
}

@Composable
internal fun OpenNowGitHubPanel() {
    val context = LocalContext.current
    val clipboard = androidx.compose.ui.platform.LocalClipboardManager.current
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(SettingsPanelAlt)
            .padding(horizontal = 14.dp, vertical = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = androidx.compose.ui.Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text("OpenNOW Repository", color = SettingsText, fontWeight = androidx.compose.ui.text.font.FontWeight.SemiBold)
            Text("OpenCloudGaming/OpenNOW", color = SettingsTextMuted, style = MaterialTheme.typography.bodySmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        OutlinedButton(onClick = {
            val url = "https://github.com/OpenCloudGaming/OpenNOW"
            try {
                context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
            } catch (e: Exception) {
                clipboard.setText(AnnotatedString(url))
                android.widget.Toast.makeText(context, "GitHub link copied", android.widget.Toast.LENGTH_SHORT).show()
            }
        }) {
            Text("GitHub", maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

internal val StreamStatsStyle.label: String
    get() = when (this) {
        StreamStatsStyle.Compact -> "Single line"
        StreamStatsStyle.Detailed -> "Multiline"
    }

internal fun StreamStatsStyle.next(): StreamStatsStyle = when (this) {
    StreamStatsStyle.Compact -> StreamStatsStyle.Detailed
    StreamStatsStyle.Detailed -> StreamStatsStyle.Compact
}

internal val StreamStatsPosition.label: String
    get() = when (this) {
        StreamStatsPosition.Left -> "Left"
        StreamStatsPosition.Center -> "Center"
        StreamStatsPosition.Right -> "Right"
    }

internal fun StreamStatsPosition.next(): StreamStatsPosition = when (this) {
    StreamStatsPosition.Left -> StreamStatsPosition.Center
    StreamStatsPosition.Center -> StreamStatsPosition.Right
    StreamStatsPosition.Right -> StreamStatsPosition.Left
}
