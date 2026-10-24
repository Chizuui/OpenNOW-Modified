package com.opencloudgaming.opennow

import android.content.ActivityNotFoundException
import android.net.Uri
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContract
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.app.ActivityOptionsCompat
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class StreamCaptureDestinationInstrumentedTest {
    @Test
    fun deviceWithoutFolderPickerDoesNotCrashAndCanRetry() {
        var pickerInstalled = false
        val launcher = folderLauncher {
            if (!pickerInstalled) throw ActivityNotFoundException("No folder picker")
        }
        onMainThread {
            assertFalse(StreamCaptureDestination.launchFolderPicker(context, launcher))
            pickerInstalled = true
            assertTrue(StreamCaptureDestination.launchFolderPicker(context, launcher))
        }
    }

    @Test
    fun restrictedFolderPickerDoesNotCrash() {
        onMainThread {
            assertFalse(StreamCaptureDestination.launchFolderPicker(context, folderLauncher {
                throw SecurityException("Folder picker disabled by device policy")
            }))
        }
    }

    @Test
    fun programmingErrorsStillPropagate() {
        var propagated = false
        onMainThread {
            try {
                StreamCaptureDestination.launchFolderPicker(context, folderLauncher {
                    throw IllegalStateException("Launcher is not registered")
                })
            } catch (_: IllegalStateException) {
                propagated = true
            }
        }
        assertTrue(propagated)
    }

    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext

    private fun onMainThread(block: () -> Unit) =
        InstrumentationRegistry.getInstrumentation().runOnMainSync(block)

    private fun folderLauncher(launch: () -> Unit) = object : ActivityResultLauncher<Uri?>() {
        override fun launch(input: Uri?, options: ActivityOptionsCompat?) = launch()
        override fun unregister() = Unit
        override val contract: ActivityResultContract<Uri?, Uri?> = ActivityResultContracts.OpenDocumentTree()
    }
}
