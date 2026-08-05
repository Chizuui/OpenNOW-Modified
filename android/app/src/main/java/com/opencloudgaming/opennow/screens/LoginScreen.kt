package com.opencloudgaming.opennow.screens

import android.content.Intent
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.BorderStroke
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.DeviceLoginPrompt
import com.opencloudgaming.opennow.OpenNowViewModel
import com.opencloudgaming.opennow.OpenNowUiState
import com.opencloudgaming.opennow.OpenNowMark
import com.opencloudgaming.opennow.ProviderPicker
import com.opencloudgaming.opennow.QrCode
import com.opencloudgaming.opennow.QrCodeView
import com.opencloudgaming.opennow.SettingSwitch
import com.opencloudgaming.opennow.LocalTvConnectorState
import com.opencloudgaming.opennow.R
import com.opencloudgaming.opennow.LocalTvLoadingProfile
import com.opencloudgaming.opennow.secondsUntil
import com.opencloudgaming.opennow.shouldUseSideBySideDeviceLoginLayout
import com.opencloudgaming.opennow.supportsDeviceCodeLogin
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import kotlinx.coroutines.delay

private val TextPrimary = OpenNowPalette.TextPrimary
private val TextMuted = OpenNowPalette.TextMuted
private val PanelAlt = OpenNowPalette.PanelAlt

@Composable
internal fun LoginScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val signInFocusRequester = remember { FocusRequester() }
    val context = LocalContext.current
    var tokenDialogVisible by remember { mutableStateOf(false) }
    var tokenInput by remember { mutableStateOf("") }
    var pendingLogText by remember { mutableStateOf("") }
    val logExportLauncher = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("text/plain")) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        runCatching {
            context.contentResolver.openOutputStream(uri)?.use { output ->
                output.write(pendingLogText.toByteArray(Charsets.UTF_8))
            } ?: error("Could not open log file")
        }.onSuccess {
            Toast.makeText(context, "Logs exported", Toast.LENGTH_SHORT).show()
        }.onFailure { error ->
            Toast.makeText(context, error.message ?: "Could not export logs", Toast.LENGTH_LONG).show()
        }
    }
    val tvLogin = state.androidTvProfile
    val deviceCodeLoginAvailable = state.selectedProvider.supportsDeviceCodeLogin
    val preferDeviceCodeLogin = tvLogin && deviceCodeLoginAvailable
    val deviceLoginPrompt = state.deviceLoginPrompt.takeIf { deviceCodeLoginAvailable }
    val normalLoginBusy = state.launchPhase.isNotBlank() && deviceLoginPrompt == null

    LaunchedEffect(preferDeviceCodeLogin, deviceLoginPrompt == null) {
        if (preferDeviceCodeLogin && deviceLoginPrompt == null) {
            runCatching { signInFocusRequester.requestFocus() }
        }
    }

    if (preferDeviceCodeLogin && deviceLoginPrompt != null) {
        TvDeviceLoginScreen(
            prompt = deviceLoginPrompt,
            phase = state.launchPhase,
            onCancel = viewModel::cancelLogin,
        )
        return
    }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        val compactForPhonePairing = tvLogin && state.localTvConnector.hosting
        val dedicatedPhonePairing = shouldUseDedicatedTvPairingLayout(
            tvProfile = tvLogin,
            hosting = state.localTvConnector.hosting,
            availableWidthDp = maxWidth.value,
            availableHeightDp = maxHeight.value,
        )

        if (dedicatedPhonePairing) {
            TvPhoneSignInConnector(
                state = state,
                viewModel = viewModel,
                dedicated = true,
                modifier = Modifier.fillMaxSize(),
            )
        } else {
            Column(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(if (compactForPhonePairing) 12.dp else 24.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                OpenNowMark(
                    size = if (compactForPhonePairing) 56.dp else 88.dp,
                    modifier = Modifier.clickable(onClick = viewModel::recordLoginIconTap),
                )
                Spacer(Modifier.height(if (compactForPhonePairing) 8.dp else 20.dp))
                Text(
                    stringResource(R.string.login_opennow_title),
                    color = TextPrimary,
                    style = if (compactForPhonePairing) MaterialTheme.typography.headlineLarge else MaterialTheme.typography.displaySmall,
                    fontWeight = FontWeight.Bold,
                )
                Text(
                    stringResource(R.string.login_opennow_subtitle),
                    color = TextMuted,
                    style = if (compactForPhonePairing) MaterialTheme.typography.bodyMedium else MaterialTheme.typography.bodyLarge,
                )
                Spacer(Modifier.height(if (compactForPhonePairing) 12.dp else 28.dp))
                ProviderPicker(state.providers, state.selectedProvider, viewModel::selectProvider)
                Spacer(Modifier.height(if (compactForPhonePairing) 8.dp else 16.dp))

                deviceLoginPrompt?.let { prompt ->
                    DeviceLoginPanel(prompt = prompt, phase = state.launchPhase, onCancel = viewModel::cancelLogin)
                } ?: Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Button(
                        onClick = { viewModel.login() },
                        enabled = !normalLoginBusy,
                        modifier = Modifier.focusRequester(signInFocusRequester),
                        colors = ButtonDefaults.buttonColors(
                            disabledContainerColor = MaterialTheme.colorScheme.primary.copy(alpha = 0.72f),
                            disabledContentColor = MaterialTheme.colorScheme.onPrimary,
                        ),
                    ) {
                        if (normalLoginBusy) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(18.dp),
                                strokeWidth = 2.dp,
                                color = MaterialTheme.colorScheme.onPrimary,
                            )
                            Spacer(Modifier.width(10.dp))
                        }
                        Text(
                            when {
                                state.launchPhase.isNotBlank() -> state.launchPhase
                                preferDeviceCodeLogin -> stringResource(R.string.login_tv_start, state.selectedProvider.displayName)
                                else -> stringResource(R.string.login_with_provider, state.selectedProvider.displayName)
                            },
                        )
                    }
                    if (!tvLogin && deviceCodeLoginAvailable) {
                        TextButton(onClick = { viewModel.loginWithCode() }, enabled = !normalLoginBusy) {
                            Text(stringResource(R.string.login_use_code))
                        }
                    }
                    Button(
                        onClick = { viewModel.loginWithChizui() },
                        enabled = !normalLoginBusy,
                        colors = ButtonDefaults.buttonColors(
                            containerColor = MaterialTheme.colorScheme.tertiary,
                            contentColor = MaterialTheme.colorScheme.onTertiary
                        )
                    ) {
                        Text(stringResource(R.string.login_with_chizui))
                    }

                    if (tvLogin) {
                        TvPhoneSignInConnector(state = state, viewModel = viewModel)
                    }
                }

                if (state.error != null) {
                    Spacer(Modifier.height(14.dp))
                    Text(state.error.orEmpty(), color = Color(0xffff9f9f))
                }
            }
        }
    }

    // Login tools dialog
    if (state.loginToolsVisible) {
        AlertDialog(
            onDismissRequest = viewModel::dismissLoginTools,
            title = { Text(stringResource(R.string.login_tools_title)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(stringResource(R.string.login_tools_description))
                    Button(
                        onClick = {
                            viewModel.dismissLoginTools()
                            tokenDialogVisible = true
                        },
                        enabled = !normalLoginBusy,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(stringResource(R.string.login_with_token))
                    }
                    OutlinedButton(
                        onClick = {
                            viewModel.dismissLoginTools()
                            if (tvLogin) {
                                viewModel.requestDiagnosticShare()
                            } else {
                                pendingLogText = viewModel.sanitizedDebugLogText()
                                logExportLauncher.launch(viewModel.debugLogFileName())
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(if (tvLogin) "Export logs with QR" else "Export logs")
                    }
                }
            },
            confirmButton = {},
            dismissButton = {
                TextButton(onClick = viewModel::dismissLoginTools) {
                    Text(stringResource(R.string.action_cancel))
                }
            },
        )
    }

    // Token login dialog
    if (tokenDialogVisible) {
        val submitToken = {
            val submittedToken = tokenInput
            tokenInput = ""
            tokenDialogVisible = false
            viewModel.loginWithToken(submittedToken)
        }
        AlertDialog(
            onDismissRequest = {
                tokenInput = ""
                tokenDialogVisible = false
            },
            title = { Text(stringResource(R.string.login_token_title)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(stringResource(R.string.login_token_description))
                    OutlinedTextField(
                        value = tokenInput,
                        onValueChange = { tokenInput = it },
                        modifier = Modifier.fillMaxWidth(),
                        label = { Text(stringResource(R.string.login_token_label)) },
                        minLines = 3,
                        maxLines = 6,
                        visualTransformation = PasswordVisualTransformation(),
                        keyboardOptions = KeyboardOptions(
                            keyboardType = KeyboardType.Password,
                            imeAction = ImeAction.Done,
                        ),
                        keyboardActions = KeyboardActions(
                            onDone = { if (tokenInput.isNotBlank() && !normalLoginBusy) submitToken() },
                        ),
                        singleLine = false,
                    )
                    Text(
                        stringResource(R.string.login_token_warning),
                        color = TextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            },
            confirmButton = {
                Button(
                    onClick = submitToken,
                    enabled = tokenInput.isNotBlank() && !normalLoginBusy,
                ) {
                    Text("Sign in")
                }
            },
            dismissButton = {
                TextButton(
                    onClick = {
                        tokenInput = ""
                        tokenDialogVisible = false
                    },
                ) {
                    Text(stringResource(R.string.action_cancel))
                }
            },
        )
    }
}

@Composable
private fun TvPhoneSignInConnector(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    dedicated: Boolean = false,
    modifier: Modifier = Modifier,
) {
    val connector = state.localTvConnector
    if (!connector.hosting) {
        OutlinedButton(
            onClick = viewModel::startLocalTvConnector,
            enabled = !connector.busy,
            modifier = modifier,
        ) {
            Text(if (connector.busy) "Starting phone pairing…" else "Sign in from OpenNOW on phone")
        }
    } else {
        val qrCode = remember(connector.pairUri) { connector.pairUri?.let(QrCode::encodeText) }
        Card(
            colors = CardDefaults.cardColors(containerColor = PanelAlt),
            shape = RoundedCornerShape(if (dedicated) 26.dp else 18.dp),
            modifier = if (dedicated) {
                modifier.padding(12.dp)
            } else {
                modifier.fillMaxWidth().padding(top = 8.dp)
            },
        ) {
            BoxWithConstraints(if (dedicated) Modifier.fillMaxSize() else Modifier.fillMaxWidth()) {
                val qrSize = if (dedicated) {
                    minOf(maxHeight - 40.dp, maxWidth * 0.36f, 240.dp).coerceAtLeast(152.dp)
                } else {
                    188.dp
                }
                Row(
                    (if (dedicated) Modifier.fillMaxSize() else Modifier.fillMaxWidth())
                        .padding(if (dedicated) 18.dp else 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(if (dedicated) 20.dp else 12.dp),
                ) {
                    if (connector.pairedDeviceName == null) {
                        qrCode?.let { code ->
                            Surface(
                                shape = RoundedCornerShape(18.dp),
                                color = Color.White,
                                border = BorderStroke(3.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.55f)),
                            ) {
                                QrCodeView(code, Modifier.size(qrSize))
                            }
                        }
                    }
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(if (dedicated) 10.dp else 8.dp)) {
                        Text(
                            if (connector.pairedDeviceName == null) "Pair your phone" else "Phone connected",
                            color = TextPrimary,
                            style = if (dedicated) MaterialTheme.typography.headlineMedium else MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.Bold,
                        )
                        Text(
                            if (connector.pairedDeviceName == null) {
                                "Your TV and phone must be on the same Wi-Fi. Scan the QR code with your phone camera; the pairing code expires after five minutes."
                            } else {
                                "${connector.pairedDeviceName} can launch games. Approve trust below for settings, overlays, sessions, and account switching."
                            },
                            color = TextMuted,
                            style = if (dedicated) MaterialTheme.typography.bodyMedium else MaterialTheme.typography.bodySmall,
                            maxLines = if (dedicated) 3 else 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                        if (connector.pairedDeviceName == null) {
                            PairingCodeDisplay(connector.pairingCode, compact = !dedicated)
                        }
                        if (connector.pairedDeviceName != null) {
                            SettingSwitch(
                                label = "Trust this phone",
                                checked = connector.pairedDeviceTrusted,
                                description = "Required before the phone can transfer an account or control TV settings and sessions.",
                            ) { trusted -> viewModel.setLocalTvDeviceTrusted(trusted) }
                        }
                        OutlinedButton(onClick = viewModel::stopLocalTvConnector) {
                            Text(if (connector.pairedDeviceName == null) "Cancel pairing" else "Disconnect phone")
                        }
                    }
                }
            }
        }
    }
    connector.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
}

@Composable
private fun PairingCodeDisplay(code: String?, compact: Boolean) {
    val digits = code?.takeIf { it.length == 4 && it.all(Char::isDigit) } ?: "----"
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text("PAIRING CODE", color = TextMuted, style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.Bold)
        Row(horizontalArrangement = Arrangement.spacedBy(if (compact) 5.dp else 8.dp)) {
            digits.forEach { digit ->
                Surface(
                    modifier = Modifier.size(if (compact) 38.dp else 46.dp),
                    shape = RoundedCornerShape(12.dp),
                    color = Color.White.copy(alpha = 0.07f),
                    border = BorderStroke(1.dp, Color.White.copy(alpha = 0.13f)),
                ) {
                    Box(contentAlignment = Alignment.Center) {
                        Text(
                            digit.toString(),
                            color = TextPrimary,
                            style = if (compact) MaterialTheme.typography.titleMedium else MaterialTheme.typography.headlineSmall,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun TvDeviceLoginScreen(prompt: DeviceLoginPrompt, phase: String, onCancel: () -> Unit) {
    BoxWithConstraints(
        modifier = Modifier.fillMaxSize().padding(horizontal = 48.dp, vertical = 36.dp),
        contentAlignment = Alignment.Center,
    ) {
        val landscape = maxWidth >= 720.dp
        val qrMaxSize = minOf(
            maxWidth * if (landscape) 0.28f else 0.68f,
            maxHeight * if (landscape) 0.58f else 0.38f,
            340.dp,
        )
        DeviceLoginPanel(
            prompt = prompt,
            phase = phase,
            onCancel = onCancel,
            modifier = Modifier.fillMaxWidth(if (landscape) 0.86f else 1f),
            qrMaxSize = qrMaxSize,
            preferLandscapeLayout = landscape,
            focusCancelOnPrompt = false,
        )
    }
}

@Composable
internal fun DeviceLoginPanel(
    prompt: DeviceLoginPrompt,
    phase: String,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier.fillMaxWidth().padding(horizontal = 24.dp),
    qrMaxSize: androidx.compose.ui.unit.Dp = 360.dp,
    preferLandscapeLayout: Boolean = false,
    focusCancelOnPrompt: Boolean = true,
) {
    val context = LocalContext.current
    val clipboardManager = LocalClipboardManager.current
    val configuration = LocalConfiguration.current
    val initialFocusRequester = remember { FocusRequester() }
    val sideBySideLayout = shouldUseSideBySideDeviceLoginLayout(
        orientation = configuration.orientation,
        preferLandscapeLayout = preferLandscapeLayout,
        availableWidthDp = configuration.screenWidthDp,
    )
    val launchUrl = remember(prompt.verificationUriComplete, prompt.verificationUri) {
        prompt.verificationUriComplete ?: prompt.verificationUri
    }
    val qrContent = launchUrl
    var urlActionMessage by remember(launchUrl) { mutableStateOf<String?>(null) }
    val qrCode = remember(qrContent, prompt.verificationUri) {
        QrCode.encodeText(qrContent) ?: QrCode.encodeText(prompt.verificationUri)
    }
    val remainingSeconds by androidx.compose.runtime.produceState(initialValue = secondsUntil(prompt.expiresAt), prompt.expiresAt) {
        while (value > 0) {
            delay(1000L)
            value = secondsUntil(prompt.expiresAt)
        }
    }

    LaunchedEffect(prompt.userCode, focusCancelOnPrompt) {
        runCatching { initialFocusRequester.requestFocus() }
    }

    Card(
        colors = CardDefaults.cardColors(containerColor = PanelAlt, contentColor = TextPrimary),
        shape = RoundedCornerShape(14.dp),
        modifier = modifier,
    ) {
        // Device login content would go here
        // This is a simplified version - the full implementation would include QR code display
        // and verification code input
    }
}

// Helper functions
internal fun shouldUseDedicatedTvPairingLayout(
    tvProfile: Boolean,
    hosting: Boolean,
    availableWidthDp: Float,
    availableHeightDp: Float,
): Boolean = tvProfile && hosting && (availableHeightDp < 500f || availableWidthDp < 760f)
