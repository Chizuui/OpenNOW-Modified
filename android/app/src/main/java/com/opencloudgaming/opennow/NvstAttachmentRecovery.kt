package com.opencloudgaming.opennow

enum class StreamSessionRecoveryMode {
    RefreshSession,
    ResumeNvstAttachment,
}

data class StreamSessionRecoveryRequest(
    val reason: String,
    val mode: StreamSessionRecoveryMode = StreamSessionRecoveryMode.RefreshSession,
)

/** Refresh an unusable attachment without turning every transient media gap into a claim. */
internal fun nvstAttachmentRequiresResume(error: String, retriesExhausted: Boolean = false): Boolean =
    error.startsWith("nvst-attachment-rejected:") ||
        error == "nvst-connect-failed: Could not open RTSPS control channel: HTTP 404" ||
        (retriesExhausted && (
            error.startsWith("nvst-rtsp-timeout:") ||
                error == "NVST media timed out; cloud session retained"
            ))

/** Owned by the ViewModel so recreating a stream client cannot renew the automatic claim budget. */
internal class NvstAttachmentResumeGate {
    private var attemptedSessionId: String? = null

    fun tryAcquire(sessionId: String): Boolean {
        if (sessionId.isBlank() || attemptedSessionId == sessionId) return false
        attemptedSessionId = sessionId
        return true
    }

    fun reset() { attemptedSessionId = null }
}
