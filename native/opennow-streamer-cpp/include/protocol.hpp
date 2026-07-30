#pragma once
// =============================================================================
// protocol.hpp — IPC message types mirroring Rust protocol.rs (protocol v4)
//
// The C++ streamer uses the SAME stdin/stdout JSON protocol as the Rust streamer.
// All field names use camelCase to match serde's rename_all = "camelCase".
// =============================================================================

#include <cstdint>
#include <optional>
#include <string>
#include <vector>
#include <nlohmann/json.hpp>

// Support std::optional in nlohmann::json
namespace nlohmann {
    template <typename T>
    struct adl_serializer<std::optional<T>> {
        static void to_json(json& j, const std::optional<T>& opt) {
            if (opt.has_value()) {
                j = *opt;
            } else {
                j = nullptr;
            }
        }
        static void from_json(const json& j, std::optional<T>& opt) {
            if (j.is_null()) {
                opt = std::nullopt;
            } else {
                opt = j.get<T>();
            }
        }
    };
}

namespace protocol {

// ---------------------------------------------------------------------------
// Protocol version — must match Electron side
// ---------------------------------------------------------------------------
inline constexpr uint64_t PROTOCOL_VERSION = 4;
inline constexpr const char* BACKEND_NAME = "cpp-native";

// ---------------------------------------------------------------------------
// Inbound: ICE candidate payload
// ---------------------------------------------------------------------------
struct IceCandidatePayload {
    std::string candidate;
    std::string sdpMid;
    std::optional<uint32_t> sdpMLineIndex;
};
NLOHMANN_DEFINE_TYPE_NON_INTRUSIVE_WITH_DEFAULT(IceCandidatePayload, candidate, sdpMid, sdpMLineIndex)

// ---------------------------------------------------------------------------
// Inbound: NativeInputPacket — forwarded directly to data channel
// ---------------------------------------------------------------------------
struct NativeInputPacket {
    std::vector<uint8_t> payload;          // Legacy: JSON number array
    std::optional<std::string> payloadBase64; // Preferred: base64-encoded bytes
    bool partiallyReliable = false;

    // Decode payload_base64 if present, else return payload bytes
    std::vector<uint8_t> payloadBytes() const;
};

inline void from_json(const nlohmann::json& j, NativeInputPacket& p) {
    if (j.contains("payloadBase64") && j["payloadBase64"].is_string()) {
        p.payloadBase64 = j["payloadBase64"].get<std::string>();
    }
    if (j.contains("payload") && j["payload"].is_array()) {
        p.payload = j["payload"].get<std::vector<uint8_t>>();
    }
    if (j.contains("partiallyReliable")) {
        p.partiallyReliable = j.value("partiallyReliable", false);
    }
}

// ---------------------------------------------------------------------------
// Inbound: NativeRenderRect
// ---------------------------------------------------------------------------
struct NativeRenderRect {
    int32_t x = 0;
    int32_t y = 0;
    int32_t width = 0;
    int32_t height = 0;
};
NLOHMANN_DEFINE_TYPE_NON_INTRUSIVE_WITH_DEFAULT(NativeRenderRect, x, y, width, height)

// ---------------------------------------------------------------------------
// Inbound: NativeRenderSurface (from "surface" command)
// ---------------------------------------------------------------------------
struct NativeRenderSurface {
    // Parent HWND as decimal string or integer
    std::optional<std::string> parentHwnd;
    std::optional<NativeRenderRect> rect;
    bool visible = true;
    double deviceScaleFactor = 1.0;
    bool showStats = false;
};

inline void from_json(const nlohmann::json& j, NativeRenderSurface& s) {
    if (j.contains("windowHandle") || j.contains("parentHwnd")) {
        std::string key = j.contains("windowHandle") ? "windowHandle" : "parentHwnd";
        if (j[key].is_string())
            s.parentHwnd = j[key].get<std::string>();
        else if (j[key].is_number())
            s.parentHwnd = std::to_string(j[key].get<int64_t>());
    }
    if (j.contains("rect") && !j["rect"].is_null())
        s.rect = j["rect"].get<NativeRenderRect>();
    s.visible = j.value("visible", true);
    s.deviceScaleFactor = j.value("deviceScaleFactor", 1.0);
    s.showStats = j.value("showStats", false);
}

// ---------------------------------------------------------------------------
// Inbound: Ice server
// ---------------------------------------------------------------------------
struct IceServer {
    std::vector<std::string> urls;
    std::optional<std::string> username;
    std::optional<std::string> credential;
};
NLOHMANN_DEFINE_TYPE_NON_INTRUSIVE_WITH_DEFAULT(IceServer, urls, username, credential)

// ---------------------------------------------------------------------------
// Inbound: Stream settings
// ---------------------------------------------------------------------------
struct StreamSettings {
    std::string resolution;
    uint32_t fps = 60;
    uint32_t maxBitrateMbps = 50;
    std::string codec;              // "H264" | "H265" | "AV1"
    std::string colorQuality;
    bool enableCloudGsync = false;
};
NLOHMANN_DEFINE_TYPE_NON_INTRUSIVE_WITH_DEFAULT(
    StreamSettings, resolution, fps, maxBitrateMbps, codec, colorQuality, enableCloudGsync)

// ---------------------------------------------------------------------------
// Inbound: MediaConnectionInfo
// ---------------------------------------------------------------------------
struct MediaConnectionInfo {
    std::optional<std::string> serverAddress;
    std::optional<uint16_t> serverPort;
    std::optional<std::string> relayAddress;
    std::optional<uint16_t> relayPort;
};
NLOHMANN_DEFINE_TYPE_NON_INTRUSIVE_WITH_DEFAULT(
    MediaConnectionInfo, serverAddress, serverPort, relayAddress, relayPort)

// ---------------------------------------------------------------------------
// Inbound: SessionInfo
// ---------------------------------------------------------------------------
struct SessionInfo {
    std::string sessionId;
    std::string serverIp;
    std::vector<IceServer> iceServers;
    std::optional<MediaConnectionInfo> mediaConnectionInfo;
};
NLOHMANN_DEFINE_TYPE_NON_INTRUSIVE_WITH_DEFAULT(
    SessionInfo, sessionId, serverIp, iceServers, mediaConnectionInfo)

// ---------------------------------------------------------------------------
// Inbound: Shortcut bindings
// ---------------------------------------------------------------------------
struct NativeStreamerShortcutBindings {
    std::string toggleStats;
    std::string togglePointerLock;
    std::string toggleFullscreen;
    std::string stopStream;
    std::string toggleAntiAfk;
    std::string toggleMicrophone;
    std::string screenshot;
    std::string toggleRecording;
};
NLOHMANN_DEFINE_TYPE_NON_INTRUSIVE_WITH_DEFAULT(
    NativeStreamerShortcutBindings,
    toggleStats, togglePointerLock, toggleFullscreen, stopStream,
    toggleAntiAfk, toggleMicrophone, screenshot, toggleRecording)

// ---------------------------------------------------------------------------
// Inbound: Full session context (from "start" command)
// ---------------------------------------------------------------------------
struct NativeStreamerSessionContext {
    SessionInfo session;
    StreamSettings settings;
    NativeStreamerShortcutBindings shortcuts;
};
NLOHMANN_DEFINE_TYPE_NON_INTRUSIVE_WITH_DEFAULT(
    NativeStreamerSessionContext, session, settings, shortcuts)

// ---------------------------------------------------------------------------
// Inbound: CommandEnvelope — top-level IPC command
// ---------------------------------------------------------------------------
struct CommandEnvelope {
    std::string id;
    std::string type;   // "hello" | "start" | "offer" | "remote-ice" | "input" | ...
    std::optional<uint64_t> protocolVersion;
    std::optional<NativeStreamerSessionContext> context;
    std::optional<std::string> sdp;
    std::optional<IceCandidatePayload> candidate;
    std::optional<NativeInputPacket> input;
    std::optional<bool> paused;
    std::optional<NativeRenderSurface> surface;
    std::optional<uint32_t> maxBitrateKbps;
    std::optional<std::string> reason;
    std::optional<NativeStreamerShortcutBindings> shortcuts;
};

inline void from_json(const nlohmann::json& j, CommandEnvelope& c) {
    c.id   = j.value("id",   "");
    c.type = j.value("type", "");
    if (j.contains("protocolVersion") && j["protocolVersion"].is_number())
        c.protocolVersion = j["protocolVersion"].get<uint64_t>();
    if (j.contains("context") && !j["context"].is_null())
        c.context = j["context"].get<NativeStreamerSessionContext>();
    if (j.contains("sdp") && j["sdp"].is_string())
        c.sdp = j["sdp"].get<std::string>();
    if (j.contains("candidate") && !j["candidate"].is_null())
        c.candidate = j["candidate"].get<IceCandidatePayload>();
    if (j.contains("input") && !j["input"].is_null())
        c.input = j["input"].get<NativeInputPacket>();
    if (j.contains("paused") && j["paused"].is_boolean())
        c.paused = j["paused"].get<bool>();
    if (j.contains("surface") && !j["surface"].is_null())
        c.surface = j["surface"].get<NativeRenderSurface>();
    if (j.contains("maxBitrateKbps") && j["maxBitrateKbps"].is_number())
        c.maxBitrateKbps = j["maxBitrateKbps"].get<uint32_t>();
    if (j.contains("reason") && j["reason"].is_string())
        c.reason = j["reason"].get<std::string>();
    if (j.contains("shortcuts") && !j["shortcuts"].is_null())
        c.shortcuts = j["shortcuts"].get<NativeStreamerShortcutBindings>();
}

// ---------------------------------------------------------------------------
// Outbound: Capabilities (sent in "ready" response)
// ---------------------------------------------------------------------------
struct NativeVideoCodecCapability {
    std::string codec;      // "H264" | "H265" | "AV1"
    bool available = false;
    std::optional<std::string> decoder;
    std::optional<std::string> reason;
};

inline void to_json(nlohmann::json& j, const NativeVideoCodecCapability& c) {
    j = { {"codec", c.codec}, {"available", c.available} };
    if (c.decoder) j["decoder"] = *c.decoder;
    if (c.reason)  j["reason"]  = *c.reason;
}

struct NativeVideoBackendCapability {
    std::string backend;
    std::string platform;
    std::vector<NativeVideoCodecCapability> codecs;
    std::vector<std::string> zeroCopyModes;
    std::optional<std::string> sink;
    bool available = false;
    std::optional<std::string> reason;
};

inline void to_json(nlohmann::json& j, const NativeVideoBackendCapability& b) {
    j = {
        {"backend",       b.backend},
        {"platform",      b.platform},
        {"codecs",        b.codecs},
        {"zeroCopyModes", b.zeroCopyModes},
        {"available",     b.available},
    };
    if (b.sink)   j["sink"]   = *b.sink;
    if (b.reason) j["reason"] = *b.reason;
}

struct NativeStreamerCapabilities {
    uint64_t protocolVersion = PROTOCOL_VERSION;
    std::string backend = BACKEND_NAME;
    bool supportsOfferAnswer = true;
    bool supportsRemoteIce = true;
    bool supportsLocalIce = true;
    bool supportsInput = true;
    std::vector<NativeVideoBackendCapability> videoBackends;
};

inline void to_json(nlohmann::json& j, const NativeStreamerCapabilities& caps) {
    j = {
        {"protocolVersion",    caps.protocolVersion},
        {"backend",            caps.backend},
        {"supportsOfferAnswer",caps.supportsOfferAnswer},
        {"supportsRemoteIce",  caps.supportsRemoteIce},
        {"supportsLocalIce",   caps.supportsLocalIce},
        {"supportsInput",      caps.supportsInput},
        {"videoBackends",      caps.videoBackends},
    };
}

// ---------------------------------------------------------------------------
// Outbound: Response messages (sent in response to a command)
// ---------------------------------------------------------------------------
struct SendAnswerRequest {
    std::string sdp;
};
inline void to_json(nlohmann::json& j, const SendAnswerRequest& a) {
    j = { {"sdp", a.sdp} };
}

nlohmann::json makeReadyResponse(const std::string& id, const NativeStreamerCapabilities& caps);
nlohmann::json makeOkResponse(const std::string& id);
nlohmann::json makeAnswerResponse(const std::string& id, const std::string& sdp);
nlohmann::json makeErrorResponse(const std::optional<std::string>& id,
                                  const std::string& code,
                                  const std::string& message);

// ---------------------------------------------------------------------------
// Outbound: Event messages (emitted asynchronously)
// ---------------------------------------------------------------------------
nlohmann::json makeLogEvent(const std::string& level, const std::string& message);
nlohmann::json makeStatusEvent(const std::string& status,
                                const std::optional<std::string>& message = std::nullopt);
nlohmann::json makeLocalIceEvent(const std::string& candidate, const std::string& sdpMid);
nlohmann::json makeInputReadyEvent(uint16_t protocolVersion);
nlohmann::json makeInputCaptureChangedEvent(bool captured);
nlohmann::json makeErrorEvent(const std::string& code, const std::string& message);
nlohmann::json makeStatsEvent(
    const std::string& codec,
    const std::string& resolution,
    const std::string& hwAccel,
    uint32_t bitrateKbps,
    uint32_t targetBitrateKbps,
    double decodedFps,
    double renderFps,
    uint64_t framesDecoded,
    uint64_t framesRendered);

} // namespace protocol
