package com.opencloudgaming.opennow


import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
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
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material.icons.rounded.KeyboardArrowUp
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.key
import androidx.compose.runtime.setValue
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.DialogProperties
import androidx.core.content.ContextCompat
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import com.opencloudgaming.opennow.ui.adaptive.CONTENT_COMPACT_MAX_WIDTH
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.util.Locale
import kotlin.math.min
import com.opencloudgaming.opennow.ui.controls.ControlActionRow
import com.opencloudgaming.opennow.ui.controls.ControlNavigationRow
import com.opencloudgaming.opennow.ui.controls.ControlRowStyle
import com.opencloudgaming.opennow.ui.controls.ControlSection
import com.opencloudgaming.opennow.ui.controls.ControlSectionStyle
import com.opencloudgaming.opennow.ui.controls.ControlSliderRow
import com.opencloudgaming.opennow.ui.controls.ControlSwitchRow
import com.opencloudgaming.opennow.ui.controls.LocalControlRowStyle
import com.opencloudgaming.opennow.ui.controls.LocalControlSectionStyle
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import com.opencloudgaming.opennow.ui.theme.numeric
import com.opencloudgaming.opennow.ui.theme.tint
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlin.math.sqrt




// StreamScreen (was OpenNowScreens.kt:7059)
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

// shouldShowAndroidTouchControls (was OpenNowScreens.kt:7962)
internal fun shouldShowAndroidTouchControls(
    tvProfile: Boolean,
    touchInputEnabled: Boolean,
    touchControlsEnabled: Boolean,
    suppressedByPhysicalController: Boolean,
): Boolean =
    !tvProfile && touchInputEnabled && touchControlsEnabled && !suppressedByPhysicalController

// SessionTimerDisplay (was OpenNowScreens.kt:7970)
private data class SessionTimerDisplay(
    val label: String,
    val value: String,
    val detail: String,
    val progress: Float,
    val warning: Boolean,
)

// StreamGuideStep (was OpenNowScreens.kt:7978)
private enum class StreamGuideStep {
    OpenControls,
    PressDone,
}

// sessionTimerDisplay (was OpenNowScreens.kt:7983)
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

// StreamSessionTimerMenuRow (was OpenNowScreens.kt:8007)
@Composable
private fun StreamSessionTimerMenuRow(
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

// formatSessionTimerDuration (was OpenNowScreens.kt:8059)
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

// formatSessionWarningThreshold (was OpenNowScreens.kt:8071)
private fun formatSessionWarningThreshold(thresholdSeconds: Int): String {
    val minutes = thresholdSeconds / 60
    return if (minutes == 1) "1 minute" else "$minutes minutes"
}

// StreamVideoSurface (was OpenNowScreens.kt:8076)
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

// streamRendererAspectRatio (was OpenNowScreens.kt:8273)
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

// streamStretchScale (was OpenNowScreens.kt:8286)
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

// streamPinchZoomEnabled (was OpenNowScreens.kt:8301)
internal fun streamPinchZoomEnabled(
    touchMouseEnabled: Boolean,
    touchControllerVisible: Boolean,
): Boolean = touchMouseEnabled && !touchControllerVisible

// streamAspectRatioForPixels (was OpenNowScreens.kt:8306)
private fun streamAspectRatioForPixels(pixels: Pair<Int, Int>): Float {
    val (width, height) = pixels
    if (width <= 0 || height <= 0) return 16f / 9f
    return width.toFloat() / height.toFloat()
}

// isStableDecodedStreamResolution (was OpenNowScreens.kt:8312)
private fun isStableDecodedStreamResolution(pixels: Pair<Int, Int>): Boolean =
    pixels.first >= MIN_STABLE_DECODED_STREAM_WIDTH_PX &&
        pixels.second >= MIN_STABLE_DECODED_STREAM_HEIGHT_PX

// MIN_STABLE_DECODED_STREAM_WIDTH_PX (was OpenNowScreens.kt:8316)
private const val MIN_STABLE_DECODED_STREAM_WIDTH_PX = 320

// MIN_STABLE_DECODED_STREAM_HEIGHT_PX (was OpenNowScreens.kt:8317)
private const val MIN_STABLE_DECODED_STREAM_HEIGHT_PX = 180

// clampStreamZoomOffset (was OpenNowScreens.kt:8319)
private fun clampStreamZoomOffset(offset: Offset, zoomScale: Float, viewportSize: IntSize): Offset {
    if (zoomScale <= 1.001f || viewportSize.width <= 0 || viewportSize.height <= 0) return Offset.Zero
    val maxX = viewportSize.width * (zoomScale - 1f) / 2f
    val maxY = viewportSize.height * (zoomScale - 1f) / 2f
    return Offset(
        x = offset.x.coerceIn(-maxX, maxX),
        y = offset.y.coerceIn(-maxY, maxY),
    )
}

// androidNullPointerIcon (was OpenNowScreens.kt:8329)
private fun androidNullPointerIcon(view: android.view.View): PointerIcon? =
    if (Build.VERSION.SDK_INT >= 24) {
        runCatching { PointerIcon.getSystemIcon(view.context, PointerIcon.TYPE_NULL) }
            .onFailure { error -> NativeInputDiagnostics.add("pointer icon unavailable error=${error.javaClass.simpleName}") }
            .getOrNull()
    } else {
        null
    }

// configureAndroidMousePointerCapture (was OpenNowScreens.kt:8338)
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

// clearAndroidMousePointerCapture (was OpenNowScreens.kt:8360)
private fun View.clearAndroidMousePointerCapture() {
    if (Build.VERSION.SDK_INT < 26) return
    setOnCapturedPointerListener(null)
    runCatching { releasePointerCapture() }
        .onFailure { error -> NativeInputDiagnostics.add("pointer capture release failed error=${error.javaClass.simpleName}") }
}

// hideAndroidPointerTree (was OpenNowScreens.kt:8367)
private fun android.view.View.hideAndroidPointerTree() {
    if (Build.VERSION.SDK_INT < 24) return
    val icon = androidNullPointerIcon(this)
    applyAndroidPointerIconTree(icon)
}

// showAndroidPointerTree (was OpenNowScreens.kt:8373)
private fun android.view.View.showAndroidPointerTree() {
    if (Build.VERSION.SDK_INT < 24) return
    applyAndroidPointerIconTree(null)
}

// applyAndroidPointerIconTree (was OpenNowScreens.kt:8378)
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

// FingerMouseInputLayer (was OpenNowScreens.kt:8389)
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

// firstTwoPointerDistance (was OpenNowScreens.kt:8466)
private fun MotionEvent.firstTwoPointerDistance(): Float {
    if (pointerCount < 2) return 0f
    val dx = getX(1) - getX(0)
    val dy = getY(1) - getY(0)
    return sqrt(dx * dx + dy * dy)
}

// firstTwoPointerCentroid (was OpenNowScreens.kt:8473)
private fun MotionEvent.firstTwoPointerCentroid(): Offset =
    if (pointerCount >= 2) {
        Offset((getX(0) + getX(1)) / 2f, (getY(0) + getY(1)) / 2f)
    } else {
        Offset.Zero
    }

// ActiveSessionDecisionScreen (was OpenNowScreens.kt:8480)
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

// NoActiveStreamScreen (was OpenNowScreens.kt:8552)
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

// StreamFirstLaunchGuide (was OpenNowScreens.kt:8588)
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

// StreamGuideCard (was OpenNowScreens.kt:8661)
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

// StreamGuideDoneCallout (was OpenNowScreens.kt:8718)
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

// PhysicalControllerTouchControlsDialog (was OpenNowScreens.kt:8760)
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

// StreamGuideEdgeCue (was OpenNowScreens.kt:8806)
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

// StreamGuidePoint (was OpenNowScreens.kt:8846)
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

// StreamControlsPage (was OpenNowScreens.kt:8872)
private enum class StreamControlsPage {
    Main,
    StatusBar,
    TouchControls,
    MouseMode,
    ReportProblem,
}

// StreamControlsPanel (was OpenNowScreens.kt:8880)
@Composable
private fun StreamControlsPanel(
    gameTitle: String,
    status: String?,
    settings: AppSettings,
    tvProfile: Boolean,
    touchControlsVisible: Boolean,
    builtInGameTouchSupported: Boolean,
    nativeTouchActive: Boolean,
    controllerMouseAssistEnabled: Boolean,
    controllerMouseEmulationEnabled: Boolean,
    showSessionTimer: Boolean,
    sessionTimerLimit: SmartSessionLimit,
    sessionStartedAtMs: Long,
    sessionNowMs: Long,
    audioMuted: Boolean,
    microphoneRequested: Boolean,
    microphonePermissionGranted: Boolean,
    microphoneEnabled: Boolean,
    statsVisible: Boolean,
    touchLayoutEditing: Boolean,
    bugReportSubmission: BugReportSubmissionState,
    bugReportVersionCheck: AndroidBugReportVersionCheckState,
    update: AndroidUpdateState,
    bugReportPreflightProvider: () -> BugReportPreflightDeck,
    onAudioToggle: () -> Unit,
    onMicrophoneToggle: () -> Unit,
    onStatsToggle: () -> Unit,
    onStatsStyleCycle: () -> Unit,
    onStatsPositionCycle: () -> Unit,
    onStatsMetricsChange: (StreamStatsMetrics) -> Unit,
    onPhoneRumbleFallbackToggle: () -> Unit,
    onTouchLayoutEditingToggle: () -> Unit,
    onKeyboardOpen: () -> Unit,
    onEsc: () -> Unit,
    onEnter: () -> Unit,
    onBackspace: () -> Unit,
    onSteamMenuOpen: () -> Unit,
    onControllerMouseAssistToggle: () -> Unit,
    onControllerMouseEmulationToggle: () -> Unit,
    onExit: () -> Unit,
    onTouchControlsToggle: () -> Unit,
    onMousePadToggle: () -> Unit,
    onMouseDirectClickToggle: () -> Unit,
    onToggleTouchControllerStyle: () -> Unit,
    onJoystickModeToggle: () -> Unit,
    onJoystickDeadZoneChange: (Float) -> Unit,
    onSharpeningToggle: () -> Unit,
    onSharpeningAmountChange: (Float) -> Unit,
    onStretchToFitToggle: () -> Unit,
    onTouchScaleChange: (Float) -> Unit,
    onButtonScaleChange: (Float) -> Unit,
    onStickScaleChange: (Float) -> Unit,
    onOpacityChange: (Float) -> Unit,
    onMouseSensitivityChange: (Float) -> Unit,
    onMouseScrollSensitivityChange: (Int) -> Unit,
    onNativeTouchScrollScaleChange: (Float) -> Unit,
    onNativeTouchJitterThresholdChange: (Float) -> Unit,
    onTouchEdgePaddingChange: (Float) -> Unit,
    onTouchBottomPaddingChange: (Float) -> Unit,
    onTouchLeftOffsetChange: (Float) -> Unit,
    onTouchRightOffsetChange: (Float) -> Unit,
    onTouchLayoutReset: () -> Unit,
    onBugReportSubmit: (String, String) -> Unit,
    onBugReportReset: () -> Unit,
    onBugReportVersionCheck: () -> Unit,
    onOpenUpdate: () -> Unit,
    onButtonTone: () -> Unit,
    highlightDone: Boolean = false,
    onClose: () -> Unit,
) {
    val doneFocusRequester = remember { FocusRequester() }
    val focusManager = LocalFocusManager.current
    var page by remember { mutableStateOf(StreamControlsPage.Main) }
    val reduceMotion = LocalReduceMotion.current
    BackHandler(enabled = page != StreamControlsPage.Main) {
        page = StreamControlsPage.Main
    }
    LaunchedEffect(page) {
        delay(120)
        runCatching { doneFocusRequester.requestFocus() }
    }
    Surface(
        modifier = Modifier
            .padding(14.dp)
            .fillMaxWidth(0.94f)
            .fillMaxHeight(0.72f)
            .streamTouchPassthrough(PASSTHROUGH_ID_PANEL),
        shape = RoundedCornerShape(OpenNowRadius.lg + 2.dp),
        // Firmer than the old 0.93: at that alpha TextMuted did not reliably clear 4.5:1 over
        // bright gameplay. The hairline keeps the panel's edge visible against a light frame.
        color = OpenNowPalette.PanelOverVideo,
        contentColor = TextPrimary,
        border = BorderStroke(1.dp, OpenNowPalette.PanelHairline),
        tonalElevation = 6.dp,
    ) {
        // Every control row inside the panel picks up the denser, over-video styling — and, more
        // importantly, becomes properly focusable. The panel's own row widgets never were.
        CompositionLocalProvider(
            LocalControlRowStyle provides ControlRowStyle.stream(),
            LocalControlSectionStyle provides ControlSectionStyle.stream(),
        ) {
        Column(Modifier.fillMaxSize()) {
        // The header stays outside the scrolling area so every focused sub-page keeps navigation
        // and session actions visible while its settings scroll independently.
        StreamPanelHeader(
            page = page,
            gameTitle = gameTitle,
            status = status,
            highlightDone = highlightDone,
            focusRequester = doneFocusRequester,
            onBack = { page = StreamControlsPage.Main },
            onKeyboardOpen = onKeyboardOpen,
            onExit = onExit,
            onClose = onClose,
            onButtonTone = onButtonTone,
        )
        AnimatedContent(
            targetState = page,
            transitionSpec = { streamPanelPageTransition(initialState, targetState, reduceMotion) },
            label = "stream-controls-page",
        ) { currentPage ->
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .onPreviewKeyEvent { handleVerticalDpadFocusMove(it, focusManager) },
            contentPadding = PaddingValues(OpenNowSpacing.md + 2.dp),
            verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
        ) {
            when (currentPage) {
                StreamControlsPage.StatusBar -> statusBarPageItems(
                    settings = settings,
                    statsVisible = statsVisible,
                    onStatsToggle = onStatsToggle,
                    onStatsStyleCycle = onStatsStyleCycle,
                    onStatsPositionCycle = onStatsPositionCycle,
                    onStatsMetricsChange = onStatsMetricsChange,
                    onButtonTone = onButtonTone,
                )
                StreamControlsPage.TouchControls -> {
                    if (builtInGameTouchSupported) {
                        item {
                            BuiltInGameTouchNotice(usingBuiltInTouch = nativeTouchActive)
                        }
                    }
                    item {
                        ControlSection(stringResource(R.string.stream_panel_section_touch_controller)) {
                            ControlSwitchRow(
                                label = stringResource(R.string.stream_panel_touch_controller),
                                checked = touchControlsVisible,
                                onCheckedChange = {
                                    onButtonTone()
                                    onTouchControlsToggle()
                                },
                                value = when {
                                    touchControlsVisible -> stringResource(R.string.common_visible)
                                    nativeTouchActive -> stringResource(R.string.stream_touch_builtin_active)
                                    else -> stringResource(R.string.common_hidden)
                                },
                            )
                            if (touchControlsVisible) {
                                val cleanStyle = settings.androidTouch.touchControllerStyle == TouchControllerStyle.V2
                                ControlSwitchRow(
                                    label = stringResource(R.string.stream_panel_clean_style),
                                    checked = cleanStyle,
                                    onCheckedChange = {
                                        onButtonTone()
                                        onToggleTouchControllerStyle()
                                    },
                                    value = onOffLabel(cleanStyle),
                                )
                            }
                            ControlSwitchRow(
                                label = stringResource(R.string.stream_panel_phone_rumble),
                                checked = settings.phoneRumbleFallback,
                                onCheckedChange = {
                                    onButtonTone()
                                    onPhoneRumbleFallbackToggle()
                                },
                                value = onOffLabel(settings.phoneRumbleFallback),
                            )
                        }
                    }
                    item {
                        ControlSection(stringResource(R.string.stream_joysticks_title)) {
                            val dynamic = settings.androidTouch.joystickMode == TouchJoystickMode.Dynamic
                            ControlSwitchRow(
                                label = stringResource(R.string.stream_joysticks_dynamic),
                                checked = dynamic,
                                onCheckedChange = {
                                    onButtonTone()
                                    onJoystickModeToggle()
                                },
                                value = stringResource(
                                    if (dynamic) R.string.stream_joysticks_dynamic_on else R.string.stream_joysticks_dynamic_off,
                                ),
                            )
                            TouchLayoutSlider(
                                R.string.stream_joysticks_stick_size,
                                settings.androidTouch.stickScale,
                                0.65f,
                                1.5f,
                                TOUCH_SCALE_SLIDER_STEP,
                                onStickScaleChange,
                            )
                            TouchLayoutSlider(
                                R.string.stream_joysticks_dead_zone,
                                settings.androidTouch.joystickDeadZone,
                                0f,
                                0.3f,
                                JOYSTICK_DEAD_ZONE_STEP,
                                onJoystickDeadZoneChange,
                            )
                            Text(
                                stringResource(R.string.stream_joysticks_explainer),
                                color = TextMuted,
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    }
                    item {
                        ControlSection(stringResource(R.string.stream_panel_section_touch_layout)) {
                            ControlSwitchRow(
                                label = stringResource(R.string.stream_panel_drag_edit),
                                checked = touchLayoutEditing,
                                onCheckedChange = {
                                    onButtonTone()
                                    onTouchLayoutEditingToggle()
                                },
                                value = onOffLabel(touchLayoutEditing),
                            )
                            ControlActionRow(
                                label = stringResource(R.string.stream_panel_reset_layout),
                                actionLabel = stringResource(R.string.action_reset),
                                onClick = {
                                    onButtonTone()
                                    onTouchLayoutReset()
                                },
                                value = stringResource(R.string.stream_panel_reset_layout_summary),
                            )
                            // These controls preview live so the player can position the overlay
                            // against the game without leaving the stream.
                            TouchLayoutSlider(R.string.stream_panel_layout_scale, settings.androidTouch.scale, 0.6f, 1.4f, TOUCH_SCALE_SLIDER_STEP, onTouchScaleChange)
                            TouchLayoutSlider(R.string.stream_panel_button_size, settings.androidTouch.buttonScale, 0.65f, 1.5f, TOUCH_SCALE_SLIDER_STEP, onButtonScaleChange)
                            TouchLayoutSlider(R.string.stream_panel_opacity, settings.androidTouch.opacity, 0.15f, 1f, TOUCH_SCALE_SLIDER_STEP, onOpacityChange)
                            TouchLayoutSlider(R.string.stream_panel_edge_padding, settings.androidTouch.edgePaddingDp, 0f, 72f, TOUCH_DP_SLIDER_STEP, onTouchEdgePaddingChange, unit = DP_UNIT)
                            TouchLayoutSlider(R.string.stream_panel_bottom_padding, settings.androidTouch.bottomPaddingDp, 0f, 120f, TOUCH_DP_SLIDER_STEP, onTouchBottomPaddingChange, unit = DP_UNIT)
                            TouchLayoutSlider(R.string.stream_panel_left_position, settings.androidTouch.leftOffsetYDp, -160f, 160f, TOUCH_DP_SLIDER_STEP, onTouchLeftOffsetChange, unit = DP_UNIT)
                            TouchLayoutSlider(R.string.stream_panel_right_position, settings.androidTouch.rightOffsetYDp, -160f, 160f, TOUCH_DP_SLIDER_STEP, onTouchRightOffsetChange, unit = DP_UNIT)
                        }
                    }
                }
                StreamControlsPage.MouseMode -> mouseModePageItems(
                    settings = settings,
                    controllerMouseEmulationEnabled = controllerMouseEmulationEnabled,
                    onControllerMouseEmulationToggle = onControllerMouseEmulationToggle,
                    onMouseSensitivityChange = onMouseSensitivityChange,
                    onMouseScrollSensitivityChange = onMouseScrollSensitivityChange,
                    onNativeTouchScrollScaleChange = onNativeTouchScrollScaleChange,
                    onNativeTouchJitterThresholdChange = onNativeTouchJitterThresholdChange,
                    onButtonTone = onButtonTone,
                )
                StreamControlsPage.ReportProblem -> {
                    item {
                        StreamBugReporter(
                            submission = bugReportSubmission,
                            versionCheck = bugReportVersionCheck,
                            update = update,
                            onSubmit = onBugReportSubmit,
                            onReset = onBugReportReset,
                            onVersionCheck = onBugReportVersionCheck,
                            onOpenUpdate = onOpenUpdate,
                            onButtonTone = onButtonTone,
                            preflightProvider = bugReportPreflightProvider,
                            initiallyExpanded = true,
                            onExpandedClose = { page = StreamControlsPage.Main },
                        )
                    }
                }
                StreamControlsPage.Main -> {
            if (showSessionTimer) {
                item {
                    StreamSessionTimerMenuRow(
                        limit = sessionTimerLimit,
                        startedAtMs = sessionStartedAtMs,
                        nowMs = sessionNowMs,
                    )
                }
            }
            item {
                ControlSection(stringResource(R.string.stream_panel_section_display)) {
                    ControlSwitchRow(
                        label = stringResource(R.string.stream_panel_audio),
                        checked = !audioMuted,
                        onCheckedChange = {
                            onButtonTone()
                            onAudioToggle()
                        },
                        value = if (audioMuted) stringResource(R.string.stream_panel_audio_muted) else onOffLabel(true),
                    )
                    ControlNavigationRow(
                        label = stringResource(R.string.stream_panel_status_bar),
                        onClick = {
                            onButtonTone()
                            page = StreamControlsPage.StatusBar
                        },
                        value = if (!statsVisible) {
                            onOffLabel(false)
                        } else {
                            stringResource(
                                R.string.stream_panel_status_bar_summary,
                                settings.streamStatsStyle.label,
                                settings.streamStatsMetrics.enabledCount(),
                            )
                        },
                    )
                    ControlSwitchRow(
                        label = stringResource(R.string.stream_panel_sharpening),
                        checked = settings.stream.streamSharpeningEnabled,
                        onCheckedChange = {
                            onButtonTone()
                            onSharpeningToggle()
                        },
                        value = onOffLabel(settings.stream.streamSharpeningEnabled),
                    )
                    if (settings.stream.streamSharpeningEnabled) {
                        ControlSliderRow(
                            label = stringResource(R.string.stream_panel_sharpening_amount),
                            value = settings.stream.streamSharpeningAmount,
                            min = 0f,
                            max = 1f,
                            step = SHARPENING_SLIDER_STEP,
                            onChange = onSharpeningAmountChange,
                        )
                    }
                    ControlSwitchRow(
                        label = stringResource(R.string.stream_panel_stretch_to_fit),
                        checked = settings.stretchStreamToFit,
                        onCheckedChange = {
                            onButtonTone()
                            onStretchToFitToggle()
                        },
                        value = onOffLabel(settings.stretchStreamToFit),
                    )
                }
            }
            item {
                ControlSection(stringResource(R.string.stream_panel_section_input)) {
                    if (microphoneRequested) {
                        ControlSwitchRow(
                            label = stringResource(R.string.stream_panel_microphone),
                            checked = microphoneEnabled && microphonePermissionGranted,
                            onCheckedChange = {
                                onButtonTone()
                                onMicrophoneToggle()
                            },
                            value = when {
                                !microphonePermissionGranted -> stringResource(R.string.stream_panel_microphone_permission)
                                microphoneEnabled -> onOffLabel(true)
                                else -> stringResource(R.string.stream_panel_audio_muted)
                            },
                        )
                    }
                    ControlActionRow(
                        label = stringResource(R.string.stream_panel_steam_menu),
                        actionLabel = stringResource(R.string.action_open),
                        onClick = {
                            onButtonTone()
                            onSteamMenuOpen()
                        },
                        value = stringResource(R.string.stream_panel_steam_menu_summary),
                    )
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        StreamPanelKeyButton(stringResource(R.string.stream_panel_key_esc), Modifier.weight(1f)) {
                            onButtonTone()
                            onEsc()
                        }
                        StreamPanelKeyButton(stringResource(R.string.stream_panel_key_enter), Modifier.weight(1f)) {
                            onButtonTone()
                            onEnter()
                        }
                        StreamPanelKeyButton(stringResource(R.string.stream_panel_key_backspace), Modifier.weight(1f)) {
                            onButtonTone()
                            onBackspace()
                        }
                    }
                    if (tvProfile) {
                        ControlSwitchRow(
                            label = stringResource(R.string.stream_panel_controller_mouse),
                            checked = controllerMouseAssistEnabled,
                            onCheckedChange = {
                                onButtonTone()
                                onControllerMouseAssistToggle()
                            },
                            value = if (controllerMouseAssistEnabled) {
                                stringResource(R.string.stream_panel_controller_mouse_summary)
                            } else {
                                onOffLabel(false)
                            },
                        )
                    } else {
                        ControlSwitchRow(
                            label = stringResource(R.string.stream_panel_finger_mouse),
                            checked = settings.androidTouch.mousePad,
                            onCheckedChange = {
                                onButtonTone()
                                onMousePadToggle()
                            },
                            value = onOffLabel(settings.androidTouch.mousePad),
                        )
                        if (settings.androidTouch.mousePad) {
                            ControlSwitchRow(
                                label = stringResource(R.string.stream_panel_direct_click),
                                checked = settings.androidTouch.mouseDirectClick,
                                onCheckedChange = {
                                    onButtonTone()
                                    onMouseDirectClickToggle()
                                },
                                value = onOffLabel(settings.androidTouch.mouseDirectClick),
                                // Reads as a child of Finger mouse; replaces a hand-written Box.
                                indentLevel = 1,
                            )

                            val scrollHint = when {
                                settings.stream.mouseScrollSensitivity <= 20 -> "Fast"
                                settings.stream.mouseScrollSensitivity <= 40 -> "Normal"
                                settings.stream.mouseScrollSensitivity <= 60 -> "Precise"
                                else -> "Slow"
                            }

                            ControlActionRow(
                                label = "Scroll sensitivity",
                                actionLabel = scrollHint,
                                onClick = {
                                    onButtonTone()
                                    val next = when {
                                        settings.stream.mouseScrollSensitivity <= 20 -> 40
                                        settings.stream.mouseScrollSensitivity <= 40 -> 60
                                        settings.stream.mouseScrollSensitivity <= 60 -> 80
                                        else -> 20
                                    }
                                    onMouseScrollSensitivityChange(next)
                                },
                                indentLevel = 1
                            )
                        }
                        ControlNavigationRow(
                            label = stringResource(R.string.stream_panel_touch_controller),
                            onClick = {
                                onButtonTone()
                                page = StreamControlsPage.TouchControls
                            },
                            value = when {
                                touchControlsVisible -> stringResource(R.string.common_visible)
                                nativeTouchActive -> stringResource(R.string.stream_touch_builtin_active)
                                else -> stringResource(R.string.common_hidden)
                            },
                        )
                    }
                    // Mouse mode (Left stick): shown for all profiles — works with both physical
                    // gamepad and touch controller.
                    ControlNavigationRow(
                        label = stringResource(R.string.stream_panel_mouse_mode),
                        onClick = {
                            onButtonTone()
                            page = StreamControlsPage.MouseMode
                        },
                        value = if (controllerMouseEmulationEnabled) {
                            stringResource(R.string.stream_panel_mouse_mode_summary)
                        } else {
                            onOffLabel(false)
                        },
                    )
                }
            }
            item {
                ControlSection(stringResource(R.string.stream_panel_section_support)) {
                    ControlNavigationRow(
                        label = stringResource(R.string.bug_report_open_label),
                        onClick = {
                            onButtonTone()
                            page = StreamControlsPage.ReportProblem
                        },
                        value = stringResource(R.string.bug_report_open_summary),
                    )
                }
            }
                } // StreamControlsPage.Main
            } // when (currentPage)
        } // LazyColumn
        } // AnimatedContent
        } // Column
        } // CompositionLocalProvider
    } // Surface
    DisposableEffect(Unit) {
        onDispose {
            NativeStreamInputRouter.clearStreamPanelTouchPassthroughBounds()
        }
    }
}

// BuiltInGameTouchNotice (was OpenNowScreens.kt:9384)
@Composable
private fun BuiltInGameTouchNotice(usingBuiltInTouch: Boolean) {
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

// BugReportDataDisclosure (was OpenNowScreens.kt:9418)
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

// streamTouchPassthrough (was OpenNowScreens.kt:9521)
@Composable
private fun Modifier.streamTouchPassthrough(id: String, inflate: Dp = 8.dp): Modifier {
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

// PASSTHROUGH_ID_PANEL (was OpenNowScreens.kt:9540)
private const val PASSTHROUGH_ID_PANEL = "controls-panel"

// PASSTHROUGH_ID_KEYBOARD (was OpenNowScreens.kt:9541)
private const val PASSTHROUGH_ID_KEYBOARD = "keyboard-bar"

// PASSTHROUGH_ID_EXIT (was OpenNowScreens.kt:9542)
private const val PASSTHROUGH_ID_EXIT = "exit-confirmation"

// StreamPanelHeader (was OpenNowScreens.kt:9544)
@Composable
private fun StreamPanelHeader(
    page: StreamControlsPage,
    gameTitle: String,
    status: String?,
    highlightDone: Boolean,
    focusRequester: FocusRequester,
    onBack: () -> Unit,
    onKeyboardOpen: () -> Unit,
    onExit: () -> Unit,
    onClose: () -> Unit,
    onButtonTone: () -> Unit,
) {
    val onMain = page == StreamControlsPage.Main
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                start = OpenNowSpacing.md + 2.dp,
                end = OpenNowSpacing.md + 2.dp,
                top = OpenNowSpacing.md + 2.dp,
                bottom = OpenNowSpacing.sm,
            ),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
    ) {
        if (!onMain) {
            StreamPanelHeaderButton(
                onClick = {
                    onButtonTone()
                    onBack()
                },
                modifier = Modifier.focusRequester(focusRequester),
            ) {
                Icon(
                    painter = painterResource(R.drawable.ic_arrow_back),
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                )
                Spacer(Modifier.width(6.dp))
                Text(stringResource(R.string.action_back), maxLines = 1)
            }
        }
        Column(Modifier.weight(1f)) {
            Text(
                stringResource(
                    when (page) {
                        StreamControlsPage.Main -> R.string.stream_panel_title
                        StreamControlsPage.StatusBar -> R.string.stream_statusbar_title
                        StreamControlsPage.TouchControls -> R.string.stream_touch_controls_title
                        StreamControlsPage.MouseMode -> R.string.stream_mouse_mode_title
                        StreamControlsPage.ReportProblem -> R.string.stream_report_problem_title
                    },
                ),
                style = MaterialTheme.typography.titleMedium,
            )
            Text(
                when (page) {
                    StreamControlsPage.Main -> gameTitle
                    StreamControlsPage.StatusBar -> stringResource(R.string.stream_statusbar_subtitle)
                    StreamControlsPage.TouchControls -> stringResource(R.string.stream_touch_controls_subtitle)
                    StreamControlsPage.MouseMode -> stringResource(R.string.stream_mouse_mode_subtitle)
                    StreamControlsPage.ReportProblem -> stringResource(R.string.stream_report_problem_subtitle)
                },
                color = TextMuted,
                style = MaterialTheme.typography.labelSmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (onMain) {
            if (status != null) {
                Text(status, color = TextMuted, style = MaterialTheme.typography.labelMedium, maxLines = 1)
            }
            StreamPanelHeaderButton(
                onClick = {
                    onButtonTone()
                    onKeyboardOpen()
                },
            ) {
                Icon(
                    painter = painterResource(R.drawable.ic_keyboard),
                    contentDescription = stringResource(R.string.stream_panel_cd_keyboard),
                    tint = TextPrimary,
                    modifier = Modifier.size(20.dp),
                )
            }
            StreamPanelHeaderButton(
                onClick = {
                    onButtonTone()
                    onExit()
                },
            ) {
                Text(stringResource(R.string.stream_panel_exit), maxLines = 1)
            }
            val doneAction = {
                onButtonTone()
                onClose()
            }
            if (highlightDone) {
                var doneFocused by remember { mutableStateOf(false) }
                Button(
                    onClick = doneAction,
                    modifier = Modifier
                        .focusRequester(focusRequester)
                        .onFocusChanged { doneFocused = it.isFocused },
                    border = BorderStroke(2.dp, if (doneFocused) MaterialTheme.colorScheme.primary else TextPrimary),
                    contentPadding = PaddingValues(horizontal = OpenNowSpacing.md, vertical = 6.dp),
                ) {
                    Text(stringResource(R.string.stream_panel_done), maxLines = 1)
                }
            } else {
                StreamPanelHeaderButton(onClick = doneAction, modifier = Modifier.focusRequester(focusRequester)) {
                    Text(stringResource(R.string.stream_panel_done), maxLines = 1)
                }
            }
        }
    }
}

/**
 * An outlined button that actually shows a focus ring. OutlinedButton alone gives no visible focus
 * state here, so the panel used to repeat this onFocusChanged + border pattern per button.
 */

// StreamPanelHeaderButton (was OpenNowScreens.kt:9668)
@Composable
private fun StreamPanelHeaderButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable RowScope.() -> Unit,
) {
    var focused by remember { mutableStateOf(false) }
    OutlinedButton(
        onClick = onClick,
        modifier = modifier.onFocusChanged { focused = it.isFocused },
        border = BorderStroke(
            width = if (focused) 2.dp else 1.dp,
            color = if (focused) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline,
        ),
        contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp),
        content = content,
    )
}

/** Slides forward going into a sub-page and back coming out of one. */

// streamPanelPageTransition (was OpenNowScreens.kt:9688)
private fun streamPanelPageTransition(
    from: StreamControlsPage,
    to: StreamControlsPage,
    reduceMotion: Boolean,
): ContentTransform {
    if (reduceMotion) {
        return fadeIn(tween(0)) togetherWith fadeOut(tween(0))
    }
    val forward = from == StreamControlsPage.Main && to != StreamControlsPage.Main
    val duration = OpenNowMotion.DurationStandard
    val easing = OpenNowMotion.EasingStandard
    return (
        slideInHorizontally(tween(duration, easing = easing)) { width -> if (forward) width / 6 else -width / 6 } +
            fadeIn(tween(duration, easing = easing))
        ) togetherWith (
        slideOutHorizontally(tween(duration, easing = easing)) { width -> if (forward) -width / 6 else width / 6 } +
            fadeOut(tween(OpenNowMotion.DurationFast, easing = easing))
        )
}

// BugReportSubmissionRequirements (was OpenNowScreens.kt:9708)
@Composable
private fun BugReportSubmissionRequirements(modifier: Modifier = Modifier) {
    Text(
        "Bug reports are currently supported only in English. Descriptions must be at least $ANDROID_BUG_REPORT_MIN_DESCRIPTION_CHARS characters and explain what happened. Non-English or non-descriptive reports will be ignored.",
        modifier = modifier.fillMaxWidth(),
        color = MaterialTheme.colorScheme.error,
        fontWeight = FontWeight.Bold,
        style = MaterialTheme.typography.bodyMedium,
    )
}

// BugReportVersionGateCard (was OpenNowScreens.kt:9719)
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

// BugReportPreflightDeckView (was OpenNowScreens.kt:9776)
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun BugReportPreflightDeckView(
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

// BugReportFormInputs (was OpenNowScreens.kt:9956)
@Composable
private fun BugReportFormInputs(
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

// StreamBugReporter (was OpenNowScreens.kt:10059)
@Composable
private fun StreamBugReporter(
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

// mouseModePageItems (was OpenNowScreens.kt:10365)
private fun LazyListScope.mouseModePageItems(
    settings: AppSettings,
    controllerMouseEmulationEnabled: Boolean,
    onControllerMouseEmulationToggle: () -> Unit,
    onMouseSensitivityChange: (Float) -> Unit,
    onMouseScrollSensitivityChange: (Int) -> Unit,
    onNativeTouchScrollScaleChange: (Float) -> Unit,
    onNativeTouchJitterThresholdChange: (Float) -> Unit,
    onButtonTone: () -> Unit,
) {
    item {
        ControlSwitchRow(
            label = "Enable Mouse Mode",
            checked = controllerMouseEmulationEnabled,
            onCheckedChange = {
                onButtonTone()
                onControllerMouseEmulationToggle()
            },
            value = onOffLabel(controllerMouseEmulationEnabled),
        )
    }
    if (controllerMouseEmulationEnabled) {
        item {
            ControlSliderRow(
                label = "Mouse sensitivity",
                value = settings.stream.mouseSensitivity,
                min = 0.25f,
                max = 3f,
                step = 0.05f,
                onChange = onMouseSensitivityChange,
                valueFormatter = { "%.2fx".format(it) }
            )
        }
        item {
            val scrollHint = when {
                settings.stream.mouseScrollSensitivity <= 20 -> "Fast"
                settings.stream.mouseScrollSensitivity <= 40 -> "Normal"
                settings.stream.mouseScrollSensitivity <= 60 -> "Precise"
                else -> "Slow"
            }
            ControlSliderRow(
                label = "Scroll sensitivity",
                value = settings.stream.mouseScrollSensitivity.toFloat(),
                min = 10f,
                max = 100f,
                step = 5f,
                onChange = { onMouseScrollSensitivityChange(it.toInt()) },
                descriptionProvider = { "Speed: $scrollHint" }
            )
        }
    }
    if (settings.androidTouch.nativeTouchMode != NativeTouchMode.Off) {
        item {
            val scrollSpeedLabel = when {
                settings.androidTouch.nativeTouchScrollScale <= 0.5f -> "Very slow"
                settings.androidTouch.nativeTouchScrollScale <= 0.8f -> "Slow"
                settings.androidTouch.nativeTouchScrollScale <= 1.2f -> "Normal"
                settings.androidTouch.nativeTouchScrollScale <= 1.6f -> "Fast"
                else -> "Very fast"
            }
            ControlSliderRow(
                label = "Touch scroll speed",
                value = settings.androidTouch.nativeTouchScrollScale,
                min = 0.25f,
                max = 2.0f,
                step = 0.05f,
                onChange = onNativeTouchScrollScaleChange,
                descriptionProvider = { scrollSpeedLabel }
            )
        }
        item {
            ControlSliderRow(
                label = "Touch tap stability",
                value = settings.androidTouch.nativeTouchJitterThresholdDp,
                min = 0f,
                max = 24f,
                step = 1f,
                onChange = onNativeTouchJitterThresholdChange,
                valueFormatter = { "${it.toInt()}dp" }
            )
        }
    }
}

// statusBarPageItems (was OpenNowScreens.kt:10450)
@OptIn(ExperimentalLayoutApi::class)
private fun LazyListScope.statusBarPageItems(
    settings: AppSettings,
    statsVisible: Boolean,
    onStatsToggle: () -> Unit,
    onStatsStyleCycle: () -> Unit,
    onStatsPositionCycle: () -> Unit,
    onStatsMetricsChange: (StreamStatsMetrics) -> Unit,
    onButtonTone: () -> Unit,
) {
    val metrics = settings.streamStatsMetrics
    item {
        ControlSwitchRow(
            label = stringResource(R.string.common_visible),
            checked = statsVisible,
            onCheckedChange = {
                onButtonTone()
                onStatsToggle()
            },
            value = onOffLabel(statsVisible),
        )
    }
    item {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
            ControlActionRow(
                label = stringResource(R.string.stream_statusbar_appearance),
                actionLabel = settings.streamStatsStyle.label,
                onClick = {
                    onButtonTone()
                    onStatsStyleCycle()
                },
                modifier = Modifier.weight(1f),
            )
            ControlActionRow(
                label = stringResource(R.string.stream_statusbar_position),
                actionLabel = settings.streamStatsPosition.label,
                onClick = {
                    onButtonTone()
                    onStatsPositionCycle()
                },
                modifier = Modifier.weight(1f),
            )
        }
    }
    item {
        Text(
            stringResource(R.string.stream_statusbar_items),
            color = TextMuted,
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.Bold,
        )
    }
    item {
        // Ten small toggles side by side; the standard row height would waste the panel.
        val statusBarMetricStyle = ControlRowStyle.stream().copy(
            verticalPadding = 6.dp,
            labelStyle = MaterialTheme.typography.labelMedium,
        )
        BoxWithConstraints(Modifier.fillMaxWidth()) {
            val columns = when {
                maxWidth >= 800.dp -> 5
                maxWidth >= 620.dp -> 4
                maxWidth >= 460.dp -> 3
                else -> 2
            }
            val gap = 8.dp
            val itemWidth = (maxWidth - gap * (columns - 1)) / columns.toFloat()
            FlowRow(
                modifier = Modifier.fillMaxWidth(),
                maxItemsInEachRow = columns,
                horizontalArrangement = Arrangement.spacedBy(gap),
                verticalArrangement = Arrangement.spacedBy(gap),
            ) {
                ControlSwitchRow(
                    label = stringResource(R.string.stream_statusbar_metric_fps),
                    checked = metrics.fps,
                    onCheckedChange = {
                    onButtonTone()
                    onStatsMetricsChange(metrics.copy(fps = !metrics.fps))
                    },
                    modifier = Modifier.width(itemWidth),
                    style = statusBarMetricStyle,
                )
                ControlSwitchRow(
                    label = stringResource(R.string.stream_statusbar_metric_ping),
                    checked = metrics.ping,
                    onCheckedChange = {
                    onButtonTone()
                    onStatsMetricsChange(metrics.copy(ping = !metrics.ping))
                    },
                    modifier = Modifier.width(itemWidth),
                    style = statusBarMetricStyle,
                )
                ControlSwitchRow(
                    label = stringResource(R.string.stream_statusbar_metric_bitrate),
                    checked = metrics.bitrate,
                    onCheckedChange = {
                    onButtonTone()
                    onStatsMetricsChange(metrics.copy(bitrate = !metrics.bitrate))
                    },
                    modifier = Modifier.width(itemWidth),
                    style = statusBarMetricStyle,
                )
                ControlSwitchRow(
                    label = stringResource(R.string.stream_statusbar_metric_battery),
                    checked = metrics.battery,
                    onCheckedChange = {
                    onButtonTone()
                    onStatsMetricsChange(metrics.copy(battery = !metrics.battery))
                    },
                    modifier = Modifier.width(itemWidth),
                    style = statusBarMetricStyle,
                )
                ControlSwitchRow(
                    label = stringResource(R.string.stream_statusbar_metric_connection),
                    checked = metrics.connection,
                    onCheckedChange = {
                    onButtonTone()
                    onStatsMetricsChange(metrics.copy(connection = !metrics.connection))
                    },
                    modifier = Modifier.width(itemWidth),
                    style = statusBarMetricStyle,
                )
                ControlSwitchRow(
                    label = stringResource(R.string.stream_statusbar_metric_resolution),
                    checked = metrics.resolution,
                    onCheckedChange = {
                    onButtonTone()
                    onStatsMetricsChange(metrics.copy(resolution = !metrics.resolution))
                    },
                    modifier = Modifier.width(itemWidth),
                    style = statusBarMetricStyle,
                )
                ControlSwitchRow(
                    label = stringResource(R.string.stream_statusbar_metric_codec),
                    checked = metrics.codec,
                    onCheckedChange = {
                    onButtonTone()
                    onStatsMetricsChange(metrics.copy(codec = !metrics.codec))
                    },
                    modifier = Modifier.width(itemWidth),
                    style = statusBarMetricStyle,
                )
                ControlSwitchRow(
                    label = stringResource(R.string.stream_statusbar_metric_server),
                    checked = metrics.location,
                    onCheckedChange = {
                    onButtonTone()
                    onStatsMetricsChange(metrics.copy(location = !metrics.location))
                    },
                    modifier = Modifier.width(itemWidth),
                    style = statusBarMetricStyle,
                )
                ControlSwitchRow(
                    label = stringResource(R.string.stream_statusbar_metric_latency),
                    checked = metrics.latency,
                    onCheckedChange = {
                    onButtonTone()
                    onStatsMetricsChange(metrics.copy(latency = !metrics.latency))
                    },
                    modifier = Modifier.width(itemWidth),
                    style = statusBarMetricStyle,
                )
                ControlSwitchRow(
                    label = stringResource(R.string.stream_statusbar_metric_loss),
                    checked = metrics.packetLoss,
                    onCheckedChange = {
                    onButtonTone()
                    onStatsMetricsChange(metrics.copy(packetLoss = !metrics.packetLoss))
                    },
                    modifier = Modifier.width(itemWidth),
                    style = statusBarMetricStyle,
                )
            }
        }
    }
}

/**
 * The three bare key buttons in the Input section. Extracted so the manual focus-ring pattern the
 * panel needs lives in one place instead of being repeated per button.
 */

// StreamPanelKeyButton (was OpenNowScreens.kt:10632)
@Composable
private fun StreamPanelKeyButton(label: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    var focused by remember { mutableStateOf(false) }
    OutlinedButton(
        onClick = onClick,
        modifier = modifier.onFocusChanged { focused = it.isFocused },
        border = BorderStroke(
            width = if (focused) 2.dp else 1.dp,
            color = if (focused) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline,
        ),
    ) {
        Text(label, maxLines = 1)
    }
}

/**
 * A touch-layout slider. Unlike the settings sliders these preview on every drag frame, because
 * the overlay they are adjusting is on screen underneath the panel and watching it move is the
 * point of the control.
 */

// TouchLayoutSlider (was OpenNowScreens.kt:10652)
@Composable
private fun TouchLayoutSlider(
    @StringRes labelRes: Int,
    value: Float,
    min: Float,
    max: Float,
    step: Float,
    onChange: (Float) -> Unit,
    unit: String? = null,
) {
    ControlSliderRow(
        label = stringResource(labelRes),
        value = value,
        min = min,
        max = max,
        step = step,
        onChange = onChange,
        onChangePreview = onChange,
        unit = unit,
    )
}

/** "On" / "Off", so the same boolean reads the same way everywhere. */

// onOffLabel (was OpenNowScreens.kt:10675)
@Composable
private fun onOffLabel(enabled: Boolean): String =
    stringResource(if (enabled) R.string.common_on else R.string.common_off)

// SHARPENING_SLIDER_STEP (was OpenNowScreens.kt:10679)
private const val SHARPENING_SLIDER_STEP = 0.05f

// TOUCH_SCALE_SLIDER_STEP (was OpenNowScreens.kt:10680)
private const val TOUCH_SCALE_SLIDER_STEP = 0.05f

// TOUCH_DP_SLIDER_STEP (was OpenNowScreens.kt:10681)
private const val TOUCH_DP_SLIDER_STEP = 2f

// JOYSTICK_DEAD_ZONE_STEP (was OpenNowScreens.kt:10682)
private const val JOYSTICK_DEAD_ZONE_STEP = 0.01f

// DP_UNIT (was OpenNowScreens.kt:10683)
private const val DP_UNIT = "dp"

// StreamKeyboardBar (was OpenNowScreens.kt:10685)
@Composable
private fun StreamKeyboardBar(
    text: String,
    onTextChange: (String) -> Unit,
    onSend: () -> Unit,
    onBackspace: () -> Unit,
    onEnter: () -> Unit,
    onEsc: () -> Unit,
    onDone: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val inputFocusRequester = remember { FocusRequester() }
    val keyboardController = LocalSoftwareKeyboardController.current
    val sendIfReady = {
        if (text.isNotBlank()) {
            onSend()
        }
    }
    LaunchedEffect(Unit) {
        delay(80)
        runCatching { inputFocusRequester.requestFocus() }
        keyboardController?.show()
    }
    Surface(
        modifier = modifier
            .fillMaxWidth()
            // The keyboard bar registered no passthrough bounds at all, so on a phone every tap on
            // it — including on the text field — was also forwarded into the game as touch input.
            .streamTouchPassthrough(PASSTHROUGH_ID_KEYBOARD),
        // Bottom-anchored, so only the top corners round. It was the one square-cornered overlay.
        shape = RoundedCornerShape(topStart = OpenNowRadius.lg, topEnd = OpenNowRadius.lg),
        color = OpenNowPalette.PanelOverVideo,
        border = BorderStroke(1.dp, OpenNowPalette.PanelHairline),
        tonalElevation = 8.dp,
    ) {
        Column(Modifier.padding(OpenNowSpacing.md), verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
            OutlinedTextField(
                value = text,
                onValueChange = onTextChange,
                modifier = Modifier
                    .fillMaxWidth()
                    .focusRequester(inputFocusRequester),
                singleLine = true,
                placeholder = { Text("Type into stream", color = TextMuted) },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email, imeAction = ImeAction.Send),
                keyboardActions = KeyboardActions(onSend = { sendIfReady() }),
                trailingIcon = {
                    TextButton(
                        onClick = {
                            if (text.length < MAX_STREAM_KEYBOARD_TEXT_LENGTH) {
                                onTextChange("$text@")
                            }
                        },
                    ) {
                        Text("@")
                    }
                },
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                Button(onClick = sendIfReady, enabled = text.isNotBlank(), modifier = Modifier.weight(1f)) { Text("Send") }
                OutlinedButton(onClick = onBackspace, modifier = Modifier.weight(1f)) { Text("⌫") }
                OutlinedButton(onClick = onEnter, modifier = Modifier.weight(1f)) { Text("Enter") }
                OutlinedButton(onClick = onEsc, modifier = Modifier.weight(1f)) { Text("Esc") }
                TextButton(
                    onClick = {
                        keyboardController?.hide()
                        onDone()
                    },
                ) { Text("Done") }
            }
        }
    }
}

// MAX_STREAM_KEYBOARD_TEXT_LENGTH (was OpenNowScreens.kt:10759)
private const val MAX_STREAM_KEYBOARD_TEXT_LENGTH = 4096

// StreamStatsPill (was OpenNowScreens.kt:10761)
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

// ActiveStreamModePill (was OpenNowScreens.kt:10816)
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

// ActiveStreamModeDisplayChange (was OpenNowScreens.kt:11091)
internal data class ActiveStreamModeDisplayChange(
    val label: String,
    val requestedValue: String,
    val actualValue: String,
)

// activeStreamModeDisplayChanges (was OpenNowScreens.kt:11097)
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

// onOffLabel (was OpenNowScreens.kt:11155)
private fun Boolean.onOffLabel(): String = if (this) "On" else "Off"

// ActiveStreamModeCauseAssessment (was OpenNowScreens.kt:11157)
internal data class ActiveStreamModeCauseAssessment(
    val summary: String,
)

// activeStreamModeCauseAssessment (was OpenNowScreens.kt:11161)
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

// ActiveStreamModeDeveloperReport (was OpenNowScreens.kt:11198)
internal data class ActiveStreamModeDeveloperReport(
    val title: String,
    val description: String,
)

// activeStreamModeDeveloperReport (was OpenNowScreens.kt:11203)
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

// StreamStatsMetricItems (was OpenNowScreens.kt:11233)
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

// NO_STAT_VALUE (was OpenNowScreens.kt:11319)
private const val NO_STAT_VALUE = "--"

// StreamStatsText (was OpenNowScreens.kt:11321)
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

// labelRes (was OpenNowScreens.kt:11350)
@StringRes
private fun StreamQualityLevel.labelRes(): Int = when (this) {
    StreamQualityLevel.Good -> R.string.stream_quality_good
    StreamQualityLevel.Fair -> R.string.stream_quality_fair
    StreamQualityLevel.Poor -> R.string.stream_quality_poor
}

// CompactStreamDeviceStatus (was OpenNowScreens.kt:11357)
private data class CompactStreamDeviceStatus(
    val batteryPercent: Int? = null,
    val batteryCharging: Boolean = false,
    val networkKind: AndroidNetworkKind = AndroidNetworkKind.Unknown,
    val networkBars: Int? = null,
    val cellularGeneration: String? = null,
)

// rememberCompactStreamDeviceStatus (was OpenNowScreens.kt:11365)
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

// readCompactStreamDeviceStatus (was OpenNowScreens.kt:11379)
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

// StreamBatteryIndicator (was OpenNowScreens.kt:11390)
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

// StreamNetworkIndicator (was OpenNowScreens.kt:11442)
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

// formatRuntimeResolution (was OpenNowScreens.kt:11500)
internal fun formatRuntimeResolution(resolution: String): String {
    val parts = resolution.lowercase(Locale.US).split("x", limit = 2)
    return if (parts.size == 2 && parts.all { it.trim().isNotBlank() }) {
        "${parts[0].trim()}x${parts[1].trim()}"
    } else {
        resolution
    }
}

// formatRuntimeBitrate (was OpenNowScreens.kt:11509)
internal fun formatRuntimeBitrate(bitrateKbps: Int?): String {
    val kbps = bitrateKbps ?: return "--"
    return if (kbps >= 1000) {
        "${(kbps / 1000.0).let { kotlin.math.round(it * 10.0) / 10.0 }} Mbps"
    } else {
        "$kbps Kbps"
    }
}

// shouldHideStreamStatusText (was OpenNowScreens.kt:11518)
private fun shouldHideStreamStatusText(status: String): Boolean =
    status.trim().replace('_', ' ').let {
        it.equals("Streaming", ignoreCase = true) ||
            it.equals("ICE CONNECTED", ignoreCase = true) ||
            it.equals("ICE COMPLETED", ignoreCase = true)
    }

// InitialStreamConnectionStatus (was OpenNowScreens.kt:11525)
internal data class InitialStreamConnectionStatus(
    val phase: String,
    val title: String,
    val detail: String,
)

// initialStreamConnectionStatus (was OpenNowScreens.kt:11531)
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

// InitialStreamConnectionOverlay (was OpenNowScreens.kt:11594)
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

// StreamExitConfirmation (was OpenNowScreens.kt:11663)
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

// QueueLoadingScreen (was OpenNowScreens.kt:11741)
@Composable
private fun QueueLoadingScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val session = state.streamSession
    val game = state.streamGame
    val ads = sessionAdItems(session?.adState)
    val ad = ads.firstOrNull { it.adId == state.queueAdActiveId } ?: ads.firstOrNull()
    val mediaUrl = ad?.adMediaFiles?.firstOrNull { !it.mediaFileUrl.isNullOrBlank() }?.mediaFileUrl
        ?: ad?.adUrl
        ?: ad?.mediaUrl
    val queuePosition = activeQueuePosition(state)
    val visibleQueuePosition = rememberStableQueuePosition(queuePosition)
    val queueCopy = queueLaunchStatusText(state, visibleQueuePosition)
    val hasPlayableAd = ad != null && mediaUrl != null

    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .clipToBounds(),
        contentAlignment = Alignment.Center,
    ) {
        QueueAmbientBackdrop(
            accent = state.settings.uiAccent.color,
            queuePosition = visibleQueuePosition,
        )
        val useLandscapeAdLayout = hasPlayableAd && maxWidth > maxHeight

        Box(
            Modifier
                .fillMaxSize()
                .padding(18.dp),
            contentAlignment = Alignment.Center,
        ) {
            if (ad != null && mediaUrl != null) {
                QueueAdPanel(
                    ad = ad,
                    mediaUrl = mediaUrl,
                    viewModel = viewModel,
                    game = game,
                    queueCopy = queueCopy,
                    queuePosition = visibleQueuePosition,
                    error = state.error,
                    playbackKey = session?.sessionId.orEmpty(),
                    compact = useLandscapeAdLayout,
                    onMinimize = viewModel::minimizeStreamLaunch,
                    onCancel = viewModel::stopStream,
                    modifier = Modifier
                        .fillMaxWidth(if (useLandscapeAdLayout) 0.72f else 1f)
                        .widthIn(max = if (useLandscapeAdLayout) 900.dp else 620.dp),
                )
            } else {
                QueueStatusPanel(
                    game = game,
                    queueCopy = queueCopy,
                    queuePosition = visibleQueuePosition,
                    error = state.error,
                    compact = false,
                    onMinimize = viewModel::minimizeStreamLaunch,
                    onCancel = viewModel::stopStream,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
}

// QueueAmbientBackdrop (was OpenNowScreens.kt:11806)
@Composable
private fun QueueAmbientBackdrop(
    accent: Color,
    queuePosition: Int?,
    modifier: Modifier = Modifier,
) {
    val transition = rememberInfiniteTransition(label = "queue-ambient")
    val driftA by transition.animateFloat(
        initialValue = -1f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 11000, easing = LinearEasing),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "queue-ambient-drift-a",
    )
    val driftB by transition.animateFloat(
        initialValue = 1f,
        targetValue = -1f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 14000, easing = LinearEasing),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "queue-ambient-drift-b",
    )
    val phase by transition.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 16000, easing = LinearEasing),
        ),
        label = "queue-ambient-phase",
    )
    val shimmer by transition.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 5200, easing = LinearEasing),
        ),
        label = "queue-ambient-shimmer",
    )
    val orbADim by transition.animateFloat(
        initialValue = 0.35f,
        targetValue = 0.72f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 8200, easing = LinearEasing),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "queue-ambient-orb-a-dim",
    )
    val orbBDim by transition.animateFloat(
        initialValue = 0.26f,
        targetValue = 0.56f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 9800, easing = LinearEasing),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "queue-ambient-orb-b-dim",
    )

    BoxWithConstraints(
        modifier
            .fillMaxSize()
            .background(
                Brush.verticalGradient(
                    listOf(
                        Color(0xff010203),
                        Color(0xff05080a),
                        Color(0xff020304),
                    ),
                ),
            ),
    ) {
        val baseSize = minOf(maxWidth, maxHeight)
        QueueAmbientOrb(
            color = accent,
            size = baseSize * 0.92f,
            alpha = orbADim,
            modifier = Modifier
                .align(Alignment.TopStart)
                .offset(
                    x = maxWidth * (-0.22f + 0.10f * driftA),
                    y = maxHeight * (0.02f + 0.08f * driftB),
                ),
        )
        QueueAmbientOrb(
            color = Color(0xff2bdcff),
            size = baseSize * 0.7f,
            alpha = orbBDim,
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .offset(
                    x = maxWidth * (0.15f + 0.08f * driftB),
                    y = maxHeight * (0.10f + 0.07f * driftA),
                ),
        )
        QueueSignalField(
            accent = accent,
            queuePosition = queuePosition,
            phase = phase,
            shimmer = shimmer,
            modifier = Modifier.matchParentSize(),
        )
        Box(
            Modifier
                .matchParentSize()
                .background(Color.Black.copy(alpha = 0.34f)),
        )
    }
}

// QueueAmbientOrb (was OpenNowScreens.kt:11917)
@Composable
private fun QueueAmbientOrb(
    color: Color,
    size: Dp,
    alpha: Float,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier
            .size(size)
            .blur(64.dp)
            .graphicsLayer(alpha = alpha.coerceIn(0f, 1f))
            .background(
                Brush.radialGradient(
                    listOf(
                        color.copy(alpha = 0.58f),
                        color.copy(alpha = 0.16f),
                        Color.Transparent,
                    ),
                ),
                CircleShape,
            ),
    )
}

// QueueSignalField (was OpenNowScreens.kt:11942)
@Composable
private fun QueueSignalField(
    accent: Color,
    queuePosition: Int?,
    phase: Float,
    shimmer: Float,
    modifier: Modifier = Modifier,
) {
    val heat = queueUrgency(queuePosition)
    Canvas(modifier) {
        val lineCount = 9
        val spacing = size.height / lineCount
        val offset = shimmer * spacing
        for (index in -1..lineCount) {
            val y = index * spacing + offset
            drawLine(
                color = accent.copy(alpha = 0.035f + heat * 0.035f),
                start = Offset(-size.width * 0.12f, y),
                end = Offset(size.width * 1.08f, y - size.height * 0.10f),
                strokeWidth = 1.dp.toPx(),
            )
        }
        repeat(12) { index ->
            val lane = index + 1
            val x = ((lane * 0.173f + phase * (0.08f + lane * 0.006f)) % 1f) * size.width
            val y = ((lane * 0.291f + shimmer * (0.12f + lane * 0.004f)) % 1f) * size.height
            drawCircle(
                color = accent.copy(alpha = 0.05f + heat * 0.04f),
                radius = (1.5f + (index % 4)) * density,
                center = Offset(x, y),
            )
        }
    }
}

// AnimatedQueueStatusText (was OpenNowScreens.kt:11977)
@Composable
private fun AnimatedQueueStatusText(
    queueCopy: String,
    queuePosition: Int?,
    compact: Boolean,
    modifier: Modifier = Modifier,
) {
    if (queuePosition == null) {
        Text(
            queueCopy,
            modifier = modifier,
            color = queueIdleStatusColor(queueCopy),
            style = (if (compact) MaterialTheme.typography.bodyLarge else MaterialTheme.typography.titleMedium)
                .copy(fontWeight = FontWeight.Normal),
            textAlign = TextAlign.Center,
        )
        return
    }

    var previousQueuePosition by remember { mutableStateOf<Int?>(null) }
    val numberProgress = remember { Animatable(1f) }
    var numberTrigger by remember { mutableStateOf(0) }
    var numberFrom by remember { mutableStateOf(queuePosition.toString()) }
    var numberTo by remember { mutableStateOf(queuePosition.toString()) }
    val heat = queueUrgency(queuePosition)
    val hotQueue = queuePosition < 10
    val transition = rememberInfiniteTransition(label = "queue-status-glow")
    val glow by transition.animateFloat(
        initialValue = 0.55f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = if (hotQueue) 520 else 1100, easing = LinearEasing),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "queue-status-glow-alpha",
    )
    val moleculePhase by transition.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(
                durationMillis = (190 - heat * 95).roundToInt().coerceIn(92, 190),
                easing = LinearEasing,
            ),
        ),
        label = "queue-status-molecule-phase",
    )
    val statusColor by animateColorAsState(
        targetValue = queueUrgencyColor(queuePosition),
        animationSpec = tween(durationMillis = 240),
        label = "queue-status-color",
    )

    LaunchedEffect(queuePosition) {
        val current = queuePosition
        val previous = previousQueuePosition
        if (previous != null && current < previous) {
            numberFrom = previous.toString()
            numberTo = current.toString()
            numberTrigger += 1
        } else {
            numberFrom = current.toString()
            numberTo = current.toString()
        }
        previousQueuePosition = current
    }

    LaunchedEffect(numberTrigger) {
        if (numberTrigger == 0) return@LaunchedEffect
        numberProgress.snapTo(0f)
        numberProgress.animateTo(
            targetValue = 1f,
            animationSpec = tween(durationMillis = if (hotQueue) 320 else 420),
        )
    }

    val moleculeCagePx = with(LocalDensity.current) {
        (if (hotQueue) (0.45f + heat * 1.45f).dp else 0.dp).toPx()
    }
    val shakeX = if (hotQueue) {
        (sin(moleculePhase * 31.415928f) * 0.64f + sin(moleculePhase * 106.81416f) * 0.36f) * moleculeCagePx
    } else {
        0f
    }
    val shakeY = if (hotQueue) {
        (sin(moleculePhase * 43.982296f) * 0.55f + sin(moleculePhase * 81.68141f) * 0.45f) * moleculeCagePx * 0.55f
    } else {
        0f
    }
    val parts = queueStatusParts(queueCopy, queuePosition)
    val textStyle = (if (compact) MaterialTheme.typography.bodyLarge else MaterialTheme.typography.titleMedium)
        .copy(fontWeight = FontWeight.Normal)
    val numberPhase = numberProgress.value
    val numberAnimating = numberPhase < 1f
    val numberTravelPx = with(LocalDensity.current) { (if (compact) 18.dp else 22.dp).toPx() }

    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            parts.prefix,
            color = TextMuted,
            style = textStyle,
            textAlign = TextAlign.Center,
        )
        AnimatedQueueNumber(
            currentNumber = parts.number,
            previousNumber = numberFrom,
            targetNumber = numberTo,
            animating = numberAnimating,
            phase = numberPhase,
            travelPx = numberTravelPx,
            color = statusColor,
            style = textStyle.copy(
                shadow = Shadow(
                    color = statusColor.copy(alpha = heat * (0.38f + 0.42f * glow)),
                    offset = Offset(0f, 0f),
                    blurRadius = 18f + heat * 18f,
                ),
            ),
            shakeX = shakeX,
            shakeY = shakeY,
        )
        Text(
            parts.suffix,
            color = TextMuted,
            style = textStyle,
            textAlign = TextAlign.Center,
        )
    }
}

// AnimatedQueueNumber (was OpenNowScreens.kt:12111)
@Composable
private fun AnimatedQueueNumber(
    currentNumber: String,
    previousNumber: String,
    targetNumber: String,
    animating: Boolean,
    phase: Float,
    travelPx: Float,
    color: Color,
    style: TextStyle,
    shakeX: Float,
    shakeY: Float,
) {
    val fromNumber = if (animating) previousNumber else currentNumber
    val toNumber = if (animating) targetNumber else currentNumber
    val slotCount = toNumber.length

    Row(
        modifier = Modifier
            .clipToBounds()
            .graphicsLayer(
                translationX = shakeX,
                translationY = shakeY,
            ),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        repeat(slotCount) { slotIndex ->
            val fromDigit = fromNumber.rightAlignedCharAt(slotIndex, slotCount)
            val toDigit = toNumber.rightAlignedCharAt(slotIndex, slotCount)
            QueueNumberDigitSlot(
                fromDigit = fromDigit,
                toDigit = toDigit,
                digitChanged = animating && fromDigit != toDigit,
                phase = phase,
                travelPx = travelPx,
                color = color,
                style = style,
            )
        }
    }
}

// QueueNumberDigitSlot (was OpenNowScreens.kt:12153)
@Composable
private fun QueueNumberDigitSlot(
    fromDigit: Char?,
    toDigit: Char?,
    digitChanged: Boolean,
    phase: Float,
    travelPx: Float,
    color: Color,
    style: TextStyle,
) {
    val from = fromDigit?.toString().orEmpty()
    val to = toDigit?.toString().orEmpty()
    Box(
        modifier = Modifier.clipToBounds(),
        contentAlignment = Alignment.Center,
    ) {
        if (from.isNotEmpty()) {
            Text(
                from,
                modifier = Modifier.graphicsLayer(alpha = 0f),
                color = color,
                style = style,
                textAlign = TextAlign.Center,
            )
        }
        if (to.isNotEmpty() && to != from) {
            Text(
                to,
                modifier = Modifier.graphicsLayer(alpha = 0f),
                color = color,
                style = style,
                textAlign = TextAlign.Center,
            )
        }
        if (digitChanged) {
            if (from.isNotEmpty()) {
                Text(
                    from,
                    modifier = Modifier.graphicsLayer(
                        translationY = -travelPx * phase,
                        scaleX = 1f - phase * 0.03f,
                        scaleY = 1f - phase * 0.03f,
                        alpha = 1f - phase,
                    ),
                    color = color,
                    style = style,
                    textAlign = TextAlign.Center,
                )
            }
            if (to.isNotEmpty()) {
                Text(
                    to,
                    modifier = Modifier.graphicsLayer(
                        translationY = travelPx * (1f - phase),
                        scaleX = 0.97f + phase * 0.03f,
                        scaleY = 0.97f + phase * 0.03f,
                        alpha = phase,
                    ),
                    color = color,
                    style = style,
                    textAlign = TextAlign.Center,
                )
            }
        } else if (to.isNotEmpty()) {
            Text(
                to,
                color = color,
                style = style,
                textAlign = TextAlign.Center,
            )
        }
    }
}

// rightAlignedCharAt (was OpenNowScreens.kt:12227)
private fun String.rightAlignedCharAt(slotIndex: Int, slotCount: Int): Char? =
    getOrNull(length - slotCount + slotIndex)

// QueueStatusParts (was OpenNowScreens.kt:12230)
private data class QueueStatusParts(
    val prefix: String,
    val number: String,
    val suffix: String,
)

// queueStatusParts (was OpenNowScreens.kt:12236)
private fun queueStatusParts(queueCopy: String, queuePosition: Int): QueueStatusParts {
    val number = queuePosition.toString()
    val index = queueCopy.indexOf(number)
    if (index < 0) {
        return QueueStatusParts(prefix = "$queueCopy ", number = number, suffix = "")
    }
    return QueueStatusParts(
        prefix = queueCopy.substring(0, index),
        number = number,
        suffix = queueCopy.substring(index + number.length),
    )
}

// queueUrgency (was OpenNowScreens.kt:12249)
private fun queueUrgency(queuePosition: Int?): Float {
    val position = queuePosition ?: return 0f
    if (position >= 10) return 0f
    return ((10 - position).toFloat() / 9f).coerceIn(0f, 1f)
}

// activeQueuePosition (was OpenNowScreens.kt:12255)
private fun activeQueuePosition(state: OpenNowUiState): Int? =
    queueDisplayPosition(state)

// rememberStableQueuePosition (was OpenNowScreens.kt:12258)
@Composable
private fun rememberStableQueuePosition(queuePosition: Int?): Int? {
    var stableQueuePosition by remember { mutableStateOf(queuePosition) }
    LaunchedEffect(queuePosition) {
        if (queuePosition == stableQueuePosition) return@LaunchedEffect
        if (queuePosition == null || stableQueuePosition == null) {
            stableQueuePosition = queuePosition
            return@LaunchedEffect
        }
        delay(QUEUE_POSITION_VISUAL_SETTLE_MS)
        stableQueuePosition = queuePosition
    }
    return stableQueuePosition
}

// queueLaunchStatusText (was OpenNowScreens.kt:12273)
internal fun queueLaunchStatusText(state: OpenNowUiState, queuePosition: Int?): String =
    queuePosition?.let { "Queue position $it" } ?: queueLaunchStatusText(state)

// queueIdleStatusColor (was OpenNowScreens.kt:12276)
private fun queueIdleStatusColor(queueCopy: String): Color =
    if (queueCopy.equals("Starting session", ignoreCase = true)) Green else TextMuted

// queueUrgencyColor (was OpenNowScreens.kt:12279)
private fun queueUrgencyColor(queuePosition: Int?): Color {
    val heat = queueUrgency(queuePosition)
    if (heat <= 0f) return TextMuted
    val green = (0.57f - 0.49f * heat).coerceIn(0.06f, 0.57f)
    val blue = (0.25f - 0.17f * heat).coerceIn(0.08f, 0.25f)
    return Color(red = 1f, green = green, blue = blue, alpha = 1f)
}

// QueueStatusPanel (was OpenNowScreens.kt:12287)
@Composable
private fun QueueStatusPanel(
    game: GameInfo?,
    queueCopy: String,
    queuePosition: Int?,
    error: String?,
    compact: Boolean,
    onMinimize: () -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    Column(
        modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        val imageWidth = if (compact) 154.dp else 220.dp
        UrlImage(
            gameTvBannerImageUrl(context, game),
            Modifier
                .width(imageWidth)
                .aspectRatio(16f / 9f)
                .clip(RoundedCornerShape(14.dp)),
        )
        Spacer(Modifier.height(if (compact) 12.dp else 16.dp))
        Text(
            game?.title ?: "Starting stream",
            color = TextPrimary,
            style = if (compact) MaterialTheme.typography.titleLarge else MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.Bold,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.Center,
        )
        AnimatedQueueStatusText(
            queueCopy = queueCopy,
            queuePosition = queuePosition,
            compact = compact,
        )
        Spacer(Modifier.height(if (compact) 14.dp else 18.dp))
        LinearProgressIndicator(Modifier.fillMaxWidth(if (compact) 0.9f else 0.7f))
        Spacer(Modifier.height(12.dp))
        Row(
            Modifier.fillMaxWidth(if (compact) 0.92f else 0.7f),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            OutlinedButton(onClick = onMinimize, modifier = Modifier.weight(1f)) {
                Text("Minimize", maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            OutlinedButton(onClick = onCancel, modifier = Modifier.weight(1f)) {
                Text("Cancel", maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        if (compact && queuePosition != null) {
            Spacer(Modifier.height(14.dp))
            LandscapeQueuePositionDock(queuePosition = queuePosition)
        }
        error?.let {
            Spacer(Modifier.height(12.dp))
            Text(it, color = MaterialTheme.colorScheme.error, textAlign = TextAlign.Center)
        }
    }
}

// LandscapeQueuePositionDock (was OpenNowScreens.kt:12352)
@Composable
private fun LandscapeQueuePositionDock(queuePosition: Int, modifier: Modifier = Modifier) {
    val accent = queueUrgencyColor(queuePosition)
    val heat = queueUrgency(queuePosition)
    val shape = RoundedCornerShape(OpenNowRadius.lg)
    Box(
        modifier
            .fillMaxWidth(0.92f)
            .clip(shape)
            .background(
                Brush.horizontalGradient(
                    listOf(
                        accent.copy(alpha = 0.18f + heat * 0.16f),
                        PanelAlt.copy(alpha = 0.94f),
                        Color.Black.copy(alpha = 0.36f),
                    ),
                ),
            )
            .border(1.dp, accent.copy(alpha = 0.32f + heat * 0.36f), shape)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Row(
            Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.Bottom,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(
                    "Queue",
                    color = TextMuted,
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = FontWeight.Bold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    "Live position",
                    color = TextMuted.copy(alpha = 0.78f),
                    style = MaterialTheme.typography.labelSmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Text(
                queuePosition.toString(),
                color = accent,
                style = MaterialTheme.typography.displaySmall.copy(
                    fontWeight = FontWeight.Black,
                    shadow = Shadow(
                        color = accent.copy(alpha = 0.24f + heat * 0.42f),
                        offset = Offset(0f, 0f),
                        blurRadius = 18f + heat * 14f,
                    ),
                ),
                maxLines = 1,
                textAlign = TextAlign.End,
            )
        }
    }
}

// QueueAdPanel (was OpenNowScreens.kt:12413)
@Composable
private fun QueueAdPanel(
    ad: SessionAdInfo,
    mediaUrl: String,
    viewModel: OpenNowViewModel,
    game: GameInfo?,
    queueCopy: String,
    queuePosition: Int?,
    error: String?,
    playbackKey: String,
    compact: Boolean,
    onMinimize: () -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(18.dp),
        color = Panel.copy(alpha = 0.95f),
        tonalElevation = 8.dp,
    ) {
        if (compact) {
            Row(
                Modifier.padding(14.dp),
                horizontalArrangement = Arrangement.spacedBy(14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                QueueAdPlayback(
                    ad = ad,
                    mediaUrl = mediaUrl,
                    playbackKey = playbackKey,
                    viewModel = viewModel,
                    modifier = Modifier
                        .weight(1.55f)
                        .aspectRatio(16f / 9f),
                )
                Column(
                    Modifier.weight(1f),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    QueueAdHeading(game = game, compact = true)
                    QueueStatusAndActions(
                        queueCopy = queueCopy,
                        queuePosition = queuePosition,
                        compact = true,
                        stackActions = true,
                        onMinimize = onMinimize,
                        onCancel = onCancel,
                    )
                    error?.let {
                        Text(it, color = MaterialTheme.colorScheme.error, textAlign = TextAlign.Center)
                    }
                }
            }
        } else {
            Column(
                Modifier.padding(16.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                QueueAdHeading(game = game, compact = false)
                QueueAdPlayback(
                    ad = ad,
                    mediaUrl = mediaUrl,
                    playbackKey = playbackKey,
                    viewModel = viewModel,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(220.dp),
                )
                QueueStatusAndActions(
                    queueCopy = queueCopy,
                    queuePosition = queuePosition,
                    compact = false,
                    stackActions = false,
                    onMinimize = onMinimize,
                    onCancel = onCancel,
                )
                error?.let {
                    Text(it, color = MaterialTheme.colorScheme.error, textAlign = TextAlign.Center)
                }
            }
        }
    }
}

// QueueAdPlayback (was OpenNowScreens.kt:12499)
@Composable
private fun QueueAdPlayback(
    ad: SessionAdInfo,
    mediaUrl: String,
    playbackKey: String,
    viewModel: OpenNowViewModel,
    modifier: Modifier = Modifier,
) {
    QueueAdPlayer(
        adId = ad.adId,
        url = mediaUrl,
        playbackKey = playbackKey,
        modifier = modifier,
        onStarted = { viewModel.reportQueueAd(ad.adId, "start") },
        onPaused = { viewModel.reportQueueAd(ad.adId, "pause") },
        onResumed = { viewModel.reportQueueAd(ad.adId, "resume") },
        onFinished = { watchedTimeInMs ->
            viewModel.reportQueueAd(ad.adId, "finish", watchedTimeInMs = watchedTimeInMs)
        },
        onError = { watchedTimeInMs ->
            viewModel.reportQueueAd(
                ad.adId,
                "cancel",
                watchedTimeInMs = watchedTimeInMs,
                cancelReason = "error",
                errorInfo = "Error loading url",
            )
        },
    )
}

// QueueAdHeading (was OpenNowScreens.kt:12530)
@Composable
private fun QueueAdHeading(game: GameInfo?, compact: Boolean) {
    Column(Modifier.fillMaxWidth()) {
        Text(
            "Advertisement",
            color = TextMuted,
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.Bold,
            maxLines = 1,
        )
        Text(
            game?.title ?: "Starting stream",
            color = TextPrimary,
            style = if (compact) MaterialTheme.typography.titleMedium else MaterialTheme.typography.titleLarge,
            fontWeight = FontWeight.Bold,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

// QueueStatusAndActions (was OpenNowScreens.kt:12551)
@Composable
private fun QueueStatusAndActions(
    queueCopy: String,
    queuePosition: Int?,
    compact: Boolean,
    stackActions: Boolean,
    onMinimize: () -> Unit,
    onCancel: () -> Unit,
) {
    Column(
        Modifier.fillMaxWidth(if (compact) 1f else 0.7f),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(if (compact) 8.dp else 10.dp),
    ) {
        AnimatedQueueStatusText(
            queueCopy = queueCopy,
            queuePosition = queuePosition,
            compact = compact,
        )
        LinearProgressIndicator(Modifier.fillMaxWidth())
        if (stackActions) {
            OutlinedButton(onClick = onMinimize, modifier = Modifier.fillMaxWidth()) {
                Text("Minimize", maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            OutlinedButton(onClick = onCancel, modifier = Modifier.fillMaxWidth()) {
                Text("Cancel", maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        } else {
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                OutlinedButton(onClick = onMinimize, modifier = Modifier.weight(1f)) {
                    Text("Minimize", maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                OutlinedButton(onClick = onCancel, modifier = Modifier.weight(1f)) {
                    Text("Cancel", maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

// MinimizedQueueDock (was OpenNowScreens.kt:12594)
@Composable
internal fun MinimizedQueueDock(
    state: OpenNowUiState,
    onRestore: () -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val queuePosition = activeQueuePosition(state)
    val visibleQueuePosition = rememberStableQueuePosition(queuePosition)
    val queueCopy = queueLaunchStatusText(state, visibleQueuePosition)
    Surface(
        modifier = modifier
            .fillMaxWidth(),
        shape = RoundedCornerShape(topStart = 18.dp, topEnd = 18.dp),
        color = Panel.copy(alpha = 0.98f),
        tonalElevation = 0.dp,
        shadowElevation = 0.dp,
    ) {
        Row(
            Modifier.padding(horizontal = 14.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            CircularProgressIndicator(Modifier.size(24.dp), strokeWidth = 2.dp, color = MaterialTheme.colorScheme.primary)
            Column(Modifier.weight(1f)) {
                Text(
                    state.streamGame?.title ?: "Starting stream",
                    color = TextPrimary,
                    fontWeight = FontWeight.Bold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                MinimizedQueueStatusText(
                    queueCopy = queueCopy,
                    queuePosition = visibleQueuePosition,
                )
            }
            TextButton(onClick = onRestore) { Text("View") }
            OutlinedButton(onClick = onCancel, contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp)) {
                Text("Cancel")
            }
        }
    }
}

// MinimizedQueueStatusText (was OpenNowScreens.kt:12639)
@Composable
private fun MinimizedQueueStatusText(
    queueCopy: String,
    queuePosition: Int?,
) {
    if (queuePosition == null) {
        Text(queueCopy, color = queueIdleStatusColor(queueCopy), style = MaterialTheme.typography.bodySmall)
        return
    }
    val parts = queueStatusParts(queueCopy, queuePosition)
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(parts.prefix, color = TextMuted, style = MaterialTheme.typography.bodySmall)
        Text(
            parts.number,
            color = queueUrgencyColor(queuePosition),
            style = MaterialTheme.typography.bodySmall,
        )
        Text(parts.suffix, color = TextMuted, style = MaterialTheme.typography.bodySmall)
    }
}

// QueueAdPlayer (was OpenNowScreens.kt:12660)
@Composable
private fun QueueAdPlayer(
    adId: String,
    url: String,
    playbackKey: String,
    modifier: Modifier = Modifier,
    onStarted: () -> Unit,
    onPaused: () -> Unit,
    onResumed: () -> Unit,
    onFinished: (watchedTimeInMs: Long) -> Unit,
    onError: (watchedTimeInMs: Long) -> Unit,
) {
    val context = LocalContext.current
    var muted by remember { mutableStateOf(false) }
    val player = remember(adId, url, playbackKey) {
        ExoPlayer.Builder(context).build().apply {
            setMediaItem(MediaItem.fromUri(url))
            volume = if (muted) 0f else 1f
            prepare()
            playWhenReady = true
        }
    }
    var reportedStart by remember(adId, url, playbackKey) { mutableStateOf(false) }
    var reportedFinish by remember(adId, url, playbackKey) { mutableStateOf(false) }
    var reportedPause by remember(adId, url, playbackKey) { mutableStateOf(false) }
    var playing by remember(adId, url, playbackKey) { mutableStateOf(player.playWhenReady) }
    var controlsVisible by remember(adId, url, playbackKey) { mutableStateOf(false) }
    LaunchedEffect(controlsVisible, playing) {
        if (controlsVisible && playing) {
            delay(2400L)
            controlsVisible = false
        }
    }
    DisposableEffect(player) {
        val listener = object : Player.Listener {
            override fun onIsPlayingChanged(isPlaying: Boolean) {
                playing = isPlaying
                if (!isPlaying) controlsVisible = true
            }

            override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
                if (!reportedStart || reportedFinish) return
                if (playWhenReady && reportedPause) {
                    reportedPause = false
                    onResumed()
                } else if (!playWhenReady && player.playbackState != Player.STATE_ENDED && !reportedPause) {
                    reportedPause = true
                    onPaused()
                }
            }

            override fun onPlaybackStateChanged(playbackState: Int) {
                if (playbackState == Player.STATE_READY && player.playWhenReady && !reportedStart) {
                    reportedStart = true
                    onStarted()
                }
                if (playbackState == Player.STATE_ENDED && !reportedFinish) {
                    reportedFinish = true
                    onFinished(player.currentPosition.coerceAtLeast(0L))
                }
            }

            override fun onPlayerError(error: androidx.media3.common.PlaybackException) {
                if (!reportedFinish) {
                    reportedFinish = true
                    onError(player.currentPosition.coerceAtLeast(0L))
                }
            }
        }
        player.addListener(listener)
        listener.onIsPlayingChanged(player.isPlaying)
        listener.onPlaybackStateChanged(player.playbackState)
        onDispose {
            player.removeListener(listener)
            player.release()
        }
    }
    Box(
        modifier = modifier
            .clip(RoundedCornerShape(OpenNowRadius.sm))
            .clickable { controlsVisible = true },
    ) {
        AndroidView(
            modifier = Modifier
                .fillMaxSize(),
            factory = { ctx -> PlayerView(ctx).apply { this.player = player; useController = false } },
            update = { it.player = player; it.useController = false },
        )
        AnimatedVisibility(
            visible = controlsVisible || !playing,
            enter = fadeIn(),
            exit = fadeOut(),
            modifier = Modifier.align(Alignment.BottomCenter),
        ) {
            Row(
                modifier = Modifier
                    .padding(bottom = 12.dp)
                    .clip(RoundedCornerShape(OpenNowRadius.full))
                    .background(Color.Black.copy(alpha = 0.58f))
                    .padding(horizontal = 8.dp, vertical = 6.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                QueueAdIconButton(
                    label = if (playing) "Pause ad" else "Play ad",
                    icon = if (playing) QueueAdControlIcon.Pause else QueueAdControlIcon.Play,
                    onClick = {
                        controlsVisible = true
                        if (playing) {
                            player.pause()
                            playing = false
                        } else {
                            player.play()
                            playing = true
                        }
                    },
                )
                QueueAdIconButton(
                    label = if (muted) "Unmute ad" else "Mute ad",
                    icon = if (muted) QueueAdControlIcon.Muted else QueueAdControlIcon.Volume,
                    onClick = {
                        controlsVisible = true
                        muted = !muted
                        player.volume = if (muted) 0f else 1f
                    },
                )
            }
        }
    }
}

// QueueAdControlIcon (was OpenNowScreens.kt:12791)
private enum class QueueAdControlIcon { Play, Pause, Volume, Muted }

// QueueAdIconButton (was OpenNowScreens.kt:12793)
@Composable
private fun QueueAdIconButton(label: String, icon: QueueAdControlIcon, onClick: () -> Unit) {
    IconButton(
        onClick = onClick,
        modifier = Modifier
            .size(42.dp)
            .semantics { contentDescription = label },
    ) {
        QueueAdControlIconView(icon = icon, modifier = Modifier.size(22.dp))
    }
}

// QueueAdControlIconView (was OpenNowScreens.kt:12805)
@Composable
private fun QueueAdControlIconView(icon: QueueAdControlIcon, modifier: Modifier = Modifier) {
    Canvas(modifier) {
        val w = size.width
        val h = size.height
        when (icon) {
            QueueAdControlIcon.Play -> {
                val path = Path().apply {
                    moveTo(w * 0.35f, h * 0.24f)
                    lineTo(w * 0.35f, h * 0.76f)
                    lineTo(w * 0.76f, h * 0.5f)
                    close()
                }
                drawPath(path, Color.White)
            }
            QueueAdControlIcon.Pause -> {
                drawRoundRect(Color.White, Offset(w * 0.28f, h * 0.24f), Size(w * 0.14f, h * 0.52f), CornerRadius(w * 0.04f, w * 0.04f))
                drawRoundRect(Color.White, Offset(w * 0.58f, h * 0.24f), Size(w * 0.14f, h * 0.52f), CornerRadius(w * 0.04f, w * 0.04f))
            }
            QueueAdControlIcon.Volume, QueueAdControlIcon.Muted -> {
                val body = Path().apply {
                    moveTo(w * 0.18f, h * 0.42f)
                    lineTo(w * 0.34f, h * 0.42f)
                    lineTo(w * 0.52f, h * 0.26f)
                    lineTo(w * 0.52f, h * 0.74f)
                    lineTo(w * 0.34f, h * 0.58f)
                    lineTo(w * 0.18f, h * 0.58f)
                    close()
                }
                drawPath(body, Color.White)
                if (icon == QueueAdControlIcon.Volume) {
                    drawLine(Color.White, Offset(w * 0.62f, h * 0.38f), Offset(w * 0.72f, h * 0.5f), strokeWidth = w * 0.08f)
                    drawLine(Color.White, Offset(w * 0.72f, h * 0.5f), Offset(w * 0.62f, h * 0.62f), strokeWidth = w * 0.08f)
                } else {
                    drawLine(Color.White, Offset(w * 0.64f, h * 0.36f), Offset(w * 0.84f, h * 0.64f), strokeWidth = w * 0.08f)
                    drawLine(Color.White, Offset(w * 0.84f, h * 0.36f), Offset(w * 0.64f, h * 0.64f), strokeWidth = w * 0.08f)
                }
            }
        }
    }
}

// TouchOverlay (was OpenNowScreens.kt:12847)
@Composable
private fun TouchOverlay(
    client: NativeStreamClient,
    touch: AndroidTouchSettings,
    onButtonTone: () -> Unit,
    layoutEditing: Boolean,
    onSaveAllOffsets: (Map<String, TouchOffset>) -> Unit,
    modifier: Modifier = Modifier,
) {
    val opacity = touch.opacity
    val layoutScale = touch.scale
    val buttonScale = touch.buttonScale
    val stickScale = touch.stickScale

    val localOffsets = remember(touch.offsets) {
        androidx.compose.runtime.mutableStateMapOf<String, TouchOffset>().apply {
            putAll(touch.offsets)
        }
    }

    fun getLocalOffset(key: String): TouchOffset {
        val saved = localOffsets[key]
        if (saved != null) return saved
        val baseKey = key.substringBeforeLast("_")
        return when (baseKey) {
            "lt", "lb", "lstick", "dpad", "l3" -> TouchOffset(touch.leftOffsetXDp, touch.leftOffsetYDp)
            "rt", "rb", "rstick", "face", "r3" -> TouchOffset(touch.rightOffsetXDp, touch.rightOffsetYDp)
            else -> TouchOffset()
        }
    }

    val onLocalOffsetChange = { key: String, x: Float, y: Float ->
        localOffsets[key] = TouchOffset(x, y)
    }

    val currentLocalOffsets by rememberUpdatedState(localOffsets.toMap())
    val currentOnSaveAllOffsets by rememberUpdatedState(onSaveAllOffsets)
    DisposableEffect(layoutEditing) {
        onDispose {
            if (layoutEditing) {
                currentOnSaveAllOffsets(currentLocalOffsets)
            }
        }
    }

    LaunchedEffect(client, touch.enabled) {
        client.setVirtualControllerVisible(touch.enabled)
        NativeStreamInputRouter.setTouchControllerVisible(touch.enabled)
    }
    DisposableEffect(client) {
        onDispose {
            client.setVirtualControllerVisible(false)
            NativeStreamInputRouter.setTouchControllerVisible(false)
            NativeStreamInputRouter.clearTouchControllerPassthroughBounds()
        }
    }

    CompositionLocalProvider(LocalTouchControllerStyle provides touch.touchControllerStyle) {
        BoxWithConstraints(
            modifier
                .fillMaxSize()
                .padding(
                    start = touch.edgePaddingDp.dp,
                    top = 10.dp,
                    end = touch.edgePaddingDp.dp,
                    bottom = touch.bottomPaddingDp.dp,
                ),
        ) {
            if (touch.enabled) {
                val landscape = maxWidth > maxHeight
                val suffix = if (landscape) "_landscape" else "_portrait"
                val getOrientationLocalOffset = { key: String -> getLocalOffset(key + suffix) }
                val onOrientationLocalOffsetChange = { key: String, x: Float, y: Float ->
                    onLocalOffsetChange(key + suffix, x, y)
                }

                if (landscape) {
                    LandscapeTouchControls(
                        client = client,
                        opacity = opacity,
                        layoutScale = layoutScale,
                        buttonScale = buttonScale,
                        stickScale = stickScale,
                        joystickMode = touch.joystickMode,
                        joystickDeadZone = touch.joystickDeadZone,
                        viewportHeight = maxHeight,
                        layoutEditing = layoutEditing,
                        getLocalOffset = getOrientationLocalOffset,
                        onLocalOffsetChange = onOrientationLocalOffsetChange,
                        onButtonTone = onButtonTone,
                    )
                } else {
                    PortraitTouchControls(
                        client = client,
                        opacity = opacity,
                        layoutScale = layoutScale,
                        buttonScale = buttonScale,
                        stickScale = stickScale,
                        joystickMode = touch.joystickMode,
                        joystickDeadZone = touch.joystickDeadZone,
                        layoutEditing = layoutEditing,
                        getLocalOffset = getOrientationLocalOffset,
                        onLocalOffsetChange = onOrientationLocalOffsetChange,
                        onButtonTone = onButtonTone,
                    )
                }
            }
        }
    }
}

// PortraitTouchControls (was OpenNowScreens.kt:12958)
@Composable
private fun PortraitTouchControls(
    client: NativeStreamClient,
    opacity: Float,
    layoutScale: Float,
    buttonScale: Float,
    stickScale: Float,
    joystickMode: TouchJoystickMode,
    joystickDeadZone: Float,
    layoutEditing: Boolean,
    getLocalOffset: (String) -> TouchOffset,
    onLocalOffsetChange: (String, Float, Float) -> Unit,
    onButtonTone: () -> Unit,
) {
    val leftStickDiameter = 116.dp * stickScale * layoutScale
    val rightStickDiameter = 104.dp * stickScale * layoutScale
    val buttonSize48 = 48.dp * buttonScale * layoutScale
    val buttonSize44 = 44.dp * buttonScale * layoutScale
    val faceWidth = buttonSize48 * 2.44f

    Box(
        Modifier.fillMaxSize().padding(horizontal = 32.dp, vertical = 24.dp)
    ) {
        val scale = buttonScale * layoutScale
        val triggerWidth = 64.dp * scale
        val bumperHeight = 32.dp * scale

        TouchControlGroup(
            id = "portrait-lt",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("lt").x.dp,
            offsetY = getLocalOffset("lt").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("lt", x, y) },
            modifier = Modifier.align(Alignment.TopStart),
        ) {
            GamepadTriggerButton(
                label = "LT",
                left = true,
                client = client,
                opacity = opacity,
                width = triggerWidth,
                height = bumperHeight,
                shape = RoundedCornerShape(50),
                onPressTone = onButtonTone,
            )
        }

        TouchControlGroup(
            id = "portrait-lb",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("lb").x.dp,
            offsetY = getLocalOffset("lb").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("lb", x, y) },
            modifier = Modifier.align(Alignment.TopStart).padding(top = bumperHeight + 6.dp),
        ) {
            GamepadBumperButton(
                label = "LB",
                mask = 0x0100,
                client = client,
                opacity = opacity,
                width = triggerWidth,
                height = bumperHeight,
                onPressTone = onButtonTone,
            )
        }

        TouchControlGroup(
            id = "portrait-lstick",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("lstick").x.dp,
            offsetY = getLocalOffset("lstick").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("lstick", x, y) },
            modifier = Modifier.align(Alignment.BottomStart),
        ) {
            VirtualStick(
                label = "L",
                client = client,
                opacity = opacity,
                diameter = leftStickDiameter,
                mode = joystickMode,
                deadZone = joystickDeadZone,
                onChange = client::setVirtualLeftStick,
            )
        }

        TouchControlGroup(
            id = "portrait-l3",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("l3").x.dp,
            offsetY = getLocalOffset("l3").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("l3", x, y) },
            modifier = Modifier.align(Alignment.BottomStart).padding(
                start = (leftStickDiameter - buttonSize48) / 2,
                bottom = leftStickDiameter + 6.dp
            ),
        ) {
            GamepadButton("LS", GamepadButtonMapping.LEFT_THUMB, client, opacity, buttonSize48, onButtonTone)
        }

        TouchControlGroup(
            id = "portrait-dpad",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("dpad").x.dp,
            offsetY = getLocalOffset("dpad").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("dpad", x, y) },
            modifier = Modifier.align(Alignment.BottomStart).padding(start = leftStickDiameter + 12.dp),
        ) {
            DpadCluster(client, opacity, buttonScale * layoutScale, onButtonTone)
        }

        TouchControlGroup(
            id = "portrait-rt",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("rt").x.dp,
            offsetY = getLocalOffset("rt").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("rt", x, y) },
            modifier = Modifier.align(Alignment.TopEnd),
        ) {
            GamepadTriggerButton(
                label = "RT",
                left = false,
                client = client,
                opacity = opacity,
                width = triggerWidth,
                height = bumperHeight,
                shape = RoundedCornerShape(50),
                onPressTone = onButtonTone,
            )
        }

        TouchControlGroup(
            id = "portrait-rb",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("rb").x.dp,
            offsetY = getLocalOffset("rb").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("rb", x, y) },
            modifier = Modifier.align(Alignment.TopEnd).padding(top = bumperHeight + 6.dp),
        ) {
            GamepadBumperButton(
                label = "RB",
                mask = 0x0200,
                client = client,
                opacity = opacity,
                width = triggerWidth,
                height = bumperHeight,
                onPressTone = onButtonTone,
            )
        }

        TouchControlGroup(
            id = "portrait-select",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("select").x.dp,
            offsetY = getLocalOffset("select").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("select", x, y) },
            modifier = Modifier.align(Alignment.TopEnd).padding(top = buttonSize48 + 8.dp, end = buttonSize44 + 8.dp),
        ) {
            GamepadButton("◀", 0x0020, client, opacity, buttonSize44, onButtonTone)
        }

        TouchControlGroup(
            id = "portrait-start",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("start").x.dp,
            offsetY = getLocalOffset("start").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("start", x, y) },
            modifier = Modifier.align(Alignment.TopEnd).padding(top = buttonSize48 + 8.dp),
        ) {
            GamepadButton("▶", 0x0010, client, opacity, buttonSize44, onButtonTone)
        }

        TouchControlGroup(
            id = "portrait-rstick",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("rstick").x.dp,
            offsetY = getLocalOffset("rstick").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("rstick", x, y) },
            modifier = Modifier.align(Alignment.BottomEnd).padding(end = faceWidth + 12.dp),
        ) {
            VirtualStick(
                label = "R",
                client = client,
                opacity = opacity,
                diameter = rightStickDiameter,
                mode = joystickMode,
                deadZone = joystickDeadZone,
                onChange = client::setVirtualRightStick,
            )
        }

        TouchControlGroup(
            id = "portrait-r3",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("r3").x.dp,
            offsetY = getLocalOffset("r3").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("r3", x, y) },
            modifier = Modifier.align(Alignment.BottomEnd).padding(
                end = faceWidth + 12.dp + (rightStickDiameter - buttonSize48) / 2,
                bottom = rightStickDiameter + 6.dp
            ),
        ) {
            GamepadButton("RS", GamepadButtonMapping.RIGHT_THUMB, client, opacity, buttonSize48, onButtonTone)
        }

        TouchControlGroup(
            id = "portrait-face",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("face").x.dp,
            offsetY = getLocalOffset("face").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("face", x, y) },
            modifier = Modifier.align(Alignment.BottomEnd),
        ) {
            FaceButtonCluster(client, opacity, buttonScale * layoutScale, onButtonTone)
        }
    }
}

// LandscapeTouchControls (was OpenNowScreens.kt:13175)
@Composable
private fun BoxScope.LandscapeTouchControls(
    client: NativeStreamClient,
    opacity: Float,
    layoutScale: Float,
    buttonScale: Float,
    stickScale: Float,
    joystickMode: TouchJoystickMode,
    joystickDeadZone: Float,
    viewportHeight: Dp,
    layoutEditing: Boolean,
    getLocalOffset: (String) -> TouchOffset,
    onLocalOffsetChange: (String, Float, Float) -> Unit,
    onButtonTone: () -> Unit,
) {
    val controlScale = buttonScale * layoutScale
    val topControlClearance = landscapeTouchTopControlClearanceDp(viewportHeight.value, controlScale).dp
    Box(Modifier.fillMaxSize().padding(horizontal = 24.dp, vertical = 24.dp)) {
        val triggerWidth = 76.dp * controlScale
        val bumperHeight = 36.dp * controlScale

        TouchControlGroup(
            id = "landscape-lt",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("lt").x.dp,
            offsetY = getLocalOffset("lt").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("lt", x, y) },
            modifier = Modifier.align(Alignment.TopStart).padding(top = topControlClearance),
        ) {
            GamepadTriggerButton(
                label = "LT",
                left = true,
                client = client,
                opacity = opacity,
                width = triggerWidth,
                height = bumperHeight,
                shape = RoundedCornerShape(50),
                onPressTone = onButtonTone,
            )
        }

        TouchControlGroup(
            id = "landscape-lb",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("lb").x.dp,
            offsetY = getLocalOffset("lb").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("lb", x, y) },
            modifier = Modifier.align(Alignment.TopStart).padding(top = topControlClearance + bumperHeight + 6.dp),
        ) {
            GamepadBumperButton(
                label = "LB",
                mask = 0x0100,
                client = client,
                opacity = opacity,
                width = triggerWidth,
                height = bumperHeight,
                onPressTone = onButtonTone,
            )
        }

        val selectSize = 42.dp * controlScale
        TouchControlGroup(
            id = "landscape-select",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("select").x.dp,
            offsetY = getLocalOffset("select").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("select", x, y) },
            modifier = Modifier.align(Alignment.BottomCenter).padding(end = selectSize / 2 + 27.dp),
        ) {
            GamepadButton("◀", 0x0020, client, opacity, selectSize, onButtonTone)
        }

        TouchControlGroup(
            id = "landscape-start",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("start").x.dp,
            offsetY = getLocalOffset("start").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("start", x, y) },
            modifier = Modifier.align(Alignment.BottomCenter).padding(start = selectSize / 2 + 27.dp),
        ) {
            GamepadButton("▶", 0x0010, client, opacity, selectSize, onButtonTone)
        }

        TouchControlGroup(
            id = "landscape-rb",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("rb").x.dp,
            offsetY = getLocalOffset("rb").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("rb", x, y) },
            modifier = Modifier.align(Alignment.TopEnd).padding(top = topControlClearance + bumperHeight + 6.dp),
        ) {
            GamepadBumperButton(
                label = "RB",
                mask = 0x0200,
                client = client,
                opacity = opacity,
                width = triggerWidth,
                height = bumperHeight,
                onPressTone = onButtonTone,
            )
        }

        TouchControlGroup(
            id = "landscape-rt",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("rt").x.dp,
            offsetY = getLocalOffset("rt").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("rt", x, y) },
            modifier = Modifier.align(Alignment.TopEnd).padding(top = topControlClearance),
        ) {
            GamepadTriggerButton(
                label = "RT",
                left = false,
                client = client,
                opacity = opacity,
                width = triggerWidth,
                height = bumperHeight,
                shape = RoundedCornerShape(50),
                onPressTone = onButtonTone,
            )
        }

        val dpadScale = controlScale * 0.88f
        val dpadButtonSize = 54.dp * dpadScale
        val dpadWidth = dpadButtonSize * 2.44f
        TouchControlGroup(
            id = "landscape-dpad",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("dpad").x.dp,
            offsetY = getLocalOffset("dpad").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("dpad", x, y) },
            modifier = Modifier.align(Alignment.BottomStart),
        ) {
            DpadCluster(client, opacity, dpadScale, onButtonTone)
        }

        val leftStickDiameter = 112.dp * stickScale * layoutScale
        TouchControlGroup(
            id = "landscape-lstick",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("lstick").x.dp,
            offsetY = getLocalOffset("lstick").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("lstick", x, y) },
            modifier = Modifier.align(Alignment.BottomStart).padding(start = dpadWidth + 14.dp),
        ) {
            VirtualStick(
                label = "L",
                client = client,
                opacity = opacity,
                diameter = leftStickDiameter,
                mode = joystickMode,
                deadZone = joystickDeadZone,
                onChange = client::setVirtualLeftStick,
            )
        }

        val l3Size = 54.dp * controlScale
        TouchControlGroup(
            id = "landscape-l3",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("l3").x.dp,
            offsetY = getLocalOffset("l3").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("l3", x, y) },
            modifier = Modifier.align(Alignment.BottomStart).padding(
                start = dpadWidth + 14.dp + (leftStickDiameter - l3Size) / 2,
                bottom = leftStickDiameter + 6.dp
            ),
        ) {
            GamepadButton("LS", GamepadButtonMapping.LEFT_THUMB, client, opacity, l3Size, onButtonTone)
        }

        val faceScale = controlScale * 0.9f
        val faceButtonSize = 54.dp * faceScale
        val faceWidth = faceButtonSize * 2.44f
        val rightStickDiameter = 112.dp * stickScale * layoutScale
        TouchControlGroup(
            id = "landscape-rstick",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("rstick").x.dp,
            offsetY = getLocalOffset("rstick").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("rstick", x, y) },
            modifier = Modifier.align(Alignment.BottomEnd).padding(end = faceWidth + 14.dp),
        ) {
            VirtualStick(
                label = "R",
                client = client,
                opacity = opacity,
                diameter = rightStickDiameter,
                mode = joystickMode,
                deadZone = joystickDeadZone,
                onChange = client::setVirtualRightStick,
            )
        }

        val r3Size = 54.dp * controlScale
        TouchControlGroup(
            id = "landscape-r3",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("r3").x.dp,
            offsetY = getLocalOffset("r3").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("r3", x, y) },
            modifier = Modifier.align(Alignment.BottomEnd).padding(
                end = faceWidth + 14.dp + (rightStickDiameter - r3Size) / 2,
                bottom = rightStickDiameter + 6.dp
            ),
        ) {
            GamepadButton("RS", GamepadButtonMapping.RIGHT_THUMB, client, opacity, r3Size, onButtonTone)
        }

        TouchControlGroup(
            id = "landscape-face",
            layoutEditing = layoutEditing,
            offsetX = getLocalOffset("face").x.dp,
            offsetY = getLocalOffset("face").y.dp,
            onOffsetChange = { x, y -> onLocalOffsetChange("face", x, y) },
            modifier = Modifier.align(Alignment.BottomEnd),
        ) {
            FaceButtonCluster(client, opacity, faceScale, onButtonTone)
        }
    }
}

// landscapeTouchTopControlClearanceDp (was OpenNowScreens.kt:13397)
internal fun landscapeTouchTopControlClearanceDp(viewportHeightDp: Float, controlScale: Float): Float {
    val viewportBand = (viewportHeightDp * 0.11f).coerceIn(34f, 58f)
    val scaledBand = viewportBand * controlScale.coerceIn(0.75f, 1.35f)
    return scaledBand.coerceIn(30f, 76f)
}

// TouchControlGroup (was OpenNowScreens.kt:13403)
@Composable
private fun TouchControlGroup(
    id: String,
    layoutEditing: Boolean,
    offsetX: Dp,
    offsetY: Dp,
    onOffsetChange: (Float, Float) -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit,
) {
    val density = LocalDensity.current
    val currentOffsetX by rememberUpdatedState(offsetX)
    val currentOffsetY by rememberUpdatedState(offsetY)
    val currentOnOffsetChange by rememberUpdatedState(onOffsetChange)
    Box(
        modifier
            .offset(x = offsetX, y = offsetY)
            .onGloballyPositioned { coordinates ->
                val bounds = coordinates.boundsInRoot()
                NativeStreamInputRouter.setTouchControllerPassthroughBound(
                    id,
                    bounds.left.roundToInt(),
                    bounds.top.roundToInt(),
                    bounds.right.roundToInt(),
                    bounds.bottom.roundToInt(),
                )
            },
        contentAlignment = Alignment.Center,
    ) {
        content()
        if (layoutEditing) {
            Box(
                Modifier
                    .matchParentSize()
                    .clip(RoundedCornerShape(18.dp))
                    .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.16f))
                    .border(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.72f), RoundedCornerShape(18.dp))
                    .pointerInput(Unit) {
                        detectDragGestures { change, dragAmount ->
                            change.consume()
                            val deltaXDp = with(density) { dragAmount.x.toDp().value }
                            val deltaYDp = with(density) { dragAmount.y.toDp().value }
                            currentOnOffsetChange(
                                (currentOffsetX.value + deltaXDp).coerceIn(-280f, 280f),
                                (currentOffsetY.value + deltaYDp).coerceIn(-280f, 280f),
                            )
                        }
                    },
                contentAlignment = Alignment.TopCenter,
            ) {
                Surface(
                    color = MaterialTheme.colorScheme.primary.copy(alpha = 0.9f),
                    shape = RoundedCornerShape(OpenNowRadius.full),
                    modifier = Modifier.padding(top = 4.dp),
                ) {
                    Text(
                        "Drag",
                        color = MaterialTheme.colorScheme.onPrimary,
                        style = MaterialTheme.typography.labelSmall,
                        modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp),
                    )
                }
            }
        }
    }
    DisposableEffect(id) {
        onDispose {
            NativeStreamInputRouter.clearTouchControllerPassthroughBound(id)
        }
    }
}

// clampStickOffset (was OpenNowScreens.kt:13475)
private fun clampStickOffset(offset: Offset, maxRadius: Float): Offset {
    val distance = sqrt(offset.x * offset.x + offset.y * offset.y)
    if (distance <= maxRadius || distance == 0f) return offset
    val scale = maxRadius / distance
    return Offset(offset.x * scale, offset.y * scale)
}

// applyTouchJoystickDeadZone (was OpenNowScreens.kt:13482)
internal fun applyTouchJoystickDeadZone(value: Float, deadZone: Float): Float {
    val clampedValue = value.coerceIn(-1f, 1f)
    val clampedDeadZone = deadZone.coerceIn(0f, 0.95f)
    val magnitude = kotlin.math.abs(clampedValue)
    if (magnitude <= clampedDeadZone) return 0f
    val adjusted = (magnitude - clampedDeadZone) / (1f - clampedDeadZone)
    return if (clampedValue < 0f) -adjusted else adjusted
}

// VirtualStick (was OpenNowScreens.kt:13492)
@Composable
private fun VirtualStick(
    label: String,
    client: NativeStreamClient,
    opacity: Float,
    diameter: androidx.compose.ui.unit.Dp,
    mode: TouchJoystickMode,
    deadZone: Float,
    onChange: (Float, Float) -> Unit,
) {
    val currentOnChange by rememberUpdatedState(onChange)
    var knobOffset by remember { mutableStateOf(Offset.Zero) }
    var baseOffset by remember { mutableStateOf(Offset.Zero) }
    val style = LocalTouchControllerStyle.current

    DisposableEffect(client) {
        onDispose {
            currentOnChange(0f, 0f)
        }
    }

    Box(
        Modifier
            .size(diameter)
            .pointerInput(client, mode, deadZone) {
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                    val fixedCenter = Offset(size.width / 2f, size.height / 2f)
                    val gestureCenter = if (mode == TouchJoystickMode.Dynamic) down.position else fixedCenter
                    val maxRadius = min(size.width, size.height) * 0.34f
                    baseOffset = gestureCenter - fixedCenter

                    fun updateStick(position: Offset) {
                        val clamped = clampStickOffset(position - gestureCenter, maxRadius)
                        val rawX = (clamped.x / maxRadius).coerceIn(-1f, 1f)
                        val rawY = (clamped.y / maxRadius).coerceIn(-1f, 1f)
                        val magnitude = sqrt(rawX * rawX + rawY * rawY).coerceIn(0f, 1f)
                        val adjustedMagnitude = applyTouchJoystickDeadZone(magnitude, deadZone)
                        val adjustment = if (magnitude > 0f) adjustedMagnitude / magnitude else 0f
                        currentOnChange(rawX * adjustment, rawY * adjustment)
                        knobOffset = clamped
                    }

                    try {
                        updateStick(down.position)
                        down.consume()
                        while (true) {
                            val event = awaitPointerEvent(PointerEventPass.Initial)
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed) {
                                change.consume()
                                break
                            }
                            updateStick(change.position)
                            change.consume()
                        }
                    } finally {
                        currentOnChange(0f, 0f)
                        knobOffset = Offset.Zero
                        baseOffset = Offset.Zero
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        val knobBackground = if (style == TouchControllerStyle.V2) {
            Color.White.copy(alpha = opacity * 0.2f)
        } else {
            Color.LightGray.copy(alpha = opacity * 0.8f)
        }
        val knobBorderModifier = if (style == TouchControllerStyle.V2) {
            Modifier.border(1.dp, Color.White.copy(alpha = opacity * 0.5f), CircleShape)
        } else {
            Modifier
        }
        Box(
            Modifier
                .size(diameter)
                .graphicsLayer {
                    translationX = baseOffset.x
                    translationY = baseOffset.y
                }
                .clip(CircleShape)
                .background(Color.Transparent)
                .border(1.dp, Color.White.copy(alpha = opacity * 0.3f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Box(
                Modifier
                    .size(diameter * 0.44f)
                    .graphicsLayer {
                        translationX = knobOffset.x
                        translationY = knobOffset.y
                    }
                    .clip(CircleShape)
                    .background(knobBackground)
                    .then(knobBorderModifier)
            )
        }
    }
}

// FaceButtonCluster (was OpenNowScreens.kt:13594)
@Composable
private fun FaceButtonCluster(client: NativeStreamClient, opacity: Float, scale: Float, onButtonTone: () -> Unit) {
    val buttonSize = 54.dp * scale
    val distance = buttonSize * 1.05f
    val boxSize = distance * 2 + buttonSize
    Box(Modifier.size(boxSize)) {
        Box(Modifier.align(Alignment.Center).offset(y = -distance)) {
            GamepadButton("Y", 0x8000, client, opacity, buttonSize, onButtonTone)
        }
        Box(Modifier.align(Alignment.Center).offset(y = distance)) {
            GamepadButton("A", 0x1000, client, opacity, buttonSize, onButtonTone)
        }
        Box(Modifier.align(Alignment.Center).offset(x = -distance)) {
            GamepadButton("X", 0x4000, client, opacity, buttonSize, onButtonTone)
        }
        Box(Modifier.align(Alignment.Center).offset(x = distance)) {
            GamepadButton("B", 0x2000, client, opacity, buttonSize, onButtonTone)
        }
    }
}

// DpadArrowhead (was OpenNowScreens.kt:13615)
@Composable
private fun DpadArrowhead(
    label: String,
    pressed: Boolean,
    opacity: Float,
) {
    val arrowColor = if (pressed) {
        Color.White
    } else {
        Color.White.copy(alpha = opacity * 0.8f)
    }
    Text(
        text = label,
        fontWeight = FontWeight.Bold,
        fontSize = 18.sp,
        color = arrowColor
    )
}

// DpadCluster (was OpenNowScreens.kt:13634)
@Composable
private fun DpadCluster(client: NativeStreamClient, opacity: Float, scale: Float, onButtonTone: () -> Unit) {
    val currentOnButtonTone by rememberUpdatedState(onButtonTone)
    val buttonSize = 54.dp * scale
    val distance = buttonSize * 1.05f
    val boxSize = distance * 2 + buttonSize

    var upPressed by remember { mutableStateOf(false) }
    var downPressed by remember { mutableStateOf(false) }
    var leftPressed by remember { mutableStateOf(false) }
    var rightPressed by remember { mutableStateOf(false) }

    val style = LocalTouchControllerStyle.current
    val crossColor = if (style == TouchControllerStyle.V2) Color.Transparent else Color.Black.copy(alpha = opacity * 0.6f)
    val crossBorderColor = if (style == TouchControllerStyle.V2) Color.White.copy(alpha = opacity * 0.5f) else Color.White.copy(alpha = opacity * 0.4f)
    val crossBorderWidth = 1.dp

    DisposableEffect(client) {
        onDispose {
            client.setVirtualButton(0x0001, false)
            client.setVirtualButton(0x0002, false)
            client.setVirtualButton(0x0004, false)
            client.setVirtualButton(0x0008, false)
        }
    }

    Box(
        Modifier
            .size(boxSize)
            .pointerInput(client) {
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)

                    fun updateDirection(position: Offset) {
                        val w = size.width
                        val h = size.height
                        val cx = w / 2f
                        val cy = h / 2f
                        val px = position.x
                        val py = position.y
                        val dx = px - cx
                        val dy = py - cy
                        val touchDist = Math.sqrt((dx * dx + dy * dy).toDouble()).toFloat()
                        val deadzone = 12.dp.toPx()
                        var newUp = false
                        var newDown = false
                        var newLeft = false
                        var newRight = false
                        if (touchDist > deadzone) {
                            val absDx = Math.abs(dx)
                            val absDy = Math.abs(dy)
                            if (dy < 0 && absDy > absDx * 0.414f) newUp = true
                            if (dy > 0 && absDy > absDx * 0.414f) newDown = true
                            if (dx < 0 && absDx > absDy * 0.414f) newLeft = true
                            if (dx > 0 && absDx > absDy * 0.414f) newRight = true
                        }

                        val playTone = (!upPressed && newUp) || (!downPressed && newDown) ||
                                       (!leftPressed && newLeft) || (!rightPressed && newRight)
                        if (upPressed != newUp) { client.setVirtualButton(0x0001, newUp); upPressed = newUp }
                        if (downPressed != newDown) { client.setVirtualButton(0x0002, newDown); downPressed = newDown }
                        if (leftPressed != newLeft) { client.setVirtualButton(0x0004, newLeft); leftPressed = newLeft }
                        if (rightPressed != newRight) { client.setVirtualButton(0x0008, newRight); rightPressed = newRight }
                        if (playTone) currentOnButtonTone()
                    }

                    try {
                        updateDirection(down.position)
                        down.consume()
                        while (true) {
                            val event = awaitPointerEvent(PointerEventPass.Initial)
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed) {
                                change.consume()
                                break
                            }
                            updateDirection(change.position)
                            change.consume()
                        }
                    } finally {
                        if (upPressed) { client.setVirtualButton(0x0001, false); upPressed = false }
                        if (downPressed) { client.setVirtualButton(0x0002, false); downPressed = false }
                        if (leftPressed) { client.setVirtualButton(0x0004, false); leftPressed = false }
                        if (rightPressed) { client.setVirtualButton(0x0008, false); rightPressed = false }
                    }
                }
            }
    ) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            val w = size.width
            val h = size.height
            val armSize = buttonSize.toPx()
            val cornerRadius = androidx.compose.ui.geometry.CornerRadius(8.dp.toPx(), 8.dp.toPx())

            val crossPath = Path().apply {
                addRoundRect(
                    androidx.compose.ui.geometry.RoundRect(
                        left = (w - armSize) / 2f,
                        top = 0f,
                        right = (w + armSize) / 2f,
                        bottom = h,
                        cornerRadius = cornerRadius
                    )
                )
                addRoundRect(
                    androidx.compose.ui.geometry.RoundRect(
                        left = 0f,
                        top = (h - armSize) / 2f,
                        right = w,
                        bottom = (h + armSize) / 2f,
                        cornerRadius = cornerRadius
                    )
                )
            }

            if (style != TouchControllerStyle.V2) {
                drawPath(crossPath, crossColor)
            }

            val pressedColor = if (style == TouchControllerStyle.V2) {
                Color.White.copy(alpha = opacity * 0.15f)
            } else {
                Color.White.copy(alpha = opacity * 0.2f)
            }

            val pressedPath = Path()
            if (upPressed) {
                pressedPath.addRoundRect(
                    androidx.compose.ui.geometry.RoundRect(
                        left = (w - armSize) / 2f,
                        top = 0f,
                        right = (w + armSize) / 2f,
                        bottom = h / 2f,
                        topLeftCornerRadius = cornerRadius,
                        topRightCornerRadius = cornerRadius
                    )
                )
            }
            if (downPressed) {
                pressedPath.addRoundRect(
                    androidx.compose.ui.geometry.RoundRect(
                        left = (w - armSize) / 2f,
                        top = h / 2f,
                        right = (w + armSize) / 2f,
                        bottom = h,
                        bottomLeftCornerRadius = cornerRadius,
                        bottomRightCornerRadius = cornerRadius
                    )
                )
            }
            if (leftPressed) {
                pressedPath.addRoundRect(
                    androidx.compose.ui.geometry.RoundRect(
                        left = 0f,
                        top = (h - armSize) / 2f,
                        right = w / 2f,
                        bottom = (h + armSize) / 2f,
                        topLeftCornerRadius = cornerRadius,
                        bottomLeftCornerRadius = cornerRadius
                    )
                )
            }
            if (rightPressed) {
                pressedPath.addRoundRect(
                    androidx.compose.ui.geometry.RoundRect(
                        left = w / 2f,
                        top = (h - armSize) / 2f,
                        right = w,
                        bottom = (h + armSize) / 2f,
                        topRightCornerRadius = cornerRadius,
                        bottomRightCornerRadius = cornerRadius
                    )
                )
            }
            drawPath(pressedPath, pressedColor)

            drawPath(
                path = crossPath,
                color = crossBorderColor,
                style = Stroke(width = crossBorderWidth.toPx())
            )
        }

        Box(Modifier.align(Alignment.Center).offset(y = -distance)) {
            DpadArrowhead("▲", upPressed, opacity)
        }
        Box(Modifier.align(Alignment.Center).offset(y = distance)) {
            DpadArrowhead("▼", downPressed, opacity)
        }
        Box(Modifier.align(Alignment.Center).offset(x = -distance)) {
            DpadArrowhead("◀", leftPressed, opacity)
        }
        Box(Modifier.align(Alignment.Center).offset(x = distance)) {
            DpadArrowhead("▶", rightPressed, opacity)
        }
    }
}

// virtualPressInput (was OpenNowScreens.kt:13832)
private fun Modifier.virtualPressInput(
    client: NativeStreamClient,
    controlKey: Any,
    onPressedChange: State<(Boolean) -> Unit>,
): Modifier = pointerInput(client, controlKey) {
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
        onPressedChange.value(true)
        try {
            down.consume()
            while (true) {
                val event = awaitPointerEvent(PointerEventPass.Initial)
                val change = event.changes.firstOrNull { it.id == down.id } ?: break
                if (!change.pressed) {
                    change.consume()
                    break
                }
                change.consume()
            }
        } finally {
            onPressedChange.value(false)
        }
    }
}

// GamepadTriggerButton (was OpenNowScreens.kt:13857)
@Composable
private fun GamepadTriggerButton(
    label: String,
    left: Boolean,
    client: NativeStreamClient,
    opacity: Float,
    width: androidx.compose.ui.unit.Dp,
    height: androidx.compose.ui.unit.Dp,
    shape: androidx.compose.ui.graphics.Shape,
    onPressTone: () -> Unit = {},
) {
    var pressed by remember { mutableStateOf(false) }
    val currentOnPressedChange = rememberUpdatedState<(Boolean) -> Unit> { down ->
        if (down != pressed) {
            client.setVirtualTrigger(left, down)
            pressed = down
            if (down) onPressTone()
        }
    }
    val style = LocalTouchControllerStyle.current
    val buttonColor = if (style == TouchControllerStyle.V2) {
        Color.Transparent
    } else {
        Color.Black.copy(alpha = opacity * 0.6f)
    }
    val pressedColor = if (style == TouchControllerStyle.V2) {
        Color.White.copy(alpha = opacity * 0.15f)
    } else {
        Color.White.copy(alpha = opacity * 0.2f)
    }
    val borderColor = if (style == TouchControllerStyle.V2) {
        if (pressed) Color.White.copy(alpha = opacity * 0.9f) else Color.White.copy(alpha = opacity * 0.5f)
    } else {
        Color.White.copy(alpha = opacity * 0.4f)
    }
    val borderWidth = if (style == TouchControllerStyle.V2 && pressed) 2.dp else 1.dp
    Box(
        Modifier
            .width(width)
            .height(height)
            .clip(shape)
            .background(if (pressed) pressedColor else buttonColor)
            .border(borderWidth, borderColor, shape)
            .virtualPressInput(client, left, currentOnPressedChange),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, fontWeight = FontWeight.SemiBold, color = Color.White.copy(alpha = opacity * 0.9f))
    }
    DisposableEffect(client, left) {
        onDispose {
            client.setVirtualTrigger(left, false)
        }
    }
}

// GamepadBumperButton (was OpenNowScreens.kt:13912)
@Composable
private fun GamepadBumperButton(
    label: String,
    mask: Int,
    client: NativeStreamClient,
    opacity: Float,
    width: androidx.compose.ui.unit.Dp,
    height: androidx.compose.ui.unit.Dp,
    onPressTone: () -> Unit = {},
) {
    var pressed by remember { mutableStateOf(false) }
    val currentOnPressedChange = rememberUpdatedState<(Boolean) -> Unit> { down ->
        if (down != pressed) {
            client.setVirtualButton(mask, down)
            pressed = down
            if (down) onPressTone()
        }
    }
    val style = LocalTouchControllerStyle.current
    val buttonColor = if (style == TouchControllerStyle.V2) {
        Color.Transparent
    } else {
        Color.Black.copy(alpha = opacity * 0.6f)
    }
    val pressedColor = if (style == TouchControllerStyle.V2) {
        Color.White.copy(alpha = opacity * 0.15f)
    } else {
        Color.White.copy(alpha = opacity * 0.2f)
    }
    val borderColor = if (style == TouchControllerStyle.V2) {
        if (pressed) Color.White.copy(alpha = opacity * 0.9f) else Color.White.copy(alpha = opacity * 0.5f)
    } else {
        Color.White.copy(alpha = opacity * 0.4f)
    }
    val borderWidth = if (style == TouchControllerStyle.V2 && pressed) 2.dp else 1.dp
    val shape = RoundedCornerShape(50)
    Box(
        Modifier
            .width(width)
            .height(height)
            .clip(shape)
            .background(if (pressed) pressedColor else buttonColor)
            .border(borderWidth, borderColor, shape)
            .virtualPressInput(client, mask, currentOnPressedChange),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, fontWeight = FontWeight.SemiBold, color = Color.White.copy(alpha = opacity * 0.9f))
    }
    DisposableEffect(client, mask) {
        onDispose {
            client.setVirtualButton(mask, false)
        }
    }
}

// GamepadButton (was OpenNowScreens.kt:13967)
@Composable
private fun GamepadButton(
    label: String,
    mask: Int,
    client: NativeStreamClient,
    opacity: Float,
    size: androidx.compose.ui.unit.Dp,
    onPressTone: () -> Unit = {},
) {
    val currentOnPressTone by rememberUpdatedState(onPressTone)
    var pressed by remember { mutableStateOf(false) }
    val currentOnPressedChange = rememberUpdatedState<(Boolean) -> Unit> { down ->
        if (down != pressed) {
            client.setVirtualButton(mask, down)
            pressed = down
            if (down) currentOnPressTone()
        }
    }
    val style = LocalTouchControllerStyle.current
    val buttonColor = if (style == TouchControllerStyle.V2) {
        Color.Transparent
    } else {
        Color.Black.copy(alpha = opacity * 0.6f)
    }
    val pressedColor = if (style == TouchControllerStyle.V2) {
        Color.White.copy(alpha = opacity * 0.15f)
    } else {
        Color.White.copy(alpha = opacity * 0.2f)
    }
    val borderColor = if (style == TouchControllerStyle.V2) {
        if (pressed) Color.White.copy(alpha = opacity * 0.9f) else Color.White.copy(alpha = opacity * 0.5f)
    } else {
        Color.White.copy(alpha = opacity * 0.4f)
    }
    val borderWidth = if (style == TouchControllerStyle.V2 && pressed) 2.dp else 1.dp
    Box(
        Modifier
            .size(size)
            .clip(CircleShape)
            .background(if (pressed) pressedColor else buttonColor)
            .border(borderWidth, borderColor, CircleShape)
            .virtualPressInput(client, mask, currentOnPressedChange),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = label,
            fontWeight = FontWeight.SemiBold,
            color = Color.White.copy(alpha = opacity * 0.9f),
        )
    }
    DisposableEffect(client, mask) {
        onDispose {
            client.setVirtualButton(mask, false)
        }
    }
}

// PrintedWasteSelector (was OpenNowScreens.kt:14179)
@Composable
internal fun PrintedWasteSelector(
    state: OpenNowUiState,
    game: GameInfo,
    viewModel: OpenNowViewModel,
    modifier: Modifier = Modifier,
) {
    BackHandler(onBack = viewModel::dismissPrintedWasteSelector)
    val zones = remember(state.printedWasteQueue, state.printedWasteMapping, state.printedWastePings) {
        state.printedWasteQueue
            .filter { (zoneId, _) -> isStandardPrintedWasteZone(zoneId) && state.printedWasteMapping[zoneId]?.nuked != true }
            .map { (zoneId, zone) ->
                val routingUrl = printedWasteZoneUrl(zoneId)
                PrintedWasteZoneOption(
                    zoneId = zoneId,
                    zone = zone,
                    routingUrl = routingUrl,
                    pingMs = state.printedWastePings[routingUrl],
                )
            }
    }
    val autoZone = remember(zones) { recommendedPrintedWasteZone(zones) }
    val sortedZones = remember(zones, autoZone) {
        val maxPing = zones.mapNotNull { it.pingMs }.maxOrNull()?.coerceAtLeast(1) ?: 1
        val maxQueue = zones.maxOfOrNull { it.zone.QueuePosition }?.coerceAtLeast(1) ?: 1
        zones.sortedWith(
            compareByDescending<PrintedWasteZoneOption> { it.zoneId == autoZone?.zoneId }
                .thenBy { printedWasteScore(it, maxPing, maxQueue) }
                .thenBy { it.zoneId },
        )
    }
    var selectedZoneId by remember(game.id, sortedZones) { mutableStateOf<String?>(autoZone?.zoneId) }
    val selectedZone = sortedZones.firstOrNull { it.zoneId == selectedZoneId } ?: autoZone
    val context = LocalContext.current

    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .lockedFocusGroup()
            .background(Color.Black.copy(alpha = 0.72f))
            .clickable(enabled = false) {},
    ) {
        val phoneLandscape = isPhoneLandscape(maxWidth, maxHeight)
        Box(
            Modifier.fillMaxSize(),
            contentAlignment = if (phoneLandscape) Alignment.CenterEnd else Alignment.Center,
        ) {
            Card(
                modifier = modifier
                    .then(
                        if (phoneLandscape) {
                            Modifier
                                .padding(end = 12.dp)
                                .fillMaxWidth(0.9f)
                                .fillMaxHeight(0.9f)
                        } else {
                            Modifier
                                .fillMaxWidth(0.94f)
                                .fillMaxHeight(0.82f)
                        },
                    ),
                colors = CardDefaults.cardColors(containerColor = Panel),
                shape = RoundedCornerShape(22.dp),
            ) {
                if (phoneLandscape) {
                    Row(
                        Modifier.fillMaxSize().padding(14.dp),
                        horizontalArrangement = Arrangement.spacedBy(14.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        PrintedWasteGameSummary(
                            game = game,
                            modifier = Modifier
                                .width(190.dp)
                                .fillMaxHeight(),
                        )
                        PrintedWasteOptionsColumn(
                            state = state,
                            zones = sortedZones,
                            selectedZoneId = selectedZoneId,
                            selectedZone = selectedZone,
                            autoZone = autoZone,
                            showRecommendedCard = true,
                            onSelectZone = { selectedZoneId = it },
                            onRetry = viewModel::refreshPrintedWasteQueues,
                            onDismiss = viewModel::dismissPrintedWasteSelector,
                            onDefault = { viewModel.launchWithPrintedWaste(null) },
                            onLaunch = { viewModel.launchWithPrintedWaste(selectedZone?.routingUrl) },
                            modifier = Modifier
                                .weight(1f)
                                .fillMaxHeight(),
                        )
                    }
                } else {
                    Column(Modifier.fillMaxSize().padding(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            UrlImage(
                                gameTvBannerImageUrl(context, game),
                                Modifier
                                    .width(98.dp)
                                    .aspectRatio(16f / 9f)
                                    .clip(RoundedCornerShape(OpenNowRadius.md)),
                            )
                            Spacer(Modifier.width(12.dp))
                            Column(Modifier.weight(1f)) {
                                Text(game.title, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                Text("Free tier queue routing", color = TextMuted, style = MaterialTheme.typography.bodySmall)
                            }
                        }
                        PrintedWasteOptionsColumn(
                            state = state,
                            zones = sortedZones,
                            selectedZoneId = selectedZoneId,
                            selectedZone = selectedZone,
                            autoZone = autoZone,
                            showRecommendedCard = true,
                            onSelectZone = { selectedZoneId = it },
                            onRetry = viewModel::refreshPrintedWasteQueues,
                            onDismiss = viewModel::dismissPrintedWasteSelector,
                            onDefault = { viewModel.launchWithPrintedWaste(null) },
                            onLaunch = { viewModel.launchWithPrintedWaste(selectedZone?.routingUrl) },
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
            }
        }
    }
}

// PrintedWasteGameSummary (was OpenNowScreens.kt:14309)
@Composable
private fun PrintedWasteGameSummary(
    game: GameInfo,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        UrlImage(
            gameTvBannerImageUrl(context, game),
            Modifier
                .fillMaxWidth()
                .aspectRatio(16f / 9f)
                .clip(RoundedCornerShape(OpenNowRadius.lg)),
        )
        Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(game.title, fontWeight = FontWeight.Bold, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text("Free tier queue routing", color = TextMuted, style = MaterialTheme.typography.bodySmall, maxLines = 1)
        }
    }
}

// PrintedWasteOptionsColumn (was OpenNowScreens.kt:14330)
@Composable
private fun PrintedWasteOptionsColumn(
    state: OpenNowUiState,
    zones: List<PrintedWasteZoneOption>,
    selectedZoneId: String?,
    selectedZone: PrintedWasteZoneOption?,
    autoZone: PrintedWasteZoneOption?,
    showRecommendedCard: Boolean,
    onSelectZone: (String) -> Unit,
    onRetry: () -> Unit,
    onDismiss: () -> Unit,
    onDefault: () -> Unit,
    onLaunch: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val zoneListState = rememberLazyListState()
    val zoneListFocusRequester = remember { FocusRequester() }
    val defaultFocusRequester = remember { FocusRequester() }
    val launchFocusRequester = remember { FocusRequester() }
    var zoneListFocused by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    fun selectZoneAt(index: Int) {
        val next = zones.getOrNull(index) ?: return
        onSelectZone(next.zoneId)
        scope.launch {
            zoneListState.animateScrollToItem(index)
        }
    }
    LaunchedEffect(state.printedWasteLoading, state.printedWasteError, zones.size) {
        delay(80)
        if (!state.printedWasteLoading && state.printedWasteError == null && zones.isNotEmpty()) {
            runCatching { launchFocusRequester.requestFocus() }
        } else {
            runCatching { defaultFocusRequester.requestFocus() }
        }
    }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (state.printedWasteLoading) {
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    CircularProgressIndicator(color = MaterialTheme.colorScheme.primary)
                    Text("Checking PrintedWaste queues and latency", color = TextMuted)
                }
            }
        } else if (state.printedWasteError != null) {
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(state.printedWasteError, color = MaterialTheme.colorScheme.error)
                    OutlinedButton(onClick = onRetry) { Text("Retry") }
                }
            }
        } else {
            if (showRecommendedCard) {
                autoZone?.let {
                    RecommendedPrintedWasteCard(it)
                }
            }
            var listFocused by remember { mutableStateOf(false) }
            LazyColumn(
                state = zoneListState,
                modifier = Modifier
                    .weight(1f)
                    .focusRequester(zoneListFocusRequester)
                    .onFocusChanged { listFocused = it.isFocused }
                    .onPreviewKeyEvent { event ->
                        if (isTvActivateKey(event)) {
                            if (selectedZone != null) {
                                onLaunch()
                                true
                            } else {
                                false
                            }
                        } else if (event.type == KeyEventType.KeyDown) {
                            val selectedIndex = zones.indexOfFirst { it.zoneId == selectedZoneId }.let { if (it >= 0) it else 0 }
                            when (event.key) {
                                Key.DirectionUp -> {
                                    if (selectedIndex > 0) {
                                        selectZoneAt(selectedIndex - 1)
                                        true
                                    } else {
                                        false
                                    }
                                }
                                Key.DirectionDown -> {
                                    if (selectedIndex < zones.lastIndex) {
                                        selectZoneAt(selectedIndex + 1)
                                        true
                                    } else {
                                        runCatching { launchFocusRequester.requestFocus() }.isSuccess
                                    }
                                }
                                else -> false
                            }
                        } else {
                            false
                        }
                    }
                    .focusable(),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                items(zones, key = { it.zoneId }) { zoneOption ->
                    val isCurrent = zoneOption.zoneId == selectedZoneId
                    PrintedWasteZoneRow(
                        zoneOption = zoneOption,
                        selected = isCurrent,
                        focused = isCurrent && listFocused,
                        listFocused = listFocused,
                        onClick = { onSelectZone(zoneOption.zoneId) },
                    )
                }
            }
        }

        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onDismiss) { Text("Cancel") }
            OutlinedButton(
                onClick = onDefault,
                modifier = Modifier
                    .weight(1f)
                    .focusRequester(defaultFocusRequester),
            ) {
                Text("Default", maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Button(
                onClick = onLaunch,
                enabled = !state.printedWasteLoading && selectedZone != null,
                modifier = Modifier
                    .weight(1f)
                    .focusRequester(launchFocusRequester)
                    .focusProperties { up = zoneListFocusRequester },
            ) {
                Text("Launch", maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

// RecommendedPrintedWasteCard (was OpenNowScreens.kt:14467)
@Composable
private fun RecommendedPrintedWasteCard(zoneOption: PrintedWasteZoneOption) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(OpenNowRadius.lg),
        color = MaterialTheme.colorScheme.primary.copy(alpha = 0.12f),
    ) {
        Row(
            Modifier.padding(horizontal = 14.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text("Best available route", color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold)
                Text(
                    "${zoneOption.zoneId} · ${regionLabel(zoneOption.zone.Region)}",
                    color = TextMuted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            QueueMetricPill("Ping", zoneOption.pingMs?.let { "$it ms" } ?: "Checking")
            QueueMetricPill("Ahead", zoneOption.zone.QueuePosition.toString(), queueColor(zoneOption.zone.QueuePosition))
        }
    }
}

// PrintedWasteZoneRow (was OpenNowScreens.kt:14494)
@Composable
private fun PrintedWasteZoneRow(
    zoneOption: PrintedWasteZoneOption,
    selected: Boolean,
    focused: Boolean,
    listFocused: Boolean,
    onClick: () -> Unit,
) {
    val zone = zoneOption.zone
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .focusProperties { canFocus = false }
            .clip(RoundedCornerShape(OpenNowRadius.md))
            .border(
                width = 2.dp,
                color = if (focused) MaterialTheme.colorScheme.primary else Color.Transparent,
                shape = RoundedCornerShape(OpenNowRadius.md)
            )
            .clickable { onClick() },
        shape = RoundedCornerShape(OpenNowRadius.md),
        color = if (focused) MaterialTheme.colorScheme.primary.copy(alpha = 0.28f) else if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.16f) else PanelAlt,
        tonalElevation = if (selected) 2.dp else 0.dp,
        border = if (selected && listFocused) BorderStroke(2.dp, MaterialTheme.colorScheme.primary) else null,
    ) {
        BoxWithConstraints(Modifier.fillMaxWidth()) {
            val compact = maxWidth < CONTENT_COMPACT_MAX_WIDTH
            if (compact) {
                Column(
                    Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(zoneOption.zoneId, fontWeight = FontWeight.Bold, color = if (selected) MaterialTheme.colorScheme.primary else TextPrimary)
                            Text(regionLabel(zone.Region), color = TextMuted, style = MaterialTheme.typography.bodySmall)
                        }
                        if (selected) {
                            Text("Selected", color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.labelMedium)
                        }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        QueueMetricPill("Ping", zoneOption.pingMs?.let { "$it ms" } ?: "--", zoneOption.pingMs?.let(::pingColor) ?: TextMuted)
                        QueueMetricPill("Ahead", zone.QueuePosition.toString(), queueColor(zone.QueuePosition))
                        zone.eta?.let { QueueMetricPill("Wait", formatPrintedWasteWait(it)) }
                    }
                }
            } else {
                Row(
                    Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(zoneOption.zoneId, fontWeight = FontWeight.Bold, color = if (selected) MaterialTheme.colorScheme.primary else TextPrimary)
                        Text(regionLabel(zone.Region), color = TextMuted, style = MaterialTheme.typography.bodySmall)
                    }
                    QueueMetricPill("Ping", zoneOption.pingMs?.let { "$it ms" } ?: "--", zoneOption.pingMs?.let(::pingColor) ?: TextMuted)
                    QueueMetricPill("Ahead", zone.QueuePosition.toString(), queueColor(zone.QueuePosition))
                    zone.eta?.let { QueueMetricPill("Wait", formatPrintedWasteWait(it)) }
                }
            }
        }
    }
}

// QueueMetricPill (was OpenNowScreens.kt:14560)
@Composable
private fun QueueMetricPill(
    label: String,
    value: String,
    valueColor: Color = TextPrimary,
) {
    Surface(
        shape = RoundedCornerShape(10.dp),
        color = Color.Black.copy(alpha = 0.22f),
        border = BorderStroke(1.dp, Color.White.copy(alpha = 0.08f)),
    ) {
        Column(
            Modifier.padding(horizontal = 9.dp, vertical = 6.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(label, color = TextMuted, style = MaterialTheme.typography.labelSmall)
            Text(value, color = valueColor, fontWeight = FontWeight.Bold, style = MaterialTheme.typography.labelMedium)
        }
    }
}

// isStandardPrintedWasteZone (was OpenNowScreens.kt:14581)
private fun isStandardPrintedWasteZone(zoneId: String): Boolean =
    zoneId.startsWith("NP-") && !zoneId.startsWith("NPA-")

// PrintedWasteZoneOption (was OpenNowScreens.kt:14584)
private data class PrintedWasteZoneOption(
    val zoneId: String,
    val zone: PrintedWasteZone,
    val routingUrl: String,
    val pingMs: Long?,
)

// recommendedPrintedWasteZone (was OpenNowScreens.kt:14591)
private fun recommendedPrintedWasteZone(zones: List<PrintedWasteZoneOption>): PrintedWasteZoneOption? {
    if (zones.isEmpty()) return null
    val pool = zones.filter { it.pingMs != null }.ifEmpty { zones }
    val maxPing = pool.mapNotNull { it.pingMs }.maxOrNull()?.coerceAtLeast(1) ?: 1
    val maxQueue = pool.maxOfOrNull { it.zone.QueuePosition }?.coerceAtLeast(1) ?: 1
    return pool.minWithOrNull(
        compareBy<PrintedWasteZoneOption> { printedWasteScore(it, maxPing, maxQueue) }
            .thenBy { it.pingMs ?: Long.MAX_VALUE }
            .thenBy { it.zone.QueuePosition },
    )
}

// printedWasteScore (was OpenNowScreens.kt:14603)
private fun printedWasteScore(zone: PrintedWasteZoneOption, maxPing: Long, maxQueue: Int): Double {
    val pingScore = ((zone.pingMs ?: maxPing).toDouble() / maxPing.toDouble()) * 0.75
    val queueScore = (zone.zone.QueuePosition.toDouble() / maxQueue.toDouble()) * 0.25
    return pingScore + queueScore
}

// printedWasteZoneUrl (was OpenNowScreens.kt:14609)
private fun printedWasteZoneUrl(zoneId: String): String =
    "https://${zoneId.lowercase()}.cloudmatchbeta.nvidiagrid.net/"

// formatPrintedWasteWait (was OpenNowScreens.kt:14612)
private fun formatPrintedWasteWait(etaMs: Long): String {
    val minutes = ((etaMs + 59_999L) / 60_000L).coerceAtLeast(1L)
    return if (minutes < 60L) "${minutes}m" else "${minutes / 60L}h ${minutes % 60L}m"
}

// queueColor (was OpenNowScreens.kt:14617)
private fun queueColor(queue: Int): Color = when {
    queue <= 5 -> Green
    queue <= 20 -> Color(0xffc7ef6b)
    queue <= 45 -> Color(0xffffc95a)
    else -> Color(0xffff8d8d)
}

// pingColor (was OpenNowScreens.kt:14624)
private fun pingColor(pingMs: Long): Color = when {
    pingMs <= 60L -> Green
    pingMs <= 120L -> Color(0xffc7ef6b)
    pingMs <= 180L -> Color(0xffffc95a)
    else -> Color(0xffff8d8d)
}

// regionLabel (was OpenNowScreens.kt:14631)
private fun regionLabel(region: String): String = when (region) {
    "US" -> "North America"
    "CA" -> "Canada"
    "EU" -> "Europe"
    "JP" -> "Japan"
    "KR" -> "South Korea"
    "THAI" -> "Southeast Asia"
    "MY" -> "Malaysia"
    else -> region
}
