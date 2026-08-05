package com.opencloudgaming.opennow.screens

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.OpenNowMark
import com.opencloudgaming.opennow.OpenNowViewModel
import com.opencloudgaming.opennow.OpenNowUiState
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette

private val TextMuted = OpenNowPalette.TextMuted

@Composable
internal fun MainAppShell(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    musicControl: TopBarMusicControl,
) {
    // This would contain the main navigation shell
    // For now, it's a simplified version
    when {
        state.page == com.opencloudgaming.opennow.AppPage.Stream -> {
            // Stream view would go here
        }
        state.page == com.opencloudgaming.opennow.AppPage.Home -> {
            // Home/Store view would go here
        }
        state.page == com.opencloudgaming.opennow.AppPage.Library -> {
            // Library view would go here
        }
        state.page == com.opencloudgaming.opennow.AppPage.Settings -> {
            // Settings view would go here
        }
    }
}

@Composable
internal fun LoadingScreen(text: String) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        androidx.compose.foundation.layout.Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(16.dp)
        ) {
            OpenNowMark(72.dp)
            CircularProgressIndicator(color = MaterialTheme.colorScheme.primary)
            Text(text, color = TextMuted)
        }
    }
}

data class TopBarMusicControl(
    val visible: Boolean,
    val playing: Boolean,
    val muted: Boolean,
    val onToggle: () -> Unit,
)
