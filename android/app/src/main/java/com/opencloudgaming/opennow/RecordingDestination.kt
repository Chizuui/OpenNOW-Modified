package com.opencloudgaming.opennow

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.widget.Toast
import androidx.activity.result.ActivityResultLauncher

/** Owns the persisted SAF folder grant and file creation for stream recordings. */
internal object RecordingDestination {
    /** Some Android TV images ship without a Storage Access Framework folder picker. */
    fun launchFolderPicker(context: Context, launcher: ActivityResultLauncher<Uri?>): Boolean {
        try {
            launcher.launch(null)
            return true
        } catch (error: ActivityNotFoundException) {
            NativeInputDiagnostics.add("recording folder picker unavailable: ${error.javaClass.simpleName}")
        } catch (error: SecurityException) {
            NativeInputDiagnostics.add("recording folder picker denied: ${error.javaClass.simpleName}")
        }
        Toast.makeText(context, R.string.stream_record_folder_picker_unavailable, Toast.LENGTH_LONG).show()
        return false
    }

    fun rememberFolder(context: Context, uri: Uri): Boolean = try {
        context.contentResolver.takePersistableUriPermission(
            uri,
            Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
        )
        true
    } catch (_: SecurityException) {
        false
    }

    fun createFile(context: Context, folder: String, fileName: String): Uri {
        val treeUri = Uri.parse(folder)
        check(context.contentResolver.persistedUriPermissions.any {
            it.uri == treeUri && it.isWritePermission
        }) { "Recording folder access has expired" }
        val parent = DocumentsContract.buildDocumentUriUsingTree(
            treeUri,
            DocumentsContract.getTreeDocumentId(treeUri),
        )
        return requireNotNull(DocumentsContract.createDocument(
            context.contentResolver,
            parent,
            "video/mp4",
            fileName,
        )) { "Could not create a file in the recording folder" }
    }

    fun folderName(context: Context, folder: String): String? = runCatching {
        val uri = Uri.parse(folder)
        val documentUri = DocumentsContract.buildDocumentUriUsingTree(uri, DocumentsContract.getTreeDocumentId(uri))
        context.contentResolver.query(documentUri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor ->
                if (cursor.moveToFirst()) cursor.getString(0) else null
            }
    }.getOrNull()
}
