package com.opencloudgaming.opennow

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class GfnSessionConnectionsTest {
    private fun resolve(raw: String) = resolveGfnSessionConnections(
        buildJsonObject { put("connectionInfo", OpenNowJson.parseToJsonElement(raw)) },
    )

    @Test fun explicitWebSocketWinsOverEarlierNativeDescriptorAndKeepsItsPort() {
        val result = resolve("""[
            {"usage":14,"appLevelProtocol":6,"resourcePath":"rtsps://native.abya.example:322"},
            {"usage":14,"ip":"203.0.113.10","port":444,"resourcePath":"wss://web.abya.example:8443/nvst/?version=1"},
            {"usage":2,"ip":"203.0.113.20","port":49005},
            {"usage":17,"ip":"203.0.113.21","port":13749}
        ]""")!!
        assertEquals("wss://web.abya.example:8443/nvst/?version=1", result.signalingUrl)
        assertEquals("web.abya.example:8443", result.signalingServer)
        assertEquals("203.0.113.10", result.serverIp)
        assertEquals(MediaConnectionInfo("203.0.113.20", 49005), result.mediaConnectionInfo)
        assertEquals(MediaConnectionInfo("203.0.113.21", 13749), result.bundleConnectionInfo)
        val session = SessionInfo("id", 2, serverIp = result.serverIp,
            signalingServer = result.signalingServer, signalingUrl = result.signalingUrl,
            mediaConnectionInfo = result.mediaConnectionInfo, bundleConnectionInfo = result.bundleConnectionInfo)
        val context = OpenNowJson.parseToJsonElement(nvstSessionContext(session, StreamSettings())).jsonObject
        val bundle = context["session"]!!.jsonObject["connectionInfo"]!!.jsonArray.single().jsonObject
        assertEquals(17, bundle["usage"]!!.jsonPrimitive.int)
        assertEquals(13749, bundle["port"]!!.jsonPrimitive.int)
    }

    @Test fun nativeOnlySeatsUseWebSocket443AndNeverRtspForUdpMedia() {
        for (usage in listOf(14, 16)) {
            val result = resolve("""[{"usage":$usage,"appLevelProtocol":6,
                "resourcePath":"rtsps://seat.abya.example:48322/rtsp"}]""")!!
            assertEquals("wss://seat.abya.example/nvst/", result.signalingUrl)
            assertEquals("seat.abya.example:443", result.signalingServer)
            assertNull(result.mediaConnectionInfo)
            assertNull(result.bundleConnectionInfo)
        }
    }

    @Test fun relativeWebSocketUses443AndKeepsIpv6MediaPortSeparate() {
        val result = resolve("""[{"usage":14,"ip":"2001:db8::10","port":8443,"resourcePath":"/nvst/"}]""")!!
        assertEquals("wss://[2001:db8::10]/nvst/", result.signalingUrl)
        assertEquals("[2001:db8::10]:443", result.signalingServer)
        assertEquals("2001:db8::10", result.serverIp)
        assertEquals(MediaConnectionInfo("2001:db8::10", 8443), result.mediaConnectionInfo)
    }

    @Test fun nvidiaRelativeSignalingDoesNotUseTheAdvertisedUdpMediaPort() {
        for (resource in listOf("null", "\"/nvst/\"", "\"/nvst/?version=1\"")) {
            val result = resolve("""[{"usage":14,"ip":"203.0.113.10","port":49003,
                "resourcePath":$resource}]""")!!
            val path = if (resource.contains("version")) "/nvst/?version=1" else "/nvst/"
            assertEquals("wss://203.0.113.10$path", result.signalingUrl)
            assertEquals("203.0.113.10:443", result.signalingServer)
            assertEquals(MediaConnectionInfo("203.0.113.10", 49003), result.mediaConnectionInfo)
            assertEquals("wss://203.0.113.10/nvst/sign_in?peer_id=peer-test&version=2&peer_role=1&pairing_id=session-test",
                buildGfnSignInUrl(result.signalingUrl, result.signalingServer, "peer-test", "session-test", reconnect = false))
        }
    }

    @Test fun nativeRelativeDescriptorDoesNotReuseRtspPortForWebSocket() {
        val result = resolve("""[{"usage":16,"ip":"seat.abya.example","port":322}]""")!!
        assertEquals("wss://seat.abya.example/nvst/", result.signalingUrl)
    }

    @Test fun invalidCandidatesDoNotHideLaterValidMappings() {
        val result = resolve("""[
            {"usage":14,"resourcePath":"wss://user:password@seat.example/nvst/"},
            {"usage":14,"ip":"0.0.0.0","resourcePath":"/nvst/"},
            {"usage":14,"ip":"","port":70000,"resourcePath":"/nvst/"},
            {"usage":14,"resourcePath":"https://seat.example:9443/nvst/"},
            {"usage":2,"resourcePath":"https://seat.example/media"},
            {"usage":17,"ip":"not-an-ip.example","port":49000},
            {"usage":17,"ip":"203.0.113.10","port":0},
            {"usage":17,"ip":"203.0.113.11","port":49003}
        ]""")!!
        assertEquals("wss://seat.example:9443/nvst/", result.signalingUrl)
        assertEquals(MediaConnectionInfo("203.0.113.11", 49003), result.bundleConnectionInfo)
        assertEquals(MediaConnectionInfo("not-an-ip.example", 49000), result.mediaConnectionInfo)
        assertNull(resolve("""[{"usage":2,"ip":"203.0.113.10","port":49005}]"""))
    }

    @Test fun legacySavedSessionsStillLoadWithoutBundleMapping() {
        val session = OpenNowJson.decodeFromString<SessionInfo>("""{"sessionId":"old","status":2,
            "serverIp":"203.0.113.10","signalingServer":"seat.example:443","signalingUrl":"wss://seat.example/nvst/"}""")
        assertNull(session.bundleConnectionInfo)
    }
}
