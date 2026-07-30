#pragma once
// =============================================================================
// peer_connection.hpp — WebRTC peer connection via libdatachannel
//
// Handles: SDP offer/answer, ICE candidate exchange, data channel (input),
// and raw RTP packet delivery for H.264 / H.265 / AV1 video tracks.
// =============================================================================

#include "protocol.hpp"
#include "ipc.hpp"
#include <functional>
#include <memory>
#include <atomic>
#include <rtc/rtc.hpp>

namespace peer {

// RTP packet callback: called on the decoder thread with raw RTP bytes
using RtpCallback = std::function<void(const uint8_t* data, size_t size, const std::string& codec)>;

// ---------------------------------------------------------------------------
// PeerConnection
// ---------------------------------------------------------------------------
class PeerConnection {
public:
    PeerConnection(ipc::AsyncEventWriter& writer, RtpCallback rtpCallback);
    ~PeerConnection();

    // Called on "start" command — configure ICE servers
    void configure(const protocol::SessionInfo& session);

    // Called on "offer" command — set remote SDP, generate answer
    // Sends "answer" response synchronously via writer
    bool handleOffer(const std::string& commandId, const std::string& offerSdp);

    // Called on "remote-ice" command
    void addRemoteIce(const protocol::IceCandidatePayload& candidate);

    // Called on "input" command — send binary payload over data channel
    bool sendInput(const std::vector<uint8_t>& payload, bool partiallyReliable);

    // Called on "stop" command
    void close();

    // Input protocol version negotiated on the data channel
    uint16_t inputProtocolVersion() const { return inputProtocolVersion_; }

    bool isConnected() const { return connected_.load(); }

    // Codec detected from the SDP offer ("H264", "H265", "AV1")
    // May differ from the codec requested in the "start" command.
    const std::string& negotiatedCodec() const { return negotiatedCodec_; }

private:
    void setupPeerCallbacks();
    void onTrack(std::shared_ptr<rtc::Track> track);
    void onDataChannel(std::shared_ptr<rtc::DataChannel> dc);
    void onLocalDescription(rtc::Description desc);
    void onLocalCandidate(rtc::Candidate candidate);
    void onStateChange(rtc::PeerConnection::State state);

    ipc::AsyncEventWriter& writer_;
    RtpCallback rtpCallback_;

    std::shared_ptr<rtc::PeerConnection> pc_;
    std::shared_ptr<rtc::DataChannel> inputDc_;      // "input" data channel
    std::shared_ptr<rtc::Track> videoTrack_;

    std::string negotiatedCodec_;     // "H264" | "H265" | "AV1" from SDP
    std::string pendingCommandId_;    // ID of the "offer" command awaiting answer

    std::atomic<bool> connected_{false};
    std::atomic<uint16_t> inputProtocolVersion_{2};
};

} // namespace peer
