package com.opencloudgaming.opennow.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.OpenNowUiState
import com.opencloudgaming.opennow.QrCode
import com.opencloudgaming.opennow.QrCodeView
import com.opencloudgaming.opennow.R
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette

private val TextMuted = OpenNowPalette.TextMuted

@Composable
internal fun DiagnosticShareDialog(
    state: OpenNowUiState,
    onUpload: () -> Unit,
    onDismiss: () -> Unit,
) {
    val share = state.diagnosticShare
    if (!share.awaitingConsent && !share.uploading && share.pasteUrl == null) return

    val clipboard = LocalClipboardManager.current

    LaunchedEffect(share.clipboardSummary, state.androidTvProfile) {
        if (!state.androidTvProfile) {
            share.clipboardSummary?.let { clipboard.setText(AnnotatedString(it)) }
        }
    }

    when {
        share.uploading -> AlertDialog(
            onDismissRequest = {},
            title = { Text(stringResource(R.string.diagnostic_preparing)) },
            text = {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    CircularProgressIndicator(Modifier.size(28.dp))
                    Text(stringResource(R.string.diagnostic_removing_sensitive))
                }
            },
            confirmButton = {},
        )
        share.pasteUrl != null -> {
            val qrCode = remember(share.pasteUrl) { share.pasteUrl?.let(QrCode::encodeText) }
            AlertDialog(
                onDismissRequest = onDismiss,
                title = { Text(if (state.androidTvProfile) "Scan diagnostics link" else "Diagnostics copied") },
                text = {
                    Column(
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        if (state.androidTvProfile) {
                            if (qrCode != null) {
                                QrCodeView(qrCode, Modifier.size(240.dp))
                                Text(stringResource(R.string.diagnostic_qr_scan))
                            } else {
                                Text(stringResource(R.string.diagnostic_qr_failed))
                            }
                        } else {
                            Text(stringResource(R.string.diagnostic_copied))
                            Text(
                                share.pasteUrl,
                                color = MaterialTheme.colorScheme.primary,
                                style = MaterialTheme.typography.bodySmall,
                                textAlign = TextAlign.Center,
                            )
                        }
                    }
                },
                confirmButton = { Button(onClick = onDismiss) { Text("Done") } },
            )
        }
        else -> AlertDialog(
            onDismissRequest = onDismiss,
            title = { Text(stringResource(R.string.diagnostic_create_paste)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(stringResource(R.string.diagnostic_remove_explanation))
                    Text("The randomized link is unlisted but not encrypted, and the paste service deletes uploads within 24 hours.", color = TextMuted)
                    share.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                }
            },
            confirmButton = { Button(onClick = onUpload) { Text(if (share.error == null) stringResource(R.string.diagnostic_upload) else stringResource(R.string.diagnostic_retry)) } },
            dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
        )
    }
}

@Composable
internal fun AnalyticsConsentDialog(
    onAllow: () -> Unit,
    onDecline: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDecline,
        title = { Text(stringResource(R.string.analytics_share_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(
                    "Share anonymous diagnostics to help us find patterns in bugs, crashes, and performance problems. Sensitive data is removed, and we do not sell your data.",
                )
                Text(
                    "If sharing is off during a crash, we may not have enough information to investigate your report. It is off by default and can be changed in Privacy settings.",
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        },
        confirmButton = {
            Button(onClick = onAllow) {
                Text(stringResource(R.string.analytics_share_allow))
            }
        },
        dismissButton = {
            TextButton(onClick = onDecline) {
                Text(stringResource(R.string.analytics_share_decline))
            }
        },
    )
}

@Composable
internal fun AndroidUpdatePromptDialog(
    update: com.opencloudgaming.opennow.AndroidUpdateState,
    onPrimary: () -> Unit,
    onDetails: () -> Unit,
    onDismiss: () -> Unit,
) {
    val version = update.availableVersionName?.let { "Version $it" }
        ?: update.availableVersionCode?.let { "Build $it" }
        ?: "A new build"

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (update.status == com.opencloudgaming.opennow.AndroidUpdateStatus.Downloaded) "Update ready" else "OpenNOW update available") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(
                    if (update.status == com.opencloudgaming.opennow.AndroidUpdateStatus.Downloaded) {
                        "$version is downloaded and ready to install."
                    } else if (update.installSource.isGooglePlay) {
                        "You are on build ${update.currentVersionCode}. Google Play has ${version.lowercase()}."
                    } else {
                        "$version is available for this device."
                    },
                )
                update.releaseNotes?.trim()?.takeIf { it.isNotBlank() }?.let { notes ->
                    Text(
                        notes,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.bodySmall,
                        maxLines = 8,
                        overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
                    )
                }
            }
        },
        confirmButton = {
            Button(onClick = onPrimary) {
                Text(
                    when {
                        update.status == com.opencloudgaming.opennow.AndroidUpdateStatus.Downloaded -> "Install"
                        update.installSource.isGooglePlay -> "Update"
                        else -> "Download"
                    },
                )
            }
        },
        dismissButton = {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = onDetails) {
                    Text("Details")
                }
                TextButton(onClick = onDismiss) {
                    Text(stringResource(R.string.action_cancel))
                }
            }
        },
    )
}

@Composable
internal fun CompletedSessionBugReportDialog(
    submission: com.opencloudgaming.opennow.BugReportSubmissionState,
    versionCheck: com.opencloudgaming.opennow.AndroidBugReportVersionCheckState,
    update: com.opencloudgaming.opennow.AndroidUpdateState,
    onSubmit: (String, String) -> Unit,
    onReset: () -> Unit,
    onVersionCheck: () -> Unit,
    onOpenUpdate: () -> Unit,
    onDismiss: () -> Unit,
) {
    val configuration = androidx.compose.ui.platform.LocalConfiguration.current
    val landscapeLayout = configuration.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
    var title by androidx.compose.runtime.saveable.rememberSaveable { mutableStateOf("") }
    var description by androidx.compose.runtime.saveable.rememberSaveable { mutableStateOf("") }
    var consentChecked by androidx.compose.runtime.saveable.rememberSaveable { mutableStateOf(false) }
    var confirmationOpen by androidx.compose.runtime.saveable.rememberSaveable { mutableStateOf(false) }

    LaunchedEffect(update.installSource.isGooglePlay) {
        if (update.installSource.isGooglePlay) onVersionCheck()
    }

    AlertDialog(
        onDismissRequest = {
            if (!submission.uploading) onDismiss()
        },
        modifier = if (landscapeLayout) {
            Modifier.widthIn(max = 960.dp).fillMaxWidth(0.94f)
        } else {
            Modifier
        },
        title = { Text(stringResource(R.string.bug_report_dialog_title)) },
        text = {
            Column(
                modifier = Modifier
                    .heightIn(
                        max = if (landscapeLayout) {
                            (configuration.screenHeightDp * 0.68f).dp
                        } else {
                            620.dp
                        },
                    )
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                when {
                    submission.submitted -> {
                        Icon(Icons.Rounded.Check, contentDescription = null, tint = Green)
                        Text("Bug report sent", color = Green, fontWeight = FontWeight.Bold)
                        submission.reference?.let { reference ->
                            Text("Reference: $reference", color = TextMuted, style = MaterialTheme.typography.bodySmall)
                        }
                    }
                    !androidBugReportsAllowed(update, versionCheck) -> BugReportVersionGateCard(
                        update = update,
                        versionCheck = versionCheck,
                        onRetry = onVersionCheck,
                        onOpenUpdate = onOpenUpdate,
                    )
                    else -> {
                        Text(
                            "Describe the bug in English. Session diagnostics are attached.",
                            color = TextMuted,
                            style = MaterialTheme.typography.bodySmall,
                        )
                        OutlinedTextField(
                            value = title,
                            onValueChange = { value ->
                                title = value
                                if (submission.error != null) onReset()
                            },
                            modifier = Modifier.fillMaxWidth(),
                            enabled = !submission.uploading,
                            singleLine = true,
                            label = { Text("Issue title") },
                            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next),
                        )
                        OutlinedTextField(
                            value = description,
                            onValueChange = { value ->
                                description = value
                                if (submission.error != null) onReset()
                            },
                            modifier = Modifier.fillMaxWidth().height(128.dp),
                            enabled = !submission.uploading,
                            minLines = 4,
                            maxLines = 7,
                            label = { Text("What happened?") },
                            supportingText = {
                                Text("${description.trim().length} / $ANDROID_BUG_REPORT_MIN_DESCRIPTION_CHARS")
                            },
                            isError = description.isNotEmpty() &&
                                description.trim().length < ANDROID_BUG_REPORT_MIN_DESCRIPTION_CHARS,
                        )
                        BugReportDataDisclosure(includeTypedTextWarning = true)
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(10.dp))
                                .clickable(enabled = !submission.uploading) {
                                    consentChecked = !consentChecked
                                },
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Checkbox(
                                checked = consentChecked,
                                onCheckedChange = { consentChecked = it },
                                enabled = !submission.uploading,
                            )
                            Text(
                                "I consent to send this report.",
                                color = TextMuted,
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                        submission.error?.let { error ->
                            Text(error, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
            }
        },
        confirmButton = {
            when {
                submission.submitted -> Button(onClick = onDismiss) { Text("Done") }
                submission.uploading -> Button(enabled = false, onClick = {}) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(18.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.onPrimary,
                    )
                    Spacer(Modifier.width(8.dp))
                    Text("Sending…")
                }
                androidBugReportsAllowed(update, versionCheck) -> Button(
                    onClick = { confirmationOpen = true },
                    enabled = title.isNotBlank() &&
                        description.trim().length >= ANDROID_BUG_REPORT_MIN_DESCRIPTION_CHARS &&
                        consentChecked,
                ) {
                    Text("Review & send")
                }
            }
        },
        dismissButton = {
            if (!submission.submitted) {
                TextButton(onClick = onDismiss, enabled = !submission.uploading) {
                    Text(stringResource(R.string.action_close))
                }
            }
        },
    )

    if (confirmationOpen) {
        AlertDialog(
            onDismissRequest = { confirmationOpen = false },
            title = { Text(stringResource(R.string.bug_report_confirm_title)) },
            text = { Text(stringResource(R.string.bug_report_confirm_body)) },
            confirmButton = {
                Button(
                    onClick = {
                        confirmationOpen = false
                        onSubmit(title, description)
                    },
                ) {
                    Text("Send")
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmationOpen = false }) {
                    Text("Back")
                }
            },
        )
    }
}
