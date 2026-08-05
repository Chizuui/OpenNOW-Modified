package com.opencloudgaming.opennow.screens.settings

import android.content.Intent
import android.net.Uri
import android.os.BatteryManager
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.AccountConnector
import com.opencloudgaming.opennow.AppSettings
import com.opencloudgaming.opennow.AuthSession
import com.opencloudgaming.opennow.LoginProvider
import com.opencloudgaming.opennow.OpenNowViewModel
import com.opencloudgaming.opennow.OpenNowUiState
import com.opencloudgaming.opennow.R
import com.opencloudgaming.opennow.SavedAccount
import com.opencloudgaming.opennow.StorageAddon
import com.opencloudgaming.opennow.SubscriptionInfo
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import java.util.Locale
import kotlin.math.roundToInt

private const val GFN_ADD_STORAGE_URL = "https://gfn.link/addstorage"
private const val GFN_ACCOUNT_HELP_URL = "https://gfn.link/5399"
private const val GFN_STORAGE_MANAGEMENT_URL = "https://gfn.link/cloudstorage"
private const val GFN_STORAGE_RESET_URL = "https://gfn.link/resetstorage"

private val SettingsText = OpenNowPalette.TextPrimary
private val SettingsTextMuted = OpenNowPalette.TextMuted
private val SettingsPanelAlt = OpenNowPalette.PanelAlt

@Composable
internal fun AccountSettingsPanel(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val currentSession = state.authSession
    val currentUserId = currentSession?.user?.userId
    val context = LocalContext.current
    var addAccountPromptOpen by remember { mutableStateOf(false) }
    val addAccountProviders = remember(state.providers, state.selectedProvider) {
        accountProviderOptions(state.providers, state.selectedProvider)
    }

    if (addAccountPromptOpen) {
        AddAccountProviderDialog(
            providers = addAccountProviders,
            selectedProvider = state.selectedProvider,
            onProviderSelected = { provider ->
                addAccountPromptOpen = false
                viewModel.selectProvider(provider)
                viewModel.login(provider)
            },
            onChizuiSelected = {
                addAccountPromptOpen = false
                viewModel.loginWithChizui(promptSelectAccount = true)
            },
            onDismiss = { addAccountPromptOpen = false },
        )
    }

    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        state.savedAccounts.ifEmpty {
            state.authSession?.toSavedAccount()?.let { listOf(it) } ?: emptyList()
        }.forEach { account ->
            val selected = account.userId == currentUserId
            val membershipTier = if (selected) {
                state.subscriptionInfo?.membershipTier?.takeIf { it.isNotBlank() }
                    ?: currentSession?.user?.membershipTier?.takeIf { it.isNotBlank() }
                    ?: account.membershipTier
            } else {
                account.membershipTier
            }

            Surface(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(14.dp),
                color = if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.16f) else MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.76f),
            ) {
                Row(
                    Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(account.displayName.ifBlank { "NVIDIA Account" }, color = SettingsText, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(
                            listOfNotNull(account.email?.takeIf { it.isNotBlank() }, account.providerCode, membershipTier).joinToString(" - "),
                            color = SettingsTextMuted,
                            style = MaterialTheme.typography.bodySmall,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    if (selected) {
                        Text("Active", color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
                    } else {
                        OutlinedButton(onClick = { viewModel.switchAccount(account.userId) }, contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp)) {
                            Text("Switch")
                        }
                    }
                }
            }
        }

        state.deviceLoginPrompt?.let { prompt ->
            com.opencloudgaming.opennow.screens.DeviceLoginPanel(
                prompt = prompt,
                phase = state.launchPhase,
                onCancel = viewModel::cancelLogin,
                modifier = Modifier.fillMaxWidth(),
                qrMaxSize = 240.dp,
            )
        }

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
            Button(onClick = { addAccountPromptOpen = true }, modifier = Modifier.weight(1f)) { Text(stringResource(R.string.settings_account_add)) }
            OutlinedButton(onClick = viewModel::logout, modifier = Modifier.weight(1f)) { Text(stringResource(R.string.settings_account_sign_out)) }
        }
        OutlinedButton(onClick = viewModel::logoutAll, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.settings_account_sign_out_all)) }

        AccountPlayTimeStatsPanel(
            subscriptionInfo = state.subscriptionInfo,
            fallbackMembershipTier = state.authSession?.user?.membershipTier,
        )

        StorageAddonPanel(
            storageAddon = state.subscriptionInfo?.storageAddon,
            openExternal = { url ->
                if (!openExternalUrl(context, url)) {
                    Toast.makeText(context, "No browser available", Toast.LENGTH_SHORT).show()
                }
            },
        )

        AccountConnectorsPanel(
            connectors = state.accountConnectors,
            loading = state.loadingAccountConnectors,
            actionStore = state.connectorActionStore,
            onRefresh = viewModel::refreshAccountConnectors,
            onConnect = { connector ->
                viewModel.connectAccountConnector(connector.store) { url ->
                    if (!openExternalUrl(context, url)) {
                        Toast.makeText(context, "No browser available", Toast.LENGTH_SHORT).show()
                    }
                }
            },
            onDisconnect = { connector -> viewModel.disconnectAccountConnector(connector.store) },
            openExternal = { url ->
                if (!openExternalUrl(context, url)) {
                    Toast.makeText(context, "No browser available", Toast.LENGTH_SHORT).show()
                }
            },
        )
    }
}

@Composable
private fun AccountPlayTimeStatsPanel(subscriptionInfo: SubscriptionInfo?, fallbackMembershipTier: String?) {
    // Implementation would go here
}

@Composable
private fun StorageAddonPanel(storageAddon: StorageAddon?, openExternal: (String) -> Unit) {
    // Implementation would go here
}

@Composable
private fun AccountConnectorsPanel(
    connectors: List<AccountConnector>,
    loading: Boolean,
    actionStore: String?,
    onRefresh: () -> Unit,
    onConnect: (AccountConnector) -> Unit,
    onDisconnect: (AccountConnector) -> Unit,
    openExternal: (String) -> Unit,
) {
    // Implementation would go here
}

@Composable
private fun AddAccountProviderDialog(
    providers: List<LoginProvider>,
    selectedProvider: LoginProvider,
    onProviderSelected: (LoginProvider) -> Unit,
    onChizuiSelected: () -> Unit,
    onDismiss: () -> Unit,
) {
    // Implementation would go here
}

private fun AuthSession.toSavedAccount(): SavedAccount = SavedAccount(
    userId = user.userId,
    displayName = user.displayName,
    email = user.email,
    avatarUrl = user.avatarUrl,
    membershipTier = user.membershipTier,
    providerCode = provider.code,
)

private fun accountProviderOptions(providers: List<LoginProvider>, selectedProvider: LoginProvider): List<LoginProvider> {
    return (providers + selectedProvider)
        .distinctBy { it.idpId.ifBlank { it.code }.lowercase(Locale.US) }
        .ifEmpty { listOf(selectedProvider) }
}

private fun openExternalUrl(context: android.content.Context, url: String): Boolean {
    return try {
        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
        true
    } catch (e: Exception) {
        false
    }
}
