package com.opencloudgaming.opennow.screens

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.SessionReport
import com.opencloudgaming.opennow.SessionReportFinding
import com.opencloudgaming.opennow.SessionReportFindingKind
import com.opencloudgaming.opennow.SessionReportRating
import com.opencloudgaming.opennow.StreamQuality
import com.opencloudgaming.opennow.StreamQualityLevel
import com.opencloudgaming.opennow.formatRuntimeBitrate
import com.opencloudgaming.opennow.formatRuntimeResolution
import com.opencloudgaming.opennow.formatSessionTimerDuration
import com.opencloudgaming.opennow.parseResolutionPixelsOrNull
import com.opencloudgaming.opennow.R
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import com.opencloudgaming.opennow.ui.theme.numeric
import com.opencloudgaming.opennow.ui.theme.tint
import java.util.Locale
import android.content.res.Configuration

private val Green = OpenNowPalette.AccentDefault
private val TextPrimary = OpenNowPalette.TextPrimary
private val TextMuted = OpenNowPalette.TextMuted
private val PanelAlt = OpenNowPalette.PanelAlt

@Composable
internal fun SessionReportDialog(
    report: SessionReport,
    onDismiss: (dontShowAgain: Boolean) -> Unit,
    onReportBug: (dontShowAgain: Boolean) -> Unit,
) {
    val scoreColor = when (report.rating) {
        SessionReportRating.Excellent, SessionReportRating.Good -> OpenNowPalette.StatusGood
        SessionReportRating.Fair -> OpenNowPalette.StatusFair
        SessionReportRating.Poor -> OpenNowPalette.StatusPoor
    }
    val configuration = LocalConfiguration.current
    val landscapeLayout = configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
    var dontShowAgain by remember(report.gameTitle, report.durationSeconds) { mutableStateOf(false) }

    AlertDialog(
        onDismissRequest = { onDismiss(dontShowAgain) },
        modifier = if (landscapeLayout) {
            Modifier.widthIn(max = 960.dp).fillMaxWidth(0.94f)
        } else {
            Modifier
        },
        title = { Text(stringResource(R.string.session_report_title)) },
        text = {
            if (landscapeLayout) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(max = (configuration.screenHeightDp * 0.66f).dp),
                    horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.lg),
                    verticalAlignment = Alignment.Top,
                ) {
                    Column(
                        modifier = Modifier
                            .weight(1f)
                            .verticalScroll(rememberScrollState()),
                        verticalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        SessionReportSummary(report, scoreColor)
                        SessionReportConnection(report)
                    }
                    Column(
                        modifier = Modifier
                            .weight(1f)
                            .verticalScroll(rememberScrollState()),
                        verticalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        SessionReportOutcome(report) { onReportBug(dontShowAgain) }
                    }
                }
            } else {
                Column(
                    modifier = Modifier
                        .heightIn(max = 510.dp)
                        .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    SessionReportSummary(report, scoreColor)
                    SessionReportConnection(report)
                    SessionReportOutcome(report) { onReportBug(dontShowAgain) }
                }
            }
        },
        dismissButton = {
            Row(
                modifier = Modifier
                    .clip(RoundedCornerShape(OpenNowRadius.sm))
                    .clickable { dontShowAgain = !dontShowAgain },
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Checkbox(
                    checked = dontShowAgain,
                    onCheckedChange = { dontShowAgain = it },
                )
                Text(
                    stringResource(R.string.session_report_dont_show_again),
                    color = TextMuted,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        },
        confirmButton = { 
            androidx.compose.material3.Button(onClick = { onDismiss(dontShowAgain) }) { 
                Text(stringResource(R.string.session_report_done)) 
            } 
        },
    )
}

@Composable
private fun SessionReportSummary(report: SessionReport, scoreColor: androidx.compose.ui.graphics.Color) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Surface(
            color = scoreColor.copy(alpha = 0.12f),
            shape = RoundedCornerShape(OpenNowRadius.lg + 2.dp),
            border = BorderStroke(1.dp, scoreColor.copy(alpha = 0.38f)),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth().padding(OpenNowSpacing.lg),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Column(Modifier.weight(1f)) {
                    Text(report.gameTitle, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                    Text(
                        stringResource(
                            R.string.session_report_subtitle,
                            formatSessionTimerDuration(report.durationSeconds),
                        ),
                        color = TextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                Column(horizontalAlignment = Alignment.End) {
                    Text(
                        stringResource(R.string.session_report_score, report.score),
                        color = scoreColor,
                        style = MaterialTheme.typography.headlineMedium.numeric(),
                    )
                    Text(report.rating.label, color = scoreColor, style = MaterialTheme.typography.labelMedium)
                }
            }
        }
        if (report.limitedData) {
            Text(
                "This was a short session, so the score may vary more than usual.",
                color = TextMuted,
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

@Composable
private fun SessionReportConnection(report: SessionReport) {
    Text(stringResource(R.string.session_report_connection), style = MaterialTheme.typography.titleSmall)
    SessionReportMetricGrid(
        listOf(
            SessionReportMetricData(
                label = stringResource(R.string.session_report_metric_latency),
                value = report.averagePingMs?.let { stringResource(R.string.session_report_ms_avg, it) },
                detail = report.peakPingMs?.let { stringResource(R.string.session_report_ms_peak, it) },
                quality = report.averagePingMs?.let(StreamQuality::latency),
            ),
            SessionReportMetricData(
                label = stringResource(R.string.session_report_metric_speed),
                value = formatRuntimeBitrate(report.averageBitrateKbps),
                detail = report.peakBitrateKbps?.let {
                    stringResource(R.string.session_report_peak, formatRuntimeBitrate(it))
                },
            ),
            SessionReportMetricData(
                label = stringResource(R.string.session_report_metric_loss),
                value = report.packetLossPct?.let { "%.2f%%".format(Locale.US, it) },
                detail = report.packetLossPct?.let {
                    stringResource(
                        if (it <= 0.5) R.string.session_report_loss_stable
                        else R.string.session_report_loss_affects,
                    )
                },
                quality = report.packetLossPct?.let(StreamQuality::packetLoss),
            ),
            SessionReportMetricData(
                label = stringResource(R.string.session_report_metric_jitter),
                value = report.averageJitterMs?.let { "%.1f ms".format(Locale.US, it) },
                detail = stringResource(R.string.session_report_jitter_detail),
                quality = report.averageJitterMs?.let(StreamQuality::jitter),
            ),
            SessionReportMetricData(
                label = stringResource(R.string.session_report_metric_fps),
                value = report.averageFps?.let { "%.1f / %d".format(Locale.US, it, report.targetFps) },
                detail = stringResource(R.string.session_report_fps_detail),
                quality = report.averageFps?.let { StreamQuality.frameRate(it, report.targetFps) },
            ),
            SessionReportMetricData(
                label = stringResource(R.string.session_report_metric_decode),
                value = report.averageDecodeMs?.let { "%.1f ms".format(Locale.US, it) },
                detail = stringResource(R.string.session_report_decode_detail),
                quality = report.averageDecodeMs?.let { StreamQuality.decode(it, report.targetFps) },
            ),
        ),
    )
    val networkLabel = when (report.networkKind) {
        com.opencloudgaming.opennow.AndroidNetworkKind.Wifi -> report.wifiBand.label
        else -> report.networkKind.label
    }
    Text(
        buildString {
            append("Network: $networkLabel")
            report.estimatedLinkDownstreamKbps?.let {
                append(" • Android link estimate ${formatRuntimeBitrate(it)}")
            }
        },
        color = TextMuted,
        style = MaterialTheme.typography.bodySmall,
    )
}

@Composable
private fun SessionReportOutcome(report: SessionReport, onReportBug: () -> Unit) {
    Text("Delivered profile", style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold)
    Text(
        buildString {
            append(formatRuntimeResolution(report.deliveredResolution ?: report.requestedResolution))
            append(" • ")
            append(report.deliveredCodec ?: report.requestedCodec.name)
            if (
                normalizeSessionReportResolution(report.deliveredResolution) !=
                normalizeSessionReportResolution(report.requestedResolution) ||
                report.deliveredCodec?.contains(report.requestedCodec.name, ignoreCase = true) == false
            ) {
                append(" (requested ${formatRuntimeResolution(report.requestedResolution)} • ${report.requestedCodec.name})")
            }
        },
        color = TextPrimary,
        style = MaterialTheme.typography.bodyMedium,
    )
    if (report.downgrades.isNotEmpty()) {
        Text("Why the profile changed", style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold)
        report.downgrades.forEach { finding -> SessionReportFindingRow(finding) }
    }
    Text("What to do next", style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold)
    report.recommendations.forEach { finding -> SessionReportFindingRow(finding) }
    TextButton(
        onClick = onReportBug,
        contentPadding = PaddingValues(horizontal = 0.dp, vertical = 4.dp),
    ) {
        Text("Experienced a bug? ", color = TextMuted)
        Text(
            "Report it",
            color = MaterialTheme.colorScheme.primary,
            textDecoration = androidx.compose.ui.text.style.TextDecoration.Underline,
        )
    }
}

@Composable
private fun SessionReportFindingRow(finding: SessionReportFinding) {
    val titleColor = if (finding.kind == SessionReportFindingKind.Warning) OpenNowPalette.StatusFair else Green
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(finding.title, color = titleColor, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
        Text(finding.detail, color = TextMuted, style = MaterialTheme.typography.bodySmall)
    }
}

private data class SessionReportMetricData(
    val label: String,
    val value: String?,
    val detail: String?,
    val quality: StreamQualityLevel? = null,
)

@Composable
private fun SessionReportMetricGrid(metrics: List<SessionReportMetricData>) {
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val columns = if (maxWidth >= 520.dp) 3 else 2
        Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
            metrics.chunked(columns).forEach { row ->
                Row(horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
                    row.forEach { metric -> SessionReportMetric(metric, Modifier.weight(1f)) }
                    repeat(columns - row.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
    }
}

@Composable
private fun SessionReportMetric(metric: SessionReportMetricData, modifier: Modifier = Modifier) {
    val notMeasured = stringResource(R.string.session_report_not_measured)
    Surface(
        modifier = modifier,
        color = PanelAlt,
        shape = RoundedCornerShape(OpenNowRadius.md),
    ) {
        Column(Modifier.padding(horizontal = OpenNowSpacing.md, vertical = 10.dp)) {
            Text(metric.label, color = TextMuted, style = MaterialTheme.typography.labelSmall, maxLines = 1)
            Text(
                metric.value ?: notMeasured,
                color = metric.quality?.tint() ?: TextPrimary,
                style = MaterialTheme.typography.bodyMedium.numeric(),
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                metric.detail.orEmpty(),
                color = TextMuted,
                style = MaterialTheme.typography.labelSmall.numeric(),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

private fun normalizeSessionReportResolution(value: String?): Pair<Int, Int>? =
    value?.let(::parseResolutionPixelsOrNull)


