package com.opencloudgaming.opennow

import java.io.EOFException
import java.net.SocketException
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class StreamConnectionDiagnosticsTest {
    private var elapsed = 1_000L
    private val epoch = 1_789_289_837_123L
    private fun buffer(capacity: Int = 32) = StreamConnectionDiagnosticsBuffer(capacity, { elapsed }, { epoch })
    private fun JsonObject.attempts() = getValue("attempts").jsonArray
    private fun StreamConnectionAttempt.receiveText(text: String) = received(runCatching { OpenNowJson.parseToJsonElement(text) }.getOrNull())

    @Test fun offerTimeoutAndWrappedEofSurviveTeardownWithoutMediaSamples() {
        val buffer = buffer()
        val attempt = buffer.begin("wss://example.com/nvst/sign_in?pairing_id=private-session", false)
        elapsed += 750
        attempt.opened(101, "TLSv1.2")
        attempt.sent("peer_info", true)
        attempt.receiveText("""{"hb":1}""")
        attempt.sent("heartbeat_reply", true)
        elapsed += 12_000
        attempt.offerTimedOut()
        attempt.failed(EOFException("private-token").apply { initCause(SocketException("private-address")) }, null)
        attempt.localClose()
        val snapshot = buffer.snapshot()
        val a = snapshot.attempts().single().jsonObject
        assertEquals(750L, a.getValue("handshakeMs").jsonPrimitive.long)
        assertEquals(12_750L, a.getValue("elapsedMs").jsonPrimitive.long)
        assertEquals("waiting_for_offer", a.getValue("phase").jsonPrimitive.content)
        assertEquals(1, a.getValue("received").jsonObject.getValue("heartbeat").jsonPrimitive.int)
        assertEquals(2, a.getValue("issues").jsonArray.size)
        assertFalse(snapshot.toString().contains("private-"))
        assertTrue(snapshot.toString().contains("SocketException"))
        val assessment = diagnosticFailureAssessment(emptyList(), null, null, streamConnections = snapshot)
        assertEquals(2, assessment.getValue("findings").jsonArray.size)
        assertEquals("not_established", assessment.getValue("crashConclusion").jsonPrimitive.content)
        val export = renderDiagnosticReport(formatStreamConnectionDiagnostics(snapshot), buildJsonObject {
            put("streamConnections", snapshot)
            put("failureAssessment", assessment)
        })
        assertTrue(export.contains("signaling_offer_timeout"))
        assertTrue(export.contains("/streamConnections/attempts/0/issues/0"))
        assertFalse(export.contains("private-"))
    }

    @Test fun successfulOfferAndIntentionalCloseDoNotInventFailure() {
        val buffer = buffer()
        val attempt = buffer.begin("wss://example.com/nvst/sign_in", false)
        attempt.opened(101, "TLSv1.2")
        attempt.receiveText("""{"peer_msg":{"msg":"{\"type\":\"offer\",\"sdp\":\"private-sdp\"}"}}""")
        attempt.localClose()
        elapsed += 2_000
        attempt.closing(1000)
        attempt.closed(1000)
        attempt.failed(EOFException(), null) // late callback from a locally retired transport
        val snapshot = buffer.snapshot()
        val a = snapshot.attempts().single().jsonObject
        assertEquals("offer_received", a.getValue("phase").jsonPrimitive.content)
        assertEquals(1000, a.getValue("closeCode").jsonPrimitive.int)
        assertEquals(0L, a.getValue("elapsedMs").jsonPrimitive.long)
        assertTrue(diagnosticFailureAssessment(emptyList(), null, null, streamConnections = snapshot)
            .getValue("findings").jsonArray.isEmpty())
        assertFalse(snapshot.toString().contains("private-sdp"))
    }

    @Test fun handshakeFailureIsDistinctFromPostOpenFailureAndRejectedSend() {
        val buffer = buffer()
        val attempt = buffer.begin("wss://example.com/sign_in", true)
        attempt.sent("peer_info", false)
        elapsed += 500
        attempt.failed(java.net.UnknownHostException("private-host"), 503)
        val a = buffer.snapshot().attempts().single().jsonObject
        assertFalse(a.containsKey("handshakeMs"))
        assertEquals("websocket_handshake", a.getValue("phase").jsonPrimitive.content)
        assertEquals(503, a.getValue("httpStatus").jsonPrimitive.int)
        assertEquals(1, a.getValue("sendRejected").jsonObject.getValue("peer_info").jsonPrimitive.int)
        elapsed += 20_000
        assertEquals(500L, buffer.snapshot().attempts().single().jsonObject.getValue("elapsedMs").jsonPrimitive.long)
    }

    @Test fun retriesHaveBoundedHistoryAndGroupedFindingsWithValidEvidencePaths() {
        val buffer = buffer(2)
        repeat(4) { buffer.begin("wss://example.com/sign_in", it > 0).failed(EOFException(), null) }
        val snapshot = buffer.snapshot()
        assertEquals(4, snapshot.getValue("totalAttempts").jsonPrimitive.int)
        assertEquals(2, snapshot.getValue("evictedAttempts").jsonPrimitive.int)
        val finding = diagnosticFailureAssessment(emptyList(), null, null, streamConnections = snapshot)
            .getValue("findings").jsonArray.single().jsonObject
        assertEquals(2, finding.getValue("occurrences").jsonPrimitive.int)
        assertEquals(listOf("/streamConnections/attempts/0/issues/0", "/streamConnections/attempts/1/issues/0"),
            finding.getValue("evidencePaths").jsonArray.map { it.jsonPrimitive.content })
    }

    @Test fun remoteCloseRetainsCodeAndDoesNotBecomeLocalWhenCleanedUp() {
        val buffer = buffer()
        val attempt = buffer.begin("wss://example.com/sign_in", false)
        attempt.opened(101, null)
        attempt.closing(1008)
        attempt.closed(1008)
        attempt.localClose()
        val a = buffer.snapshot().attempts().single().jsonObject
        assertEquals(1008, a.getValue("closeCode").jsonPrimitive.int)
        assertEquals("signaling_remote_closed", a.getValue("issues").jsonArray.single().jsonObject.getValue("reasonCode").jsonPrimitive.content)
    }

    @Test fun packetClassificationUsesFixedLabelsAndHandlesMalformedShapes() {
        val packets = mapOf(
            "not json private-token" to "invalid_json",
            "[]" to "non_object",
            """{"peer_info":{"name":"private-peer"}}""" to "peer_info",
            """{"ack":42}""" to "ack",
            """{"error":"private-error"}""" to "server_error",
            """{"hb":1}""" to "heartbeat",
            """{"private-key":"private-value"}""" to "unknown_packet",
            """{"peer_msg":{"msg":"{}"}}""" to "unknown_peer_message",
            """{"peer_msg":{"msg":[]}}""" to "invalid_peer_message",
            """{"peer_msg":{"msg":"{\"candidate\":\"private-ice\"}"}}""" to "ice",
        )
        packets.forEach { (packet, kind) -> assertEquals(packet, kind, signalingPacketDiagnosticKind(runCatching { OpenNowJson.parseToJsonElement(packet) }.getOrNull())) }
        val attempt = buffer().begin("wss://example.com/sign_in", false)
        packets.keys.forEach { attempt.receiveText(it) }
        assertFalse(attempt.snapshot().toString().contains("private-"))
    }

    @Test fun handshakeStagesUseMonotonicOffsetsAndStripEndpointSecrets() {
        val attempt = buffer().begin("wss://private-user:private-password@example.com:443/private-path?token=private-token", false)
        elapsed += 10
        attempt.handshakeStage("dns_start")
        elapsed += 100
        attempt.handshakeStage("dns_end")
        elapsed += 50
        attempt.handshakeStage("tls_start")
        attempt.failed(javax.net.ssl.SSLHandshakeException("private-certificate"), null)
        val snapshot = attempt.snapshot()
        assertEquals("wss://example.com:443", snapshot.getValue("endpoint").jsonPrimitive.content)
        assertEquals(110, snapshot.getValue("handshakeStagesMs").jsonObject.getValue("dns_end").jsonPrimitive.int)
        assertEquals("tls_start", snapshot.getValue("issues").jsonArray.single().jsonObject.getValue("handshakeStage").jsonPrimitive.content)
        assertFalse(snapshot.toString().contains("private-"))
    }
}
