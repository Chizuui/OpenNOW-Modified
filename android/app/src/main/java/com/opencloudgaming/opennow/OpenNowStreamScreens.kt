package com.opencloudgaming.opennow


import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.annotation.StringRes
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.PointerIcon
import android.view.View
import android.view.ViewGroup
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material.icons.automirrored.rounded.BatteryUnknown
import androidx.compose.material.icons.rounded.Battery0Bar
import androidx.compose.material.icons.rounded.Battery1Bar
import androidx.compose.material.icons.rounded.Battery2Bar
import androidx.compose.material.icons.rounded.Battery3Bar
import androidx.compose.material.icons.rounded.Battery4Bar
import androidx.compose.material.icons.rounded.Battery5Bar
import androidx.compose.material.icons.rounded.Battery6Bar
import androidx.compose.material.icons.rounded.BatteryFull
import androidx.compose.material.icons.rounded.Bolt
import androidx.compose.material.icons.rounded.SignalCellular0Bar
import androidx.compose.material.icons.rounded.SignalCellular4Bar
import androidx.compose.material.icons.rounded.SignalCellularAlt
import androidx.compose.material.icons.rounded.SignalCellularAlt1Bar
import androidx.compose.material.icons.rounded.SignalCellularAlt2Bar
import androidx.compose.material.icons.rounded.SignalWifi0Bar
import androidx.compose.material.icons.rounded.Wifi
import androidx.compose.material.icons.rounded.Wifi1Bar
import androidx.compose.material.icons.rounded.Wifi2Bar
import androidx.compose.material.icons.rounded.WifiOff
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.key
import androidx.compose.runtime.setValue
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.input.key.key
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.util.Locale
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import com.opencloudgaming.opennow.ui.theme.numeric
import com.opencloudgaming.opennow.ui.theme.tint
import kotlin.math.roundToInt
import kotlin.math.sqrt




@Composable
internal fun StreamScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val context = LocalContext.current
    val activity = context as? Activity
    val view = LocalView.current
    val audioController = remember(context) { AndroidNerdAudioController(context.applicationContext) }
    val session = state.streamSession
    val game = state.streamGame
    var streamState by remember { mutableStateOf("Preparing") }
    var initialVideoFrameRendered by remember(session?.sessionId) { mutableStateOf(false) }
    val markInitialVideoFrameRendered by rememberUpdatedState<() -> Unit> {
        initialVideoFrameRendered = true
    }
    var controlsOpen by remember { mutableStateOf(false) }
    var exitConfirmOpen by remember { mutableStateOf(false) }
    var keyboardOpen by remember { mutableStateOf(false) }
    var keyboardText by remember { mutableStateOf("") }
    var audioMuted by remember { mutableStateOf(false) }
    var touchLayoutEditing by remember { mutableStateOf(false) }
    var streamGuideOpen by remember(session?.sessionId) { mutableStateOf(false) }
    var streamGuideStep by remember(session?.sessionId) { mutableStateOf(StreamGuideStep.OpenControls) }
    var statsVisible by remember(state.settings.showStatsOnLaunch) { mutableStateOf(state.settings.showStatsOnLaunch) }
    var streamStats by remember { mutableStateOf(StreamRuntimeStats()) }
    var videoTransportFallbackReason by remember { mutableStateOf<String?>(null) }
    var controllerMouseAssistEnabled by remember(session?.sessionId) { mutableStateOf(false) }
    var controllerMouseEmulationEnabled by remember(session?.sessionId) { mutableStateOf(state.settings.controllerMouseEmulation) }
    val streamReady = state.isNativeStreamReady()
    val tvProfile = state.androidTvProfile
    LaunchedEffect(session?.sessionId) {
        videoTransportFallbackReason = null
    }
    val physicalControllerConnected = rememberPhysicalControllerConnected(enabled = streamReady)
    var showTouchControlsWithPhysicalController by remember(session?.sessionId) { mutableStateOf(false) }
    var preferVirtualController by remember(session?.sessionId) { mutableStateOf(false) }
    var physicalControllerPromptOpen by remember(session?.sessionId) { mutableStateOf(false) }
    var physicalControllerPromptHandled by remember(session?.sessionId) { mutableStateOf(false) }
    var physicalControllerPromptDoNotShowAgain by remember(session?.sessionId) { mutableStateOf(false) }
    val touchInputEnabled = !state.androidPictureInPictureActive
    val touchControlsSuppressedByPhysicalController =
        physicalControllerConnected &&
            state.settings.androidTouch.enabled &&
            !showTouchControlsWithPhysicalController
    val builtInGameTouchSupported = !tvProfile && game?.let(::catalogClaimsTouchSupport) == true
    val nativeTouchAvailable = !tvProfile && shouldUseNativeTouch(
        state.settings.androidTouch.nativeTouchMode,
        game,
        state.activeStreamSettings ?: state.settings.stream,
    )
    // Native game touch and the virtual controller need exclusive ownership of the same fingers.
    // Catalog touch remains the default, while a player's in-session controller choice wins.
    val nativeTouchActive = !tvProfile && shouldUseNativeTouchForStream(
        state.settings.androidTouch.nativeTouchMode,
        game,
        state.activeStreamSettings ?: state.settings.stream,
        preferVirtualController = preferVirtualController,
    )
    val touchControlsVisible = shouldShowAndroidTouchControls(
        tvProfile = tvProfile,
        touchInputEnabled = touchInputEnabled,
        touchControlsEnabled = state.settings.androidTouch.enabled,
        suppressedByPhysicalController = touchControlsSuppressedByPhysicalController,
    ) && !nativeTouchActive
    val fallbackSessionStartedAtMs = remember(session?.sessionId) { System.currentTimeMillis() }
    val sessionStartedAtMs = session?.timerStartedAtMs ?: fallbackSessionStartedAtMs
    var timerNowMs by remember(session?.sessionId) { mutableStateOf(System.currentTimeMillis()) }
    val smartSessionLimit = smartSessionLimitFor(state.subscriptionInfo, state.authSession?.user?.membershipTier)
    val buttonToneEnabled = state.settings.controllerUiSounds
    val stretchToFit = state.settings.stretchStreamToFit
    val playButtonTone = {
        audioController.playButtonTone(buttonToneEnabled)
    }
    val launchStreamSettings = state.activeStreamSettings ?: state.settings.stream
    // activeStreamSettings tracks the transport profile and can deliberately
    // change during safe-codec recovery. Keep the original launch profile so
    // requested, server-selected, decoded, and recovery modes remain distinct.
    val requestedStreamSettings = remember(session?.sessionId) { launchStreamSettings }
    val microphoneRequested = launchStreamSettings.microphoneMode != MicrophoneMode.Disabled
    val initialMicrophonePermissionGranted = remember(session?.sessionId, microphoneRequested) {
        !microphoneRequested ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) ==
            PackageManager.PERMISSION_GRANTED
    }
    var microphonePermissionGranted by remember(session?.sessionId, microphoneRequested) {
        mutableStateOf(initialMicrophonePermissionGranted)
    }
    var microphonePermissionResolved by remember(session?.sessionId, microphoneRequested) {
        mutableStateOf(!microphoneRequested || initialMicrophonePermissionGranted)
    }
    var microphoneEnabled by remember(session?.sessionId) { mutableStateOf(false) }
    val microphonePermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        microphonePermissionGranted = granted
        microphonePermissionResolved = true
        if (!granted) {
            Toast.makeText(
                context,
                context.getString(R.string.settings_microphone_permission_denied),
                Toast.LENGTH_LONG,
            ).show()
        }
    }
    val streamSettings = launchStreamSettings.copy(
        mouseSensitivity = state.settings.stream.mouseSensitivity,
        mouseAcceleration = state.settings.stream.mouseAcceleration,
        streamSharpeningEnabled = launchStreamSettings.streamSharpeningEnabled && state.settings.stream.streamSharpeningEnabled,
        streamSharpeningAmount = state.settings.stream.streamSharpeningAmount,
        mouseScrollSensitivity = state.settings.stream.mouseScrollSensitivity,
    )
    val statsAlignment = when (state.settings.streamStatsPosition) {
        StreamStatsPosition.Left -> Alignment.TopStart
        StreamStatsPosition.Center -> Alignment.TopCenter
        StreamStatsPosition.Right -> Alignment.TopEnd
    }
    val dismissStreamGuide = {
        streamGuideOpen = false
        if (!state.settings.androidStreamGuideDismissed) {
            viewModel.updateSettings(state.settings.copy(androidStreamGuideDismissed = true))
        }
    }
    val openControlsForGuide = {
        keyboardOpen = false
        exitConfirmOpen = false
        physicalControllerPromptOpen = false
        if (streamGuideOpen && streamGuideStep == StreamGuideStep.OpenControls) {
            streamGuideStep = StreamGuideStep.PressDone
        }
        controlsOpen = true
    }
    LaunchedEffect(state.remoteStreamMenuRequestToken) {
        if (state.remoteStreamMenuRequestToken > 0 && streamReady) {
            openControlsForGuide()
        }
    }
    LaunchedEffect(state.remoteStatsToggleRequestToken) {
        if (state.remoteStatsToggleRequestToken > 0 && streamReady) {
            statsVisible = !statsVisible
        }
    }
    val streamOverlayOpen = controlsOpen || exitConfirmOpen || keyboardOpen || streamGuideOpen || physicalControllerPromptOpen || touchLayoutEditing
    val externalMousePassthroughActive = streamReady && !streamOverlayOpen
    val handleStreamBack = {
        when {
            streamGuideOpen && streamGuideStep == StreamGuideStep.OpenControls -> openControlsForGuide()
            streamGuideOpen && streamGuideStep == StreamGuideStep.PressDone && controlsOpen -> {
                controlsOpen = false
                dismissStreamGuide()
            }
            streamGuideOpen -> dismissStreamGuide()
            exitConfirmOpen -> exitConfirmOpen = false
            keyboardOpen -> keyboardOpen = false
            physicalControllerPromptOpen -> physicalControllerPromptOpen = false
            controlsOpen -> controlsOpen = false
            else -> controlsOpen = true
        }
    }
    BackHandler(enabled = streamReady) {
        handleStreamBack()
    }
    val client = remember {
        NativeStreamClient(
            context = context.applicationContext,
            onState = {
                streamState = it
                viewModel.recordNativeStreamState(it)
                if (it == "Streaming") viewModel.markStreamConnected()
            },
            onError = {
                streamState = it
                viewModel.markStreamError(it)
            },
            onVideoTransportFallbackApplied = { reason, fallback ->
                streamState = reason
                videoTransportFallbackReason = reason
                viewModel.recordLocalVideoTransportFallback(reason, fallback)
            },
            onSessionRecoveryRequired = {
                streamState = it
                viewModel.recoverStreamSession(it)
            },
            onFirstVideoFrameRendered = {
                markInitialVideoFrameRendered()
            },
            onStats = {
                streamStats = it
                viewModel.updateStreamRuntimeStats(it)
            },
            onControllerMouseAssistChanged = {
                controllerMouseAssistEnabled = it
            },
            onStreamStopped = {
                viewModel.stopStream()
            },
        )
    }

    DisposableEffect(Unit) {
        val decor = activity?.window?.decorView
        NativeStreamInputRouter.attach(client)
        NativeStreamInputRouter.setAndroidTvProfile(tvProfile)
        onDispose {
            if (Build.VERSION.SDK_INT >= 26) {
                decor?.releasePointerCapture()
            }
            NativeStreamInputRouter.clearUiTouchPassthroughBounds()
            NativeStreamInputRouter.clearStreamPanelTouchPassthroughBounds()
            NativeStreamInputRouter.setSystemMenuHandler(null)
            NativeStreamInputRouter.setSystemBackHandler(null)
            NativeStreamInputRouter.setAndroidTvProfile(false)
            NativeStreamInputRouter.setStreamUiActive(false)
            NativeStreamInputRouter.detach(client)
            client.release()
        }
    }
    DisposableEffect(audioController) {
        onDispose {
            audioController.release()
        }
    }

    LaunchedEffect(streamReady, streamOverlayOpen, streamGuideOpen, streamGuideStep, touchLayoutEditing) {
        NativeStreamInputRouter.setStreamUiActive(streamReady && streamOverlayOpen)
        NativeStreamInputRouter.setSystemMenuHandler {
            openControlsForGuide()
        }
        NativeStreamInputRouter.setSystemBackHandler {
            handleStreamBack()
        }
    }

    LaunchedEffect(client, tvProfile) {
        client.updateAndroidTvProfile(tvProfile)
        client.updateControllerMouseAssistAutoArm(tvProfile)
    }

    LaunchedEffect(streamReady, state.settings.androidStreamGuideDismissed, session?.sessionId) {
        val shouldOpenGuide = streamReady && !state.settings.androidStreamGuideDismissed
        streamGuideOpen = shouldOpenGuide
        if (shouldOpenGuide) {
            streamGuideStep = StreamGuideStep.OpenControls
        }
    }

    LaunchedEffect(controlsOpen, streamGuideOpen, streamGuideStep) {
        if (controlsOpen && streamGuideOpen && streamGuideStep == StreamGuideStep.OpenControls) {
            streamGuideStep = StreamGuideStep.PressDone
        }
    }

    LaunchedEffect(
        physicalControllerConnected,
        touchControlsSuppressedByPhysicalController,
        streamGuideOpen,
        controlsOpen,
        exitConfirmOpen,
        keyboardOpen,
    ) {
        if (!physicalControllerConnected) {
            showTouchControlsWithPhysicalController = false
            physicalControllerPromptOpen = false
            return@LaunchedEffect
        }
        if (
            !tvProfile &&
            touchControlsSuppressedByPhysicalController &&
            !state.settings.androidPhysicalControllerPromptDismissed &&
            !physicalControllerPromptHandled &&
            !streamGuideOpen &&
            !controlsOpen &&
            !exitConfirmOpen &&
            !keyboardOpen
        ) {
            physicalControllerPromptOpen = true
        }
    }

    LaunchedEffect(streamReady, state.settings.sessionCounterEnabled, session?.sessionId, sessionStartedAtMs, smartSessionLimit) {
        var previousRemainingSeconds: Int? = null
        val sentSessionWarnings = mutableSetOf<Int>()
        while (streamReady && state.settings.sessionCounterEnabled) {
            val nowMs = System.currentTimeMillis()
            timerNowMs = nowMs
            val remainingSeconds = sessionRemainingSeconds(smartSessionLimit, sessionStartedAtMs, nowMs)
            sessionWarningThresholdCrossed(previousRemainingSeconds, remainingSeconds)?.let { thresholdSeconds ->
                if (sentSessionWarnings.add(thresholdSeconds)) {
                    Toast.makeText(
                        context,
                        "${formatSessionWarningThreshold(thresholdSeconds)} left in this session",
                        Toast.LENGTH_SHORT,
                    ).show()
                }
            }
            previousRemainingSeconds = remainingSeconds
            delay(1000L)
        }
    }

    // Also gated on nativeTouchActive: dispatchTouch would take the native branch first anyway, but
    // leaving two input modes both flagged "enabled" is how they end up fighting later.
    LaunchedEffect(streamReady, touchInputEnabled, state.settings.androidTouch.mousePad, nativeTouchActive) {
        NativeStreamInputRouter.setTouchMouseEnabled(
            streamReady && touchInputEnabled && state.settings.androidTouch.mousePad && !nativeTouchActive,
        )
    }
    // Gated on touchInputEnabled as well as the setting: finger touches already stop at
    // setTouchMouseEnabled during PiP, but external mouse and touchpad events reach direct click
    // through their own path and would otherwise be mapped against the tiny PiP window.
    LaunchedEffect(state.settings.androidTouch.mouseDirectClick, touchInputEnabled) {
        NativeStreamInputRouter.setMouseDirectClick(
            state.settings.androidTouch.mouseDirectClick && touchInputEnabled,
        )
    }
    LaunchedEffect(
        streamReady,
        touchInputEnabled,
        nativeTouchActive,
        state.streamGame?.id,
    ) {
        val game = state.streamGame
        val enabled = streamReady && touchInputEnabled && nativeTouchActive
        NativeStreamInputRouter.setNativeTouchEnabled(enabled)
        // Records what the catalog says about this game even when we leave touch off, so the fixed
        // list in NativeTouchGames.kt can be filled in — and eventually retired — from real data.
        if (game != null && streamReady) {
            NativeInputDiagnostics.add(nativeTouchDiagnostics(game, enabled))
        }
    }

    LaunchedEffect(streamReady, touchInputEnabled, state.settings.androidTouch.mousePad, nativeTouchActive, controlsOpen, exitConfirmOpen, keyboardOpen, streamGuideOpen, touchControlsVisible) {
        NativeStreamInputRouter.setCaptureAllTouch(
            streamReady &&
                touchInputEnabled &&
                (state.settings.androidTouch.mousePad || nativeTouchActive) &&
                !controlsOpen &&
                !exitConfirmOpen &&
                !keyboardOpen &&
                !streamGuideOpen,
        )
    }
    DisposableEffect(Unit) {
        onDispose {
            NativeStreamInputRouter.setCaptureAllTouch(false)
        }
    }

    LaunchedEffect(state.settings.phoneRumbleFallback) {
        client.updateHapticsSettings(state.settings.phoneRumbleFallback)
    }
    LaunchedEffect(streamReady, microphoneRequested, microphonePermissionResolved, session?.sessionId) {
        if (streamReady && microphoneRequested && !microphonePermissionResolved) {
            microphonePermissionLauncher.launch(Manifest.permission.RECORD_AUDIO)
        }
    }
    LaunchedEffect(
        session,
        streamReady,
        microphonePermissionGranted,
        microphonePermissionResolved,
    ) {
        if (session != null && streamReady && microphonePermissionResolved) {
            val captureMicrophone = shouldCaptureMicrophone(
                mode = launchStreamSettings.microphoneMode,
                permissionGranted = microphonePermissionGranted,
            )
            microphoneEnabled = captureMicrophone
            client.setMicrophoneEnabled(captureMicrophone)
            client.start(
                session,
                launchStreamSettings.copy(
                    microphoneMode = if (captureMicrophone) {
                        launchStreamSettings.microphoneMode
                    } else {
                        MicrophoneMode.Disabled
                    },
                ),
            )
        }
    }
    LaunchedEffect(client, controllerMouseEmulationEnabled, streamReady) {
        if (streamReady) {
            client.setControllerMouseEmulationActive(controllerMouseEmulationEnabled)
        }
    }
    val activeStreamMode = activeStreamModeStatus(
        requestedSettings = requestedStreamSettings,
        transportSettings = launchStreamSettings,
        decodedResolution = streamStats.resolution,
        serverNegotiatedResolution = session?.monitorSnapshot?.returnedResolution
            ?: session?.negotiatedStreamProfile?.resolution,
        serverFinalSelectedResolution = session?.monitorSnapshot?.finalSelectedResolution,
    )
    LaunchedEffect(
        session?.sessionId,
        streamReady,
        activeStreamMode,
    ) {
        if (streamReady && activeStreamMode != null) {
            viewModel.recordActiveStreamMode(activeStreamMode)
        }
    }

    Box(Modifier.fillMaxSize().background(Color.Black)) {
        if (state.activeSessionDecision != null) {
            ActiveSessionDecisionScreen(
                state = state,
                onResumeSession = viewModel::resumeActiveSession,
                onReplaceSession = viewModel::terminateActiveSessionAndStartNew,
                onCancel = viewModel::dismissActiveSessionDecision,
            )
        } else if (session == null && state.streamStatus != "idle") {
            QueueLoadingScreen(state, viewModel)
        } else if (session == null) {
            NoActiveStreamScreen(
                canResumeSession = state.activeSession != null,
                canEndSession = state.authSession != null,
                onBack = { viewModel.setPage(AppPage.Home) },
                onResumeSession = viewModel::resumeActiveSession,
                onEndSession = viewModel::stopStream,
            )
        } else if (!streamReady) {
            QueueLoadingScreen(state, viewModel)
        } else {
            StreamVideoSurface(
                client = client,
                settings = streamSettings,
                androidTouch = state.settings.androidTouch,
                decodedResolution = streamStats.resolution,
                serverNegotiatedResolution = session.negotiatedStreamProfile?.resolution,
                hideExternalMousePointer = externalMousePassthroughActive,
                touchMouseEnabled = touchInputEnabled && state.settings.androidTouch.mousePad,
                pinchZoomEnabled = streamPinchZoomEnabled(
                    touchMouseEnabled = touchInputEnabled && state.settings.androidTouch.mousePad,
                    touchControllerVisible = touchControlsVisible,
                ),
                externalMouseRoot = activity?.window?.decorView,
                onMouseCaptureInput = { (activity as? MainActivity)?.enforceStreamSystemUiFromInput() },
                stretchToFit = stretchToFit,
            )
            if (statsVisible) {
                StreamStatsPill(
                    streamStats = streamStats,
                    streamSettings = launchStreamSettings,
                    style = state.settings.streamStatsStyle,
                    metrics = state.settings.streamStatsMetrics,
                    serverLocation = session.zone,
                    modifier = Modifier.align(statsAlignment),
                )
            }
            if (activeStreamMode != null) {
                ActiveStreamModePill(
                    status = activeStreamMode,
                    recoveryReason = videoTransportFallbackReason,
                    bugReportSubmission = state.bugReportSubmission,
                    bugReportVersionCheck = state.bugReportVersionCheck,
                    update = state.androidUpdate,
                    onBugReportSubmit = viewModel::submitBugReport,
                    onBugReportReset = viewModel::resetBugReportSubmission,
                    onBugReportVersionCheck = viewModel::verifyBugReportVersion,
                    onOpenUpdate = viewModel::performAndroidUpdatePrimaryAction,
                    modifier = Modifier
                        .align(Alignment.TopCenter)
                        .padding(top = if (statsVisible && statsAlignment == Alignment.TopCenter) 48.dp else 8.dp),
                )
            }
            if (touchControlsVisible) {
                TouchOverlay(
                    client = client,
                    touch = state.settings.androidTouch.copy(enabled = true),
                    onButtonTone = {
                        if (state.settings.phoneRumbleFallback) {
                            view.performHapticFeedback(
                                android.view.HapticFeedbackConstants.KEYBOARD_TAP,
                                android.view.HapticFeedbackConstants.FLAG_IGNORE_GLOBAL_SETTING
                            )
                        }
                    },
                    layoutEditing = touchLayoutEditing,
                    onSaveAllOffsets = { allOffsets ->
                        var touch = state.settings.androidTouch
                        allOffsets.forEach { (key, offset) ->
                            touch = touch.withOffset(key, offset.x, offset.y)
                        }
                        viewModel.updateSettings(state.settings.copy(androidTouch = touch))
                    },
                    modifier = Modifier.align(Alignment.BottomCenter),
                )
            }
            AnimatedVisibility(
                visible = !initialVideoFrameRendered,
                enter = fadeIn(animationSpec = tween(180)) + scaleIn(initialScale = 0.96f),
                exit = fadeOut(animationSpec = tween(180)) + scaleOut(targetScale = 0.98f),
                modifier = Modifier.align(Alignment.Center),
            ) {
                InitialStreamConnectionOverlay(
                    gameTitle = game?.title,
                    status = initialStreamConnectionStatus(streamState),
                )
            }
            if (touchLayoutEditing) {
                val doneButtonTone = playButtonTone
                Box(
                    Modifier
                        .align(Alignment.Center),
                    contentAlignment = Alignment.Center,
                ) {
                    Button(
                        onClick = {
                            doneButtonTone()
                            touchLayoutEditing = false
                        },
                        shape = RoundedCornerShape(OpenNowRadius.full),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = MaterialTheme.colorScheme.primary,
                        ),
                        contentPadding = PaddingValues(horizontal = 28.dp, vertical = 14.dp),
                        elevation = ButtonDefaults.buttonElevation(defaultElevation = 8.dp),
                        modifier = Modifier.pointerInteropFilter { event ->
                            if (event.action == MotionEvent.ACTION_UP ||
                                event.action == MotionEvent.ACTION_DOWN
                            ) {
                                false // let Button's click handling still work
                            } else {
                                false
                            }
                        },
                    ) {
                        Icon(
                            Icons.Rounded.Check,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(
                            "Done",
                            style = MaterialTheme.typography.labelLarge,
                            fontWeight = FontWeight.SemiBold,
                        )
                    }
                }
            }
            if (streamGuideOpen) {
                AnimatedLaunchOverlay(Modifier.align(Alignment.Center)) {
                    StreamFirstLaunchGuide(
                        step = streamGuideStep,
                        controlsOpen = controlsOpen,
                        touchControlsEnabled = touchControlsVisible,
                        onOpenControls = {
                            playButtonTone()
                            openControlsForGuide()
                        },
                        onSkip = {
                            playButtonTone()
                            controlsOpen = false
                            dismissStreamGuide()
                        },
                    )
                }
            }
            if (physicalControllerPromptOpen) {
                PhysicalControllerTouchControlsDialog(
                    doNotShowAgain = physicalControllerPromptDoNotShowAgain,
                    onDoNotShowAgainChange = { physicalControllerPromptDoNotShowAgain = it },
                    onOk = {
                        physicalControllerPromptHandled = true
                        physicalControllerPromptOpen = false
                        showTouchControlsWithPhysicalController = false
                        if (physicalControllerPromptDoNotShowAgain) {
                            viewModel.updateSettings(
                                state.settings.copy(androidPhysicalControllerPromptDismissed = true),
                            )
                        }
                    },
                    onUndo = {
                        physicalControllerPromptHandled = true
                        physicalControllerPromptOpen = false
                        showTouchControlsWithPhysicalController = true
                        if (physicalControllerPromptDoNotShowAgain) {
                            viewModel.updateSettings(
                                state.settings.copy(androidPhysicalControllerPromptDismissed = true),
                            )
                        }
                    },
                )
            }
            // A wash behind the panel. Backdrop blur is impossible here — the video is a
            // SurfaceView on its own hardware layer, which neither Modifier.blur nor RenderEffect
            // can sample across — so separation comes from a gradient plus the panel's own fill.
            AnimatedVisibility(
                visible = controlsOpen,
                enter = fadeIn(),
                exit = fadeOut(),
                modifier = Modifier.matchParentSize(),
            ) {
                Box(
                    Modifier
                        .fillMaxSize()
                        .background(
                            Brush.verticalGradient(
                                0f to Color.Transparent,
                                1f to OpenNowPalette.StreamScrim,
                            ),
                        ),
                )
            }
            AnimatedVisibility(
                visible = controlsOpen,
                enter = fadeIn() + slideInVertically(initialOffsetY = { it / 4 }) + scaleIn(initialScale = 0.96f),
                exit = fadeOut() + slideOutVertically(targetOffsetY = { it / 4 }) + scaleOut(targetScale = 0.96f),
                modifier = Modifier.align(Alignment.BottomEnd),
            ) {
                StreamControlsPanel(
                    gameTitle = game?.title ?: "Stream",
                    status = (state.queuePosition?.let { "Queue $it" } ?: streamState).takeUnless(::shouldHideStreamStatusText),
                    settings = state.settings,
                    tvProfile = tvProfile,
                    touchControlsVisible = touchControlsVisible,
                    builtInGameTouchSupported = builtInGameTouchSupported,
                    nativeTouchActive = nativeTouchActive,
                    controllerMouseAssistEnabled = controllerMouseAssistEnabled,
                    controllerMouseEmulationEnabled = controllerMouseEmulationEnabled,
                    showSessionTimer = state.settings.sessionCounterEnabled,
                    sessionTimerLimit = smartSessionLimit,
                    sessionStartedAtMs = sessionStartedAtMs,
                    sessionNowMs = timerNowMs,
                    audioMuted = audioMuted,
                    microphoneRequested = microphoneRequested,
                    microphonePermissionGranted = microphonePermissionGranted,
                    microphoneEnabled = microphoneEnabled,
                    statsVisible = statsVisible,
                    touchLayoutEditing = touchLayoutEditing,
                    bugReportSubmission = state.bugReportSubmission,
                    bugReportVersionCheck = state.bugReportVersionCheck,
                    update = state.androidUpdate,
                    bugReportPreflightProvider = {
                        buildBugReportPreflightDeck(
                            BugReportPreflightEvidence(
                                requestedSettings = requestedStreamSettings,
                                runtimeStats = streamStats,
                                runtimeDiagnostics = AndroidRuntimeDiagnostics.snapshot(context),
                                deliveredResolution = activeStreamMode?.displayedResolution
                                    ?: session.monitorSnapshot?.returnedResolution
                                    ?: streamStats.resolution,
                                deliveredCodec = activeStreamMode?.transportCodec?.name
                                    ?: streamStats.codec,
                                codecReport = state.codecReport,
                                androidTvProfile = tvProfile,
                                serverZone = session.zone,
                                inputDiagnostics = NativeInputDiagnostics.snapshot(),
                            ),
                        )
                    },
                    onAudioToggle = {
                        audioMuted = !audioMuted
                        client.setAudioMuted(audioMuted)
                    },
                    onMicrophoneToggle = {
                        if (!microphonePermissionGranted) {
                            microphonePermissionLauncher.launch(Manifest.permission.RECORD_AUDIO)
                        } else {
                            microphoneEnabled = !microphoneEnabled
                            client.setMicrophoneEnabled(microphoneEnabled)
                        }
                    },
                    onStatsToggle = {
                        statsVisible = !statsVisible
                        viewModel.updateSettings(state.settings.copy(showStatsOnLaunch = statsVisible))
                    },
                    onStatsStyleCycle = {
                        viewModel.updateSettings(state.settings.copy(streamStatsStyle = state.settings.streamStatsStyle.next()))
                    },
                    onStatsPositionCycle = {
                        viewModel.updateSettings(state.settings.copy(streamStatsPosition = state.settings.streamStatsPosition.next()))
                    },
                    onStatsMetricsChange = { metrics ->
                        viewModel.updateSettings(state.settings.copy(streamStatsMetrics = metrics))
                    },
                    onPhoneRumbleFallbackToggle = {
                        viewModel.updateSettings(state.settings.copy(phoneRumbleFallback = !state.settings.phoneRumbleFallback))
                    },
                    onTouchLayoutEditingToggle = {
                        touchLayoutEditing = !touchLayoutEditing
                    },
                    onKeyboardOpen = {
                        controlsOpen = false
                        keyboardOpen = true
                    },
                    onEsc = { client.sendKeyCode(KeyEvent.KEYCODE_ESCAPE) },
                    onEnter = { client.sendKeyCode(KeyEvent.KEYCODE_ENTER) },
                    onBackspace = { client.sendKeyCode(KeyEvent.KEYCODE_DEL) },
                    onSteamMenuOpen = {
                        controlsOpen = false
                        client.openSteamMenu()
                    },
                    onControllerMouseAssistToggle = {
                        client.setControllerMouseAssistEnabled(!controllerMouseAssistEnabled)
                    },
                    onControllerMouseEmulationToggle = {
                        val newState = !controllerMouseEmulationEnabled
                        controllerMouseEmulationEnabled = newState
                        client.setControllerMouseEmulationActive(newState)
                    },
                    onExit = {
                        controlsOpen = false
                        exitConfirmOpen = true
                    },
                    onTouchControlsToggle = {
                        when {
                            nativeTouchActive -> {
                                preferVirtualController = true
                                if (physicalControllerConnected) {
                                    showTouchControlsWithPhysicalController = true
                                }
                                if (!state.settings.androidTouch.enabled) {
                                    viewModel.updateSettings(
                                        state.settings.copy(
                                            androidTouch = state.settings.androidTouch.copy(enabled = true),
                                        ),
                                    )
                                }
                            }
                            preferVirtualController && nativeTouchAvailable && touchControlsVisible -> {
                                // Turning the overlay back off restores the game's built-in touch
                                // without changing the player's persisted controller preference.
                                preferVirtualController = false
                            }
                            physicalControllerConnected && !touchControlsVisible -> {
                                showTouchControlsWithPhysicalController = true
                                if (!state.settings.androidTouch.enabled) {
                                    viewModel.updateSettings(
                                        state.settings.copy(
                                            androidTouch = state.settings.androidTouch.copy(enabled = true),
                                        ),
                                    )
                                }
                            }
                            else -> {
                                viewModel.updateSettings(
                                    state.settings.copy(
                                        androidTouch = state.settings.androidTouch.copy(
                                            enabled = !state.settings.androidTouch.enabled,
                                        ),
                                    ),
                                )
                            }
                        }
                    },
                    onMousePadToggle = {
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.copy(mousePad = !state.settings.androidTouch.mousePad),
                            ),
                        )
                    },
                    onMouseDirectClickToggle = {
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.copy(mouseDirectClick = !state.settings.androidTouch.mouseDirectClick),
                            ),
                        )
                    },
                    onToggleTouchControllerStyle = {
                        val nextStyle = if (state.settings.androidTouch.touchControllerStyle == TouchControllerStyle.V1) {
                            TouchControllerStyle.V2
                        } else {
                            TouchControllerStyle.V1
                        }
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.copy(touchControllerStyle = nextStyle),
                            ),
                        )
                    },
                    onJoystickModeToggle = {
                        val nextMode = if (state.settings.androidTouch.joystickMode == TouchJoystickMode.Fixed) {
                            TouchJoystickMode.Dynamic
                        } else {
                            TouchJoystickMode.Fixed
                        }
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.copy(joystickMode = nextMode),
                            ),
                        )
                    },
                    onJoystickDeadZoneChange = { value ->
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.copy(joystickDeadZone = value),
                            ),
                        )
                    },
                    onSharpeningToggle = {
                        viewModel.updateStreamSettings { settings ->
                            settings.copy(streamSharpeningEnabled = !settings.streamSharpeningEnabled)
                        }
                    },
                    onSharpeningAmountChange = { value ->
                        viewModel.updateStreamSettings { settings ->
                            settings.copy(streamSharpeningAmount = value)
                        }
                    },
                    onStretchToFitToggle = {
                        val next = !state.settings.stretchStreamToFit
                        viewModel.updateSettings(
                            state.settings.copy(
                                legacyCropStreamToFill = false,
                                stretchStreamToFit = next,
                            ),
                        )
                    },
                    onTouchScaleChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(scale = value)))
                    },
                    onButtonScaleChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(buttonScale = value)))
                    },
                    onStickScaleChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(stickScale = value)))
                    },
                    onOpacityChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(opacity = value)))
                    },
                    onMouseSensitivityChange = { value ->
                        viewModel.updateStreamSettings { s -> s.copy(mouseSensitivity = value) }
                    },
                    onMouseScrollSensitivityChange = { value ->
                        viewModel.updateStreamSettings { s -> s.copy(mouseScrollSensitivity = value) }
                    },
                    onNativeTouchScrollScaleChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(nativeTouchScrollScale = value)))
                    },
                    onNativeTouchJitterThresholdChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(nativeTouchJitterThresholdDp = value)))
                    },
                    onTouchEdgePaddingChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(edgePaddingDp = value)))
                    },
                    onTouchBottomPaddingChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(bottomPaddingDp = value)))
                    },
                    onTouchLeftOffsetChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(leftOffsetYDp = value)))
                    },
                    onTouchRightOffsetChange = { value ->
                        viewModel.updateSettings(state.settings.copy(androidTouch = state.settings.androidTouch.copy(rightOffsetYDp = value)))
                    },
                    onTouchLayoutReset = {
                        viewModel.updateSettings(
                            state.settings.copy(
                                androidTouch = state.settings.androidTouch.withResetOffsets()
                            )
                        )
                    },
                    onBugReportSubmit = viewModel::submitBugReport,
                    onBugReportReset = viewModel::resetBugReportSubmission,
                    onBugReportVersionCheck = viewModel::verifyBugReportVersion,
                    onOpenUpdate = viewModel::performAndroidUpdatePrimaryAction,
                    onButtonTone = playButtonTone,
                    highlightDone = streamGuideOpen && streamGuideStep == StreamGuideStep.PressDone,
                    onClose = {
                        controlsOpen = false
                        if (streamGuideOpen && streamGuideStep == StreamGuideStep.PressDone) {
                            dismissStreamGuide()
                        }
                    },
                )
            }
            if (keyboardOpen) {
                AnimatedLaunchOverlay(Modifier.align(Alignment.BottomCenter)) {
                    StreamKeyboardBar(
                        text = keyboardText,
                        onTextChange = { keyboardText = it },
                        onSend = {
                            val text = keyboardText
                            if (text.isNotBlank()) {
                                client.sendText(text)
                                keyboardText = ""
                                keyboardOpen = false
                            }
                        },
                        onBackspace = { client.sendKeyCode(KeyEvent.KEYCODE_DEL) },
                        onEnter = { client.sendKeyCode(KeyEvent.KEYCODE_ENTER) },
                        onEsc = { client.sendKeyCode(KeyEvent.KEYCODE_ESCAPE) },
                        onDone = { keyboardOpen = false },
                    )
                }
            }
            if (exitConfirmOpen) {
                AnimatedLaunchOverlay(Modifier.align(Alignment.Center)) {
                    StreamExitConfirmation(
                        gameTitle = game?.title ?: "this game",
                        onKeepPlaying = { exitConfirmOpen = false },
                        onExit = {
                            exitConfirmOpen = false
                            viewModel.stopStream()
                        },
                    )
                }
            }
        }
    }
}

internal fun shouldShowAndroidTouchControls(
    tvProfile: Boolean,
    touchInputEnabled: Boolean,
    touchControlsEnabled: Boolean,
    suppressedByPhysicalController: Boolean,
): Boolean =
    !tvProfile && touchInputEnabled && touchControlsEnabled && !suppressedByPhysicalController

private data class SessionTimerDisplay(
    val label: String,
    val value: String,
    val detail: String,
    val progress: Float,
    val warning: Boolean,
)

private enum class StreamGuideStep {
    OpenControls,
    PressDone,
}

private fun sessionTimerDisplay(limit: SmartSessionLimit, startedAtMs: Long, nowMs: Long): SessionTimerDisplay {
    val elapsedSeconds = sessionElapsedSeconds(startedAtMs, nowMs)
    val limitSeconds = limit.limitHours * 60 * 60
    val remainingSeconds = sessionRemainingSeconds(limit, startedAtMs, nowMs)
    val warning = remainingSeconds <= 10 * 60
    val progress = if (limitSeconds > 0) (elapsedSeconds.toFloat() / limitSeconds).coerceIn(0f, 1f) else 0f
    return when (limit.mode) {
        SessionTimerMode.Countdown -> SessionTimerDisplay(
            label = "${limit.tierLabel} countdown",
            value = formatSessionTimerDuration(remainingSeconds),
            detail = "${limit.limitHours}h session limit",
            progress = progress,
            warning = warning,
        )
        SessionTimerMode.Stopwatch -> SessionTimerDisplay(
            label = "${limit.tierLabel} session",
            value = "${formatSessionTimerDuration(elapsedSeconds)} / ${limit.limitHours}h",
            detail = "Session stopwatch",
            progress = progress,
            warning = warning,
        )
    }
}

@Composable
internal fun StreamSessionTimerMenuRow(
    limit: SmartSessionLimit,
    startedAtMs: Long,
    nowMs: Long,
    modifier: Modifier = Modifier,
) {
    val display = sessionTimerDisplay(limit, startedAtMs, nowMs)
    val progressColor = when {
        display.warning -> OpenNowPalette.StatusNotice
        else -> MaterialTheme.colorScheme.primary
    }
    Column(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(OpenNowRadius.md))
            .background(Color.White.copy(alpha = 0.06f))
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            Column(Modifier.weight(1f)) {
                Text("Session timer", fontWeight = FontWeight.SemiBold)
                Text(display.label, color = TextMuted, style = MaterialTheme.typography.labelSmall)
            }
            Text(
                display.value,
                color = if (display.warning) OpenNowPalette.StatusNotice else TextPrimary,
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.Bold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Box(
            Modifier
                .fillMaxWidth()
                .height(4.dp)
                .clip(RoundedCornerShape(OpenNowRadius.full))
                .background(Color.White.copy(alpha = 0.12f)),
        ) {
            Box(
                Modifier
                    .fillMaxWidth(display.progress)
                    .height(4.dp)
                    .background(progressColor),
            )
        }
        Text(display.detail, color = TextMuted, style = MaterialTheme.typography.labelSmall)
    }
}

internal fun formatSessionTimerDuration(totalSeconds: Int): String {
    val seconds = totalSeconds.coerceAtLeast(0)
    val hours = seconds / 3600
    val minutes = (seconds % 3600) / 60
    val remainingSeconds = seconds % 60
    return if (hours > 0) {
        "%d:%02d:%02d".format(Locale.US, hours, minutes, remainingSeconds)
    } else {
        "%d:%02d".format(Locale.US, minutes, remainingSeconds)
    }
}

private fun formatSessionWarningThreshold(thresholdSeconds: Int): String {
    val minutes = thresholdSeconds / 60
    return if (minutes == 1) "1 minute" else "$minutes minutes"
}

@Composable
private fun StreamVideoSurface(
    client: NativeStreamClient,
    settings: StreamSettings,
    androidTouch: AndroidTouchSettings,
    decodedResolution: String?,
    serverNegotiatedResolution: String?,
    hideExternalMousePointer: Boolean,
    touchMouseEnabled: Boolean,
    pinchZoomEnabled: Boolean,
    externalMouseRoot: android.view.View?,
    onMouseCaptureInput: () -> Unit,
    stretchToFit: Boolean,
    modifier: Modifier = Modifier,
) {
    val rootView = LocalView.current
    val configuration = LocalConfiguration.current
    val pointerRootView = externalMouseRoot ?: rootView
    val currentOnMouseCaptureInput by rememberUpdatedState(onMouseCaptureInput)
    var zoomScale by remember { mutableFloatStateOf(1f) }
    var zoomOffset by remember { mutableStateOf(Offset.Zero) }
    var viewportSize by remember { mutableStateOf(IntSize.Zero) }
    val streamAspectRatio = remember(decodedResolution, serverNegotiatedResolution, settings.resolution, settings.aspectRatio) {
        streamRendererAspectRatio(settings, decodedResolution, serverNegotiatedResolution)
    }
    val viewportAspectRatio = remember(viewportSize) {
        if (viewportSize.width > 0 && viewportSize.height > 0) {
            viewportSize.width.toFloat() / viewportSize.height.toFloat()
        } else {
            0f
        }
    }
    val rendererModifier = if (viewportAspectRatio <= 0f) {
        Modifier.fillMaxSize()
    } else if (viewportAspectRatio > streamAspectRatio) {
        // Screen is wider than stream (e.g. 2400×1080 screen, 1920×1080 stream).
        // Fit by height so the renderer has no black bars internally; horizontal
        // stretch (if enabled) is applied later via View.scaleX.
        Modifier
            .fillMaxHeight()
            .aspectRatio(streamAspectRatio)
    } else {
        // Screen is taller than stream — fit by width; vertical stretch via scaleY.
        Modifier
            .fillMaxWidth()
            .aspectRatio(streamAspectRatio)
    }

    // SCALE_ASPECT_FIT preserves every decoded pixel. Stretching the View on only
    // the mismatching axis removes the bars without cropping HUD or edge content.
    val stretchScale = remember(stretchToFit, viewportAspectRatio, streamAspectRatio) {
        streamStretchScale(
            enabled = stretchToFit,
            viewportAspectRatio = viewportAspectRatio,
            streamAspectRatio = streamAspectRatio,
        )
    }
    LaunchedEffect(
        settings.resolution,
        settings.aspectRatio,
        settings.streamSharpeningEnabled,
        touchMouseEnabled,
        pinchZoomEnabled,
        stretchToFit,
        streamAspectRatio,
        configuration.orientation,
        configuration.screenWidthDp,
        configuration.screenHeightDp,
    ) {
        zoomScale = 1f
        zoomOffset = Offset.Zero
    }
    LaunchedEffect(stretchToFit) {
        NativeStreamInputRouter.setStretchToFit(stretchToFit)
    }
    LaunchedEffect(streamAspectRatio) {
        NativeStreamInputRouter.setRenderingAspectRatio(streamAspectRatio)
    }
    LaunchedEffect(
        settings.mouseSensitivity,
        settings.mouseScrollSensitivity,
        settings.mouseAcceleration,
        settings.streamSharpeningEnabled,
        settings.streamSharpeningAmount,
    ) {
        client.updateRendererSettings(settings)
    }
    LaunchedEffect(
        androidTouch.nativeTouchScrollScale,
        androidTouch.nativeTouchJitterThresholdDp,
    ) {
        NativeStreamInputRouter.setNativeTouchSettings(
            scrollScale = androidTouch.nativeTouchScrollScale,
            jitterThresholdDp = androidTouch.nativeTouchJitterThresholdDp,
        )
    }
    DisposableEffect(client, rootView, pointerRootView, hideExternalMousePointer) {
        pointerRootView.configureAndroidMousePointerCapture(hideExternalMousePointer, { currentOnMouseCaptureInput() }) { event ->
            client.dispatchMotion(event)
        }
        if (hideExternalMousePointer) {
            pointerRootView.hideAndroidPointerTree()
        } else {
            pointerRootView.showAndroidPointerTree()
        }
        onDispose {
            pointerRootView.clearAndroidMousePointerCapture()
            pointerRootView.showAndroidPointerTree()
        }
    }
    Box(
        modifier
            .fillMaxSize()
            .background(Color.Black)
            .onSizeChanged {
                if (viewportSize != it) {
                    viewportSize = it
                    zoomScale = 1f
                    zoomOffset = Offset.Zero
                } else {
                    zoomOffset = clampStreamZoomOffset(zoomOffset, zoomScale, it)
                }
            }
            .clipToBounds(),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .matchParentSize()
                .graphicsLayer {
                    scaleX = zoomScale
                    scaleY = zoomScale
                    translationX = zoomOffset.x
                    translationY = zoomOffset.y
                },
            contentAlignment = Alignment.Center,
        ) {
            // AndroidView resizes the SurfaceView in place. Re-keying it as the viewport
            // settles creates overlapping renderer surfaces during stream startup.
            key(settings.streamSharpeningEnabled) {
                AndroidView(
                    modifier = rendererModifier,
                    factory = { ctx ->
                        client.createRenderer(ctx, settings).apply {
                            isFocusable = false
                            isFocusableInTouchMode = false
                            hideAndroidPointerTree()
                            scaleX = stretchScale.first
                            scaleY = stretchScale.second
                        }
                    },
                    update = { renderer ->
                        client.updateRendererSettings(settings)
                        renderer.scaleX = stretchScale.first
                        renderer.scaleY = stretchScale.second
                        renderer.isFocusable = false
                        renderer.isFocusableInTouchMode = false
                        pointerRootView.configureAndroidMousePointerCapture(hideExternalMousePointer, { currentOnMouseCaptureInput() }) { event ->
                            client.dispatchMotion(event)
                        }
                        if (hideExternalMousePointer) {
                            pointerRootView.hideAndroidPointerTree()
                            renderer.hideAndroidPointerTree()
                        } else {
                            pointerRootView.showAndroidPointerTree()
                            renderer.showAndroidPointerTree()
                        }
                        renderer.setOnKeyListener(null)
                        renderer.setOnGenericMotionListener { _, event ->
                            if (hideExternalMousePointer) pointerRootView.hideAndroidPointerTree()
                            client.dispatchMotion(event)
                        }
                        renderer.setOnTouchListener { view, event ->
                            NativeStreamInputRouter.dispatchTouch(event, view.width, view.height)
                        }
                    },
                    onRelease = client::releaseRenderer,
                )
            }
        }
        FingerMouseInputLayer(
            enabled = touchMouseEnabled,
            pinchZoomEnabled = pinchZoomEnabled,
            onZoomGesture = { scaleChange, pan ->
                val nextScale = (zoomScale * scaleChange).coerceIn(1f, 3f)
                zoomScale = nextScale
                zoomOffset = if (nextScale <= 1.001f) {
                    Offset.Zero
                } else {
                    clampStreamZoomOffset(zoomOffset + pan, nextScale, viewportSize)
                }
            },
            modifier = Modifier.matchParentSize(),
        )
    }
}

internal fun streamRendererAspectRatio(
    settings: StreamSettings,
    decodedResolution: String?,
    serverNegotiatedResolution: String? = null,
): Float {
    val expectedPixels = streamResolutionPixels(settings)
    val decodedPixels = parseResolutionPixelsOrNull(decodedResolution)
        ?.takeIf(::isStableDecodedStreamResolution)
    val negotiatedPixels = parseResolutionPixelsOrNull(serverNegotiatedResolution)
        ?.takeIf(::isStableDecodedStreamResolution)
    return streamAspectRatioForPixels(decodedPixels ?: negotiatedPixels ?: expectedPixels)
}

internal fun streamStretchScale(
    enabled: Boolean,
    viewportAspectRatio: Float,
    streamAspectRatio: Float,
): Pair<Float, Float> {
    if (!enabled || viewportAspectRatio <= 0f || streamAspectRatio <= 0f) return 1f to 1f
    return when {
        viewportAspectRatio > streamAspectRatio ->
            (viewportAspectRatio / streamAspectRatio).coerceIn(1f, 3f) to 1f
        viewportAspectRatio < streamAspectRatio ->
            1f to (streamAspectRatio / viewportAspectRatio).coerceIn(1f, 3f)
        else -> 1f to 1f
    }
}

internal fun streamPinchZoomEnabled(
    touchMouseEnabled: Boolean,
    touchControllerVisible: Boolean,
): Boolean = touchMouseEnabled && !touchControllerVisible

private fun streamAspectRatioForPixels(pixels: Pair<Int, Int>): Float {
    val (width, height) = pixels
    if (width <= 0 || height <= 0) return 16f / 9f
    return width.toFloat() / height.toFloat()
}

private fun isStableDecodedStreamResolution(pixels: Pair<Int, Int>): Boolean =
    pixels.first >= MIN_STABLE_DECODED_STREAM_WIDTH_PX &&
        pixels.second >= MIN_STABLE_DECODED_STREAM_HEIGHT_PX

private const val MIN_STABLE_DECODED_STREAM_WIDTH_PX = 320

private const val MIN_STABLE_DECODED_STREAM_HEIGHT_PX = 180

private fun clampStreamZoomOffset(offset: Offset, zoomScale: Float, viewportSize: IntSize): Offset {
    if (zoomScale <= 1.001f || viewportSize.width <= 0 || viewportSize.height <= 0) return Offset.Zero
    val maxX = viewportSize.width * (zoomScale - 1f) / 2f
    val maxY = viewportSize.height * (zoomScale - 1f) / 2f
    return Offset(
        x = offset.x.coerceIn(-maxX, maxX),
        y = offset.y.coerceIn(-maxY, maxY),
    )
}

private fun androidNullPointerIcon(view: android.view.View): PointerIcon? =
    if (Build.VERSION.SDK_INT >= 24) {
        runCatching { PointerIcon.getSystemIcon(view.context, PointerIcon.TYPE_NULL) }
            .onFailure { error -> NativeInputDiagnostics.add("pointer icon unavailable error=${error.javaClass.simpleName}") }
            .getOrNull()
    } else {
        null
    }

private fun View.configureAndroidMousePointerCapture(enabled: Boolean, onCaptureInput: () -> Unit = {}, onMotion: (MotionEvent) -> Boolean) {
    if (Build.VERSION.SDK_INT < 26) return
    if (!enabled) {
        clearAndroidMousePointerCapture()
        return
    }
    setOnCapturedPointerListener { _, event ->
        onCaptureInput()
        onMotion(event)
    }
    post {
        if (isAttachedToWindow && hasWindowFocus() && !hasPointerCapture()) {
            isFocusable = true
            isFocusableInTouchMode = true
            requestFocus()
            onCaptureInput()
            runCatching { requestPointerCapture() }
                .onFailure { error -> NativeInputDiagnostics.add("pointer capture request failed error=${error.javaClass.simpleName}") }
        }
    }
}

private fun View.clearAndroidMousePointerCapture() {
    if (Build.VERSION.SDK_INT < 26) return
    setOnCapturedPointerListener(null)
    runCatching { releasePointerCapture() }
        .onFailure { error -> NativeInputDiagnostics.add("pointer capture release failed error=${error.javaClass.simpleName}") }
}

private fun android.view.View.hideAndroidPointerTree() {
    if (Build.VERSION.SDK_INT < 24) return
    val icon = androidNullPointerIcon(this)
    applyAndroidPointerIconTree(icon)
}

private fun android.view.View.showAndroidPointerTree() {
    if (Build.VERSION.SDK_INT < 24) return
    applyAndroidPointerIconTree(null)
}

private fun android.view.View.applyAndroidPointerIconTree(icon: PointerIcon?) {
    if (Build.VERSION.SDK_INT < 24) return
    runCatching { pointerIcon = icon }
        .onFailure { error -> NativeInputDiagnostics.add("pointer icon apply failed error=${error.javaClass.simpleName}") }
    if (this is ViewGroup) {
        for (index in 0 until childCount) {
            getChildAt(index).applyAndroidPointerIconTree(icon)
        }
    }
}

@Composable
private fun FingerMouseInputLayer(
    enabled: Boolean,
    pinchZoomEnabled: Boolean,
    onZoomGesture: (scaleChange: Float, pan: Offset) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (!enabled) return
    var width by remember { mutableStateOf(0) }
    var height by remember { mutableStateOf(0) }
    var pinchActive by remember { mutableStateOf(false) }
    var lastPinchDistance by remember { mutableFloatStateOf(0f) }
    var lastPinchCentroid by remember { mutableStateOf(Offset.Zero) }
    Box(
        modifier
            .onSizeChanged {
                width = it.width
                height = it.height
            }
            .pointerInteropFilter { event ->
                if (NativeStreamInputRouter.isNativeUiTouchGestureActive()) {
                    pinchActive = false
                    lastPinchDistance = 0f
                    lastPinchCentroid = Offset.Zero
                    return@pointerInteropFilter true
                }
                if (event.pointerCount >= 2) {
                    // 3-finger touch is reserved for the Direct Click toggle gesture
                    // (handled in NativeStreamInputRouter.dispatchTouch). Do not
                    // interpret it as a pinch-zoom — reset pinch state and let it through.
                    if (event.pointerCount >= 3) {
                        pinchActive = false
                        lastPinchDistance = 0f
                        lastPinchCentroid = Offset.Zero
                        NativeStreamInputRouter.dispatchTouch(event, width, height)
                        return@pointerInteropFilter true
                    }
                    NativeStreamInputRouter.cancelTouchMouse()
                    if (!pinchZoomEnabled) {
                        // Multiple fingers while the touch controller is visible are
                        // controller input, not a request to crop the video surface.
                        pinchActive = true
                        lastPinchDistance = 0f
                        lastPinchCentroid = Offset.Zero
                        return@pointerInteropFilter true
                    }
                    val distance = event.firstTwoPointerDistance()
                    val centroid = event.firstTwoPointerCentroid()
                    if (pinchActive && lastPinchDistance > 0f && distance > 0f) {
                        onZoomGesture(
                            (distance / lastPinchDistance).coerceIn(0.82f, 1.22f),
                            centroid - lastPinchCentroid,
                        )
                    }
                    pinchActive = true
                    lastPinchDistance = distance
                    lastPinchCentroid = centroid
                    return@pointerInteropFilter true
                }
                if (pinchActive) {
                    if (event.actionMasked == MotionEvent.ACTION_UP || event.actionMasked == MotionEvent.ACTION_CANCEL) {
                        pinchActive = false
                        lastPinchDistance = 0f
                        lastPinchCentroid = Offset.Zero
                    }
                    return@pointerInteropFilter true
                }
                if (event.actionMasked == MotionEvent.ACTION_DOWN) {
                    NativeInputDiagnostics.retainTouchRoute("compose.finger-layer") {
                        "compose finger layer down size=${width}x$height"
                    }
                }
                NativeStreamInputRouter.dispatchTouch(event, width, height)
            },
    )
}

private fun MotionEvent.firstTwoPointerDistance(): Float {
    if (pointerCount < 2) return 0f
    val dx = getX(1) - getX(0)
    val dy = getY(1) - getY(0)
    return sqrt(dx * dx + dy * dy)
}

private fun MotionEvent.firstTwoPointerCentroid(): Offset =
    if (pointerCount >= 2) {
        Offset((getX(0) + getX(1)) / 2f, (getY(0) + getY(1)) / 2f)
    } else {
        Offset.Zero
    }

@Composable
private fun ActiveSessionDecisionScreen(
    state: OpenNowUiState,
    onResumeSession: () -> Unit,
    onReplaceSession: () -> Unit,
    onCancel: () -> Unit,
) {
    val decision = state.activeSessionDecision ?: return
    val active = decision.activeSession
    val activeGame = activeSessionGame(state, active)
    Column(
        Modifier
            .fillMaxSize()
            .padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Surface(
            modifier = Modifier.fillMaxWidth().widthIn(max = 560.dp),
            shape = RoundedCornerShape(18.dp),
            color = PanelAlt.copy(alpha = 0.96f),
            contentColor = TextPrimary,
            tonalElevation = 4.dp,
        ) {
            Column(
                Modifier.padding(18.dp),
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                    UrlImage(
                        activeGame?.imageUrl ?: state.streamGame?.imageUrl,
                        Modifier
                            .width(56.dp)
                            .height(74.dp)
                            .clip(RoundedCornerShape(10.dp)),
                    )
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text("Cloud session already active", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                        Text(
                            activeGame?.title ?: "App ${active.appId}",
                            color = TextPrimary,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(
                            activeSessionSummary(active),
                            color = TextMuted,
                            style = MaterialTheme.typography.bodySmall,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
                Text(
                    "Resume the existing session, or terminate it and start ${decision.requestedGameTitle}.",
                    color = TextMuted,
                    style = MaterialTheme.typography.bodyMedium,
                )
                FlowRow(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.End),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    TextButton(onClick = onCancel) { Text("Cancel") }
                    OutlinedButton(onClick = onReplaceSession) { Text("Terminate and start new") }
                    Button(onClick = onResumeSession) { Text(stringResource(R.string.action_resume)) }
                }
            }
        }
    }
}

@Composable
private fun NoActiveStreamScreen(
    canResumeSession: Boolean,
    canEndSession: Boolean,
    onBack: () -> Unit,
    onResumeSession: () -> Unit,
    onEndSession: () -> Unit,
) {
    Column(
        Modifier
            .fillMaxSize()
            .padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text("No active stream", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
        Spacer(Modifier.height(8.dp))
        Text(
            "OpenNOW does not have a local stream attached right now.",
            color = TextMuted,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(18.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            OutlinedButton(onClick = onBack) { Text("Back to library") }
            if (canResumeSession) {
                Button(onClick = onResumeSession) { Text(stringResource(R.string.action_resume)) }
            }
            if (canEndSession) {
                Button(onClick = onEndSession) { Text("End cloud session") }
            }
        }
    }
}

@Composable
private fun StreamFirstLaunchGuide(
    step: StreamGuideStep,
    controlsOpen: Boolean,
    touchControlsEnabled: Boolean,
    onOpenControls: () -> Unit,
    onSkip: () -> Unit,
) {
    val primaryFocusRequester = remember { FocusRequester() }
    val overlayInteraction = remember { MutableInteractionSource() }
    LaunchedEffect(step, controlsOpen) {
        delay(80)
        if (step == StreamGuideStep.OpenControls || !controlsOpen) {
            runCatching { primaryFocusRequester.requestFocus() }
        }
    }
    BoxWithConstraints(
        if (step == StreamGuideStep.OpenControls) {
            Modifier
                .fillMaxSize()
                .background(Color.Black.copy(alpha = 0.62f))
                .clickable(
                    interactionSource = overlayInteraction,
                    indication = null,
                    onClick = {},
                )
        } else {
            Modifier.fillMaxSize()
        },
    ) {
        val landscape = maxWidth > maxHeight
        if (step == StreamGuideStep.OpenControls) {
            StreamGuideEdgeCue(Modifier.align(Alignment.CenterStart))
            StreamGuideCard(
                stepLabel = "Step 1 of 2",
                title = "Open the stream menu",
                body = "Press Android Back, Menu, or swipe from the left edge. That opens the menu without exiting the stream.",
                details = listOf(
                    "Back or the left-edge gesture opens controls.",
                    if (touchControlsEnabled) {
                        "Touch controls pause while this guide is up."
                    } else {
                        "You can turn touch controls on from the menu."
                    },
                    "Use Skip tutorial if you already know this flow.",
                ),
                modifier = Modifier
                    .align(Alignment.Center)
                    .padding(18.dp)
                    .fillMaxWidth(if (landscape) 0.54f else 0.92f)
                    .then(if (landscape) Modifier.fillMaxHeight(0.82f) else Modifier),
                primaryLabel = "Open controls",
                primaryFocusRequester = primaryFocusRequester,
                onPrimary = onOpenControls,
                secondaryLabel = "Skip tutorial",
                onSecondary = onSkip,
            )
        } else {
            StreamGuideDoneCallout(
                controlsOpen = controlsOpen,
                onOpenControls = onOpenControls,
                onSkip = onSkip,
                primaryFocusRequester = primaryFocusRequester,
                modifier = Modifier
                    .align(if (landscape) Alignment.TopStart else Alignment.TopCenter)
                    .padding(18.dp)
                    .then(if (landscape) Modifier.fillMaxWidth(0.34f) else Modifier.fillMaxWidth(0.86f))
                    .widthIn(max = 340.dp),
            )
        }
    }
}

@Composable
private fun StreamGuideCard(
    stepLabel: String,
    title: String,
    body: String,
    details: List<String>,
    primaryLabel: String,
    primaryFocusRequester: FocusRequester,
    onPrimary: () -> Unit,
    secondaryLabel: String,
    onSecondary: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(20.dp),
        color = Panel.copy(alpha = 0.96f),
        contentColor = TextPrimary,
        tonalElevation = 8.dp,
    ) {
        Column(
            Modifier
                .padding(18.dp)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(stepLabel, color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
                Text(title, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                Text(body, color = TextMuted, style = MaterialTheme.typography.bodyMedium)
            }
            details.forEachIndexed { index, detail ->
                StreamGuidePoint(number = index + 1, body = detail)
            }
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                OutlinedButton(
                    onClick = onSecondary,
                    modifier = Modifier.weight(1f),
                ) {
                    Text(secondaryLabel, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Button(
                    onClick = onPrimary,
                    modifier = Modifier
                        .weight(1f)
                        .focusRequester(primaryFocusRequester),
                ) {
                    Text(primaryLabel, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

@Composable
private fun StreamGuideDoneCallout(
    controlsOpen: Boolean,
    onOpenControls: () -> Unit,
    onSkip: () -> Unit,
    primaryFocusRequester: FocusRequester,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(18.dp),
        color = Panel.copy(alpha = 0.9f),
        tonalElevation = 6.dp,
    ) {
        Row(
            Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
                Text("Step 2 of 2", color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.Bold)
                Text("Press Done", style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            TextButton(
                onClick = onSkip,
                contentPadding = PaddingValues(horizontal = 8.dp, vertical = 4.dp),
            ) {
                Text("Skip", maxLines = 1)
            }
            if (!controlsOpen) {
                Button(
                    onClick = onOpenControls,
                    modifier = Modifier.focusRequester(primaryFocusRequester),
                    contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp),
                ) {
                    Text("Open", maxLines = 1)
                }
            }
        }
    }
}

@Composable
private fun PhysicalControllerTouchControlsDialog(
    doNotShowAgain: Boolean,
    onDoNotShowAgainChange: (Boolean) -> Unit,
    onOk: () -> Unit,
    onUndo: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onOk,
        title = { Text(stringResource(R.string.controller_detected_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(
                    "The on-screen controller was hidden because a physical controller is connected.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(OpenNowRadius.md))
                        .clickable { onDoNotShowAgainChange(!doNotShowAgain) }
                        .padding(horizontal = 8.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Checkbox(
                        checked = doNotShowAgain,
                        onCheckedChange = onDoNotShowAgainChange,
                    )
                    Text("Don't show again", color = MaterialTheme.colorScheme.onSurface)
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onOk) {
                Text("OK")
            }
        },
        dismissButton = {
            TextButton(onClick = onUndo) {
                Text("Undo")
            }
        },
    )
}

@Composable
private fun StreamGuideEdgeCue(modifier: Modifier = Modifier) {
    Box(
        modifier
            .fillMaxHeight()
            .width(112.dp)
            .background(
                Brush.horizontalGradient(
                    listOf(
                        MaterialTheme.colorScheme.primary.copy(alpha = 0.28f),
                        Color.Transparent,
                    ),
                ),
            ),
    ) {
        Surface(
            modifier = Modifier
                .align(Alignment.CenterStart)
                .padding(start = 14.dp),
            shape = RoundedCornerShape(OpenNowRadius.full),
            color = MaterialTheme.colorScheme.primary.copy(alpha = 0.92f),
            tonalElevation = 6.dp,
        ) {
            Row(
                Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    painter = painterResource(R.drawable.ic_arrow_back),
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onPrimary,
                    modifier = Modifier.size(18.dp),
                )
                Text("Back", color = MaterialTheme.colorScheme.onPrimary, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
            }
        }
    }
}

@Composable
private fun StreamGuidePoint(number: Int, body: String) {
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(OpenNowRadius.md))
            .background(Color.White.copy(alpha = 0.06f))
            .padding(horizontal = 12.dp, vertical = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Surface(
            modifier = Modifier.size(22.dp),
            shape = CircleShape,
            color = MaterialTheme.colorScheme.primary,
        ) {
            Box(contentAlignment = Alignment.Center) {
                Text(number.toString(), color = MaterialTheme.colorScheme.onPrimary, style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.Bold)
            }
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(body, color = TextMuted, style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
internal fun BuiltInGameTouchNotice(usingBuiltInTouch: Boolean) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(14.dp),
        color = OpenNowPalette.StatusNotice.copy(alpha = 0.10f),
        contentColor = TextPrimary,
        border = BorderStroke(1.dp, OpenNowPalette.StatusNotice.copy(alpha = 0.38f)),
    ) {
        Column(
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 11.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                stringResource(R.string.stream_touch_builtin_title),
                color = OpenNowPalette.StatusNotice,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.Bold,
            )
            Text(
                stringResource(
                    if (usingBuiltInTouch) {
                        R.string.stream_touch_builtin_available
                    } else {
                        R.string.stream_touch_builtin_overridden
                    },
                ),
                color = TextMuted,
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

@Composable
internal fun Modifier.streamTouchPassthrough(id: String, inflate: Dp = 8.dp): Modifier {
    val inflatePx = with(LocalDensity.current) { inflate.roundToPx() }
    DisposableEffect(id) {
        onDispose { NativeStreamInputRouter.clearOverlayTouchPassthroughBound(id) }
    }
    return onGloballyPositioned { coordinates ->
        val bounds = coordinates.boundsInRoot()
        if (bounds.width <= 0f || bounds.height <= 0f) return@onGloballyPositioned
        NativeStreamInputRouter.setOverlayTouchPassthroughBound(
            id,
            bounds.left.roundToInt() - inflatePx,
            bounds.top.roundToInt() - inflatePx,
            bounds.right.roundToInt() + inflatePx,
            bounds.bottom.roundToInt() + inflatePx,
        )
    }
}

internal const val PASSTHROUGH_ID_PANEL = "controls-panel"

internal const val PASSTHROUGH_ID_KEYBOARD = "keyboard-bar"

private const val PASSTHROUGH_ID_EXIT = "exit-confirmation"

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun StreamStatsPill(
    streamStats: StreamRuntimeStats,
    streamSettings: StreamSettings,
    style: StreamStatsStyle,
    metrics: StreamStatsMetrics,
    serverLocation: String?,
    modifier: Modifier = Modifier,
) {
    if (metrics.enabledCount() == 0) return
    val compact = style == StreamStatsStyle.Compact
    val deviceStatus = rememberCompactStreamDeviceStatus()
    Surface(
        modifier = modifier
            .padding(OpenNowSpacing.sm)
            .widthIn(max = if (compact) 720.dp else 300.dp),
        shape = RoundedCornerShape(if (compact) OpenNowRadius.full else OpenNowRadius.lg),
        // Stays genuinely see-through — this one sits over gameplay by design. The hairline is
        // what keeps its edge readable against a bright frame.
        color = Panel.copy(alpha = 0.52f),
        border = BorderStroke(1.dp, OpenNowPalette.PanelHairline),
        tonalElevation = 0.dp,
    ) {
        if (compact) {
            Row(
                Modifier.padding(horizontal = OpenNowSpacing.md, vertical = 6.dp),
                horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                StreamStatsMetricItems(streamStats, streamSettings, metrics, deviceStatus, serverLocation)
            }
        } else {
            FlowRow(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = OpenNowSpacing.md, vertical = OpenNowSpacing.sm),
                maxItemsInEachRow = 2,
                horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                StreamStatsMetricItems(
                    streamStats,
                    streamSettings,
                    metrics,
                    deviceStatus,
                    serverLocation,
                    // Two aligned columns instead of a ragged pair of runs.
                    itemModifier = Modifier.weight(1f),
                )
            }
        }
    }
}

@Composable
private fun ActiveStreamModePill(
    status: ActiveStreamModeStatus,
    recoveryReason: String?,
    bugReportSubmission: BugReportSubmissionState,
    bugReportVersionCheck: AndroidBugReportVersionCheckState,
    update: AndroidUpdateState,
    onBugReportSubmit: (String, String) -> Unit,
    onBugReportReset: () -> Unit,
    onBugReportVersionCheck: () -> Unit,
    onOpenUpdate: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val changes = remember(status) { activeStreamModeDisplayChanges(status) }
    if (changes.isEmpty()) return
    val causeAssessment = remember(status, recoveryReason) {
        activeStreamModeCauseAssessment(status, recoveryReason)
    }
    val developerReport = remember(status, recoveryReason) {
        activeStreamModeDeveloperReport(status, recoveryReason)
    }
    val headline = changes.first().let { "${it.label} ${it.requestedValue} → ${it.actualValue}" }
    val noticeKey = remember(changes, recoveryReason) {
        changes.joinToString("|") { "${it.label}:${it.requestedValue}:${it.actualValue}" } +
            "|${recoveryReason.orEmpty()}"
    }
    var noticeVisible by remember(noticeKey) { mutableStateOf(true) }
    var detailsOpen by remember(noticeKey) { mutableStateOf(false) }
    var reportConfirmationOpen by remember(noticeKey) { mutableStateOf(false) }

    LaunchedEffect(detailsOpen, update.installSource.isGooglePlay) {
        if (detailsOpen && update.installSource.isGooglePlay) {
            onBugReportVersionCheck()
        }
    }

    LaunchedEffect(noticeKey) {
        noticeVisible = true
        delay(ACTIVE_STREAM_MODE_NOTICE_DURATION_MS)
        noticeVisible = false
    }

    AnimatedVisibility(
        visible = noticeVisible,
        modifier = modifier.padding(horizontal = 8.dp),
        enter = fadeIn(),
        exit = fadeOut(),
    ) {
        Surface(
            modifier = Modifier
                .semantics { contentDescription = "$headline. Tap for details." }
                .clickable {
                    if (!bugReportSubmission.uploading) onBugReportReset()
                    detailsOpen = true
                }
                .focusable(),
            shape = RoundedCornerShape(OpenNowRadius.full),
            color = Color(0xff4a2f0b).copy(alpha = 0.88f),
            tonalElevation = 0.dp,
        ) {
            Text(
                text = headline,
                modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
                color = Color(0xffffd38a),
                style = MaterialTheme.typography.labelSmall,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }

    if (detailsOpen) {
        AlertDialog(
            onDismissRequest = {
                if (!bugReportSubmission.uploading) detailsOpen = false
            },
            title = { Text("Stream profile changed") },
            text = {
                Column(
                    modifier = Modifier
                        .heightIn(max = 560.dp)
                        .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Surface(
                        modifier = Modifier.fillMaxWidth(),
                        shape = RoundedCornerShape(OpenNowRadius.md),
                        color = OpenNowPalette.StatusNotice.copy(alpha = 0.10f),
                        contentColor = TextPrimary,
                        border = BorderStroke(1.dp, OpenNowPalette.StatusNotice.copy(alpha = 0.32f)),
                    ) {
                        Column(
                            modifier = Modifier.padding(12.dp),
                            verticalArrangement = Arrangement.spacedBy(4.dp),
                        ) {
                            Text(
                                text = "Why it happened",
                                color = OpenNowPalette.StatusNotice,
                                style = MaterialTheme.typography.labelLarge,
                                fontWeight = FontWeight.Bold,
                            )
                            Text(
                                text = causeAssessment.summary,
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    }
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        changes.forEach { change ->
                            Column {
                                Text(
                                    text = change.label,
                                    color = TextMuted,
                                    style = MaterialTheme.typography.labelMedium,
                                )
                                Text(
                                    text = "${change.requestedValue} → ${change.actualValue}",
                                    color = TextPrimary,
                                    style = MaterialTheme.typography.bodyMedium,
                                    fontWeight = FontWeight.SemiBold,
                                )
                            }
                        }
                    }
                    when {
                        bugReportSubmission.uploading -> Text(
                            text = "Sending the report and redacted diagnostics…",
                            color = TextMuted,
                            style = MaterialTheme.typography.bodySmall,
                        )
                        bugReportSubmission.submitted -> Surface(
                            modifier = Modifier.fillMaxWidth(),
                            shape = RoundedCornerShape(10.dp),
                            color = Green.copy(alpha = 0.12f),
                            contentColor = Green,
                        ) {
                            Text(
                                text = bugReportSubmission.reference?.let { "Sent to developer • Reference $it" }
                                    ?: "Sent to developer",
                                modifier = Modifier.padding(10.dp),
                                style = MaterialTheme.typography.bodySmall,
                                fontWeight = FontWeight.Bold,
                            )
                        }
                        bugReportSubmission.error != null -> Surface(
                            modifier = Modifier.fillMaxWidth(),
                            shape = RoundedCornerShape(10.dp),
                            color = MaterialTheme.colorScheme.error.copy(alpha = 0.12f),
                            contentColor = MaterialTheme.colorScheme.error,
                        ) {
                            Text(
                                text = bugReportSubmission.error,
                                modifier = Modifier.padding(10.dp),
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    }
                    Text(
                        text = "Your saved stream settings were not changed.",
                        color = TextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                    if (!androidBugReportsAllowed(update, bugReportVersionCheck)) {
                        BugReportVersionGateCard(
                            update = update,
                            versionCheck = bugReportVersionCheck,
                            onRetry = onBugReportVersionCheck,
                            onOpenUpdate = onOpenUpdate,
                        )
                    }
                }
            },
            confirmButton = {
                when {
                    bugReportSubmission.uploading -> Button(
                        enabled = false,
                        onClick = {},
                    ) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(18.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onPrimary,
                        )
                        Spacer(Modifier.width(8.dp))
                        Text("Sending…")
                    }
                    bugReportSubmission.submitted -> TextButton(onClick = { detailsOpen = false }) {
                        Text("Done")
                    }
                    !androidBugReportsAllowed(update, bugReportVersionCheck) -> when {
                        update.status == AndroidUpdateStatus.Available ||
                            bugReportVersionCheck.status == AndroidBugReportVersionCheckStatus.UpdateRequired ->
                            Button(onClick = onOpenUpdate) {
                                Text("Update in Google Play")
                            }
                        bugReportVersionCheck.status == AndroidBugReportVersionCheckStatus.Checking -> Button(
                            enabled = false,
                            onClick = {},
                        ) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(18.dp),
                                strokeWidth = 2.dp,
                                color = MaterialTheme.colorScheme.onPrimary,
                            )
                            Spacer(Modifier.width(8.dp))
                            Text("Checking Google Play…")
                        }
                        else -> Button(onClick = onBugReportVersionCheck) {
                            Text("Retry version check")
                        }
                    }
                    else -> Button(
                        onClick = {
                            onBugReportReset()
                            detailsOpen = false
                            reportConfirmationOpen = true
                        },
                    ) {
                        Text(if (bugReportSubmission.error == null) "Send to developer" else "Try again")
                    }
                }
            },
            dismissButton = {
                if (!bugReportSubmission.uploading && !bugReportSubmission.submitted) {
                    TextButton(onClick = { detailsOpen = false }) {
                        Text("Close")
                    }
                }
            },
        )
    }

    if (reportConfirmationOpen) {
        AlertDialog(
            onDismissRequest = {
                reportConfirmationOpen = false
                detailsOpen = true
            },
            title = { Text("Send stream diagnostics?") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(
                        "This sends the profile-change summary and likely cause to PrintedWaste and OpenNOW maintainers so they can investigate it.",
                    )
                    BugReportDataDisclosure(
                        includeTypedTextWarning = false,
                    )
                }
            },
            confirmButton = {
                Button(
                    onClick = {
                        reportConfirmationOpen = false
                        onBugReportSubmit(developerReport.title, developerReport.description)
                        detailsOpen = true
                    },
                ) {
                    Text("Send diagnostics")
                }
            },
            dismissButton = {
                TextButton(
                    onClick = {
                        reportConfirmationOpen = false
                        detailsOpen = true
                    },
                ) {
                    Text("Cancel")
                }
            },
        )
    }
}

internal data class ActiveStreamModeDisplayChange(
    val label: String,
    val requestedValue: String,
    val actualValue: String,
)

internal fun activeStreamModeDisplayChanges(status: ActiveStreamModeStatus): List<ActiveStreamModeDisplayChange> {
    val requested = status.requestedProfile
    val actual = status.transportProfile
    return buildList {
        if (status.requestedResolution != status.displayedResolution) {
            add(
                ActiveStreamModeDisplayChange(
                    label = "Resolution",
                    requestedValue = formatRuntimeResolution(status.requestedResolution),
                    actualValue = formatRuntimeResolution(status.displayedResolution),
                ),
            )
        }
        if (requested.codec != actual.codec) {
            add(ActiveStreamModeDisplayChange("Codec", requested.codec.name, actual.codec.name))
        }
        if (requested.fps != actual.fps) {
            add(ActiveStreamModeDisplayChange("FPS", requested.fps.toString(), actual.fps.toString()))
        }
        if (requested.maxBitrateMbps != actual.maxBitrateMbps) {
            add(
                ActiveStreamModeDisplayChange(
                    "Bitrate",
                    "${requested.maxBitrateMbps} Mbps",
                    "${actual.maxBitrateMbps} Mbps",
                ),
            )
        }
        if (requested.hdrEnabled != actual.hdrEnabled) {
            add(ActiveStreamModeDisplayChange("HDR", requested.hdrEnabled.onOffLabel(), actual.hdrEnabled.onOffLabel()))
        }
        if (requested.colorQuality != actual.colorQuality) {
            add(ActiveStreamModeDisplayChange("Color", requested.colorQuality.label, actual.colorQuality.label))
        }
        if (requested.enableCloudGsync != actual.enableCloudGsync) {
            add(
                ActiveStreamModeDisplayChange(
                    "Cloud G-Sync",
                    requested.enableCloudGsync.onOffLabel(),
                    actual.enableCloudGsync.onOffLabel(),
                ),
            )
        }
        if (requested.enableL4S != actual.enableL4S) {
            add(ActiveStreamModeDisplayChange("L4S", requested.enableL4S.onOffLabel(), actual.enableL4S.onOffLabel()))
        }
        if (requested.streamSharpeningEnabled != actual.streamSharpeningEnabled) {
            add(
                ActiveStreamModeDisplayChange(
                    "Sharpening",
                    requested.streamSharpeningEnabled.onOffLabel(),
                    actual.streamSharpeningEnabled.onOffLabel(),
                ),
            )
        }
    }
}

internal data class ActiveStreamModeCauseAssessment(
    val summary: String,
)

internal fun activeStreamModeCauseAssessment(
    status: ActiveStreamModeStatus,
    recoveryReason: String?,
): ActiveStreamModeCauseAssessment {
    val requestedCodec = status.requestedProfile.codec.name
    val actualCodec = status.transportProfile.codec.name
    val primaryChange = activeStreamModeDisplayChanges(status).firstOrNull()
    val saferProfileSummary = primaryChange?.let {
        "a safer profile (${it.label} ${it.requestedValue} to ${it.actualValue})"
    } ?: "a safer live profile"
    val recordedReason = recoveryReason?.trim()?.takeIf(String::isNotEmpty)
    val lowerReason = recordedReason?.lowercase(Locale.US).orEmpty()
    val summary = when {
        "did not negotiate" in lowerReason ->
            "WebRTC could not negotiate the requested $requestedCodec codec for this connection, so OpenNOW retried the local video transport with $actualCodec."
        "video offer" in lowerReason ->
            "The session did not provide a video offer before the startup timeout, so OpenNOW retried the local video transport with $actualCodec."
        "no frame rendered" in lowerReason || "first video frame" in lowerReason ->
            "Video data arrived, but the device did not render a frame before the recovery timeout. OpenNOW applied $saferProfileSummary to restore video."
        "decoder stalled" in lowerReason || "media stall" in lowerReason ->
            "The device decoder stopped producing video frames during startup. OpenNOW applied $saferProfileSummary while keeping the same cloud session."
        "decoded at" in lowerReason ->
            "The decoder produced an unexpected output size for the requested stream mode, so OpenNOW tried the $actualCodec transport profile. Recorded detail: $recordedReason"
        status.resolutionSource == StreamResolutionChangeSource.ServerNegotiatedFallback ->
            "The cloud server selected ${status.displayedResolution} instead of the requested ${status.requestedResolution}. This was a server/session negotiation decision, not a change to your saved setting."
        status.resolutionSource == StreamResolutionChangeSource.ProviderOrGameModeChange ->
            "The decoded stream changed to ${status.displayedResolution} after startup without matching the server's initial mode. This points to a game or cloud-provider output-mode change."
        recordedReason != null ->
            "OpenNOW recorded this recovery reason: $recordedReason"
        status.safeVideoRecoveryActive ->
            "The original video transport stopped progressing, so OpenNOW adjusted the local profile to keep video playing without ending the cloud session."
        else ->
            "The live stream profile no longer matched the requested profile."
    }
    return ActiveStreamModeCauseAssessment(summary)
}

internal data class ActiveStreamModeDeveloperReport(
    val title: String,
    val description: String,
)

internal fun activeStreamModeDeveloperReport(
    status: ActiveStreamModeStatus,
    recoveryReason: String?,
): ActiveStreamModeDeveloperReport {
    val changes = activeStreamModeDisplayChanges(status)
    val primary = changes.first()
    val cause = activeStreamModeCauseAssessment(status, recoveryReason)
    return ActiveStreamModeDeveloperReport(
        title = "Automatic stream change: ${primary.label} ${primary.requestedValue} to ${primary.actualValue}",
        description = buildString {
            appendLine("OpenNOW detected an automatic stream profile change while the session was active.")
            appendLine()
            appendLine("Cause assessment:")
            appendLine(cause.summary)
            appendLine()
            appendLine("Requested to actual changes:")
            changes.forEach { change ->
                appendLine("- ${change.label}: ${change.requestedValue} -> ${change.actualValue}")
            }
            recoveryReason?.trim()?.takeIf(String::isNotEmpty)?.let { reason ->
                appendLine()
                appendLine("Recorded recovery event:")
                appendLine(reason)
            }
            appendLine()
            append("Sent from the in-stream profile-change notice. The user's saved stream settings were not changed.")
        },
    )
}

@Composable
private fun StreamStatsMetricItems(
    streamStats: StreamRuntimeStats,
    streamSettings: StreamSettings,
    metrics: StreamStatsMetrics,
    deviceStatus: CompactStreamDeviceStatus,
    serverLocation: String?,
    /** Applied to every item; the expanded layout passes a weight so its two columns line up. */
    itemModifier: Modifier = Modifier,
) {
    // The target is what the user asked for; streamStats.fps is what is actually arriving.
    val targetFps = streamSettings.fps
    if (metrics.fps) {
        val fps = streamStats.fps
        StreamStatsText(
            value = "FPS ${fps ?: targetFps}",
            modifier = itemModifier,
            quality = fps?.let { StreamQuality.frameRate(it.toDouble(), targetFps) },
            contentDescription = stringResource(R.string.stream_stats_cd_fps, fps ?: targetFps),
        )
    }
    if (metrics.ping) {
        val ping = streamStats.pingMs
        StreamStatsText(
            value = stringResource(R.string.stream_stats_ping, ping?.let { "${it}ms" } ?: NO_STAT_VALUE),
            modifier = itemModifier,
            quality = ping?.let(StreamQuality::latency),
            contentDescription = ping?.let { stringResource(R.string.stream_stats_cd_ping, it) },
        )
    }
    if (metrics.latency) {
        streamStats.decodeMs?.let { decode ->
            StreamStatsText(
                value = stringResource(R.string.stream_stats_decode, "%.1f".format(Locale.US, decode)),
                modifier = itemModifier,
                quality = StreamQuality.decode(decode, targetFps),
                contentDescription = stringResource(R.string.stream_stats_cd_decode, "%.1f".format(Locale.US, decode)),
            )
        }
        streamStats.jitterMs?.let { jitter ->
            StreamStatsText(
                value = stringResource(R.string.stream_stats_jitter, "%.1f".format(Locale.US, jitter)),
                modifier = itemModifier,
                quality = StreamQuality.jitter(jitter),
                contentDescription = stringResource(R.string.stream_stats_cd_jitter, "%.1f".format(Locale.US, jitter)),
            )
        }
    }
    if (metrics.packetLoss) {
        streamStats.packetLossPct?.let { loss ->
            // %.2f, matching the session report — %.1f hid the 0.5% boundary the ladder cares about.
            val formatted = "%.2f".format(Locale.US, loss)
            StreamStatsText(
                value = stringResource(R.string.stream_stats_loss, formatted),
                modifier = itemModifier,
                quality = StreamQuality.packetLoss(loss),
                contentDescription = stringResource(R.string.stream_stats_cd_loss, formatted),
            )
        }
    }
    if (metrics.bitrate) {
        StreamStatsText(formatRuntimeBitrate(streamStats.bitrateKbps), modifier = itemModifier)
    }
    if (metrics.battery) {
        StreamBatteryIndicator(deviceStatus, itemModifier)
    }
    if (metrics.connection) {
        StreamNetworkIndicator(deviceStatus, itemModifier)
    }
    if (metrics.resolution) {
        StreamStatsText(
            streamStats.resolution?.let(::formatRuntimeResolution)
                ?: formatRuntimeResolution(normalizeStreamResolutionForAspect(streamSettings.resolution, streamSettings.aspectRatio)),
            modifier = itemModifier,
        )
    }
    if (metrics.codec) {
        StreamStatsText(streamStats.codec?.takeIf { it.isNotBlank() } ?: streamSettings.codec.name, modifier = itemModifier)
    }
    if (metrics.location && !serverLocation.isNullOrBlank()) {
        val displayName = serverLocation.removePrefix("NPA-").removePrefix("NP-").uppercase()
        StreamStatsText(displayName, modifier = itemModifier)
    }
}

/** Shown in place of a metric that has not been measured yet. */

private const val NO_STAT_VALUE = "--"

@Composable
private fun StreamStatsText(
    value: String,
    modifier: Modifier = Modifier,
    quality: StreamQualityLevel? = null,
    contentDescription: String? = null,
) {
    // Colour alone used to carry the warning, which says nothing to a colour-blind user or to
    // TalkBack. The quality level is spelled out in the description instead.
    val qualityLabel = quality?.let { stringResource(it.labelRes()) }
    val describedAs = contentDescription?.let { base ->
        if (qualityLabel != null) "$base, $qualityLabel" else base
    }
    Text(
        value,
        modifier = if (describedAs != null) {
            modifier.semantics { this.contentDescription = describedAs }
        } else {
            modifier
        },
        color = quality?.tint() ?: TextPrimary,
        // Tabular figures: without these every value is a different width each tick, so the whole
        // row reflows roughly once a second.
        style = MaterialTheme.typography.labelSmall.numeric(),
        fontWeight = FontWeight.SemiBold,
        maxLines = 1,
    )
}

@StringRes
internal fun StreamQualityLevel.labelRes(): Int = when (this) {
    StreamQualityLevel.Good -> R.string.stream_quality_good
    StreamQualityLevel.Fair -> R.string.stream_quality_fair
    StreamQualityLevel.Poor -> R.string.stream_quality_poor
}

private data class CompactStreamDeviceStatus(
    val batteryPercent: Int? = null,
    val batteryCharging: Boolean = false,
    val networkKind: AndroidNetworkKind = AndroidNetworkKind.Unknown,
    val networkBars: Int? = null,
    val cellularGeneration: String? = null,
)

@Composable
private fun rememberCompactStreamDeviceStatus(): CompactStreamDeviceStatus {
    val context = LocalContext.current
    val appContext = remember(context) { context.applicationContext }
    var status by remember(appContext) { mutableStateOf(readCompactStreamDeviceStatus(appContext)) }
    LaunchedEffect(appContext) {
        while (true) {
            status = readCompactStreamDeviceStatus(appContext)
            delay(COMPACT_STREAM_DEVICE_STATUS_REFRESH_MS)
        }
    }
    return status
}

private fun readCompactStreamDeviceStatus(context: Context): CompactStreamDeviceStatus {
    val diagnostics = AndroidRuntimeDiagnostics.snapshot(context)
    return CompactStreamDeviceStatus(
        batteryPercent = diagnostics.batteryPercent,
        batteryCharging = diagnostics.batteryCharging,
        networkKind = diagnostics.networkKind,
        networkBars = diagnostics.networkSignalBars,
        cellularGeneration = diagnostics.cellularGeneration,
    )
}

@Composable
private fun StreamBatteryIndicator(status: CompactStreamDeviceStatus, modifier: Modifier = Modifier) {
    val description = status.batteryPercent?.let { percent ->
        "Battery $percent percent${if (status.batteryCharging) ", charging" else ""}"
    } ?: "Battery unknown"
    val level = streamBatteryLevel(status.batteryPercent)
    val batteryIcon = when (level) {
        StreamBatteryLevel.Unknown -> Icons.AutoMirrored.Rounded.BatteryUnknown
        StreamBatteryLevel.Empty -> Icons.Rounded.Battery0Bar
        StreamBatteryLevel.One -> Icons.Rounded.Battery1Bar
        StreamBatteryLevel.Two -> Icons.Rounded.Battery2Bar
        StreamBatteryLevel.Three -> Icons.Rounded.Battery3Bar
        StreamBatteryLevel.Four -> Icons.Rounded.Battery4Bar
        StreamBatteryLevel.Five -> Icons.Rounded.Battery5Bar
        StreamBatteryLevel.Six -> Icons.Rounded.Battery6Bar
        StreamBatteryLevel.Full -> Icons.Rounded.BatteryFull
    }
    val batteryTint = when {
        status.batteryCharging -> Green
        status.batteryPercent != null && status.batteryPercent <= 20 -> MaterialTheme.colorScheme.error
        else -> TextPrimary
    }
    Row(
        modifier = modifier.semantics { contentDescription = description },
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(18.dp)) {
            Icon(
                imageVector = batteryIcon,
                contentDescription = null,
                tint = batteryTint,
                modifier = Modifier.matchParentSize().graphicsLayer { rotationZ = 90f },
            )
            if (status.batteryCharging) {
                Icon(
                    imageVector = Icons.Rounded.Bolt,
                    contentDescription = null,
                    tint = batteryTint,
                    modifier = Modifier.align(Alignment.Center).size(10.dp),
                )
            }
        }
        Text(
            status.batteryPercent?.let { "$it%" } ?: "--%",
            color = batteryTint,
            style = MaterialTheme.typography.labelSmall,
            maxLines = 1,
        )
    }
}

@Composable
private fun StreamNetworkIndicator(status: CompactStreamDeviceStatus, modifier: Modifier = Modifier) {
    val bars = status.networkBars?.coerceIn(0, 4)
    val label = when (status.networkKind) {
        AndroidNetworkKind.Cellular -> status.cellularGeneration ?: status.networkKind.label
        AndroidNetworkKind.Ethernet,
        AndroidNetworkKind.Other,
        AndroidNetworkKind.None,
        AndroidNetworkKind.Unknown,
        -> status.networkKind.label
        AndroidNetworkKind.Wifi -> null
    }
    val description = "${label ?: status.networkKind.label} signal ${bars?.toString() ?: "unknown"} bars"
    Row(
        modifier = modifier.semantics { contentDescription = description },
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (label != null) {
            Text(
                label,
                color = TextPrimary,
                style = MaterialTheme.typography.labelSmall,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
            )
        }
        if (status.networkKind == AndroidNetworkKind.Wifi) {
            Icon(
                imageVector = when (bars) {
                    4 -> Icons.Rounded.Wifi
                    3 -> Icons.Rounded.Wifi
                    2 -> Icons.Rounded.Wifi2Bar
                    1 -> Icons.Rounded.Wifi1Bar
                    0 -> Icons.Rounded.SignalWifi0Bar
                    else -> Icons.Rounded.WifiOff
                },
                contentDescription = null,
                tint = TextPrimary,
                modifier = Modifier.size(20.dp),
            )
        } else if (status.networkKind == AndroidNetworkKind.Cellular || status.networkKind == AndroidNetworkKind.Other || status.networkKind == AndroidNetworkKind.Unknown) {
            Icon(
                imageVector = when (bars) {
                    4 -> Icons.Rounded.SignalCellular4Bar
                    3 -> Icons.Rounded.SignalCellularAlt
                    2 -> Icons.Rounded.SignalCellularAlt2Bar
                    1 -> Icons.Rounded.SignalCellularAlt1Bar
                    else -> Icons.Rounded.SignalCellular0Bar
                },
                contentDescription = null,
                tint = TextPrimary,
                modifier = Modifier.size(20.dp),
            )
        }
    }
}

internal fun formatRuntimeResolution(resolution: String): String {
    val parts = resolution.lowercase(Locale.US).split("x", limit = 2)
    return if (parts.size == 2 && parts.all { it.trim().isNotBlank() }) {
        "${parts[0].trim()}x${parts[1].trim()}"
    } else {
        resolution
    }
}

internal fun formatRuntimeBitrate(bitrateKbps: Int?): String {
    val kbps = bitrateKbps ?: return "--"
    return if (kbps >= 1000) {
        "${(kbps / 1000.0).let { kotlin.math.round(it * 10.0) / 10.0 }} Mbps"
    } else {
        "$kbps Kbps"
    }
}

private fun shouldHideStreamStatusText(status: String): Boolean =
    status.trim().replace('_', ' ').let {
        it.equals("Streaming", ignoreCase = true) ||
            it.equals("ICE CONNECTED", ignoreCase = true) ||
            it.equals("ICE COMPLETED", ignoreCase = true)
    }

internal data class InitialStreamConnectionStatus(
    val phase: String,
    val title: String,
    val detail: String,
)

internal fun initialStreamConnectionStatus(nativeState: String): InitialStreamConnectionStatus {
    val normalized = nativeState.trim().replace('_', ' ')
    return when {
        normalized.equals("Preparing", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Preparing",
            title = "Preparing your stream",
            detail = "Getting the secure video connection ready.",
        )
        normalized.startsWith("Connecting signaling", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Connecting",
            title = "Connecting to your game",
            detail = "Opening a secure connection to the streaming server.",
        )
        normalized.startsWith("Waiting for offer", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Waiting for video",
            title = "Starting the video stream",
            detail = "The server is preparing the first video frame.",
        )
        normalized.equals("ICE CHECKING", ignoreCase = true) ||
            normalized.equals("ICE NEW", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Securing connection",
            title = "Almost ready",
            detail = "Checking the best route for the live video stream.",
        )
        normalized.equals("ICE DISCONNECTED", ignoreCase = true) ||
            normalized.equals("ICE FAILED", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Retrying",
            title = "Connection interrupted",
            detail = "OpenNOW is retrying the initial stream connection.",
        )
        normalized.startsWith("Recovering video", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Recovering video",
            title = "Waiting for a clear frame",
            detail = "Requesting a fresh video frame before showing the stream.",
        )
        normalized.contains("safe H264 profile", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Optimizing video",
            title = "Trying a compatible video mode",
            detail = "Restarting the initial video connection with safer settings.",
        )
        normalized.startsWith("Reconnecting", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Retrying connection",
            title = "Connecting again",
            detail = "The initial connection did not finish, so OpenNOW is retrying it.",
        )
        normalized.startsWith("Recovering cloud session", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Checking session",
            title = "Restoring your game session",
            detail = "Checking the existing cloud session before continuing.",
        )
        normalized.equals("Streaming", ignoreCase = true) -> InitialStreamConnectionStatus(
            phase = "Starting video",
            title = "Connection established",
            detail = "Waiting for the first video frame to appear.",
        )
        else -> InitialStreamConnectionStatus(
            phase = "Starting stream",
            title = "Preparing your game",
            detail = "OpenNOW is waiting for the live video to begin.",
        )
    }
}

@Composable
private fun InitialStreamConnectionOverlay(
    gameTitle: String?,
    status: InitialStreamConnectionStatus,
    modifier: Modifier = Modifier,
) {
    BoxWithConstraints(
        modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.18f))
            .padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        val cardWidthFraction = if (maxWidth > maxHeight) 0.54f else 0.9f
        Surface(
            modifier = Modifier
                .fillMaxWidth(cardWidthFraction)
                .widthIn(max = 560.dp),
            shape = RoundedCornerShape(22.dp),
            color = Panel.copy(alpha = 0.96f),
            contentColor = TextPrimary,
            tonalElevation = 10.dp,
        ) {
            Row(
                Modifier.padding(horizontal = 22.dp, vertical = 20.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(18.dp),
            ) {
                CircularProgressIndicator(
                    modifier = Modifier
                        .size(38.dp)
                        .semantics { contentDescription = status.phase },
                    strokeWidth = 3.dp,
                    color = MaterialTheme.colorScheme.primary,
                )
                Column(
                    Modifier.weight(1f),
                    verticalArrangement = Arrangement.spacedBy(5.dp),
                ) {
                    Text(
                        gameTitle?.takeIf { it.isNotBlank() } ?: "OpenNOW stream",
                        color = MaterialTheme.colorScheme.primary,
                        style = MaterialTheme.typography.labelLarge,
                        fontWeight = FontWeight.Bold,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        status.title,
                        style = MaterialTheme.typography.titleLarge,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        status.detail,
                        color = TextMuted,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    Text(
                        status.phase,
                        color = TextMuted.copy(alpha = 0.78f),
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.SemiBold,
                    )
                }
            }
        }
    }
}

@Composable
private fun StreamExitConfirmation(
    gameTitle: String,
    onKeepPlaying: () -> Unit,
    onExit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val keepPlayingFocusRequester = remember { FocusRequester() }
    LaunchedEffect(Unit) {
        delay(80)
        runCatching { keepPlayingFocusRequester.requestFocus() }
    }
    val scrimInteraction = remember { MutableInteractionSource() }
    Box(
        Modifier
            .fillMaxSize()
            // The scrim covers everything, so it reports the full screen — otherwise a mis-tap on
            // "Exit Stream" also lands in the game underneath.
            .streamTouchPassthrough(PASSTHROUGH_ID_EXIT, inflate = 0.dp)
            .background(OpenNowPalette.StreamScrim)
            // indication = null: a full-screen ripple is wrong, and without its own interaction
            // source the scrim competes with the two buttons for D-pad focus.
            .clickable(
                interactionSource = scrimInteraction,
                indication = null,
                onClick = onKeepPlaying,
            ),
        contentAlignment = Alignment.Center,
    ) {
        Surface(
            modifier = modifier
                .padding(OpenNowSpacing.xl)
                .fillMaxWidth()
                // Unbounded fillMaxWidth made this enormous on a tablet or TV.
                .widthIn(max = 440.dp),
            // Same radius as the controls panel, so the two overlays read as one family.
            shape = RoundedCornerShape(OpenNowRadius.lg + 2.dp),
            color = OpenNowPalette.PanelOverVideo,
            contentColor = TextPrimary,
            border = BorderStroke(1.dp, OpenNowPalette.PanelHairline),
            tonalElevation = 8.dp,
        ) {
            Column(
                Modifier.padding(OpenNowSpacing.lg + 2.dp),
                verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
            ) {
                Text(
                    stringResource(R.string.stream_exit_eyebrow),
                    color = TextMuted,
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.Bold,
                )
                Text(stringResource(R.string.stream_exit_title), style = MaterialTheme.typography.titleLarge)
                Text(stringResource(R.string.stream_exit_body, gameTitle), color = TextMuted)
                Text(
                    stringResource(R.string.stream_exit_caveat),
                    color = TextMuted,
                    style = MaterialTheme.typography.bodySmall,
                )
                Row(
                    horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    OutlinedButton(
                        onClick = onKeepPlaying,
                        modifier = Modifier
                            .weight(1f)
                            .focusRequester(keepPlayingFocusRequester),
                    ) { Text(stringResource(R.string.stream_exit_keep_playing), maxLines = 1) }
                    Button(onClick = onExit, modifier = Modifier.weight(1f)) {
                        Text(stringResource(R.string.stream_exit_confirm), maxLines = 1)
                    }
                }
            }
        }
    }
}
