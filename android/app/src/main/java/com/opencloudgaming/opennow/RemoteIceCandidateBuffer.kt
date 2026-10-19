package com.opencloudgaming.opennow

import org.webrtc.IceCandidate

/** Owned by the native lifecycle executor, alongside the peer and its remote description. */
internal class RemoteIceCandidateBuffer {
    private val pending = ArrayDeque<IceCandidate>()
    private var remoteDescriptionReady = false

    fun receive(candidate: IceCandidate, deliver: (IceCandidate) -> Unit) {
        // Signaling may identify a media section by index alone. WebRTC's Android JNI string
        // bridge aborts on a null MID; an empty MID preserves native index-based routing.
        val nativeCandidate = if (candidate.sdpMid == null) {
            IceCandidate("", candidate.sdpMLineIndex, candidate.sdp)
        } else candidate
        if (remoteDescriptionReady) {
            deliver(nativeCandidate)
            return
        }
        if (pending.size == MAX_PENDING_CANDIDATES) pending.removeFirst()
        pending.addLast(nativeCandidate)
    }

    fun onRemoteDescriptionSet(deliver: (IceCandidate) -> Unit) {
        remoteDescriptionReady = true
        while (pending.isNotEmpty()) deliver(pending.removeFirst())
    }

    fun clear() {
        pending.clear()
        remoteDescriptionReady = false
    }

    private companion object {
        const val MAX_PENDING_CANDIDATES = 120
    }
}
