package com.opencloudgaming.opennow


import android.provider.Settings
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.togetherWith
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material.icons.rounded.KeyboardArrowUp
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.DialogProperties
import com.opencloudgaming.opennow.ui.controls.ControlActionRow
import com.opencloudgaming.opennow.ui.controls.ControlSection
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import com.opencloudgaming.opennow.ui.theme.tint




@Composable
internal fun BugReportDataDisclosure(
    includeTypedTextWarning: Boolean,
    modifier: Modifier = Modifier,
) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(14.dp),
        color = OpenNowPalette.StatusNotice.copy(alpha = 0.10f),
        contentColor = TextPrimary,
        border = BorderStroke(1.dp, OpenNowPalette.StatusNotice.copy(alpha = 0.38f)),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth(),
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .minimumInteractiveComponentSize()
                    .clip(RoundedCornerShape(14.dp))
                    .clickable { expanded = !expanded }
                    .padding(horizontal = 12.dp, vertical = 11.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(
                    "PrintedWaste API",
                    modifier = Modifier.weight(1f),
                    color = OpenNowPalette.StatusNotice,
                    fontWeight = FontWeight.Bold,
                    style = MaterialTheme.typography.labelLarge,
                )
                Text(
                    "What is collected?",
                    color = TextPrimary,
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.SemiBold,
                )
                Icon(
                    imageVector = if (expanded) Icons.Rounded.KeyboardArrowUp else Icons.Rounded.KeyboardArrowDown,
                    contentDescription = if (expanded) "Collapse collection details" else "Expand collection details",
                    tint = TextMuted,
                )
            }
            AnimatedVisibility(visible = expanded) {
                Column(
                    modifier = Modifier.padding(start = 12.dp, end = 12.dp, bottom = 12.dp),
                    verticalArrangement = Arrangement.spacedBy(7.dp),
                ) {
                    Text(
                        "PrintedWaste and OpenNOW maintainers may view the report text, app version/build, device model, Android version, provider and membership category, current game, stream status/settings, and a redacted diagnostic log.",
                        color = TextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                    Text(
                        "The automatic log removes account names, credentials, session IDs, and network addresses before upload.",
                        color = TextPrimary,
                        style = MaterialTheme.typography.bodySmall,
                        fontWeight = FontWeight.SemiBold,
                    )
                    if (includeTypedTextWarning) {
                        Text(
                            "Your typed title and description are sent exactly as written, so do not include personal or sensitive information.",
                            color = TextPrimary,
                            style = MaterialTheme.typography.bodySmall,
                            fontWeight = FontWeight.SemiBold,
                        )
                    }
                    Text(
                        "Your data is not sold and is used only to investigate and fix bugs.",
                        color = TextPrimary,
                        style = MaterialTheme.typography.bodySmall,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        "The same timestamped log available from Settings > Advanced > Debug Logs is attached automatically. No other files are added.",
                        color = TextMuted,
                        style = MaterialTheme.typography.labelSmall,
                    )
                }
            }
        }
    }
}

/**
 * Shared header for the main panel and every focused settings/support page. It stays put while the
 * selected page scrolls.
 */
/**
 * Publishes this composable's screen bounds to the native input router so touches landing on it are
 * treated as UI rather than forwarded into the game.
 *
 * Two guards the hand-written version did not have:
 *  - a zero-size measurement is ignored, instead of publishing a degenerate rect;
 *  - the rect is inflated slightly, because boundsInRoot() includes graphicsLayer transforms and
 *    the panel enters under scaleIn(0.96f) — mid-animation it would otherwise under-report and
 *    leak touches around its edge.
 *
 * The caller must keep this on a node whose size does not depend on its content. A content-driven
 * height would shrink the rect during a transition and leak touches into the game.
 */

@Composable
internal fun BugReportSubmissionRequirements(modifier: Modifier = Modifier) {
    Text(
        "Bug reports are currently supported only in English. Descriptions must be at least $ANDROID_BUG_REPORT_MIN_DESCRIPTION_CHARS characters and explain what happened. Non-English or non-descriptive reports will be ignored.",
        modifier = modifier.fillMaxWidth(),
        color = MaterialTheme.colorScheme.error,
        fontWeight = FontWeight.Bold,
        style = MaterialTheme.typography.bodyMedium,
    )
}

@Composable
internal fun BugReportVersionGateCard(
    update: AndroidUpdateState,
    versionCheck: AndroidBugReportVersionCheckState,
    onRetry: () -> Unit,
    onOpenUpdate: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val updateRequired = update.status == AndroidUpdateStatus.Available ||
        versionCheck.status == AndroidBugReportVersionCheckStatus.UpdateRequired
    val checking = versionCheck.status == AndroidBugReportVersionCheckStatus.Checking
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(14.dp),
        color = MaterialTheme.colorScheme.error.copy(alpha = 0.10f),
        contentColor = TextPrimary,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.error.copy(alpha = 0.38f)),
    ) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                when {
                    updateRequired -> "Update required before reporting"
                    checking -> "Checking Google Play"
                    else -> "Google Play version check required"
                },
                color = MaterialTheme.colorScheme.error,
                fontWeight = FontWeight.Bold,
                style = MaterialTheme.typography.labelLarge,
            )
            Text(
                androidBugReportBlockMessage(update, versionCheck)
                    ?: "OpenNOW must verify the installed Play Store build before sending a report.",
                color = TextMuted,
                style = MaterialTheme.typography.bodySmall,
            )
            when {
                updateRequired -> Button(onClick = onOpenUpdate) {
                    Text("Update in Google Play")
                }
                checking -> Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    CircularProgressIndicator(modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
                    Text("Checking latest build…", style = MaterialTheme.typography.bodySmall)
                }
                else -> OutlinedButton(onClick = onRetry) {
                    Text("Retry version check")
                }
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun BugReportPreflightDeckView(
    deck: BugReportPreflightDeck,
    page: Int,
    onPrevious: () -> Unit,
    onNext: () -> Unit,
    onRefresh: () -> Unit,
    onCancel: () -> Unit,
) {
    val card = deck.cards[page]
    val accent = when (card.tone) {
        BugReportPreflightTone.Healthy -> Green
        BugReportPreflightTone.Notice -> MaterialTheme.colorScheme.primary
        BugReportPreflightTone.Warning -> OpenNowPalette.StatusNotice
    }
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text("Before you report", fontWeight = FontWeight.Bold, style = MaterialTheme.typography.titleMedium)
                Text(
                    "Live checks from this device and session",
                    color = TextMuted,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            Text(
                "${page + 1} / ${deck.cards.size}",
                color = accent,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.Bold,
            )
        }

        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            deck.cards.indices.forEach { index ->
                Box(
                    Modifier
                        .height(4.dp)
                        .weight(1f)
                        .clip(RoundedCornerShape(OpenNowRadius.full))
                        .background(if (index <= page) accent else Color.White.copy(alpha = 0.10f)),
                )
            }
        }

        AnimatedContent(
            targetState = page,
            transitionSpec = { fadeIn(tween(140)) togetherWith fadeOut(tween(100)) },
            label = "bug-report-preflight-card",
        ) { targetPage ->
            val targetCard = deck.cards[targetPage]
            val targetAccent = when (targetCard.tone) {
                BugReportPreflightTone.Healthy -> Green
                BugReportPreflightTone.Notice -> MaterialTheme.colorScheme.primary
                BugReportPreflightTone.Warning -> OpenNowPalette.StatusNotice
            }
            Surface(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(18.dp),
                color = targetAccent.copy(alpha = 0.08f),
                border = BorderStroke(1.dp, targetAccent.copy(alpha = 0.34f)),
            ) {
                Column(
                    modifier = Modifier.padding(15.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Text(
                        targetCard.label,
                        color = targetAccent,
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.Bold,
                        letterSpacing = 0.8.sp,
                    )
                    Text(
                        targetCard.title,
                        color = TextPrimary,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        targetCard.summary,
                        color = TextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                    if (targetCard.facts.isNotEmpty()) {
                        FlowRow(
                            horizontalArrangement = Arrangement.spacedBy(6.dp),
                            verticalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            targetCard.facts.forEach { fact ->
                                Surface(
                                    shape = RoundedCornerShape(OpenNowRadius.full),
                                    color = PanelAlt,
                                    border = BorderStroke(1.dp, Color.White.copy(alpha = 0.08f)),
                                ) {
                                    Text(
                                        fact,
                                        modifier = Modifier.padding(horizontal = 9.dp, vertical = 5.dp),
                                        color = TextPrimary,
                                        style = MaterialTheme.typography.labelSmall,
                                    )
                                }
                            }
                        }
                    }
                    if (targetCard.recommendations.isNotEmpty()) {
                        Text(
                            "MATCHED SUGGESTIONS",
                            color = targetAccent,
                            style = MaterialTheme.typography.labelSmall,
                            fontWeight = FontWeight.Bold,
                        )
                        targetCard.recommendations.forEach { finding ->
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.spacedBy(9.dp),
                                verticalAlignment = Alignment.Top,
                            ) {
                                Surface(
                                    modifier = Modifier.size(7.dp).offset(y = 6.dp),
                                    shape = CircleShape,
                                    color = targetAccent,
                                ) {}
                                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                                    Text(
                                        finding.title,
                                        color = TextPrimary,
                                        style = MaterialTheme.typography.bodySmall,
                                        fontWeight = FontWeight.Bold,
                                    )
                                    Text(
                                        finding.detail,
                                        color = TextMuted,
                                        style = MaterialTheme.typography.labelSmall,
                                    )
                                }
                            }
                        }
                    } else {
                        Text(
                            "No irrelevant fixes are being suggested for this check.",
                            color = targetAccent,
                            style = MaterialTheme.typography.labelSmall,
                            fontWeight = FontWeight.SemiBold,
                        )
                    }
                }
            }
        }

        Text(
            "Still happening after any matched suggestion? Continue and the measured evidence will be attached automatically.",
            color = TextMuted,
            style = MaterialTheme.typography.labelSmall,
        )

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TextButton(onClick = onRefresh) {
                Text("Refresh")
            }
            Spacer(Modifier.weight(1f))
            OutlinedButton(onClick = if (page == 0) onCancel else onPrevious) {
                Text(if (page == 0) "Cancel" else "Back")
            }
            Button(onClick = onNext) {
                Text(if (page == deck.cards.lastIndex) "Continue" else "Next")
            }
        }
    }
}

@Composable
internal fun BugReportFormInputs(
    title: String,
    description: String,
    consentChecked: Boolean,
    submission: BugReportSubmissionState,
    onTitleChange: (String) -> Unit,
    onDescriptionChange: (String) -> Unit,
    onConsentChange: (Boolean) -> Unit,
    onConfirm: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        OutlinedTextField(
            value = title,
            onValueChange = onTitleChange,
            modifier = Modifier.fillMaxWidth(),
            enabled = !submission.uploading,
            singleLine = true,
            label = { Text("Issue title") },
            placeholder = { Text("Stream froze after reconnecting") },
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next),
        )
        OutlinedTextField(
            value = description,
            onValueChange = onDescriptionChange,
            modifier = Modifier
                .fillMaxWidth()
                .height(128.dp),
            enabled = !submission.uploading,
            minLines = 4,
            maxLines = 7,
            label = { Text("What happened?") },
            placeholder = { Text("What were you doing, what went wrong, and can you reproduce it?") },
            supportingText = {
                Text("${description.trim().length} / $ANDROID_BUG_REPORT_MIN_DESCRIPTION_CHARS minimum characters")
            },
            isError = description.isNotEmpty() &&
                description.trim().length < ANDROID_BUG_REPORT_MIN_DESCRIPTION_CHARS,
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Default),
        )
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(10.dp))
                .clickable(enabled = !submission.uploading) {
                    onConsentChange(!consentChecked)
                }
                .padding(vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Checkbox(
                checked = consentChecked,
                onCheckedChange = onConsentChange,
                enabled = !submission.uploading,
            )
            Text(
                "I understand what will be uploaded and consent to send it to the PrintedWaste API.",
                modifier = Modifier.weight(1f),
                color = TextMuted,
                style = MaterialTheme.typography.bodySmall,
            )
        }
        submission.error?.let { error ->
            Surface(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(10.dp),
                color = MaterialTheme.colorScheme.error.copy(alpha = 0.12f),
                contentColor = MaterialTheme.colorScheme.error,
            ) {
                Text(
                    error,
                    modifier = Modifier.padding(10.dp),
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
        Button(
            onClick = onConfirm,
            enabled = title.isNotBlank() &&
                description.trim().length >= ANDROID_BUG_REPORT_MIN_DESCRIPTION_CHARS &&
                consentChecked &&
                !submission.uploading,
            modifier = Modifier.fillMaxWidth(),
        ) {
            if (submission.uploading) {
                CircularProgressIndicator(
                    modifier = Modifier.size(18.dp),
                    strokeWidth = 2.dp,
                    color = MaterialTheme.colorScheme.onPrimary,
                )
                Spacer(Modifier.width(8.dp))
                Text("Uploading report…")
            } else {
                Text(stringResource(R.string.bug_report_open_label))
            }
        }
    }
}

@Composable
internal fun StreamBugReporter(
    submission: BugReportSubmissionState,
    versionCheck: AndroidBugReportVersionCheckState,
    update: AndroidUpdateState,
    onSubmit: (String, String) -> Unit,
    onReset: () -> Unit,
    onVersionCheck: () -> Unit,
    onOpenUpdate: () -> Unit,
    onButtonTone: () -> Unit,
    preflightProvider: () -> BugReportPreflightDeck,
    initiallyExpanded: Boolean = false,
    onExpandedClose: () -> Unit = {},
    landscapeLayout: Boolean = false,
) {
    var expanded by rememberSaveable(initiallyExpanded) { mutableStateOf(initiallyExpanded) }
    var title by rememberSaveable { mutableStateOf("") }
    var description by rememberSaveable { mutableStateOf("") }
    var consentChecked by rememberSaveable { mutableStateOf(false) }
    var confirmationOpen by rememberSaveable { mutableStateOf(false) }
    var preflightReviewed by rememberSaveable { mutableStateOf(false) }
    var preflightPage by rememberSaveable { mutableStateOf(0) }
    var preflightDeck by remember { mutableStateOf<BugReportPreflightDeck?>(null) }

    LaunchedEffect(expanded, update.installSource.isGooglePlay) {
        if (expanded && update.installSource.isGooglePlay) {
            onVersionCheck()
        }
        if (expanded && !preflightReviewed && preflightDeck == null) {
            preflightDeck = preflightProvider()
        }
    }

    ControlSection(stringResource(R.string.bug_report_section)) {
        if (!expanded) {
            ControlActionRow(
                label = stringResource(R.string.bug_report_open_label),
                actionLabel = stringResource(R.string.action_open),
                onClick = {
                    onButtonTone()
                    preflightReviewed = false
                    preflightPage = 0
                    preflightDeck = preflightProvider()
                    expanded = true
                },
                value = stringResource(R.string.bug_report_open_summary),
            )
            return@ControlSection
        }

        if (submission.submitted) {
            Surface(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(14.dp),
                color = Green.copy(alpha = 0.12f),
                contentColor = TextPrimary,
                border = BorderStroke(1.dp, Green.copy(alpha = 0.45f)),
            ) {
                Column(
                    modifier = Modifier.padding(14.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Icon(Icons.Rounded.Check, contentDescription = null, tint = Green)
                        Text("Bug report sent", fontWeight = FontWeight.Bold)
                    }
                    Text(
                        submission.reference?.let { "PrintedWaste reference: $it" }
                            ?: "PrintedWaste received your report.",
                        color = TextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(
                            onClick = {
                                onButtonTone()
                                title = ""
                                description = ""
                                consentChecked = false
                                confirmationOpen = false
                                preflightReviewed = false
                                preflightPage = 0
                                preflightDeck = preflightProvider()
                                onReset()
                            },
                        ) {
                            Text("Send another")
                        }
                        TextButton(
                            onClick = {
                                onButtonTone()
                                preflightReviewed = false
                                preflightPage = 0
                                preflightDeck = null
                                expanded = false
                                onExpandedClose()
                            },
                        ) {
                            Text("Close")
                        }
                    }
                }
            }
            return@ControlSection
        }

        if (!androidBugReportsAllowed(update, versionCheck)) {
            BugReportVersionGateCard(
                update = update,
                versionCheck = versionCheck,
                onRetry = onVersionCheck,
                onOpenUpdate = onOpenUpdate,
            )
            return@ControlSection
        }

        if (!preflightReviewed) {
            val deck = preflightDeck
            if (deck == null) {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(vertical = 20.dp),
                    horizontalArrangement = Arrangement.Center,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    CircularProgressIndicator(modifier = Modifier.size(22.dp), strokeWidth = 2.dp)
                    Spacer(Modifier.width(10.dp))
                    Text("Checking this session…", color = TextMuted)
                }
            } else {
                BugReportPreflightDeckView(
                    deck = deck,
                    page = preflightPage.coerceIn(deck.cards.indices),
                    onPrevious = {
                        onButtonTone()
                        preflightPage = (preflightPage - 1).coerceAtLeast(0)
                    },
                    onNext = {
                        onButtonTone()
                        if (preflightPage < deck.cards.lastIndex) {
                            preflightPage += 1
                        } else {
                            preflightReviewed = true
                        }
                    },
                    onRefresh = {
                        onButtonTone()
                        preflightPage = 0
                        preflightDeck = preflightProvider()
                    },
                    onCancel = {
                        onButtonTone()
                        preflightReviewed = false
                        preflightPage = 0
                        preflightDeck = null
                        expanded = false
                        onExpandedClose()
                    },
                )
            }
            return@ControlSection
        }

        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text("Report a stream bug", fontWeight = FontWeight.Bold)
                    Text(
                        "Describe the problem without leaving your game.",
                        color = TextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                TextButton(
                    enabled = !submission.uploading,
                    onClick = {
                        onButtonTone()
                        preflightReviewed = false
                        preflightPage = 0
                        preflightDeck = preflightProvider()
                    },
                ) {
                    Text("Checks")
                }
                TextButton(
                    enabled = !submission.uploading,
                    onClick = {
                        onButtonTone()
                        preflightReviewed = false
                        preflightPage = 0
                        preflightDeck = null
                        expanded = false
                        onExpandedClose()
                    },
                ) {
                    Text("Cancel")
                }
            }

            if (landscapeLayout) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.lg),
                    verticalAlignment = Alignment.Top,
                ) {
                    Column(
                        modifier = Modifier.weight(0.9f),
                        verticalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        BugReportSubmissionRequirements()
                        BugReportDataDisclosure(includeTypedTextWarning = true)
                    }
                    BugReportFormInputs(
                        title = title,
                        description = description,
                        consentChecked = consentChecked,
                        submission = submission,
                        onTitleChange = { value ->
                            title = value
                            if (submission.error != null) onReset()
                        },
                        onDescriptionChange = { value ->
                            description = value
                            if (submission.error != null) onReset()
                        },
                        onConsentChange = { consentChecked = it },
                        onConfirm = {
                            onButtonTone()
                            confirmationOpen = true
                        },
                        modifier = Modifier.weight(1.1f),
                    )
                }
            } else {
                BugReportSubmissionRequirements()
                BugReportDataDisclosure(includeTypedTextWarning = true)
                BugReportFormInputs(
                    title = title,
                    description = description,
                    consentChecked = consentChecked,
                    submission = submission,
                    onTitleChange = { value ->
                        title = value
                        if (submission.error != null) onReset()
                    },
                    onDescriptionChange = { value ->
                        description = value
                        if (submission.error != null) onReset()
                    },
                    onConsentChange = { consentChecked = it },
                    onConfirm = {
                        onButtonTone()
                        confirmationOpen = true
                    },
                )
            }
        }
    }

    if (confirmationOpen) {
        AlertDialog(
            onDismissRequest = { confirmationOpen = false },
            modifier = if (landscapeLayout) {
                Modifier.widthIn(max = 760.dp).fillMaxWidth(0.82f)
            } else {
                Modifier
            },
            properties = DialogProperties(usePlatformDefaultWidth = !landscapeLayout),
            title = { Text("Upload bug report?") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    BugReportSubmissionRequirements()
                    BugReportDataDisclosure(includeTypedTextWarning = true)
                }
            },
            confirmButton = {
                Button(
                    onClick = {
                        onButtonTone()
                        confirmationOpen = false
                        onSubmit(title, description)
                    },
                ) {
                    Text("Upload report")
                }
            },
            dismissButton = {
                TextButton(
                    onClick = {
                        onButtonTone()
                        confirmationOpen = false
                    },
                ) {
                    Text("Go back")
                }
            },
        )
    }
}
