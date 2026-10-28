package com.opencloudgaming.opennow

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.SystemUpdateAlt
import androidx.compose.material.icons.outlined.WarningAmber
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

internal enum class GameDetailsLaunchAction {
    Play,
    ChooseStore,
    PlayOnTv,
}

private data class AvailabilityNotice(
    @param:StringRes val title: Int,
    @param:StringRes val message: Int,
    val icon: ImageVector,
)

private fun GameAvailability.notice(): AvailabilityNotice? = when (this) {
    GameAvailability.Available -> null
    GameAvailability.Maintenance -> AvailabilityNotice(
        R.string.game_status_maintenance_title,
        R.string.game_status_maintenance_message,
        Icons.Outlined.WarningAmber,
    )
    GameAvailability.Patching -> AvailabilityNotice(
        R.string.game_status_patching_title,
        R.string.game_status_patching_message,
        Icons.Outlined.SystemUpdateAlt,
    )
}

@Composable
internal fun GameAvailabilityBanner(availability: GameAvailability, compact: Boolean) {
    val notice = availability.notice() ?: return
    val colors = MaterialTheme.colorScheme
    val maintenance = availability == GameAvailability.Maintenance
    Surface(
        modifier = Modifier.fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite },
        shape = RoundedCornerShape(if (compact) 12.dp else 14.dp),
        color = if (maintenance) colors.errorContainer else colors.tertiaryContainer,
        contentColor = if (maintenance) colors.onErrorContainer else colors.onTertiaryContainer,
    ) {
        Row(
            modifier = Modifier.padding(if (compact) 12.dp else 14.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(notice.icon, contentDescription = null)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(stringResource(notice.title), style = MaterialTheme.typography.titleSmall)
                Text(
                    stringResource(notice.message),
                    style = if (compact) MaterialTheme.typography.bodySmall else MaterialTheme.typography.bodyMedium,
                )
            }
        }
    }
}

@Composable
internal fun GameAvailabilityDialog(
    availability: GameAvailability,
    onContinue: () -> Unit,
    onDismiss: () -> Unit,
) {
    val notice = availability.notice() ?: return
    AlertDialog(
        onDismissRequest = onDismiss,
        icon = { Icon(notice.icon, contentDescription = null) },
        title = { Text(stringResource(notice.title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(notice.message))
                Text(stringResource(R.string.game_status_launch_warning))
            }
        },
        confirmButton = {
            TextButton(onClick = onContinue) {
                Text(stringResource(R.string.game_status_continue_anyway))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.action_cancel)) }
        },
    )
}
