package com.opencloudgaming.opennow.domain

import android.util.Log
import com.opencloudgaming.opennow.BugReportSubmissionState
import com.opencloudgaming.opennow.AndroidBugReport
import com.opencloudgaming.opennow.AndroidBugReportAttachment
import com.opencloudgaming.opennow.uploadAndroidBugReport
import com.opencloudgaming.opennow.sanitizedDebugLogText
import com.opencloudgaming.opennow.debugLogFileName
import com.opencloudgaming.opennow.sanitizeDiagnosticExport
import com.opencloudgaming.opennow.uploadAndroidDiagnosticPaste
import com.opencloudgaming.opennow.DiagnosticShareState
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient

private const val TAG = "BugReportUseCase"
private const val BUG_REPORT_MIN_DESCRIPTION_CHARS = 20

/**
 * Use case handling bug report and diagnostic operations.
 * Extracted from OpenNowViewModel for better separation of concerns.
 */
class BugReportUseCase(
    private val http: OkHttpClient,
) {
    /**
     * Submit a bug report.
     */
    suspend fun submitBugReport(
        title: String,
        description: String,
        versionName: String,
        versionCode: String,
        metadata: Map<String, String>,
    ): Result<String> {
        return try {
            val logFileName = debugLogFileName()
            val logBytes = withContext(Dispatchers.Default) {
                sanitizedDebugLogText().toByteArray(Charsets.UTF_8)
            }
            val receipt = uploadAndroidBugReport(
                http = http,
                report = AndroidBugReport(
                    title = title,
                    description = description,
                    versionName = versionName,
                    versionCode = versionCode,
                    metadata = metadata,
                    files = listOf(
                        AndroidBugReportAttachment(
                            fileName = logFileName,
                            contentType = "text/plain; charset=utf-8",
                            bytes = logBytes,
                        ),
                    ),
                ),
            )
            Log.d(TAG, "Bug report submitted")
            Result.success(receipt.reference)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            Log.e(TAG, "Bug report failed: ${e.message}", e)
            Result.failure(e)
        }
    }

    /**
     * Upload diagnostic share.
     */
    suspend fun uploadDiagnosticShare(): Result<String> {
        return try {
            val sanitizedLog = sanitizedDebugLogText()
            val payload = sanitizeDiagnosticExport(sanitizedLog)
            val pasteUrl = uploadAndroidDiagnosticPaste(http, payload)
            Log.d(TAG, "Diagnostic share uploaded")
            Result.success(pasteUrl)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            Log.e(TAG, "Diagnostic share failed: ${e.message}", e)
            Result.failure(e)
        }
    }

    /**
     * Validate bug report before submission.
     */
    fun validateBugReport(title: String, description: String): String? {
        return when {
            title.isBlank() -> "Enter a short issue title"
            description.trim().length < BUG_REPORT_MIN_DESCRIPTION_CHARS ->
                "Describe what happened in at least $BUG_REPORT_MIN_DESCRIPTION_CHARS characters"
            else -> null
        }
    }

    /**
     * Get the sanitized debug log text.
     */
    fun getDebugLogText(): String {
        return sanitizedDebugLogText()
    }

    /**
     * Get the debug log file name.
     */
    fun getDebugLogFileName(): String {
        return debugLogFileName()
    }

    companion object {
        const val MIN_DESCRIPTION_CHARS = BUG_REPORT_MIN_DESCRIPTION_CHARS
    }
}
