package com.opencloudgaming.opennow.screens.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.AndroidUpdateState
import com.opencloudgaming.opennow.AndroidUpdateStatus
import com.opencloudgaming.opennow.OpenNowViewModel
import com.opencloudgaming.opennow.OpenNowUiState
import com.opencloudgaming.opennow.formatAndroidUpdateProgress
import com.opencloudgaming.opennow.isAndroidUpdateCheckBlockedByStream
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import java.text.DateFormat
import java.util.Date
import java.util.Locale

private val SettingsText = OpenNowPalette.TextPrimary
private val SettingsTextMuted = OpenNowPalette.TextMuted

@Composable
internal fun AndroidUpdatePanel(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val update = state.androidUpdate
    if (!update.updateChecksSupported) {
        AndroidUpdateUnavailablePanel(update)
        return
    }

    val updateCheckingDisabled = !state.settings.autoCheckForUpdates
    val checkBlockedByStream = state.isAndroidUpdateCheckBlockedByStream()
    val showCheckPauseMessage = checkBlockedByStream && when (update.status) {
        AndroidUpdateStatus.Available,
        AndroidUpdateStatus.Downloading,
        AndroidUpdateStatus.Downloaded -> false
        else -> true
    }
    val statusMessage = when {
        updateCheckingDisabled -> "Automatic checks are off."
        showCheckPauseMessage -> "Checks pause while streaming."
        else -> updateStatusSubtitle(update)
    }

    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(22.dp),
        color = if (update.status in updateAvailableStatuses) {
            MaterialTheme.colorScheme.primary.copy(alpha = 0.12f)
        } else {
            MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.72f)
        },
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Text(
                        updateStatusTitle(update),
                        color = SettingsText,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        statusMessage,
                        color = SettingsTextMuted,
                        style = MaterialTheme.typography.bodySmall,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                UpdateStatusBadge(update.status)
            }

            UpdateVersionSummary(update)

            if (update.status == AndroidUpdateStatus.Downloading) {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                    update.progress?.let { progress ->
                        Text(
                            formatAndroidUpdateProgress(progress),
                            color = SettingsTextMuted,
                            style = MaterialTheme.typography.labelSmall,
                        )
                    }
                }
            }

            UpdateReleaseNotes(update.releaseNotes)

            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                OutlinedButton(
                    onClick = viewModel::checkAndroidUpdate,
                    enabled = update.canCheck && !checkBlockedByStream && !updateCheckingDisabled,
                    modifier = Modifier.weight(1f),
                ) {
                    Text(if (update.status == AndroidUpdateStatus.Checking) "Checking..." else "Check", maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                when {
                    update.status == AndroidUpdateStatus.Available -> {
                        Button(
                            onClick = viewModel::performAndroidUpdatePrimaryAction,
                            enabled = update.canDownload || update.canOpenPlayStore,
                            modifier = Modifier.weight(1f),
                        ) {
                            Text(
                                if (update.installSource.isGooglePlay) "Update" else "Download",
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                    update.status == AndroidUpdateStatus.Downloaded -> {
                        Button(
                            onClick = viewModel::installAndroidUpdate,
                            enabled = update.canInstall,
                            modifier = Modifier.weight(1f),
                        ) {
                            Text("Install", maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun AndroidUpdateUnavailablePanel(update: AndroidUpdateState) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(22.dp),
        color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.72f),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Text(
                        if (update.installSource.isGooglePlay) "Updates managed by Google Play" else "APK updates unavailable",
                        color = SettingsText,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        update.message,
                        color = SettingsTextMuted,
                        style = MaterialTheme.typography.bodySmall,
                        maxLines = 3,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Surface(
                    shape = RoundedCornerShape(999.dp),
                    color = MaterialTheme.colorScheme.secondary.copy(alpha = 0.16f),
                ) {
                    Text(
                        if (update.installSource.isGooglePlay) "PLAY" else "LOCKED",
                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
                        color = MaterialTheme.colorScheme.secondary,
                        style = MaterialTheme.typography.labelMedium,
                        fontWeight = FontWeight.Bold,
                        maxLines = 1,
                    )
                }
            }
        }
    }
}

@Composable
private fun UpdateStatusBadge(status: AndroidUpdateStatus) {
    Surface(
        shape = RoundedCornerShape(999.dp),
        color = updateMessageColor(status).copy(alpha = 0.16f),
    ) {
        Text(
            updateStatusBadgeText(status),
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
            color = updateMessageColor(status),
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.Bold,
            maxLines = 1,
        )
    }
}

@Composable
private fun UpdateVersionSummary(update: AndroidUpdateState) {
    val checked = update.lastCheckedAt?.let { checkedAt ->
        DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(checkedAt))
    }
    val availableVersion = formatAvailableUpdateVersion(update)

    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
        color = MaterialTheme.colorScheme.surface.copy(alpha = 0.52f),
    ) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxWidth()) {
                UpdateInfoValue("Current", formatCurrentUpdateVersion(update), Modifier.weight(1f))
                availableVersion?.let {
                    UpdateInfoValue("Available", it, Modifier.weight(1f))
                }
            }
            checked?.let {
                Text(
                    "Last checked $it",
                    color = SettingsTextMuted,
                    style = MaterialTheme.typography.labelSmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@Composable
private fun UpdateInfoValue(label: String, value: String, modifier: Modifier = Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(label, color = SettingsTextMuted, style = MaterialTheme.typography.labelSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(value, color = SettingsText, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
private fun UpdateReleaseNotes(notes: String?) {
    val releaseNotes = notes?.trim()?.takeIf { it.isNotBlank() } ?: return
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
        color = MaterialTheme.colorScheme.surface.copy(alpha = 0.52f),
    ) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text("Release notes", color = SettingsText, style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.SemiBold)
            Text(releaseNotes, color = SettingsTextMuted, style = MaterialTheme.typography.bodySmall, maxLines = 8, overflow = TextOverflow.Ellipsis)
        }
    }
}

private val updateAvailableStatuses = setOf(
    AndroidUpdateStatus.Available,
    AndroidUpdateStatus.Downloading,
    AndroidUpdateStatus.Downloaded,
)

private fun formatAvailableUpdateVersion(update: AndroidUpdateState): String? {
    val pieces = listOfNotNull(
        update.availableVersionName?.let { "v$it" },
        update.availableVersionCode?.let { "build $it" },
    )
    return pieces.takeIf { it.isNotEmpty() }?.joinToString(" ")
}

private fun formatCurrentUpdateVersion(update: AndroidUpdateState): String = listOfNotNull(
    update.currentVersionName.takeIf(String::isNotBlank)?.let { "v$it" },
    "build ${update.currentVersionCode}",
).joinToString(" ")

private fun updateStatusTitle(update: AndroidUpdateState): String = when (update.status) {
    AndroidUpdateStatus.Available -> "Update available"
    AndroidUpdateStatus.Downloading -> "Downloading update"
    AndroidUpdateStatus.Downloaded -> "Ready to install"
    AndroidUpdateStatus.NotAvailable -> "OpenNOW is up to date"
    AndroidUpdateStatus.Checking -> "Checking for updates"
    AndroidUpdateStatus.Error -> "Update check failed"
    AndroidUpdateStatus.Idle -> "App updates"
}

private fun updateStatusSubtitle(update: AndroidUpdateState): String = when (update.status) {
    AndroidUpdateStatus.Available -> if (update.installSource.isGooglePlay) {
        update.message
    } else {
        update.availableVersionName?.let { "Version $it is available." } ?: "A new build is available."
    }
    AndroidUpdateStatus.Downloading -> "Keep OpenNOW open while the APK downloads."
    AndroidUpdateStatus.Downloaded -> update.availableVersionName?.let { "Version $it has been downloaded." } ?: "The update has been downloaded."
    AndroidUpdateStatus.NotAvailable -> update.message
    AndroidUpdateStatus.Checking -> if (update.installSource.isGooglePlay) "Checking Google Play." else "Contacting the update source."
    AndroidUpdateStatus.Error -> update.message
    AndroidUpdateStatus.Idle -> update.message
}

private fun updateStatusBadgeText(status: AndroidUpdateStatus): String = when (status) {
    AndroidUpdateStatus.Available -> "NEW"
    AndroidUpdateStatus.Downloading -> "DOWNLOADING"
    AndroidUpdateStatus.Downloaded -> "READY"
    AndroidUpdateStatus.NotAvailable -> "CURRENT"
    AndroidUpdateStatus.Checking -> "CHECKING"
    AndroidUpdateStatus.Error -> "ERROR"
    AndroidUpdateStatus.Idle -> "IDLE"
}

@Composable
private fun updateMessageColor(status: AndroidUpdateStatus): Color = when (status) {
    AndroidUpdateStatus.Available,
    AndroidUpdateStatus.Downloaded,
    AndroidUpdateStatus.NotAvailable -> MaterialTheme.colorScheme.primary
    AndroidUpdateStatus.Error -> Color(0xffff9f9f)
    else -> SettingsTextMuted
}
