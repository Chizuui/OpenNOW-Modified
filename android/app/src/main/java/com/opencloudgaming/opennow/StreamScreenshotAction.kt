package com.opencloudgaming.opennow

import android.graphics.Bitmap
import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.launch

internal data class StreamScreenshotAction(val busy: Boolean, val capture: () -> Unit)

/** Captures on tap, retaining that image while the user chooses the shared capture folder. */
@Composable
internal fun rememberStreamScreenshotAction(
    client: NativeStreamClient,
    sessionId: String?,
    streamReady: Boolean,
    gameTitle: String?,
    folder: String?,
    onFolderSelected: (String) -> Unit,
): StreamScreenshotAction {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val scope = rememberCoroutineScope()
    val currentSessionId by rememberUpdatedState(sessionId)
    val currentStreamReady by rememberUpdatedState(streamReady)
    val updateFolder by rememberUpdatedState(onFolderSelected)
    var busy by remember { mutableStateOf(false) }
    var folderResult by remember { mutableStateOf<CompletableDeferred<Uri?>?>(null) }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        folderResult?.complete(uri)
    }
    return StreamScreenshotAction(busy = busy, capture = {
        if (!busy && streamReady && sessionId != null && lifecycleOwner.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) {
            busy = true
            val requestedSessionId = sessionId
            val fileName = streamCaptureFileName(gameTitle, "png")
            scope.launch {
                var bitmap: Bitmap? = null
                try {
                    bitmap = client.captureStreamScreenshot()
                    // A renderer/session replacement during readback must not capture the next game.
                    if (currentSessionId != requestedSessionId || !currentStreamReady) return@launch
                    val selectedFolder = folder ?: run {
                        val result = CompletableDeferred<Uri?>()
                        folderResult = result
                        if (!StreamCaptureDestination.launchFolderPicker(context, picker)) return@launch
                        val uri = result.await() ?: return@launch
                        check(StreamCaptureDestination.rememberFolder(context, uri)) { "Folder access denied" }
                        uri.toString().also(updateFolder)
                    }
                    saveStreamScreenshot(context, selectedFolder, fileName, bitmap)
                    Toast.makeText(context, R.string.stream_screenshot_saved, Toast.LENGTH_SHORT).show()
                } catch (cancelled: CancellationException) {
                    throw cancelled
                } catch (error: Exception) {
                    NativeInputDiagnostics.add("screenshot failed: ${error.javaClass.simpleName}")
                    Toast.makeText(context, R.string.stream_screenshot_failed, Toast.LENGTH_LONG).show()
                } finally {
                    bitmap?.recycle()
                    folderResult?.cancel()
                    folderResult = null
                    busy = false
                }
            }
        }
    })
}
