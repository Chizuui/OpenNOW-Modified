#include "protocol.hpp"
#include <spdlog/spdlog.h>

namespace protocol {

// ---------------------------------------------------------------------------
// NativeInputPacket::payloadBytes
// ---------------------------------------------------------------------------
std::vector<uint8_t> NativeInputPacket::payloadBytes() const {
    if (payloadBase64.has_value() && !payloadBase64->empty()) {
        // Decode base64
        const std::string& b64 = *payloadBase64;
        static const std::string kChars =
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

        std::vector<uint8_t> out;
        out.reserve((b64.size() * 3) / 4);

        int val = 0, bits = -8;
        for (unsigned char c : b64) {
            if (c == '=') break;
            auto pos = kChars.find(static_cast<char>(c));
            if (pos == std::string::npos) continue;
            val = (val << 6) | static_cast<int>(pos);
            bits += 6;
            if (bits >= 0) {
                out.push_back(static_cast<uint8_t>((val >> bits) & 0xFF));
                bits -= 8;
            }
        }
        return out;
    }
    return payload;
}

// ---------------------------------------------------------------------------
// Response builders
// ---------------------------------------------------------------------------
nlohmann::json makeReadyResponse(const std::string& id, const NativeStreamerCapabilities& caps) {
    nlohmann::json j;
    j["type"] = "ready";
    j["id"]   = id;
    to_json(j["capabilities"], caps);
    return j;
}

nlohmann::json makeOkResponse(const std::string& id) {
    return { {"type", "ok"}, {"id", id} };
}

nlohmann::json makeAnswerResponse(const std::string& id, const std::string& sdp) {
    return {
        {"type",   "answer"},
        {"id",     id},
        {"answer", { {"sdp", sdp} }}
    };
}

nlohmann::json makeErrorResponse(const std::optional<std::string>& id,
                                  const std::string& code,
                                  const std::string& message) {
    nlohmann::json j = { {"type", "error"}, {"code", code}, {"message", message} };
    if (id) j["id"] = *id;
    return j;
}

// ---------------------------------------------------------------------------
// Event builders
// ---------------------------------------------------------------------------
nlohmann::json makeLogEvent(const std::string& level, const std::string& message) {
    return { {"type", "log"}, {"level", level}, {"message", message} };
}

nlohmann::json makeStatusEvent(const std::string& status,
                                const std::optional<std::string>& message) {
    nlohmann::json j = { {"type", "status"}, {"status", status} };
    if (message) j["message"] = *message;
    return j;
}

nlohmann::json makeLocalIceEvent(const std::string& candidate, const std::string& sdpMid) {
    return {
        {"type", "local-ice"},
        {"candidate", {
            {"candidate", candidate},
            {"sdpMid",    sdpMid},
        }}
    };
}

nlohmann::json makeInputReadyEvent(uint16_t protocolVersion) {
    return { {"type", "input-ready"}, {"protocolVersion", protocolVersion} };
}

nlohmann::json makeInputCaptureChangedEvent(bool captured) {
    return { {"type", "input-capture-changed"}, {"captured", captured} };
}

nlohmann::json makeErrorEvent(const std::string& code, const std::string& message) {
    return { {"type", "error"}, {"code", code}, {"message", message} };
}

nlohmann::json makeStatsEvent(
    const std::string& codec,
    const std::string& resolution,
    const std::string& hwAccel,
    uint32_t bitrateKbps,
    uint32_t targetBitrateKbps,
    double decodedFps,
    double renderFps,
    uint64_t framesDecoded,
    uint64_t framesRendered)
{
    return {
        {"type",                       "stats"},
        {"codec",                      codec},
        {"resolution",                 resolution},
        {"hardwareAcceleration",       hwAccel},
        {"bitrateKbps",                bitrateKbps},
        {"targetBitrateKbps",          targetBitrateKbps},
        {"bitratePerformancePercent",  targetBitrateKbps > 0
            ? (bitrateKbps * 100.0 / targetBitrateKbps) : 0.0},
        {"decodedFps",                 decodedFps},
        {"renderFps",                  renderFps},
        {"framesDecoded",              framesDecoded},
        {"framesRendered",             framesRendered},
        {"framesPendingToPresent",     0},
        {"memoryMode",                 "D3D11Memory"},
        {"zeroCopy",                   true},
        {"queueMode",                  "adaptive"},
        {"queueDepthChanges",          0},
        {"presentPacingChanges",       0},
        {"partialFlushCount",          0},
        {"completeFlushCount",         0},
        {"zeroCopyD3d11",              true},
        {"zeroCopyD3d12",              false},
        {"requestedStreamingFeaturesSummary",  ""},
        {"finalizedStreamingFeaturesSummary",  ""},
    };
}

} // namespace protocol
