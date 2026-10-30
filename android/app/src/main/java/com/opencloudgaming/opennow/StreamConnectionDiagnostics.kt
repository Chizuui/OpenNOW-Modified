package com.opencloudgaming.opennow

import android.os.SystemClock
import kotlinx.serialization.json.*
import okhttp3.Call
import okhttp3.EventListener
import okhttp3.Handshake
import okhttp3.Protocol
import java.io.IOException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.Proxy
import java.net.URI

/** Separate capacity from input/stats logs; failed attempts remain useful after stream teardown. */
internal class StreamConnectionDiagnosticsBuffer(
    private val capacity: Int = 32,
    private val elapsedMs: () -> Long,
    private val epochMs: () -> Long = System::currentTimeMillis,
) {
    private val attempts = ArrayDeque<StreamConnectionAttempt>()
    private var totalAttempts = 0L

    init { require(capacity > 0) }

    @Synchronized fun begin(endpoint: String, reconnect: Boolean): StreamConnectionAttempt {
        if (attempts.size == capacity) attempts.removeFirst()
        return StreamConnectionAttempt(++totalAttempts, endpoint, reconnect, elapsedMs, epochMs()).also(attempts::addLast)
    }

    @Synchronized fun snapshot(): JsonObject = buildJsonObject {
        put("attemptLimit", capacity)
        put("totalAttempts", totalAttempts)
        put("evictedAttempts", totalAttempts - attempts.size)
        put("attempts", JsonArray(attempts.map { it.snapshot() }))
    }
}

internal class StreamConnectionAttempt(
    private val number: Long,
    endpoint: String,
    private val reconnect: Boolean,
    private val elapsedMs: () -> Long,
    private val startedAtEpochMs: Long,
) {
    // No pairing/session/peer IDs, query strings, payloads, SDP, or arbitrary protocol values.
    private val endpoint = runCatching {
        val uri = URI(endpoint)
        val host = requireNotNull(uri.host)
        "${uri.scheme}://$host${if (uri.port >= 0) ":${uri.port}" else ""}"
    }.getOrDefault("unavailable")
    private val startedAtMs = elapsedMs()
    private var openedAtMs: Long? = null
    private var endedAtMs: Long? = null
    private var lastReceivedAtMs: Long? = null
    private var phase = "websocket_handshake"
    private var httpStatus: Int? = null
    private var tls: String? = null
    private var closeCode: Int? = null
    private var localCloseRequested = false
    private var handshakeStage = "request_created"
    private val handshakeStages = linkedMapOf<String, Long>()
    private val received = linkedMapOf<String, Long>()
    private val queued = linkedMapOf<String, Long>()
    private val rejected = linkedMapOf<String, Long>()
    private val issues = linkedMapOf<String, JsonObject>()

    @Synchronized fun handshakeStage(stage: String) {
        handshakeStage = stage
        handshakeStages[stage] = (elapsedMs() - startedAtMs).coerceAtLeast(0)
    }

    @Synchronized fun opened(status: Int, tlsVersion: String?) {
        openedAtMs = elapsedMs()
        httpStatus = status
        tls = tlsVersion
        phase = "waiting_for_offer"
    }

    @Synchronized fun received(packet: JsonElement?) {
        lastReceivedAtMs = elapsedMs()
        val kind = signalingPacketDiagnosticKind(packet)
        received[kind] = (received[kind] ?: 0) + 1
        if (kind == "offer") phase = "offer_received"
    }

    @Synchronized fun sent(kind: String, accepted: Boolean) {
        val counts = if (accepted) queued else rejected
        counts[kind] = (counts[kind] ?: 0) + 1
    }

    @Synchronized fun offerTimedOut() { issue("signaling_offer_timeout") }

    @Synchronized fun closing(code: Int) { closeCode = code }

    @Synchronized fun closed(code: Int) {
        closeCode = code
        if (endedAtMs == null) endedAtMs = elapsedMs()
        if (!localCloseRequested) issue("signaling_remote_closed")
    }

    @Synchronized fun failed(error: Throwable, status: Int?) {
        if (endedAtMs == null) endedAtMs = elapsedMs()
        status?.let { httpStatus = it }
        if (!localCloseRequested) issue("signaling_connection_failed", error)
    }

    @Synchronized fun localClose() {
        localCloseRequested = true
        if (endedAtMs == null) endedAtMs = elapsedMs()
    }

    private fun issue(reason: String, error: Throwable? = null) {
        issues[reason] = buildJsonObject {
            put("reasonCode", reason)
            put("timestamp", diagnosticTimestamp(startedAtEpochMs + elapsedMs() - startedAtMs))
            put("phase", phase)
            put("handshakeStage", handshakeStage)
            error?.let {
                // Class chain distinguishes wrapped DNS/TLS/socket errors without retaining secrets
                // embedded in exception messages. The ordinary timeline still has sanitized text.
                val seen = mutableSetOf<Throwable>()
                val causes = generateSequence(it) { cause -> cause.cause }.takeWhile { cause -> seen.add(cause) }.take(5)
                put("exceptionClasses", JsonArray(causes.map { cause -> JsonPrimitive(cause.javaClass.simpleName.take(80)) }.toList()))
            }
        }
    }

    @Synchronized fun snapshot(): JsonObject = buildJsonObject {
        val now = endedAtMs ?: elapsedMs()
        put("attempt", number)
        put("endpoint", endpoint)
        put("reconnect", reconnect)
        put("startedAt", diagnosticTimestamp(startedAtEpochMs))
        put("uptimeMs", startedAtMs)
        put("elapsedMs", (now - startedAtMs).coerceAtLeast(0))
        put("phase", phase)
        put("handshakeStage", handshakeStage)
        put("handshakeStagesMs", JsonObject(handshakeStages.mapValues { JsonPrimitive(it.value) }))
        put("localCloseRequested", localCloseRequested)
        put("ended", endedAtMs != null)
        openedAtMs?.let { put("handshakeMs", (it - startedAtMs).coerceAtLeast(0)) }
        lastReceivedAtMs?.let { put("lastReceiveAfterStartMs", (it - startedAtMs).coerceAtLeast(0)) }
        httpStatus?.let { put("httpStatus", it) }
        tls?.let { put("tlsVersion", it) }
        closeCode?.let { put("closeCode", it) }
        fun counts(values: Map<String, Long>) = JsonObject(values.mapValues { JsonPrimitive(it.value) })
        put("received", counts(received))
        put("sendQueued", counts(queued))
        put("sendRejected", counts(rejected))
        put("issues", JsonArray(issues.values.toList()))
    }
}

/** Fixed labels keep both cardinality and disclosure bounded, even for unexpected packets. */
internal fun signalingPacketDiagnosticKind(element: JsonElement?): String = runCatching {
    if (element == null) return "invalid_json"
    val packet = element as? JsonObject ?: return "non_object"
    val peerMessage = packet["peer_msg"] as? JsonObject
    when {
        peerMessage != null -> {
            val nested = (peerMessage["msg"] as? JsonPrimitive)?.contentOrNull
            val payload = nested?.let { OpenNowJson.parseToJsonElement(it) as? JsonObject }
            when {
                payload == null -> "invalid_peer_message"
                (payload["type"] as? JsonPrimitive)?.contentOrNull == "offer" && (payload["sdp"] as? JsonPrimitive)?.contentOrNull != null -> "offer"
                payload["candidate"] is JsonPrimitive -> "ice"
                else -> "unknown_peer_message"
            }
        }
        "hb" in packet -> "heartbeat"
        "peer_info" in packet -> "peer_info"
        "ack" in packet -> "ack"
        "error" in packet -> "server_error"
        else -> "unknown_packet"
    }
}.getOrDefault("invalid_json")

/** Fixed stage names only: never retain DNS answers, proxy addresses or socket addresses. */
internal fun signalingDiagnosticEventListener(attempt: StreamConnectionAttempt): EventListener = object : EventListener() {
    override fun dnsStart(call: Call, domainName: String) { attempt.handshakeStage("dns_start") }
    override fun dnsEnd(call: Call, domainName: String, inetAddressList: List<InetAddress>) { attempt.handshakeStage("dns_end") }
    override fun connectStart(call: Call, inetSocketAddress: InetSocketAddress, proxy: Proxy) { attempt.handshakeStage("tcp_start") }
    override fun secureConnectStart(call: Call) { attempt.handshakeStage("tls_start") }
    override fun secureConnectEnd(call: Call, handshake: Handshake?) { attempt.handshakeStage("tls_end") }
    override fun connectEnd(call: Call, inetSocketAddress: InetSocketAddress, proxy: Proxy, protocol: Protocol?) { attempt.handshakeStage("tcp_end") }
    override fun connectFailed(call: Call, inetSocketAddress: InetSocketAddress, proxy: Proxy, protocol: Protocol?, ioe: IOException) { attempt.handshakeStage("connect_failed") }
    override fun responseHeadersStart(call: Call) { attempt.handshakeStage("response_headers") }
}

internal object StreamConnectionDiagnostics {
    private val buffer = StreamConnectionDiagnosticsBuffer(elapsedMs = SystemClock::elapsedRealtime)
    fun begin(endpoint: String, reconnect: Boolean) = buffer.begin(endpoint, reconnect)
    fun snapshot() = buffer.snapshot()
}

internal fun formatStreamConnectionDiagnostics(snapshot: JsonObject): String = buildString {
    appendLine("Retention: total=${snapshot["totalAttempts"]} evicted=${snapshot["evictedAttempts"]} limit=${snapshot["attemptLimit"]}")
    val attempts = snapshot["attempts"]?.jsonArray.orEmpty()
    if (attempts.isEmpty()) appendLine("No signaling connection attempts recorded in this app run.")
    attempts.forEach { item ->
        val a = item.jsonObject
        appendLine("attempt=${a["attempt"]} ${a["startedAt"]?.jsonPrimitive?.content} endpoint=${a["endpoint"]?.jsonPrimitive?.content} reconnect=${a["reconnect"]} phase=${a["phase"]?.jsonPrimitive?.content} elapsedMs=${a["elapsedMs"]} handshakeMs=${a["handshakeMs"] ?: "none"} http=${a["httpStatus"] ?: "none"} tls=${a["tlsVersion"] ?: "none"} ended=${a["ended"]} localClose=${a["localCloseRequested"]} closeCode=${a["closeCode"] ?: "none"}")
        appendLine("  handshakeStage=${a["handshakeStage"]} stagesMs=${a["handshakeStagesMs"]} lastReceiveAfterStartMs=${a["lastReceiveAfterStartMs"] ?: "none"}")
        appendLine("  received=${a["received"]} sendQueued=${a["sendQueued"]} sendRejected=${a["sendRejected"]} issues=${a["issues"]}")
    }
}.trimEnd()
