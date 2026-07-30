#include "peer_connection.hpp"
#include <spdlog/spdlog.h>
#include <algorithm>
#include <regex>

namespace peer {

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
static std::string detectCodecFromSdp(const std::string& sdp) {
    // Look for codec in SDP: a=rtpmap:<PT> AV1/, H265/, H264/
    std::regex av1R("a=rtpmap:[0-9]+ AV1/",  std::regex::icase);
    std::regex h265R("a=rtpmap:[0-9]+ H265/", std::regex::icase);
    std::regex h264R("a=rtpmap:[0-9]+ H264/", std::regex::icase);

    if (std::regex_search(sdp, av1R))  return "AV1";
    if (std::regex_search(sdp, h265R)) return "H265";
    if (std::regex_search(sdp, h264R)) return "H264";
    return "H264"; // safe fallback
}

// ---------------------------------------------------------------------------
// PeerConnection
// ---------------------------------------------------------------------------
PeerConnection::PeerConnection(ipc::AsyncEventWriter& writer, RtpCallback rtpCallback)
    : writer_(writer)
    , rtpCallback_(std::move(rtpCallback))
{}

PeerConnection::~PeerConnection() {
    close();
}

void PeerConnection::configure(const protocol::SessionInfo& session) {
    rtc::Configuration config;

    // ICE servers
    for (const auto& srv : session.iceServers) {
        for (const auto& url : srv.urls) {
            rtc::IceServer server(url);
            if (srv.username && srv.credential) {
                server.username = *srv.username;
                server.password = *srv.credential;
            }
            config.iceServers.push_back(server);
        }
    }

    // Performance settings
    config.enableIceTcp = true;
    config.portRangeBegin = 49152;
    config.portRangeEnd   = 65535;

    pc_ = std::make_shared<rtc::PeerConnection>(config);
    setupPeerCallbacks();

    spdlog::info("[Peer] PeerConnection configured with {} ICE servers",
                 session.iceServers.size());
}

void PeerConnection::setupPeerCallbacks() {
    pc_->onStateChange([this](rtc::PeerConnection::State state) {
        onStateChange(state);
    });

    pc_->onLocalDescription([this](rtc::Description desc) {
        onLocalDescription(std::move(desc));
    });

    pc_->onLocalCandidate([this](rtc::Candidate candidate) {
        onLocalCandidate(std::move(candidate));
    });

    pc_->onTrack([this](std::shared_ptr<rtc::Track> track) {
        onTrack(std::move(track));
    });

    pc_->onDataChannel([this](std::shared_ptr<rtc::DataChannel> dc) {
        onDataChannel(std::move(dc));
    });
}

bool PeerConnection::handleOffer(const std::string& commandId, const std::string& offerSdp) {
    if (!pc_) {
        spdlog::error("[Peer] handleOffer called before configure()");
        return false;
    }

    pendingCommandId_ = commandId;
    negotiatedCodec_  = detectCodecFromSdp(offerSdp);
    spdlog::info("[Peer] Detected codec from SDP: {}", negotiatedCodec_);

    try {
        pc_->setRemoteDescription(rtc::Description(offerSdp, "offer"));
        // Answer will be emitted via onLocalDescription callback
        return true;
    } catch (const std::exception& ex) {
        spdlog::error("[Peer] Failed to set remote description: {}", ex.what());
        return false;
    }
}

void PeerConnection::addRemoteIce(const protocol::IceCandidatePayload& candidate) {
    if (!pc_) return;
    try {
        pc_->addRemoteCandidate(
            rtc::Candidate(candidate.candidate, candidate.sdpMid));
    } catch (const std::exception& ex) {
        spdlog::warn("[Peer] addRemoteIce failed: {}", ex.what());
    }
}

bool PeerConnection::sendInput(const std::vector<uint8_t>& payload, bool partiallyReliable) {
    if (!inputDc_ || !inputDc_->isOpen()) return false;
    try {
        // libdatachannel sendMessage accepts std::byte span
        std::vector<std::byte> bytes(payload.size());
        std::transform(payload.begin(), payload.end(), bytes.begin(),
                       [](uint8_t b) { return static_cast<std::byte>(b); });
        inputDc_->send(bytes);
        return true;
    } catch (const std::exception& ex) {
        spdlog::warn("[Peer] sendInput failed: {}", ex.what());
        return false;
    }
}

void PeerConnection::close() {
    if (pc_) {
        try { pc_->close(); } catch (...) {}
        pc_.reset();
    }
    inputDc_.reset();
    videoTrack_.reset();
    connected_.store(false);
}

// ---------------------------------------------------------------------------
// Internal callbacks
// ---------------------------------------------------------------------------
void PeerConnection::onTrack(std::shared_ptr<rtc::Track> track) {
    spdlog::info("[Peer] onTrack: mid={}", track->mid());
    videoTrack_ = track;

    track->onMessage([this](rtc::message_variant data) {
        if (std::holds_alternative<rtc::binary>(data)) {
            auto& bin = std::get<rtc::binary>(data);
            // Raw RTP bytes — pass to video decoder
            if (rtpCallback_) {
                rtpCallback_(
                    reinterpret_cast<const uint8_t*>(bin.data()),
                    bin.size(),
                    negotiatedCodec_);
            }
        }
    });

    track->onOpen([this]() {
        spdlog::info("[Peer] Video track open");
        writer_.push(protocol::makeStatusEvent("video-track-open"));
        // Signal to Electron/renderer that we're now streaming so it activates
        // native input, sends the surface command, and makes nativeRendererActive=true.
        writer_.push(protocol::makeStatusEvent("streaming", "Video track open, streaming active"));
    });
}

void PeerConnection::onDataChannel(std::shared_ptr<rtc::DataChannel> dc) {
    spdlog::info("[Peer] onDataChannel: label={}", dc->label());

    // GFN uses a data channel labeled "input" for binary input packets
    if (dc->label() == "input" || dc->label().find("input") != std::string::npos) {
        inputDc_ = dc;

        dc->onOpen([this]() {
            spdlog::info("[Peer] Input data channel open");
            // Negotiate protocol version — GFN expects a version handshake byte
            // Protocol v2 or v3 — emit input-ready with the agreed version
            writer_.push(protocol::makeInputReadyEvent(inputProtocolVersion_.load()));
        });

        dc->onMessage([this](rtc::message_variant data) {
            if (std::holds_alternative<rtc::binary>(data)) {
                auto& bin = std::get<rtc::binary>(data);
                // Server may send protocol version negotiation response
                if (!bin.empty()) {
                    uint8_t first = static_cast<uint8_t>(bin[0]);
                    if (first == 1 || first == 2 || first == 3) {
                        uint16_t ver = first;
                        inputProtocolVersion_.store(ver);
                        spdlog::info("[Peer] Input protocol version negotiated: {}", ver);
                        writer_.push(protocol::makeInputReadyEvent(ver));
                    }
                }
            }
        });

        dc->onClosed([this]() {
            spdlog::warn("[Peer] Input data channel closed");
        });
    }
}

void PeerConnection::onLocalDescription(rtc::Description desc) {
    if (desc.type() == rtc::Description::Type::Answer) {
        spdlog::info("[Peer] Local answer ready, sending to Electron");
        auto response = protocol::makeAnswerResponse(
            pendingCommandId_,
            std::string(desc));
        // Write synchronously (we're on the libdatachannel callback thread — use the writer)
        writer_.push(std::move(response));
    }
}

void PeerConnection::onLocalCandidate(rtc::Candidate candidate) {
    writer_.push(protocol::makeLocalIceEvent(
        std::string(candidate),
        candidate.mid()));
}

void PeerConnection::onStateChange(rtc::PeerConnection::State state) {
    switch (state) {
    case rtc::PeerConnection::State::Connected:
        spdlog::info("[Peer] PeerConnection connected");
        connected_.store(true);
        writer_.push(protocol::makeStatusEvent("connected"));
        break;
    case rtc::PeerConnection::State::Disconnected:
        spdlog::warn("[Peer] PeerConnection disconnected");
        connected_.store(false);
        writer_.push(protocol::makeStatusEvent("disconnected"));
        break;
    case rtc::PeerConnection::State::Failed:
        spdlog::error("[Peer] PeerConnection failed");
        connected_.store(false);
        writer_.push(protocol::makeErrorEvent("peer-failed",
                                               "WebRTC peer connection failed"));
        break;
    case rtc::PeerConnection::State::Closed:
        spdlog::info("[Peer] PeerConnection closed");
        connected_.store(false);
        break;
    default:
        break;
    }
}

} // namespace peer
