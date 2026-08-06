package com.opencloudgaming.opennow


import android.app.Activity
import android.content.Context
import android.content.res.Configuration
import android.content.Intent
import android.hardware.input.InputManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.speech.RecognizerIntent
import android.view.InputDevice
import android.view.KeyEvent
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items as gridItems
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ElevatedButton
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Typography
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.ScaffoldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.material.icons.rounded.Wifi
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.pulltorefresh.PullToRefreshDefaults
import androidx.compose.material3.pulltorefresh.rememberPullToRefreshState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.key
import androidx.compose.runtime.setValue
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.FocusManager
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import coil3.compose.AsyncImage
import com.opencloudgaming.opennow.screens.tv.TvTypographyScheme
import com.opencloudgaming.opennow.ui.adaptive.CONTENT_COMPACT_MAX_WIDTH
import com.opencloudgaming.opennow.ui.adaptive.isAtLeastMedium
import com.opencloudgaming.opennow.ui.adaptive.windowSizeClassOf
import com.opencloudgaming.opennow.ui.adaptive.windowWidthSizeClassOf
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.io.File
import java.util.Locale
import kotlin.math.min
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowShapes
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import com.opencloudgaming.opennow.ui.theme.OpenNowTypography
import com.opencloudgaming.opennow.ui.theme.numeric
import com.opencloudgaming.opennow.ui.theme.tint
import kotlin.math.sin




// Green (was OpenNowScreens.kt:302)
internal val Green = OpenNowPalette.AccentDefault

// Background (was OpenNowScreens.kt:303)
internal val Background = OpenNowPalette.Background

// Panel (was OpenNowScreens.kt:304)
internal val Panel = OpenNowPalette.Panel

// PanelAlt (was OpenNowScreens.kt:305)
internal val PanelAlt = OpenNowPalette.PanelAlt

// TextPrimary (was OpenNowScreens.kt:306)
internal val TextPrimary = OpenNowPalette.TextPrimary

// TextMuted (was OpenNowScreens.kt:307)
internal val TextMuted = OpenNowPalette.TextMuted

// ChromeScrim (was OpenNowScreens.kt:308)
private val ChromeScrim = OpenNowPalette.ChromeScrim

// TopBarCompactControlHeight (was OpenNowScreens.kt:309)
internal val TopBarCompactControlHeight = 30.dp

// COMPACT_STREAM_DEVICE_STATUS_REFRESH_MS (was OpenNowScreens.kt:310)
internal const val COMPACT_STREAM_DEVICE_STATUS_REFRESH_MS = 5_000L

// QUEUE_POSITION_VISUAL_SETTLE_MS (was OpenNowScreens.kt:311)
internal const val QUEUE_POSITION_VISUAL_SETTLE_MS = 1100L

// ACTIVE_STREAM_MODE_NOTICE_DURATION_MS (was OpenNowScreens.kt:312)
internal const val ACTIVE_STREAM_MODE_NOTICE_DURATION_MS = 8_000L

// color (was OpenNowScreens.kt:313)
internal val UiAccent.color: Color
    get() = when (this) {
        UiAccent.OpenNow -> OpenNowPalette.AccentDefault
        UiAccent.Pixel -> OpenNowPalette.AccentPixel
        UiAccent.HotPink -> OpenNowPalette.AccentHotPink
        UiAccent.Lime -> OpenNowPalette.AccentLime
        UiAccent.Coral -> OpenNowPalette.AccentCoral
        UiAccent.Violet -> OpenNowPalette.AccentViolet
    }

// uiAccentLabel (was OpenNowScreens.kt:323)
@Composable
internal fun uiAccentLabel(accent: UiAccent): String = when (accent) {
    UiAccent.OpenNow -> stringResource(R.string.accent_opennow)
    UiAccent.Pixel -> stringResource(R.string.accent_pixel)
    UiAccent.HotPink -> stringResource(R.string.accent_hot_pink)
    UiAccent.Lime -> stringResource(R.string.accent_lime)
    UiAccent.Coral -> stringResource(R.string.accent_coral)
    UiAccent.Violet -> stringResource(R.string.accent_violet)
}

// OpenNowTheme (was OpenNowScreens.kt:333)
@Composable
fun OpenNowTheme(
    settings: AppSettings,
    typography: Typography = OpenNowTypography,
    content: @Composable () -> Unit,
) {
    val context = LocalContext.current
    val accent = settings.uiAccent.color
    val fallbackScheme = darkColorScheme(
        primary = accent,
        onPrimary = OpenNowPalette.OnAccent,
        background = Background,
        surface = Panel,
        surfaceVariant = PanelAlt,
        onBackground = TextPrimary,
        onSurface = TextPrimary,
        onSurfaceVariant = TextMuted,
        errorContainer = OpenNowPalette.ErrorContainer,
        onErrorContainer = OpenNowPalette.OnErrorContainer,
    )
    val colorScheme = if (settings.dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        dynamicDarkColorScheme(context).copy(
            primary = accent,
            onPrimary = OpenNowPalette.OnAccent,
            secondary = accent,
            tertiary = Green,
            errorContainer = OpenNowPalette.ErrorContainer,
            onErrorContainer = OpenNowPalette.OnErrorContainer,
        )
    } else {
        fallbackScheme
    }
    // Honour both the system-wide animation switch and the in-app toggle. Infinite transitions
    // (shimmer, focus pulse, carousel auto-advance) read this and stop entirely.
    val reduceMotion = remember(settings.controllerBackgroundAnimations, context) {
        val systemScale = runCatching {
            Settings.Global.getFloat(
                context.contentResolver,
                Settings.Global.ANIMATOR_DURATION_SCALE,
                1f,
            )
        }.getOrDefault(1f)
        systemScale == 0f || !settings.controllerBackgroundAnimations
    }
    CompositionLocalProvider(LocalReduceMotion provides reduceMotion) {
        MaterialTheme(
            colorScheme = colorScheme,
            typography = typography,
            shapes = OpenNowShapes,
            content = content,
        )
    }
}

// OpenNowApp (was OpenNowScreens.kt:387)
@Composable
fun OpenNowApp(viewModel: OpenNowViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val physicalControllerConnected = rememberPhysicalControllerConnected(enabled = !state.androidTvProfile)
    val controllerFocusEnabled = shouldShowControllerFocus(
        focused = true,
        tvProfile = state.androidTvProfile,
        physicalControllerConnected = physicalControllerConnected,
    )
    val launchAudioController = remember(context) { AndroidNerdAudioController(context.applicationContext) }
    val playIntroOnAppLaunch = remember { state.settings.streamIntroMusic }
    val introStartsMutedOnLaunch = remember {
        state.settings.streamIntroMusic && state.settings.streamIntroStartMode == IntroMusicStartMode.Muted
    }
    val musicControlsEnabled = state.settings.streamIntroMusic || state.settings.queueReadyMusic
    val streamActive = state.page == AppPage.Stream || state.streamStatus != "idle"
    var launchIntroStarted by remember { mutableStateOf(false) }
    var launchMusicMuted by remember { mutableStateOf(introStartsMutedOnLaunch) }
    var launchMusicPlaying by remember { mutableStateOf(false) }
    var previousStreamStatus by remember { mutableStateOf(state.streamStatus) }
    var queuedForStartCue by remember { mutableStateOf(false) }
    var lastStartCueSessionId by remember { mutableStateOf<String?>(null) }
    var hiddenUpdatePromptKey by remember { mutableStateOf<String?>(null) }
    var completedSessionBugReportOpen by rememberSaveable { mutableStateOf(false) }
    val updatePromptKey = state.androidUpdate.visibleNoticeKey(state.dismissedAndroidUpdateNoticeKey)
    val showAnalyticsConsent = !state.settings.analyticsConsentAsked
    val diagnosticDialogVisible = state.diagnosticShare.awaitingConsent ||
        state.diagnosticShare.uploading ||
        state.diagnosticShare.pasteUrl != null
    val showCompletedSessionBugReport = completedSessionBugReportOpen && !showAnalyticsConsent && !diagnosticDialogVisible
    val showSessionReport = state.sessionReport != null &&
        state.settings.showSessionReportAfterStream &&
        !showAnalyticsConsent &&
        !diagnosticDialogVisible &&
        !showCompletedSessionBugReport
    val showUpdatePrompt = updatePromptKey != null &&
        updatePromptKey != hiddenUpdatePromptKey &&
        !showAnalyticsConsent &&
        !showSessionReport &&
        !showCompletedSessionBugReport &&
        !diagnosticDialogVisible &&
        state.androidUpdate.status in setOf(AndroidUpdateStatus.Available, AndroidUpdateStatus.Downloaded)

    DisposableEffect(launchAudioController) {
        onDispose {
            launchAudioController.release()
        }
    }
    DisposableEffect(lifecycleOwner, launchAudioController) {
        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_PAUSE -> launchAudioController.pauseAll { launchMusicPlaying = it }
                Lifecycle.Event.ON_RESUME -> launchAudioController.resumeAll { launchMusicPlaying = it }
                else -> Unit
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose {
            lifecycleOwner.lifecycle.removeObserver(observer)
        }
    }
    LaunchedEffect(
        playIntroOnAppLaunch,
        state.settings.streamIntroMusic,
        state.settings.queueReadyMusic,
        launchMusicMuted,
        state.page,
        state.streamStatus,
        state.launchPhase,
        state.queuePosition,
        state.streamSession?.sessionId,
        state.streamSession?.queuePosition,
        state.streamSession?.seatSetupStep,
    ) {
        val sessionId = state.streamSession?.sessionId
        val queueReadyForStream =
            previousStreamStatus == "queue" &&
                state.streamStatus == "connecting" &&
                sessionId != null &&
                sessionId != lastStartCueSessionId
        previousStreamStatus = state.streamStatus
        if (state.streamStatus == "queue") {
            queuedForStartCue = queuedForStartCue ||
                queueDisplayPosition(state) != null ||
                state.launchPhase.equals("Queue", ignoreCase = true)
        }

        if (!musicControlsEnabled) {
            launchIntroStarted = false
            launchMusicMuted = false
            launchAudioController.stopAll { launchMusicPlaying = it }
            if (state.streamStatus == "idle") {
                queuedForStartCue = false
            }
            return@LaunchedEffect
        }
        if (!state.settings.streamIntroMusic && launchMusicMuted) {
            launchMusicMuted = false
        }
        if (!state.settings.streamIntroMusic) {
            launchAudioController.stopIntro { launchMusicPlaying = it }
        }
        if (!state.settings.queueReadyMusic) {
            launchAudioController.stopQueueReadyReminder { launchMusicPlaying = it }
        }

        if (!state.settings.streamIntroMusic && !state.settings.queueReadyMusic) {
            launchAudioController.stopAll { launchMusicPlaying = it }
        } else if (queueReadyForStream && queuedForStartCue) {
            launchMusicMuted = false
            lastStartCueSessionId = sessionId
            queuedForStartCue = false
            launchAudioController.startQueueReadyReminder(enabled = state.settings.queueReadyMusic) { launchMusicPlaying = it }
        } else if (launchMusicMuted) {
            launchAudioController.stopIntro { launchMusicPlaying = it }
        } else if (playIntroOnAppLaunch && state.settings.streamIntroMusic && !streamActive) {
            if (!launchIntroStarted) {
                launchIntroStarted = true
                launchAudioController.startIntro(enabled = true) { launchMusicPlaying = it }
            }
        } else {
            launchAudioController.stopIntro { launchMusicPlaying = it }
        }
        if (state.streamStatus == "idle") {
            queuedForStartCue = false
        }
    }
    val musicControl = TopBarMusicControl(
        visible = musicControlsEnabled,
        playing = launchMusicPlaying,
        muted = launchMusicMuted,
        onToggle = {
            when {
                launchMusicMuted -> {
                    launchMusicMuted = false
                    if (state.settings.streamIntroMusic && !streamActive) {
                        launchIntroStarted = true
                        launchAudioController.startIntro(enabled = true) { launchMusicPlaying = it }
                    }
                }
                launchMusicPlaying -> {
                    launchMusicMuted = true
                    launchAudioController.stopAll { launchMusicPlaying = it }
                }
                state.settings.streamIntroMusic && !streamActive -> {
                    launchIntroStarted = true
                    launchAudioController.startIntro(enabled = true) { launchMusicPlaying = it }
                }
                else -> {
                    launchMusicMuted = true
                    launchAudioController.stopAll { launchMusicPlaying = it }
                }
            }
        },
    )

    OpenNowTheme(
        settings = state.settings,
        // TV gets the TV Design Kit type scale (titles 32sp, body 24sp, actions 22sp) across every
        // screen — game details, settings, dialogs and stream chrome — without touching call sites.
        typography = if (state.androidTvProfile) TvTypographyScheme else OpenNowTypography,
    ) {
        val primaryColor = MaterialTheme.colorScheme.primary
        CompositionLocalProvider(
            LocalTvLoadingProfile provides state.androidTvProfile,
            LocalControllerFocusEnabled provides controllerFocusEnabled,
        ) {
            Box(
                Modifier
                    .fillMaxSize()
                    .background(MaterialTheme.colorScheme.background)
                    .drawWithCache {
                        val brush = Brush.radialGradient(
                            colors = listOf(
                                primaryColor.copy(alpha = 0.15f),
                                Color.Transparent
                            ),
                            center = Offset(size.width, 0f),
                            radius = size.width.coerceAtLeast(size.height) * 0.8f
                        )
                        onDrawBehind {
                            drawRect(brush)
                        }
                    }
            ) {
                Surface(Modifier.fillMaxSize(), color = Color.Transparent) {
                    when {
                        state.authSession != null -> MainShell(state, viewModel, musicControl)
                        else -> LoginScreen(state, viewModel)
                    }
                }
                state.sessionReport?.takeIf { showSessionReport }?.let { report ->
                    SessionReportDialog(
                        report = report,
                        onDismiss = { dontShowAgain ->
                            if (dontShowAgain) {
                                viewModel.updateSettings(
                                    state.settings.copy(showSessionReportAfterStream = false),
                                )
                            }
                            viewModel.dismissSessionReport()
                        },
                        onReportBug = { dontShowAgain ->
                            if (dontShowAgain) {
                                viewModel.updateSettings(
                                    state.settings.copy(showSessionReportAfterStream = false),
                                )
                            }
                            viewModel.resetBugReportSubmission()
                            completedSessionBugReportOpen = true
                        },
                    )
                }
                if (showCompletedSessionBugReport) {
                    CompletedSessionBugReportDialog(
                        submission = state.bugReportSubmission,
                        versionCheck = state.bugReportVersionCheck,
                        update = state.androidUpdate,
                        onSubmit = viewModel::submitBugReport,
                        onReset = viewModel::resetBugReportSubmission,
                        onVersionCheck = viewModel::verifyBugReportVersion,
                        onOpenUpdate = viewModel::performAndroidUpdatePrimaryAction,
                        onDismiss = {
                            if (!state.bugReportSubmission.uploading) {
                                completedSessionBugReportOpen = false
                                viewModel.dismissSessionReport()
                                viewModel.resetBugReportSubmission()
                            }
                        },
                    )
                }
                updatePromptKey?.takeIf { showUpdatePrompt }?.let { promptKey ->
                    AndroidUpdatePromptDialog(
                        update = state.androidUpdate,
                        onPrimary = {
                            hiddenUpdatePromptKey = promptKey
                            when (state.androidUpdate.status) {
                                AndroidUpdateStatus.Available -> viewModel.performAndroidUpdatePrimaryAction()
                                AndroidUpdateStatus.Downloaded -> viewModel.installAndroidUpdate()
                                else -> Unit
                            }
                        },
                        onDetails = {
                            hiddenUpdatePromptKey = promptKey
                            viewModel.openAndroidUpdateSettings()
                        },
                        onDismiss = viewModel::dismissAndroidUpdateNotice,
                    )
                }
                if (showAnalyticsConsent) {
                    AnalyticsConsentDialog(
                        onAllow = {
                            viewModel.updateSettings(
                                state.settings.copy(
                                    analyticsConsentAsked = true,
                                    analyticsOptOut = false,
                                ),
                            )
                        },
                        onDecline = {
                            viewModel.updateSettings(
                                state.settings.copy(
                                    analyticsConsentAsked = true,
                                    analyticsOptOut = true,
                                ),
                            )
                        },
                    )
                }
                DiagnosticShareDialog(
                    state = state,
                    onUpload = viewModel::uploadDiagnosticShare,
                    onDismiss = viewModel::dismissDiagnosticShare,
                )
            }
        }
    }
}

// SessionReportDialog (was OpenNowScreens.kt:668)
@Composable
private fun SessionReportDialog(
    report: SessionReport,
    onDismiss: (dontShowAgain: Boolean) -> Unit,
    onReportBug: (dontShowAgain: Boolean) -> Unit,
) {
    // Four tones for a 0-100 score was more colour than information, and AccentLime vs
    // AccentDefault is indistinguishable at the 0.12 alpha this fills with.
    val scoreColor = when (report.rating) {
        SessionReportRating.Excellent, SessionReportRating.Good -> OpenNowPalette.StatusGood
        SessionReportRating.Fair -> OpenNowPalette.StatusFair
        SessionReportRating.Poor -> OpenNowPalette.StatusPoor
    }
    val configuration = LocalConfiguration.current
    val landscapeLayout = configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
    var dontShowAgain by rememberSaveable(report.gameTitle, report.durationSeconds) { mutableStateOf(false) }
    AlertDialog(
        onDismissRequest = { onDismiss(dontShowAgain) },
        modifier = if (landscapeLayout) {
            Modifier.widthIn(max = 960.dp).fillMaxWidth(0.94f)
        } else {
            Modifier
        },
        properties = DialogProperties(usePlatformDefaultWidth = !landscapeLayout),
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
        confirmButton = { Button(onClick = { onDismiss(dontShowAgain) }) { Text(stringResource(R.string.session_report_done)) } },
    )
}

// SessionReportSummary (was OpenNowScreens.kt:755)
@Composable
private fun SessionReportSummary(report: SessionReport, scoreColor: Color) {
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

// SessionReportConnection (was OpenNowScreens.kt:799)
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
        AndroidNetworkKind.Wifi -> report.wifiBand.label
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

// SessionReportOutcome (was OpenNowScreens.kt:864)
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
            textDecoration = TextDecoration.Underline,
        )
    }
}

// CompletedSessionBugReportDialog (was OpenNowScreens.kt:902)
@Composable
private fun CompletedSessionBugReportDialog(
    submission: BugReportSubmissionState,
    versionCheck: AndroidBugReportVersionCheckState,
    update: AndroidUpdateState,
    onSubmit: (String, String) -> Unit,
    onReset: () -> Unit,
    onVersionCheck: () -> Unit,
    onOpenUpdate: () -> Unit,
    onDismiss: () -> Unit,
) {
    val configuration = LocalConfiguration.current
    val landscapeLayout = configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
    var title by rememberSaveable { mutableStateOf("") }
    var description by rememberSaveable { mutableStateOf("") }
    var consentChecked by rememberSaveable { mutableStateOf(false) }
    var confirmationOpen by rememberSaveable { mutableStateOf(false) }

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
        properties = DialogProperties(usePlatformDefaultWidth = !landscapeLayout),
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
                                .clip(RoundedCornerShape(OpenNowRadius.sm))
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

// SessionReportMetricData (was OpenNowScreens.kt:1080)
private data class SessionReportMetricData(
    val label: String,
    /** Null when the metric was never measured. */
    val value: String?,
    val detail: String?,
    val quality: StreamQualityLevel? = null,
)

/**
 * Six cards in an even two- or three-column grid.
 *
 * They used to be a FlowRow of fixed 136dp cards, which left a ragged right edge at every width
 * and, because `value` was unbounded while `detail` was capped at one line, let cards in the same
 * row end up different heights.
 */

// SessionReportMetricGrid (was OpenNowScreens.kt:1095)
@Composable
private fun SessionReportMetricGrid(metrics: List<SessionReportMetricData>) {
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val columns = if (maxWidth >= CONTENT_COMPACT_MAX_WIDTH) 3 else 2
        Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
            metrics.chunked(columns).forEach { row ->
                Row(horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
                    row.forEach { metric -> SessionReportMetric(metric, Modifier.weight(1f)) }
                    // Six items divide evenly into 2 and 3, so this is defensive only.
                    repeat(columns - row.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
    }
}

// SessionReportMetric (was OpenNowScreens.kt:1111)
@Composable
private fun SessionReportMetric(metric: SessionReportMetricData, modifier: Modifier = Modifier) {
    val notMeasured = stringResource(R.string.session_report_not_measured)
    Surface(
        modifier = modifier,
        color = PanelAlt,
        shape = RoundedCornerShape(OpenNowRadius.md),
    ) {
        // A fixed three-line structure keeps every card the same height without an intrinsics
        // pass, which would be a second measure inside an already-scrolling dialog.
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
            // Rendered even when absent so the line box is still reserved.
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

// SessionReportFindingRow (was OpenNowScreens.kt:1143)
@Composable
private fun SessionReportFindingRow(finding: SessionReportFinding) {
    val titleColor = if (finding.kind == SessionReportFindingKind.Warning) OpenNowPalette.StatusFair else Green
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(finding.title, color = titleColor, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
        Text(finding.detail, color = TextMuted, style = MaterialTheme.typography.bodySmall)
    }
}

// normalizeSessionReportResolution (was OpenNowScreens.kt:1152)
private fun normalizeSessionReportResolution(value: String?): Pair<Int, Int>? =
    value?.let(::parseResolutionPixelsOrNull)

// DiagnosticShareDialog (was OpenNowScreens.kt:1155)
@Composable
private fun DiagnosticShareDialog(
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
            val qrCode = remember(share.pasteUrl) { QrCode.encodeText(share.pasteUrl) }
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

// AnalyticsConsentDialog (was OpenNowScreens.kt:1228)
@Composable
private fun AnalyticsConsentDialog(
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

// AndroidUpdatePromptDialog (was OpenNowScreens.kt:1261)
@Composable
private fun AndroidUpdatePromptDialog(
    update: AndroidUpdateState,
    onPrimary: () -> Unit,
    onDetails: () -> Unit,
    onDismiss: () -> Unit,
) {
    val version = update.availableVersionName?.let { "Version $it" }
        ?: update.availableVersionCode?.let { "Build $it" }
        ?: "A new build"
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (update.status == AndroidUpdateStatus.Downloaded) "Update ready" else "OpenNOW update available") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(
                    if (update.status == AndroidUpdateStatus.Downloaded) {
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
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        },
        confirmButton = {
            Button(onClick = onPrimary) {
                Text(
                    when {
                        update.status == AndroidUpdateStatus.Downloaded -> "Install"
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

// secondsUntil (was OpenNowScreens.kt:1928)
internal fun secondsUntil(deadlineMs: Long): Int =
    ((deadlineMs - System.currentTimeMillis()).coerceAtLeast(0L) / 1000L).toInt()

// isPhoneLandscape (was OpenNowScreens.kt:1931)
internal fun isPhoneLandscape(width: androidx.compose.ui.unit.Dp, height: androidx.compose.ui.unit.Dp): Boolean =
    width > height && windowSizeClassOf(width, height).isPhone

// rememberPhysicalControllerConnected (was OpenNowScreens.kt:1935)
@Composable
internal fun rememberPhysicalControllerConnected(enabled: Boolean): Boolean {
    return rememberPhysicalControllerFamily(enabled) != null
}

// rememberPhysicalControllerFamily (was OpenNowScreens.kt:1940)
@Composable
internal fun rememberPhysicalControllerFamily(enabled: Boolean): AndroidControllerFamily? {
    val context = LocalContext.current.applicationContext
    var family by remember { mutableStateOf(connectedPhysicalControllerFamily().takeIf { enabled }) }
    DisposableEffect(context, enabled) {
        fun refresh() {
            family = connectedPhysicalControllerFamily().takeIf { enabled }
        }
        refresh()
        if (!enabled) {
            onDispose {}
        } else {
            val inputManager = context.getSystemService(Context.INPUT_SERVICE) as? InputManager
            val listener = object : InputManager.InputDeviceListener {
                override fun onInputDeviceAdded(deviceId: Int) = refresh()
                override fun onInputDeviceRemoved(deviceId: Int) = refresh()
                override fun onInputDeviceChanged(deviceId: Int) = refresh()
            }
            inputManager?.registerInputDeviceListener(listener, null)
            onDispose {
                inputManager?.unregisterInputDeviceListener(listener)
            }
        }
    }
    return family
}

// connectedPhysicalControllerFamily (was OpenNowScreens.kt:1967)
private fun connectedPhysicalControllerFamily(): AndroidControllerFamily? {
    val families = InputDevice.getDeviceIds()
        .asSequence()
        .mapNotNull { deviceId -> AndroidControllerInput.controllerFamily(InputDevice.getDevice(deviceId)) }
        .toList()
    return families.firstOrNull { it != AndroidControllerFamily.Generic } ?: families.firstOrNull()
}

// CatalogWallpaperSelection (was OpenNowScreens.kt:1975)
internal sealed interface CatalogWallpaperSelection {
    data class BuiltIn(val preset: CatalogBackgroundPreset) : CatalogWallpaperSelection
    data class Custom(val source: String) : CatalogWallpaperSelection
}

// MainShell (was OpenNowScreens.kt:2058)
@Composable
private fun MainShell(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    musicControl: TopBarMusicControl,
) {
    val context = LocalContext.current
    val inStream = state.page == AppPage.Stream
    val streamingActive = inStream && state.streamStatus != "idle"
    val modalPickerOpen = state.pendingPrintedWasteGame != null || state.pendingStoreChoiceGame != null
    val tvProfile = state.androidTvProfile
    val physicalControllerConnected = rememberPhysicalControllerConnected(enabled = !tvProfile)
    val navAudioController = remember(context) { AndroidNerdAudioController(context.applicationContext) }
    var visibleSearchTarget by remember { mutableStateOf<SearchTarget?>(null) }
    var settingsSearchQuery by remember { mutableStateOf("") }
    var settingsDetailRouteOpen by remember { mutableStateOf(false) }
    var settingsBackRequestToken by remember { mutableStateOf(0) }
    val tvStreamReturnFocusRequester = remember { FocusRequester() }
    var previouslyInStream by remember { mutableStateOf(inStream) }
    val navigationToneEnabled = state.settings.controllerUiSounds && !inStream
    val showMinimizedQueueDock = state.streamLaunchMinimized && shouldShowQueueLaunchStatus(state)
    DisposableEffect(navAudioController) {
        onDispose {
            navAudioController.release()
        }
    }
    LaunchedEffect(state.page) {
        if (state.page != AppPage.Settings) {
            settingsDetailRouteOpen = false
        }
    }
    LaunchedEffect(inStream, tvProfile) {
        val shouldRestoreFocus = shouldRestoreTvNavigationFocus(
            previouslyInStream = previouslyInStream,
            currentlyInStream = inStream,
            tvProfile = tvProfile,
        )
        previouslyInStream = inStream
        if (shouldRestoreFocus) {
            delay(120)
            repeat(3) { attempt ->
                if (runCatching { tvStreamReturnFocusRequester.requestFocus() }.isSuccess) {
                    return@LaunchedEffect
                }
                if (attempt < 2) delay(80)
            }
        }
    }
    fun revealSearch(
        target: SearchTarget = when (state.page) {
            AppPage.Library -> SearchTarget.Library
            AppPage.Settings -> SearchTarget.Settings
            else -> SearchTarget.Store
        },
    ) {
        visibleSearchTarget = target
        if (target == SearchTarget.Store && state.page != AppPage.Home) {
            viewModel.setPage(AppPage.Home)
        } else if (target == SearchTarget.Library && state.page != AppPage.Library) {
            viewModel.setPage(AppPage.Library)
        } else if (target == SearchTarget.Settings && state.page != AppPage.Settings) {
            viewModel.setPage(AppPage.Settings)
        }
    }
    fun navigateFromAppChrome(page: AppPage) {
        if (page == AppPage.Settings) {
            viewModel.recordSettingsIconTap()
        }
        visibleSearchTarget = null
        viewModel.setPage(page)
    }
    BackHandler(enabled = state.selectedGame != null && !inStream) {
        viewModel.clearSelectedGame()
    }
    BackHandler(
        enabled = (tvProfile || physicalControllerConnected) &&
            !inStream &&
            state.selectedGame == null &&
            state.page != AppPage.Home,
    ) {
        viewModel.setPage(AppPage.Home)
    }
    BoxWithConstraints(Modifier.fillMaxSize()) {
        var phoneLandscapeScrollChromeHidden by remember { mutableStateOf(false) }
        val horizontalChrome = maxWidth > maxHeight
        val phoneLandscapeChrome = !tvProfile && !inStream && isPhoneLandscape(maxWidth, maxHeight)
        val portraitChrome = !inStream && maxHeight >= maxWidth
        // Material 3 adaptive navigation: medium+ screens (tablets, foldables, large phones in
        // landscape) get the NavigationRail even in portrait, instead of the compact bottom bar.
        val tabletChrome = !tvProfile && !inStream && windowWidthSizeClassOf(maxWidth).isAtLeastMedium
        val showNavigationRail = !inStream && (tvProfile || phoneLandscapeChrome || tabletChrome)
        val scrollChromePage = state.page == AppPage.Home || state.page == AppPage.Library
        val tvCatalogChrome = tvProfile && scrollChromePage
        val storeControlsInTopBar = (phoneLandscapeChrome || tvCatalogChrome) && state.page == AppPage.Home
        val libraryControlsInTopBar = (phoneLandscapeChrome || tvCatalogChrome) && state.page == AppPage.Library
        val screenEdgePadding = appContentEdgePaddingDp(
            settings = state.settings,
            inStream = inStream,
            tvProfile = tvProfile,
        ).dp
        LaunchedEffect(phoneLandscapeChrome, scrollChromePage) {
            if (!phoneLandscapeChrome || !scrollChromePage) {
                phoneLandscapeScrollChromeHidden = false
            }
        }
        Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background))
        if (!inStream && (state.page == AppPage.Home || state.page == AppPage.Library)) {
            CatalogWallpaperBackdrop(
                settings = state.settings,
                tvProfile = tvProfile,
                width = maxWidth,
                height = maxHeight,
            )
        }

        Scaffold(
            containerColor = Color.Transparent,
            contentWindowInsets = if (streamingActive || tvProfile) WindowInsets(0, 0, 0, 0) else ScaffoldDefaults.contentWindowInsets,
            bottomBar = {
                if (!inStream && !showNavigationRail) {
                    Column {
                        if (showMinimizedQueueDock) {
                            MinimizedQueueDock(
                                state = state,
                                onRestore = viewModel::restoreStreamLaunch,
                                onCancel = viewModel::stopStream,
                            )
                        }
                        NavigationBar(
                            containerColor = if (state.page == AppPage.Settings) SettingsBackground else MaterialTheme.colorScheme.background,
                            tonalElevation = 0.dp,
                        ) {
                            BottomNavItem(
                                selected = state.page == AppPage.Home,
                                onClick = {
                                    visibleSearchTarget = null
                                    viewModel.setPage(AppPage.Home)
                                },
                                iconRes = R.drawable.ic_tab_store,
                                label = stringResource(R.string.nav_store),
                            )
                            BottomNavItem(
                                // Search is a mode, not a destination: it never claims selection.
                                selected = false,
                                onClick = { revealSearch() },
                                iconRes = R.drawable.ic_search,
                                label = stringResource(R.string.nav_search),
                            )
                            BottomNavItem(
                                selected = state.page == AppPage.Library,
                                onClick = {
                                    visibleSearchTarget = null
                                    viewModel.setPage(AppPage.Library)
                                },
                                iconRes = R.drawable.ic_tab_library,
                                label = stringResource(R.string.nav_library),
                            )
                            BottomNavItem(
                                selected = state.page == AppPage.Settings,
                                onClick = {
                                    navigateFromAppChrome(AppPage.Settings)
                                },
                                iconRes = R.drawable.ic_tab_settings,
                                label = stringResource(R.string.nav_settings),
                            )
                        }
                    }
                }
            },
        ) { padding ->
            Box(
                Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .onPreviewKeyEvent { event ->
                        if (isNavigationToneKey(event)) {
                            navAudioController.playButtonTone(navigationToneEnabled)
                        }
                        false
                    },
            ) {
                Row(
                    Modifier
                        .fillMaxSize()
                        .padding(screenEdgePadding),
                ) {
                    if (showNavigationRail) {
                        AppNavigationRail(
                            state = state,
                            activeSearchTarget = visibleSearchTarget,
                            showAppIcon = showNavigationRail && horizontalChrome,
                            largeIcons = phoneLandscapeChrome,
                            showSettingsBack = shouldShowSettingsBackRail(
                                tvProfile = tvProfile,
                                settingsPageOpen = state.page == AppPage.Settings,
                                horizontalChrome = horizontalChrome,
                                detailRouteOpen = settingsDetailRouteOpen,
                            ),
                            showCatalogControllerActions = false,
                            onNavigate = { page ->
                                navigateFromAppChrome(page)
                            },
                            onSearch = { revealSearch(it) },
                            onSettingsBack = { settingsBackRequestToken += 1 },
                            streamReturnFocusRequester = tvStreamReturnFocusRequester,
                        )
                    }
                    Column(
                        Modifier
                            .weight(1f)
                            .fillMaxHeight(),
                    ) {
                        AnimatedVisibility(
                            visible =
                                portraitChrome ||
                                    (phoneLandscapeChrome && !phoneLandscapeScrollChromeHidden) ||
                                    tvCatalogChrome,
                        ) {
                            if (!inStream) {
                                TopStatusBar(
                                    state = state,
                                    onResumeActiveSession = viewModel::resumeActiveSession,
                                    onOpenStreamSettings = viewModel::openStreamSettings,
                                    musicControl = musicControl,
                                    showLogo = portraitChrome,
                                ) {
                                    if (storeControlsInTopBar) {
                                        StoreCatalogToolbar(
                                            state = state,
                                            onSortChange = viewModel::setCatalogSort,
                                            onFilterToggle = viewModel::toggleCatalogFilter,
                                            modifier = Modifier.widthIn(max = 220.dp),
                                            compact = true,
                                        )
                                    } else if (libraryControlsInTopBar) {
                                        val orderedLibraryGames = remember(state.libraryGames, state.settings.favoriteGameIds) {
                                            favoriteOrderedGames(state.libraryGames, state.settings.favoriteGameIds)
                                        }
                                        val visibleLibraryGames = remember(orderedLibraryGames, state.librarySearch, state.libraryFilterIds) {
                                            orderedLibraryGames.filter { game ->
                                                gameMatchesSearch(game, state.librarySearch) &&
                                                    gameMatchesLibraryFilters(game, state.libraryFilterIds)
                                            }
                                        }
                                        val libraryFilterOptions = remember(orderedLibraryGames) {
                                            libraryStoreFilterOptions(orderedLibraryGames)
                                        }
                                        LibraryFilterControls(
                                            gameCount = visibleLibraryGames.size,
                                            totalCount = state.libraryGames.size,
                                            options = libraryFilterOptions,
                                            selectedIds = state.libraryFilterIds,
                                            onToggle = viewModel::toggleLibraryFilter,
                                            modifier = Modifier.widthIn(max = 190.dp),
                                            compact = true,
                                            showSelectedChips = false,
                                        )
                                    }
                                }
                            }
                        }
                        Box(
                            Modifier
                                .weight(1f)
                                .fillMaxWidth(),
                        ) {
                            when (state.page) {
                                AppPage.Home -> HomeScreen(
                                    state = state,
                                    viewModel = viewModel,
                                    tvProfile = tvProfile,
                                    hideChromeWhenScrolled = phoneLandscapeChrome,
                                    controlsInTopBar = storeControlsInTopBar,
                                    searchRequested = visibleSearchTarget == SearchTarget.Store,
                                    onSearchDismissed = {
                                        if (visibleSearchTarget == SearchTarget.Store) visibleSearchTarget = null
                                    },
                                    onScrollChromeHiddenChange = { phoneLandscapeScrollChromeHidden = it },
                                )
                                AppPage.Library -> LibraryScreen(
                                    state = state,
                                    viewModel = viewModel,
                                    tvProfile = tvProfile,
                                    hideChromeWhenScrolled = phoneLandscapeChrome,
                                    controlsInTopBar = libraryControlsInTopBar,
                                    searchRequested = visibleSearchTarget == SearchTarget.Library,
                                    onSearchDismissed = {
                                        if (visibleSearchTarget == SearchTarget.Library) visibleSearchTarget = null
                                    },
                                    onScrollChromeHiddenChange = { phoneLandscapeScrollChromeHidden = it },
                                )
                                AppPage.Settings -> SettingsScreen(
                                    state = state,
                                    viewModel = viewModel,
                                    tvProfile = tvProfile,
                                    searchRequested = visibleSearchTarget == SearchTarget.Settings,
                                    searchQuery = settingsSearchQuery,
                                    backRequestToken = settingsBackRequestToken,
                                    onSearchQueryChange = { next ->
                                        settingsSearchQuery = next
                                        if (next.isBlank() && visibleSearchTarget == SearchTarget.Settings) {
                                            visibleSearchTarget = null
                                        }
                                    },
                                    onDetailRouteChange = { settingsDetailRouteOpen = it },
                                )
                                // Keep the in-stream chrome (controls panel over live video, status
                                // readouts) at the dense phone type scale even on TV — it overlays
                                // gameplay and must not push fixed-height rows off-screen.
                                AppPage.Stream -> MaterialTheme(typography = OpenNowTypography) {
                                    StreamScreen(state, viewModel)
                                }
                            }
                        }
                        if (showMinimizedQueueDock && showNavigationRail) {
                            MinimizedQueueDock(
                                state = state,
                                onRestore = viewModel::restoreStreamLaunch,
                                onCancel = viewModel::stopStream,
                            )
                        }
                    }
                }
                AnimatedVisibility(
                    visible = state.selectedGame != null && !inStream && !modalPickerOpen,
                    enter = fadeIn() + slideInVertically(initialOffsetY = { it / 3 }) + scaleIn(initialScale = 0.96f),
                    exit = fadeOut() + slideOutVertically(targetOffsetY = { it / 3 }) + scaleOut(targetScale = 0.96f),
                    modifier = Modifier.align(Alignment.Center),
                ) {
                    state.selectedGame?.let { game ->
                        GameDetailsSheet(
                            game = game,
                            favorite = game.id in state.settings.favoriteGameIds,
                            defaultVariantId = state.settings.defaultGameVariantIds[game.id],
                            fullScreen = tvProfile,
                            safeAreaPadding = screenEdgePadding,
                            onPlay = viewModel::play,
                            onChooseStore = viewModel::chooseStore,
                            onFavorite = viewModel::updateFavorites,
                            connectedTvName = state.localTvConnector.connectedTvName,
                            onPlayOnTv = viewModel::playOnLocalTv,
                            onDismiss = viewModel::clearSelectedGame,
                            similarGames = similarGamesFor(
                                game = game,
                                catalog = state.games.ifEmpty { state.catalogResult.games },
                            ),
                            onSelectGame = viewModel::selectGame,
                        )
                    }
                }
                state.pendingPrintedWasteGame?.let { game ->
                    AnimatedLaunchOverlay(Modifier.align(Alignment.Center)) {
                        PrintedWasteSelector(state, game, viewModel)
                    }
                }
                state.pendingStoreChoiceGame?.let { game ->
                    AnimatedLaunchOverlay(Modifier.align(Alignment.Center)) {
                        StoreLaunchSelector(
                            game = game,
                            defaultVariantId = state.settings.defaultGameVariantIds[game.id],
                            onLaunch = viewModel::playVariant,
                            onSetDefaultStore = viewModel::setDefaultGameVariant,
                            onDismiss = viewModel::dismissStoreChoice,
                        )
                    }
                }
            }
        }
    }
}

// shouldRestoreTvNavigationFocus (was OpenNowScreens.kt:2429)
internal fun shouldRestoreTvNavigationFocus(
    previouslyInStream: Boolean,
    currentlyInStream: Boolean,
    tvProfile: Boolean,
): Boolean = tvProfile && previouslyInStream && !currentlyInStream

// AppNavigationRail (was OpenNowScreens.kt:2435)
@Composable
private fun AppNavigationRail(
    state: OpenNowUiState,
    activeSearchTarget: SearchTarget?,
    showAppIcon: Boolean,
    largeIcons: Boolean,
    showSettingsBack: Boolean,
    showCatalogControllerActions: Boolean,
    onNavigate: (AppPage) -> Unit,
    onSearch: (SearchTarget) -> Unit,
    onSettingsBack: () -> Unit,
    streamReturnFocusRequester: FocusRequester,
) {
    Box(
        modifier = Modifier
            .width(APP_NAV_RAIL_WIDTH)
            .fillMaxHeight()
            .padding(start = 6.dp, top = 8.dp, end = 6.dp, bottom = 8.dp),
    ) {
        Surface(
            modifier = Modifier.fillMaxSize(),
            shape = RoundedCornerShape(26.dp),
            color = ChromeScrim,
            tonalElevation = 0.dp,
            shadowElevation = 0.dp,
        ) {
            BoxWithConstraints(Modifier.fillMaxSize()) {
                val canFitCatalogControllerActions = maxHeight >= 440.dp
                if (showAppIcon) {
                    Box(
                        modifier = Modifier
                            .align(Alignment.TopCenter)
                            .fillMaxWidth()
                            .focusProperties { canFocus = false }
                            .clickable { onNavigate(AppPage.Home) }
                            .padding(top = 12.dp, bottom = 8.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        OpenNowAppIcon(
                            if (largeIcons) 44.dp else 34.dp,
                        )
                    }
                }
                Column(
                    modifier = Modifier
                        .align(if (showSettingsBack) Alignment.BottomCenter else Alignment.Center)
                        .fillMaxWidth()
                        .padding(bottom = if (showSettingsBack) 8.dp else 0.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    if (!showAppIcon) {
                        Spacer(Modifier.height(8.dp))
                    }
                    AppNavigationRailItem(
                        selected = state.page == AppPage.Home,
                        onClick = { onNavigate(AppPage.Home) },
                        iconRes = R.drawable.ic_tab_store,
                        label = stringResource(R.string.nav_store),
                        iconSize = if (largeIcons) 30.dp else 24.dp,
                        focusRequester = streamReturnFocusRequester,
                    )
                    AppNavigationRailItem(
                        // See the bottom bar: search is a mode, not a destination.
                        selected = false,
                        onClick = {
                            onSearch(
                                when (state.page) {
                                    AppPage.Library -> SearchTarget.Library
                                    AppPage.Settings -> SearchTarget.Settings
                                    else -> SearchTarget.Store
                                },
                            )
                        },
                        iconRes = R.drawable.ic_search,
                        label = stringResource(R.string.nav_search),
                        iconSize = if (largeIcons) 30.dp else 24.dp,
                    )
                    AppNavigationRailItem(
                        selected = state.page == AppPage.Library,
                        onClick = { onNavigate(AppPage.Library) },
                        iconRes = R.drawable.ic_tab_library,
                        label = stringResource(R.string.nav_library),
                        iconSize = if (largeIcons) 30.dp else 24.dp,
                    )
                    AppNavigationRailItem(
                        selected = state.page == AppPage.Settings,
                        onClick = { onNavigate(AppPage.Settings) },
                        iconRes = R.drawable.ic_tab_settings,
                        label = stringResource(R.string.nav_settings),
                        iconSize = if (largeIcons) 30.dp else 24.dp,
                        showConnectionDot = shouldShowLocalTvConnectionDot(
                            tvProfile = state.androidTvProfile,
                            pairedDeviceName = state.localTvConnector.pairedDeviceName,
                        ),
                    )
                    AnimatedVisibility(visible = showCatalogControllerActions && canFitCatalogControllerActions) {
                        Column(horizontalAlignment = Alignment.CenterHorizontally) {
                            Spacer(Modifier.height(8.dp))
                            ControllerCatalogRailActionHints()
                        }
                    }
                    AnimatedVisibility(visible = showSettingsBack) {
                        Column(horizontalAlignment = Alignment.CenterHorizontally) {
                            Spacer(Modifier.height(6.dp))
                            AppNavigationRailItem(
                                selected = false,
                                onClick = onSettingsBack,
                                iconRes = R.drawable.ic_arrow_back,
                                label = "Back",
                                iconSize = if (largeIcons) 30.dp else 24.dp,
                            )
                        }
                    }
                }
            }
        }
    }
}

// shouldShowLocalTvConnectionDot (was OpenNowScreens.kt:2554)
internal fun shouldShowLocalTvConnectionDot(tvProfile: Boolean, pairedDeviceName: String?): Boolean =
    tvProfile && !pairedDeviceName.isNullOrBlank()

// shouldShowSettingsBackRail (was OpenNowScreens.kt:2557)
internal fun shouldShowSettingsBackRail(
    tvProfile: Boolean,
    settingsPageOpen: Boolean,
    horizontalChrome: Boolean,
    detailRouteOpen: Boolean,
): Boolean = !tvProfile && settingsPageOpen && horizontalChrome && detailRouteOpen

// AppNavigationRailItem (was OpenNowScreens.kt:2564)
@Composable
private fun AppNavigationRailItem(
    selected: Boolean,
    onClick: () -> Unit,
    iconRes: Int,
    label: String,
    modifier: Modifier = Modifier,
    iconSize: Dp = 24.dp,
    focusRequester: FocusRequester? = null,
    showConnectionDot: Boolean = false,
) {
    var focused by remember { mutableStateOf(false) }
    val accent = MaterialTheme.colorScheme.primary
    val contentColor = when {
        focused -> Color.White
        selected -> accent
        else -> TextMuted
    }
    Surface(
        onClick = onClick,
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 5.dp, vertical = 2.dp)
            .onFocusChanged { focused = it.isFocused }
            .then(focusRequester?.let { Modifier.focusRequester(it) } ?: Modifier),
        shape = RoundedCornerShape(18.dp),
        color = if (selected && !focused) accent.copy(alpha = 0.10f) else Color.Transparent,
        border = if (focused) BorderStroke(2.dp, Color.White.copy(alpha = 0.96f)) else null,
        tonalElevation = 0.dp,
        shadowElevation = 0.dp,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 58.dp)
                .padding(horizontal = 4.dp, vertical = 6.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            Icon(
                painter = painterResource(iconRes),
                contentDescription = label,
                tint = contentColor,
                modifier = Modifier.size(iconSize),
            )
            if (showConnectionDot) {
                Spacer(Modifier.height(2.dp))
                Box(
                    Modifier
                        .size(6.dp)
                        .clip(CircleShape)
                        .background(Color(0xffb56cff)),
                )
            }
            Spacer(Modifier.height(2.dp))
            Text(
                label,
                color = contentColor,
                style = MaterialTheme.typography.labelSmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

// TopBarMusicControl (was OpenNowScreens.kt:2630)
private data class TopBarMusicControl(
    val visible: Boolean,
    val playing: Boolean,
    val muted: Boolean,
    val onToggle: () -> Unit,
)

// BottomNavItem (was OpenNowScreens.kt:2637)
@Composable
private fun RowScope.BottomNavItem(selected: Boolean, onClick: () -> Unit, iconRes: Int, label: String) {
    NavigationBarItem(
        selected = selected,
        onClick = onClick,
        colors = NavigationBarItemDefaults.colors(
            selectedIconColor = MaterialTheme.colorScheme.primary,
            selectedTextColor = MaterialTheme.colorScheme.primary,
            indicatorColor = MaterialTheme.colorScheme.primary.copy(alpha = 0.18f),
            unselectedIconColor = TextMuted,
            unselectedTextColor = TextMuted,
        ),
        icon = {
            Icon(
                painter = painterResource(iconRes),
                contentDescription = null,
                modifier = Modifier.size(24.dp),
            )
        },
        label = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
    )
}

// TopStatusBar (was OpenNowScreens.kt:2660)
@Composable
private fun TopStatusBar(
    state: OpenNowUiState,
    onResumeActiveSession: () -> Unit,
    onOpenStreamSettings: () -> Unit,
    musicControl: TopBarMusicControl,
    showLogo: Boolean = true,
    content: @Composable RowScope.() -> Unit = {},
) {
    val displayName = state.authSession?.user?.displayName ?: "OpenNOW"
    val tier = state.subscriptionInfo?.membershipTier ?: state.authSession?.user?.membershipTier ?: "GFN"
    val barScrim = if (showLogo) ChromeScrim else Color.Transparent
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = 8.dp, top = 8.dp, end = 8.dp, bottom = 5.dp),
        shape = RoundedCornerShape(24.dp),
        color = barScrim,
        tonalElevation = 0.dp,
        shadowElevation = 0.dp,
    ) {
        Row(
            Modifier.padding(horizontal = 12.dp, vertical = 7.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (showLogo) {
                OpenNowMark(30.dp)
                Spacer(Modifier.width(8.dp))
            }
            Row(
                Modifier.weight(1f),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    listOf(displayName, tier).filter { it.isNotBlank() }.joinToString(" • "),
                    color = TextPrimary,
                    fontWeight = FontWeight.SemiBold,
                    style = MaterialTheme.typography.labelLarge,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                content()
                if (state.settings.nerdMode) {
                    TopStatusDetails(state, onOpenStreamSettings)
                }
            }
            if (musicControl.visible) {
                Spacer(Modifier.width(6.dp))
                TopBarMusicButton(musicControl)
            }
            if (state.activeSession != null) {
                Spacer(Modifier.width(6.dp))
                ElevatedButton(
                    onClick = onResumeActiveSession,
                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 7.dp),
                ) {
                    Text(stringResource(R.string.action_resume), style = MaterialTheme.typography.labelMedium)
                }
            }
        }
    }
}

// TopStatusDetails (was OpenNowScreens.kt:2725)
@Composable
private fun TopStatusDetails(
    state: OpenNowUiState,
    onOpenStreamSettings: () -> Unit,
) {
    val stream = state.activeStreamSettings ?: state.settings.stream
    val summary = streamStatusSummary(stream)
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(999.dp)
    Row(
        horizontalArrangement = Arrangement.spacedBy(5.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Surface(
            modifier = Modifier
                .height(TopBarCompactControlHeight)
                .onFocusChanged { focused = it.isFocused }
                .semantics { contentDescription = "Open Stream settings: $summary" }
                .clickable(onClick = onOpenStreamSettings)
                .then(
                    if (focused) Modifier.border(2.dp, MaterialTheme.colorScheme.primary, shape) else Modifier,
                ),
            shape = shape,
            color = if (focused) MaterialTheme.colorScheme.primary.copy(alpha = 0.22f) else PanelAlt.copy(alpha = 0.9f),
            tonalElevation = 0.dp,
        ) {
            Box(Modifier.fillMaxHeight().padding(horizontal = 8.dp), contentAlignment = Alignment.Center) {
                Text(
                    summary,
                    color = TextMuted,
                    style = MaterialTheme.typography.labelSmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

// TopBarMusicButton (was OpenNowScreens.kt:2764)
@Composable
private fun TopBarMusicButton(control: TopBarMusicControl) {
    val description = when {
        control.muted -> "Music muted"
        control.playing -> "Music playing"
        else -> "Music ready"
    }
    Surface(
        modifier = Modifier
            .width(38.dp)
            .height(TopBarCompactControlHeight)
            .semantics { contentDescription = description }
            .clickable(onClick = control.onToggle),
        shape = RoundedCornerShape(999.dp),
        color = if (control.muted) OpenNowPalette.ErrorContainer.copy(alpha = 0.92f) else PanelAlt.copy(alpha = 0.78f),
        tonalElevation = 0.dp,
    ) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            if (control.muted) {
                Icon(
                    painter = painterResource(R.drawable.ic_volume_off),
                    contentDescription = null,
                    tint = OpenNowPalette.OnErrorContainer,
                    modifier = Modifier.size(17.dp),
                )
            } else {
                MusicBars(playing = control.playing)
            }
        }
    }
}

// MusicBars (was OpenNowScreens.kt:2796)
@Composable
private fun MusicBars(playing: Boolean, modifier: Modifier = Modifier) {
    val transition = rememberInfiniteTransition(label = "top-bar-music-bars")
    val phase by transition.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 820, easing = LinearEasing),
            repeatMode = RepeatMode.Restart,
        ),
        label = "top-bar-music-bars-phase",
    )
    val color = MaterialTheme.colorScheme.primary
    Canvas(modifier.size(width = 18.dp, height = 16.dp)) {
        val barWidth = size.width / 5.8f
        val gap = (size.width - barWidth * 3f) / 2f
        repeat(3) { index ->
            val wave = if (playing) {
                ((sin((phase.toDouble() * 6.283185307179586) + index * 1.35) + 1.0) / 2.0).toFloat()
            } else {
                0.36f + index * 0.12f
            }
            val barHeight = size.height * (0.32f + wave * 0.58f)
            val left = index * (barWidth + gap)
            drawRoundRect(
                color = color,
                topLeft = Offset(left, size.height - barHeight),
                size = Size(barWidth, barHeight),
                cornerRadius = CornerRadius(barWidth, barWidth),
            )
        }
    }
}

// streamStatusSummary (was OpenNowScreens.kt:2830)
private fun streamStatusSummary(stream: StreamSettings): String =
    listOf(
        formatTopBarResolution(stream.resolution),
        stream.aspectRatio,
        stream.codec.name,
        "${stream.fps} FPS",
    ).filter { it.isNotBlank() }.joinToString(" • ")

// formatTopBarResolution (was OpenNowScreens.kt:2838)
private fun formatTopBarResolution(resolution: String): String {
    val parts = resolution.lowercase(Locale.US).split("x", limit = 2)
    return if (parts.size == 2 && parts.all { it.trim().isNotBlank() }) {
        "${parts[0].trim()} × ${parts[1].trim()}"
    } else {
        resolution
    }
}

// NativeSearchField (was OpenNowScreens.kt:2847)
@Composable
internal fun NativeSearchField(
    query: String,
    onQueryChange: (String) -> Unit,
    placeholder: String,
    searching: Boolean = false,
    modifier: Modifier = Modifier,
    focusRequester: FocusRequester? = null,
    onOpen: (() -> Unit)? = null,
) {
    val focusManager = LocalFocusManager.current
    val speechLauncher = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        if (result.resultCode == Activity.RESULT_OK) {
            val spoken = result.data
                ?.getStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS)
                ?.firstOrNull()
            if (!spoken.isNullOrBlank()) onQueryChange(spoken)
        }
    }
    val voiceSearchIntent = remember(placeholder) {
        Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
            .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            .putExtra(RecognizerIntent.EXTRA_LANGUAGE, Locale.getDefault())
            .putExtra(RecognizerIntent.EXTRA_PROMPT, placeholder)
    }
    Surface(
        modifier = modifier.height(56.dp),
        shape = RoundedCornerShape(28.dp),
        color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.72f),
        tonalElevation = 2.dp,
    ) {
        Row(
            Modifier
                .fillMaxSize()
                .padding(start = 18.dp, end = if (query.isBlank()) 18.dp else 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(
                painter = painterResource(R.drawable.ic_search),
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.size(22.dp),
            )
            BasicTextField(
                value = query,
                onValueChange = onQueryChange,
                singleLine = true,
                textStyle = MaterialTheme.typography.bodyLarge.copy(color = MaterialTheme.colorScheme.onSurface),
                cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                modifier = Modifier
                    .weight(1f)
                    .then(focusRequester?.let { Modifier.focusRequester(it) } ?: Modifier)
                    .onFocusChanged { if (it.isFocused) onOpen?.invoke() }
                    .onPreviewKeyEvent { handleDpadFocusMove(it, focusManager) },
                decorationBox = { innerTextField ->
                    Box(Modifier.fillMaxWidth()) {
                        if (query.isBlank()) {
                            Text(
                                placeholder,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        innerTextField()
                    }
                },
            )
            if (query.isNotBlank()) {
                if (searching) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(18.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.primary,
                    )
                }
                IconButton(onClick = { onQueryChange("") }) {
                    Icon(
                        painter = painterResource(R.drawable.ic_clear),
                        contentDescription = stringResource(R.string.search_clear),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.size(26.dp),
                    )
                }
            } else {
                if (searching) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(18.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.primary,
                    )
                }
                IconButton(onClick = { runCatching { speechLauncher.launch(voiceSearchIntent) } }) {
                    Icon(
                        painter = painterResource(R.drawable.ic_mic),
                        contentDescription = stringResource(R.string.search_voice),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.size(22.dp),
                    )
                }
            }
        }
    }
}

// handleDpadFocusMove (was OpenNowScreens.kt:2953)
internal fun handleDpadFocusMove(event: androidx.compose.ui.input.key.KeyEvent, focusManager: FocusManager): Boolean {
    if (event.type != KeyEventType.KeyDown) return false
    val direction = when (event.key) {
        Key.DirectionUp -> FocusDirection.Up
        Key.DirectionDown -> FocusDirection.Down
        Key.DirectionLeft -> FocusDirection.Left
        Key.DirectionRight -> FocusDirection.Right
        else -> return false
    }
    return focusManager.moveFocus(direction)
}

// lockedFocusGroup (was OpenNowScreens.kt:2965)
internal fun Modifier.lockedFocusGroup(): Modifier =
    focusProperties { onExit = { cancelFocusChange() } }
        .focusGroup()

// isNavigationToneKey (was OpenNowScreens.kt:2969)
private fun isNavigationToneKey(event: androidx.compose.ui.input.key.KeyEvent): Boolean =
    event.type == KeyEventType.KeyDown &&
        event.key in setOf(
            Key.DirectionUp,
            Key.DirectionDown,
            Key.DirectionLeft,
            Key.DirectionRight,
        )

// handleVerticalDpadFocusMove (was OpenNowScreens.kt:2978)
internal fun handleVerticalDpadFocusMove(event: androidx.compose.ui.input.key.KeyEvent, focusManager: FocusManager): Boolean {
    if (event.type != KeyEventType.KeyDown) return false
    val direction = when (event.key) {
        Key.DirectionUp -> FocusDirection.Up
        Key.DirectionDown -> FocusDirection.Down
        else -> return false
    }
    return focusManager.moveFocus(direction)
}

/**
 * Key event handler for Compose Sliders when navigated by TV remote or D-pad controller.
 * - D-pad Up/Down  → moves focus to the next/previous focusable element.
 * - D-pad Left     → decrements the slider value by [step], clamped to [min].
 * - D-pad Right    → increments the slider value by [step], clamped to [max].
 * Returns true when the event is consumed (Left/Right) so that Compose does not
 * move focus sideways instead of changing the value.
 */

// handleSliderDpadInput (was OpenNowScreens.kt:2996)
internal fun handleSliderDpadInput(
    event: androidx.compose.ui.input.key.KeyEvent,
    value: Float,
    min: Float,
    max: Float,
    step: Float,
    focusManager: FocusManager,
    onValueAdjusted: (Float) -> Unit,
): Boolean {
    if (event.type != KeyEventType.KeyDown) return false
    return when (event.key) {
        Key.DirectionUp -> focusManager.moveFocus(FocusDirection.Up)
        Key.DirectionDown -> focusManager.moveFocus(FocusDirection.Down)
        Key.DirectionLeft -> {
            val newValue = (value - step).coerceIn(min, max)
            onValueAdjusted(newValue)
            true
        }
        Key.DirectionRight -> {
            val newValue = (value + step).coerceIn(min, max)
            onValueAdjusted(newValue)
            true
        }
        else -> false
    }
}

// isTvActivateKey (was OpenNowScreens.kt:3023)
internal fun isTvActivateKey(event: androidx.compose.ui.input.key.KeyEvent): Boolean =
    event.type == KeyEventType.KeyUp &&
        event.key in setOf(
            Key.DirectionCenter,
            Key.Enter,
            Key.NumPadEnter,
        )

// LocalShimmerOffset (was OpenNowScreens.kt:3595)
private val LocalShimmerOffset = staticCompositionLocalOf<State<Float>?> { null }

// LocalTvLoadingPulse (was OpenNowScreens.kt:3596)
private val LocalTvLoadingPulse = staticCompositionLocalOf<State<Float>?> { null }

// LocalTvLoadingProfile (was OpenNowScreens.kt:3597)
private val LocalTvLoadingProfile = staticCompositionLocalOf { false }

// LocalTouchControllerStyle (was OpenNowScreens.kt:3598)
internal val LocalTouchControllerStyle = staticCompositionLocalOf { TouchControllerStyle.V1 }

// SHIMMER_CYCLE_DURATION_MS (was OpenNowScreens.kt:3599)
private const val SHIMMER_CYCLE_DURATION_MS = 760

// GameGridSkeleton (was OpenNowScreens.kt:3601)
@Composable
internal fun GameGridSkeleton(
    settings: AppSettings,
    tvProfile: Boolean,
    storeLayout: Boolean,
    modifier: Modifier = Modifier,
) {
    val scale = settings.posterSizeScale.coerceIn(MIN_GAME_CARD_SCALE, MAX_GAME_CARD_SCALE)
    val compact = settings.compactGameCards
    val landscapeLayout = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    val physicalControllerConnected = rememberPhysicalControllerConnected(enabled = landscapeLayout && !tvProfile)
    val artworkOnly = shouldUseArtworkOnlyCatalogCards(
        tvProfile = tvProfile,
        controllerActionMode = landscapeLayout && !tvProfile && physicalControllerConnected,
    )

    val shimmerOffset: State<Float>?
    val tvPulse: State<Float>?
    // Under reduced motion the skeletons still show — they just stop animating. A never-ending
    // sweep is exactly the kind of movement the setting exists to stop.
    if (LocalReduceMotion.current) {
        shimmerOffset = null
        tvPulse = null
    } else if (tvProfile) {
        val transition = rememberInfiniteTransition(label = "loading-pulse-global")
        val pulse = transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 900, easing = LinearEasing),
                repeatMode = RepeatMode.Reverse,
            ),
            label = "loading-pulse-global",
        )
        shimmerOffset = null
        tvPulse = pulse
    } else {
        val transition = rememberInfiniteTransition(label = "shimmer-global")
        val shimmer = transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = SHIMMER_CYCLE_DURATION_MS, easing = LinearEasing),
            ),
            label = "shimmer-offset-global",
        )
        shimmerOffset = shimmer
        tvPulse = null
    }

    CompositionLocalProvider(
        LocalShimmerOffset provides shimmerOffset,
        LocalTvLoadingPulse provides tvPulse,
    ) {
        BoxWithConstraints(modifier.fillMaxSize()) {
            val gridSpec = gameGridSpec(maxWidth, compact, landscapeLayout, settings, handheldLayout = !tvProfile)
            val placeholderItems = remember(gridSpec.estimatedColumns, storeLayout) {
                List(gridSpec.estimatedColumns * if (storeLayout) 4 else 3) { it }
            }
            LazyVerticalGrid(
                modifier = Modifier.fillMaxSize(),
                columns = gridSpec.cells,
                contentPadding = gridSpec.contentPadding,
                horizontalArrangement = Arrangement.spacedBy(gridSpec.horizontalSpacing),
                verticalArrangement = Arrangement.spacedBy(gridSpec.verticalSpacing),
                userScrollEnabled = false,
            ) {
                if (storeLayout) {
                    item(span = { GridItemSpan(maxLineSpan) }) {
                        StoreStartRailsSkeleton(
                            settings = settings,
                            tvProfile = tvProfile,
                        )
                    }
                }
                gridItems(placeholderItems, key = { it }) {
                    GameCardSkeleton(
                        squareCard = gridSpec.squareCards,
                        thumbnailFavoriteOverlay = !artworkOnly && !tvProfile,
                        showStoreLabels = !artworkOnly && shouldShowGameStoreLabels(
                            tvProfile = tvProfile,
                            enabled = settings.showGameStoreLabels,
                        ),
                        showCardTitles = !artworkOnly && shouldShowCatalogCardTitles(
                            tvProfile = tvProfile,
                            enabled = settings.showCardTitles,
                        ),
                    )
                }
            }
        }
    }
}

// StoreStartRailsSkeleton (was OpenNowScreens.kt:3695)
@Composable
private fun StoreStartRailsSkeleton(
    settings: AppSettings,
    tvProfile: Boolean,
) {
    val landscapeLayout = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    Column(
        Modifier
            .fillMaxWidth()
            .padding(top = 2.dp, bottom = 6.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        repeat(2) {
            StoreRailSectionSkeleton(
                expressiveUi = settings.expressiveUi,
                tvProfile = tvProfile,
                landscapeLayout = landscapeLayout,
                cardScale = settings.posterSizeScale,
            )
        }
    }
}

// StoreRailSectionSkeleton (was OpenNowScreens.kt:3718)
@Composable
private fun StoreRailSectionSkeleton(
    expressiveUi: Boolean,
    tvProfile: Boolean,
    landscapeLayout: Boolean,
    cardScale: Float,
) {
    val spacing = 10.dp
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SkeletonLine(widthFraction = 0.34f, height = 15.dp)
        BoxWithConstraints(
            Modifier
                .fillMaxWidth()
                .clipToBounds(),
        ) {
            val baseCardWidth = storeRailCardWidth(tvProfile, landscapeLayout)
            val visibleCount = storeRailVisibleCardCount(
                availableWidthDp = maxWidth.value,
                baseCardWidthDp = baseCardWidth.value,
                spacingDp = spacing.value,
                cardScale = cardScale,
            )
            val fittedCardWidth = ((maxWidth.value - spacing.value * (visibleCount - 1)) / visibleCount)
                .coerceAtLeast(1f)
                .dp
            val mediaCard = !tvProfile && windowWidthSizeClassOf(maxWidth).isAtLeastMedium
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(spacing),
            ) {
                repeat(visibleCount) {
                    StoreRailGameCardSkeleton(
                        width = fittedCardWidth,
                        expressiveUi = expressiveUi,
                        portraitCard = !tvProfile,
                        mediaCard = mediaCard,
                    )
                }
            }
        }
    }
}

// StoreRailGameCardSkeleton (was OpenNowScreens.kt:3761)
@Composable
private fun StoreRailGameCardSkeleton(
    width: Dp,
    expressiveUi: Boolean,
    portraitCard: Boolean,
    mediaCard: Boolean,
) {
    val shape = RoundedCornerShape(if (expressiveUi) 12.dp else 8.dp)
    Surface(
        modifier = Modifier
            .width(width)
            .then(
                // Mirror the real card: the media form gets art + caption, so no whole-card ratio.
                if (mediaCard) Modifier
                else Modifier.aspectRatio(if (portraitCard) GAME_BOX_ART_ASPECT_RATIO else 1f),
            )
            .border(1.dp, Color.White.copy(alpha = 0.08f), shape),
        shape = shape,
        color = Color.Black,
        tonalElevation = 0.dp,
        shadowElevation = 1.dp,
    ) {
        Column(Modifier.fillMaxWidth().clip(shape)) {
            Box(
                Modifier
                    .fillMaxWidth()
                    .then(
                        if (mediaCard) {
                            Modifier.aspectRatio(if (portraitCard) GAME_BOX_ART_ASPECT_RATIO else 1f)
                        } else {
                            Modifier.fillMaxSize()
                        },
                    ),
            ) {
                LoadingShimmer(Modifier.fillMaxSize())
                if (portraitCard) {
                    SkeletonCircle(
                        size = 44.dp,
                        modifier = Modifier
                            .align(Alignment.BottomStart)
                            .padding(6.dp),
                    )
                }
            }
            if (mediaCard) {
                // Caption placeholder — two lines the height of the real media-card caption.
                Column(
                    Modifier
                        .fillMaxWidth()
                        .padding(horizontal = OpenNowSpacing.lg, vertical = OpenNowSpacing.md),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    SkeletonLine(widthFraction = 0.85f, height = 14.dp)
                    SkeletonLine(widthFraction = 0.5f, height = 11.dp)
                }
            }
        }
    }
}

/** Mirrors [GameCard]'s layout exactly, so nothing shifts when real content replaces it. */

// GameCardSkeleton (was OpenNowScreens.kt:3822)
@Composable
private fun GameCardSkeleton(
    squareCard: Boolean,
    thumbnailFavoriteOverlay: Boolean,
    showStoreLabels: Boolean,
    showCardTitles: Boolean,
) {
    val cardShape = RoundedCornerShape(OpenNowRadius.md)
    Column(Modifier.fillMaxWidth()) {
        Card(
            modifier = Modifier
                .fillMaxWidth()
                .then(
                    if (squareCard) Modifier.aspectRatio(1f)
                    else Modifier.aspectRatio(GAME_BOX_ART_ASPECT_RATIO),
                ),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.58f)),
            shape = cardShape,
        ) {
            Box(Modifier.fillMaxSize()) {
                LoadingShimmer(Modifier.fillMaxSize())
                if (thumbnailFavoriteOverlay) {
                    SkeletonCircle(
                        size = 34.dp,
                        modifier = Modifier
                            .align(Alignment.TopStart)
                            .padding(6.dp),
                    )
                }
            }
        }
        if (showCardTitles || showStoreLabels) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .padding(top = OpenNowSpacing.sm),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                if (showCardTitles) {
                    SkeletonLine(widthFraction = 0.86f)
                    SkeletonLine(widthFraction = 0.52f)
                }
                if (showStoreLabels) {
                    SkeletonLine(widthFraction = 0.4f)
                }
            }
        }
    }
}

// SkeletonLine (was OpenNowScreens.kt:3872)
@Composable
private fun SkeletonLine(widthFraction: Float, height: Dp = 9.dp) {
    LoadingShimmer(
        Modifier
            .fillMaxWidth(widthFraction)
            .height(height)
            .clip(RoundedCornerShape(999.dp)),
    )
}

// SkeletonCircle (was OpenNowScreens.kt:3882)
@Composable
private fun SkeletonCircle(size: Dp, modifier: Modifier = Modifier) {
    LoadingShimmer(
        modifier
            .size(size)
            .clip(CircleShape),
    )
}

// SwipeToRefreshContainer (was OpenNowScreens.kt:3891)
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun SwipeToRefreshContainer(
    refreshing: Boolean,
    onRefresh: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    showRefreshIndicator: Boolean = true,
    content: @Composable () -> Unit,
) {
    if (!enabled) {
        Box(modifier) {
            content()
        }
        return
    }
    val pullRefreshState = rememberPullToRefreshState()
    PullToRefreshBox(
        isRefreshing = refreshing,
        onRefresh = onRefresh,
        modifier = modifier,
        state = pullRefreshState,
        indicator = {
            if (showRefreshIndicator) {
                PullToRefreshDefaults.Indicator(
                    state = pullRefreshState,
                    isRefreshing = refreshing,
                    modifier = Modifier.align(Alignment.TopCenter),
                )
            }
        },
    ) {
        content()
    }
}

// UrlImageState (was OpenNowScreens.kt:14661)
private sealed interface UrlImageState {
    data object Empty : UrlImageState
    data object Loading : UrlImageState
    data object Failed : UrlImageState
    data object Loaded : UrlImageState
}

// imageDataForSource (was OpenNowScreens.kt:14668)
internal fun imageDataForSource(source: String): Any? {
    val key = source.trim()
    if (key.isBlank()) return null
    val uri = runCatching { Uri.parse(key) }.getOrNull() ?: return null
    val scheme = uri.scheme.orEmpty().lowercase(Locale.US)
    return when {
        scheme == "http" || scheme == "https" -> key
        scheme == "content" || scheme == "android.resource" || scheme == "file" -> uri
        scheme.isBlank() && key.startsWith("/") -> File(key)
        else -> uri
    }
}

// UrlImage (was OpenNowScreens.kt:14681)
@Composable
internal fun UrlImage(
    url: String?,
    modifier: Modifier = Modifier,
    fallbackUrl: String? = null,
    contentScale: ContentScale = ContentScale.Crop,
) {
    val source = url?.trim().orEmpty()
    val fallbackSource = fallbackUrl?.trim()?.takeIf { it.isNotBlank() && it != source }
    var activeSource by remember(source, fallbackSource) {
        mutableStateOf(source.takeIf { it.isNotBlank() } ?: fallbackSource)
    }
    var imageState by remember(source, fallbackSource) {
        mutableStateOf(if (activeSource == null) UrlImageState.Empty else UrlImageState.Loading)
    }
    val imageData = remember(activeSource) { activeSource?.let(::imageDataForSource) }
    LaunchedEffect(activeSource, imageData, fallbackSource, source) {
        if (activeSource == null) {
            imageState = UrlImageState.Empty
        } else if (imageData == null) {
            if (activeSource == source && fallbackSource != null) {
                activeSource = fallbackSource
                imageState = UrlImageState.Loading
            } else {
                imageState = UrlImageState.Failed
            }
        }
    }
    Box(modifier.background(OpenNowPalette.ImagePlaceholder), contentAlignment = Alignment.Center) {
        if (imageData != null) {
            key(activeSource) {
                AsyncImage(
                    model = imageData,
                    contentDescription = null,
                    modifier = Modifier.fillMaxSize(),
                    contentScale = contentScale,
                    onLoading = { imageState = UrlImageState.Loading },
                    onSuccess = { imageState = UrlImageState.Loaded },
                    onError = {
                        if (activeSource == source && fallbackSource != null) {
                            activeSource = fallbackSource
                            imageState = UrlImageState.Loading
                        } else {
                            imageState = UrlImageState.Failed
                        }
                    },
                )
            }
        }
        when (imageState) {
            UrlImageState.Loading -> LoadingShimmer(Modifier.fillMaxSize())
            UrlImageState.Loaded -> Unit
            UrlImageState.Empty,
            UrlImageState.Failed,
            -> OpenNowMark(42.dp)
        }
    }
}

// LoadingShimmer (was OpenNowScreens.kt:14740)
@Composable
private fun LoadingShimmer(modifier: Modifier = Modifier) {
    // Use the shared shimmer offset from GameGridSkeleton if available; fall back to a
    // local animation only when LoadingShimmer is used outside a GameGridSkeleton context.
    // Using nullable avoids treating 0f (a valid animation start value) as "not provided".
    val reduceMotion = LocalReduceMotion.current
    val sharedPulse = LocalTvLoadingPulse.current
    val localPulse = if (!reduceMotion && LocalTvLoadingProfile.current && sharedPulse == null) {
        val transition = rememberInfiniteTransition(label = "loading-pulse-local")
        val pulse = transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 900, easing = LinearEasing),
                repeatMode = RepeatMode.Reverse,
            ),
            label = "loading-pulse-local",
        )
        pulse
    } else {
        null
    }
    val pulse = sharedPulse ?: localPulse
    // Same rule as the shared driver above: no perpetual sweep under reduced motion.
    val shimmer = LocalShimmerOffset.current ?: if (pulse == null && !reduceMotion) run {
        val transition = rememberInfiniteTransition(label = "shimmer-local")
        val localOffset = transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = SHIMMER_CYCLE_DURATION_MS, easing = LinearEasing),
            ),
            label = "shimmer-offset-local",
        )
        localOffset
    } else null
    val baseColor = OpenNowPalette.ShimmerBase
    val highlightColor1 = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.32f)
    val highlightColor2 = MaterialTheme.colorScheme.primary.copy(alpha = 0.18f)

    Spacer(
        modifier = modifier
            .background(baseColor)
            .drawBehind {
                if (pulse != null) {
                    drawRect(highlightColor1.copy(alpha = 0.08f + pulse.value * 0.18f))
                } else {
                    val width = size.width
                    val height = size.height
                    // Keep the highlight narrow and move it fully beyond both edges. The
                    // repeat seam then joins two identical base-color frames instead of
                    // visibly snapping a full-card gradient back to the beginning.
                    val bandWidth = (width * 0.52f).coerceAtLeast(1f)
                    val bandCenter = -2f * bandWidth + (shimmer?.value ?: 0f) * (width + 4f * bandWidth)
                    val brush = Brush.linearGradient(
                        colors = listOf(
                            Color.Transparent,
                            highlightColor1,
                            highlightColor2,
                            highlightColor1,
                            Color.Transparent,
                        ),
                        start = Offset(bandCenter - bandWidth, -height),
                        end = Offset(bandCenter + bandWidth, height * 2f),
                    )
                    drawRect(brush)
                }
            }
    )
}

// OpenNowMark (was OpenNowScreens.kt:14811)
@Composable
internal fun OpenNowMark(size: androidx.compose.ui.unit.Dp, modifier: Modifier = Modifier) {
    Image(
        painter = painterResource(R.drawable.opennow_logo_mark),
        contentDescription = "OpenNOW",
        modifier = modifier
            .width(size * 1.85f)
            .height(size),
        contentScale = ContentScale.Fit,
    )
}

// OpenNowAppIcon (was OpenNowScreens.kt:14823)
@Composable
private fun OpenNowAppIcon(size: androidx.compose.ui.unit.Dp) {
    Image(
        painter = painterResource(R.drawable.opennow_icon),
        contentDescription = "OpenNOW",
        modifier = Modifier.size(size),
        contentScale = ContentScale.Fit,
    )
}

// label (was OpenNowScreens.kt:14833)
internal val ColorQuality.label: String
    get() = when (this) {
        ColorQuality.EightBit420 -> "8-bit 4:2:0"
        ColorQuality.EightBit444 -> "8-bit 4:4:4"
        ColorQuality.TenBit420 -> "10-bit 4:2:0"
        ColorQuality.TenBit444 -> "10-bit 4:4:4"
    }

// GameCardOverlayGradient (was OpenNowScreens.kt:14841)
internal val GameCardOverlayGradient = Brush.verticalGradient(
    colors = listOf(Color.Transparent, Color.Transparent, Color.Black.copy(alpha = 0.95f))
)
