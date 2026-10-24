package com.opencloudgaming.opennow

internal fun streamCaptureFileName(gameTitle: String?, extension: String, timestampMs: Long = System.currentTimeMillis()): String {
    val safeTitle = gameTitle?.replace(Regex("[^\\p{L}\\p{N}._-]+"), "_")?.take(48).orEmpty().ifBlank { "Game" }
    return "OpenNOW_${safeTitle}_${timestampMs}.$extension"
}
