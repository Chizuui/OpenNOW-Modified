package com.opencloudgaming.opennow


import androidx.annotation.StringRes
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.runtime.key
import androidx.compose.runtime.setValue
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
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
import com.opencloudgaming.opennow.ui.theme.tint




internal enum class StreamControlsPage {
    Main,
    StatusBar,
    TouchControls,
    MouseMode,
    ReportProblem,
}

@Composable
internal fun StreamControlsPanel(
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

@Composable
internal fun StreamPanelHeader(
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

@Composable
internal fun StreamPanelHeaderButton(
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

internal fun streamPanelPageTransition(
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

internal fun LazyListScope.mouseModePageItems(
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

@OptIn(ExperimentalLayoutApi::class)
internal fun LazyListScope.statusBarPageItems(
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

@Composable
internal fun StreamPanelKeyButton(label: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
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

@Composable
internal fun TouchLayoutSlider(
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

@Composable
internal fun onOffLabel(enabled: Boolean): String =
    stringResource(if (enabled) R.string.common_on else R.string.common_off)

internal const val SHARPENING_SLIDER_STEP = 0.05f

internal const val TOUCH_SCALE_SLIDER_STEP = 0.05f

internal const val TOUCH_DP_SLIDER_STEP = 2f

internal const val JOYSTICK_DEAD_ZONE_STEP = 0.01f

internal const val DP_UNIT = "dp"

@Composable
internal fun StreamKeyboardBar(
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

internal const val MAX_STREAM_KEYBOARD_TEXT_LENGTH = 4096

internal fun Boolean.onOffLabel(): String = if (this) "On" else "Off"
