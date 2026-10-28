package com.opencloudgaming.opennow

import android.content.ContentResolver
import android.net.Uri
import android.provider.OpenableColumns
import java.io.ByteArrayOutputStream
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.MultipartBody
import okhttp3.RequestBody.Companion.toRequestBody

/** Common validation for initial reports and conversation uploads. */
internal fun MultipartBody.Builder.addBugReportAttachments(files: List<AndroidBugReportAttachment>): MultipartBody.Builder {
    require(files.size <= ANDROID_BUG_REPORT_MAX_FILES) {
        "Bug reports support up to $ANDROID_BUG_REPORT_MAX_FILES files"
    }
    files.forEach { attachment ->
        require(attachment.fileName.isNotBlank()) { "Bug report files must have a name" }
        require(attachment.bytes.size.toLong() <= ANDROID_BUG_REPORT_MAX_FILE_BYTES) {
            "${attachment.fileName} is larger than 10 MiB"
        }
        addFormDataPart("files", attachment.fileName, attachment.bytes.toRequestBody(attachment.contentType.toMediaType()))
    }
    return this
}

internal fun readBugReportAttachment(contentResolver: ContentResolver, uri: Uri): AndroidBugReportAttachment {
    val fileName = contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
        if (cursor.moveToFirst()) cursor.getString(0) else null
    }?.take(180)?.takeIf { it.isNotBlank() } ?: "attachment"
    val declaredSize = contentResolver.openAssetFileDescriptor(uri, "r")?.use { it.length } ?: -1L
    require(declaredSize <= ANDROID_BUG_REPORT_MAX_FILE_BYTES || declaredSize < 0) { "$fileName is larger than 10 MiB" }
    val bytes = contentResolver.openInputStream(uri)?.use { input ->
        val output = ByteArrayOutputStream()
        val buffer = ByteArray(16 * 1024)
        var total = 0L
        while (true) {
            val count = input.read(buffer)
            if (count < 0) break
            total += count
            require(total <= ANDROID_BUG_REPORT_MAX_FILE_BYTES) { "$fileName is larger than 10 MiB" }
            output.write(buffer, 0, count)
        }
        output.toByteArray()
    } ?: error("Could not read $fileName")
    return AndroidBugReportAttachment(
        fileName = fileName,
        contentType = contentResolver.getType(uri)?.take(120) ?: "application/octet-stream",
        bytes = bytes,
    )
}
