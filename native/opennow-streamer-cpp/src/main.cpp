// =============================================================================
// main.cpp — OpenNOW C++ Native Streamer
//
// Entry point: sets up IPC dispatch loop, wires together WebRTC peer,
// video decoder (WMF/D3D11), and Raw Input capture.
//
// IPC protocol (stdin/stdout, JSON lines): version 4
// Compatible with the existing Rust streamer Electron integration.
// =============================================================================

// NOTE: Do NOT use WinMain / Windows subsystem here.
// This binary must be a CONSOLE-subsystem app so Electron (Node.js spawn)
// can attach piped stdin/stdout handles. See CMakeLists.txt /SUBSYSTEM:CONSOLE.

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <shellscalingapi.h>  // PROCESS_PER_MONITOR_DPI_AWARE

#include "ipc.hpp"
#include "protocol.hpp"
#include "peer_connection.hpp"
#include "video_decoder.hpp"
#include "input_capture.hpp"

#include <spdlog/spdlog.h>
#include <spdlog/sinks/stdout_color_sinks.h>
#include <memory>
#include <atomic>
#include <chrono>
#include <thread>

// ---------------------------------------------------------------------------
// Application State
// ---------------------------------------------------------------------------
struct StreamerApp {
    ipc::AsyncEventWriter          writer;
    ipc::IpcDispatcher             ipc{writer};
    std::unique_ptr<peer::PeerConnection>  peer;
    std::unique_ptr<video::VideoDecoder>   video;
    std::unique_ptr<input::InputCapture>   inputCapture;

    // Session state
    std::string sessionCodec{"H264"};
    uint32_t    targetBitrateKbps{25000};
    std::atomic<bool> streamStarted{false};

    // Stats timer thread
    std::thread statsThread;
    std::atomic<bool> statsRunning{false};

    void startStatsTimer() {
        statsRunning.store(true);
        statsThread = std::thread([this] {
            while (statsRunning.load()) {
                std::this_thread::sleep_for(std::chrono::seconds(2));
                if (!streamStarted.load() || !video) continue;

                int w = video->width();
                int h = video->height();
                std::string res = std::to_string(w) + "x" + std::to_string(h);

                writer.push(protocol::makeStatsEvent(
                    sessionCodec, res,
                    video->hwAccelName(),
                    0,                         // bitrateKbps (not measured yet)
                    targetBitrateKbps,
                    video->renderFps(),
                    video->renderFps(),
                    video->framesDecoded(),
                    video->framesRendered()));
            }
        });
    }

    void stopStatsTimer() {
        statsRunning.store(false);
        if (statsThread.joinable())
            statsThread.join();
    }
};

// ---------------------------------------------------------------------------
// GUID definitions for Media Foundation formats
static const GUID LOCAL_MFVideoFormat_H264 = { 0x34363248, 0x0000, 0x0010, { 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71 } };
static const GUID LOCAL_MFVideoFormat_HEVC = { 0x43564548, 0x0000, 0x0010, { 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71 } };
static const GUID LOCAL_MFVideoFormat_AV1  = { 0x694DD1D3, 0x3CAB, 0x4E99, { 0x8A, 0xD5, 0xFF, 0x34, 0xAC, 0x08, 0xEE, 0x5D } };

static bool testDecoderSupport(const GUID& inputSubtype) {
    // Ensure Media Foundation is initialized
    MFStartup(MF_VERSION);
    MFT_REGISTER_TYPE_INFO inputInfo = { MFMediaType_Video, inputSubtype };
    
    UINT32 count = 0;
    IMFActivate** activates = nullptr;
    
    // Check hardware decoders first
    HRESULT hr = MFTEnumEx(
        MFT_CATEGORY_VIDEO_DECODER,
        MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
        &inputInfo, nullptr,
        &activates, &count);
    
    bool available = false;
    if (SUCCEEDED(hr) && count > 0) {
        available = true;
    }
    
    for (UINT32 i = 0; i < count; i++) activates[i]->Release();
    if (activates) CoTaskMemFree(activates);
    
    if (available) return true;
    
    // Check software decoders fallback
    count = 0;
    activates = nullptr;
    hr = MFTEnumEx(
        MFT_CATEGORY_VIDEO_DECODER,
        MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER,
        &inputInfo, nullptr,
        &activates, &count);
        
    if (SUCCEEDED(hr) && count > 0) {
        available = true;
    }
    
    for (UINT32 i = 0; i < count; i++) activates[i]->Release();
    if (activates) CoTaskMemFree(activates);
    
    return available;
}

// Build capabilities response
// ---------------------------------------------------------------------------
static protocol::NativeStreamerCapabilities buildCapabilities() {
    using namespace protocol;

    bool h264Available = testDecoderSupport(LOCAL_MFVideoFormat_H264);
    bool h265Available = testDecoderSupport(LOCAL_MFVideoFormat_HEVC);
    bool av1Available = testDecoderSupport(LOCAL_MFVideoFormat_AV1);

    // Check what codecs WMF supports on this machine
    NativeVideoCodecCapability h264{
        "H264",
        h264Available,
        h264Available ? std::optional<std::string>{"WMF/DXVA2"} : std::nullopt,
        h264Available ? std::nullopt : std::optional<std::string>{"H.264 MFT decoder not found"}
    };
    NativeVideoCodecCapability h265{
        "H265",
        h265Available,
        h265Available ? std::optional<std::string>{"WMF/NVDEC"} : std::nullopt,
        h265Available ? std::nullopt : std::optional<std::string>{"HEVC Video Extensions not installed"}
    };
    NativeVideoCodecCapability av1{
        "AV1",
        av1Available,
        av1Available ? std::optional<std::string>{"WMF/AV1"} : std::nullopt,
        av1Available ? std::nullopt : std::optional<std::string>{"AV1 Video Extension not installed"}
    };

    NativeVideoBackendCapability backend{
        "cpp-native-wmf",
        "windows",
        {h264, h265, av1},
        {"D3D11Memory"},
        "d3d11videosink",
        true,
        std::nullopt
    };

    NativeStreamerCapabilities caps;
    caps.protocolVersion    = protocol::PROTOCOL_VERSION;
    caps.backend            = protocol::BACKEND_NAME;
    caps.supportsOfferAnswer = true;
    caps.supportsRemoteIce  = true;
    caps.supportsLocalIce   = true;
    caps.supportsInput      = true;
    caps.videoBackends      = {backend};
    return caps;
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------
static bool handleHello(StreamerApp& app, const protocol::CommandEnvelope& cmd) {
    uint64_t requestedVersion = cmd.protocolVersion.value_or(0);
    if (requestedVersion != protocol::PROTOCOL_VERSION) {
        app.ipc.writeResponse(protocol::makeErrorResponse(
            cmd.id, "protocol-version-mismatch",
            "Unsupported native streamer protocol version."));
        return true;
    }

    app.ipc.writeResponse(protocol::makeReadyResponse(
        cmd.id, buildCapabilities()));
    return true;
}

static bool handleStart(StreamerApp& app, const protocol::CommandEnvelope& cmd) {
    if (!cmd.context.has_value()) {
        app.ipc.writeResponse(protocol::makeErrorResponse(
            cmd.id, "missing-field", "start command missing context"));
        return true;
    }

    const auto& ctx = *cmd.context;
    app.sessionCodec = ctx.settings.codec;

    spdlog::info("[Main] Starting stream: codec={}, fps={}, resolution={}",
                 ctx.settings.codec, ctx.settings.fps, ctx.settings.resolution);

    // Initialize video decoder
    spdlog::info("[Main] Initializing VideoDecoder...");
    app.video = std::make_unique<video::VideoDecoder>();
    {
        // Parse resolution e.g. "1920x1080"
        int w = 1920, h = 1080;
        auto& res = ctx.settings.resolution;
        auto pos = res.find('x');
        if (pos != std::string::npos) {
            try {
                w = std::stoi(res.substr(0, pos));
                h = std::stoi(res.substr(pos + 1));
            } catch (...) {}
        }

        HRESULT hr = app.video->initialize(ctx.settings.codec, w, h);
        if (FAILED(hr)) {
            spdlog::error("[Main] VideoDecoder initialize failed: HRESULT=0x{:08X}", static_cast<uint32_t>(hr));
            app.ipc.writeResponse(protocol::makeErrorResponse(
                cmd.id, "video-init-failed",
                "Failed to initialize video decoder"));
            return true;
        }
    }
    spdlog::info("[Main] VideoDecoder initialized successfully.");

    // Create peer connection
    spdlog::info("[Main] Initializing PeerConnection...");
    app.peer = std::make_unique<peer::PeerConnection>(
        app.writer,
        [&app](const uint8_t* data, size_t size, const std::string& /*codec*/) {
            app.video->feedRtp(data, size);
        });
    spdlog::info("[Main] Configuring PeerConnection...");
    app.peer->configure(ctx.session);
    spdlog::info("[Main] PeerConnection configured successfully.");

    // Create input capture
    spdlog::info("[Main] Initializing InputCapture...");
    app.inputCapture = std::make_unique<input::InputCapture>();
    spdlog::info("[Main] Starting InputCapture...");
    app.inputCapture->start(
        // Send callback — forward encoded bytes to data channel
        [&app](std::vector<uint8_t> payload, bool partiallyReliable) {
            if (app.peer) {
                app.peer->sendInput(payload, partiallyReliable);
            }
        },
        // Shortcut callback
        [&app](const std::string& action) {
            app.writer.push({
                {"type",   "shortcut"},
                {"action", action},
            });
        },
        // Capture changed callback
        [&app](bool captured) {
            app.writer.push(protocol::makeInputCaptureChangedEvent(captured));
        });
    spdlog::info("[Main] InputCapture started successfully.");

    // Update shortcuts from context
    spdlog::info("[Main] Updating shortcuts...");
    app.inputCapture->updateShortcuts(
        ctx.shortcuts.togglePointerLock,
        ctx.shortcuts.stopStream,
        ctx.shortcuts.toggleStats,
        ctx.shortcuts.toggleFullscreen);
    spdlog::info("[Main] Shortcuts updated.");

    app.streamStarted.store(true);
    app.startStatsTimer();

    // Send status
    app.writer.push(protocol::makeStatusEvent("starting",
        "Connecting to " + ctx.session.serverIp));

    app.ipc.writeResponse(protocol::makeOkResponse(cmd.id));
    spdlog::info("[Main] handleStart completed successfully.");
    return true;
}

static bool handleOffer(StreamerApp& app, const protocol::CommandEnvelope& cmd) {
    if (!cmd.sdp.has_value()) {
        app.ipc.writeResponse(protocol::makeErrorResponse(
            cmd.id, "missing-field", "offer missing sdp"));
        return true;
    }
    if (!app.peer) {
        app.ipc.writeResponse(protocol::makeErrorResponse(
            cmd.id, "not-started", "No active peer connection"));
        return true;
    }

    bool ok = app.peer->handleOffer(cmd.id, *cmd.sdp);
    if (!ok) {
        app.ipc.writeResponse(protocol::makeErrorResponse(
            cmd.id, "offer-failed", "Failed to handle SDP offer"));
        return true;
    }

    // GFN may negotiate a different codec than what was requested in the start command.
    // If so, we must re-initialize the VideoDecoder with the actual codec from the SDP.
    const std::string& sdpCodec = app.peer->negotiatedCodec();
    if (!sdpCodec.empty() && sdpCodec != app.sessionCodec) {
        spdlog::warn("[Main] SDP codec '{}' differs from requested codec '{}' — re-initializing VideoDecoder.",
                     sdpCodec, app.sessionCodec);
        int w = app.video ? app.video->width() : 1920;
        int h = app.video ? app.video->height() : 1080;
        if (w <= 0) w = 1920;
        if (h <= 0) h = 1080;
        auto newVideo = std::make_unique<video::VideoDecoder>();
        HRESULT hr = newVideo->initialize(sdpCodec, w, h);
        if (FAILED(hr)) {
            spdlog::error("[Main] VideoDecoder re-init for codec '{}' failed: HRESULT=0x{:08X}", sdpCodec, static_cast<uint32_t>(hr));
            // Fall through — the existing decoder (wrong codec) will be used; streaming will fail, but at least we don't crash.
        } else {
            app.video = std::move(newVideo);
            app.sessionCodec = sdpCodec;
            spdlog::info("[Main] VideoDecoder re-initialized for codec '{}'.", sdpCodec);
        }
    }

    // The answer response is sent asynchronously by PeerConnection::onLocalDescription
    return true;
}

static bool handleRemoteIce(StreamerApp& app, const protocol::CommandEnvelope& cmd) {
    if (!cmd.candidate.has_value() || !app.peer) {
        app.ipc.writeResponse(protocol::makeOkResponse(cmd.id));
        return true;
    }
    app.peer->addRemoteIce(*cmd.candidate);
    app.ipc.writeResponse(protocol::makeOkResponse(cmd.id));
    return true;
}

static bool handleInput(StreamerApp& app, const protocol::CommandEnvelope& cmd) {
    if (!cmd.input.has_value() || !app.peer) return true;
    auto bytes = cmd.input->payloadBytes();
    if (!bytes.empty()) {
        app.peer->sendInput(bytes, cmd.input->partiallyReliable);
    }
    return true;
}

static bool handleInputPaused(StreamerApp& app, const protocol::CommandEnvelope& cmd) {
    bool paused = cmd.paused.value_or(false);
    if (app.inputCapture) app.inputCapture->setPaused(paused);
    app.ipc.writeResponse(protocol::makeOkResponse(cmd.id));
    return true;
}

static bool handleSurface(StreamerApp& app, const protocol::CommandEnvelope& cmd) {
    if (!cmd.surface.has_value()) {
        app.ipc.writeResponse(protocol::makeOkResponse(cmd.id));
        return true;
    }

    const auto& surface = *cmd.surface;

    if (app.video) {
        app.video->updateSurface(surface);
    }

    // Lock cursor to the render rect when visible
    if (app.inputCapture && surface.visible && surface.rect.has_value()) {
        RECT rect{
            surface.rect->x,
            surface.rect->y,
            surface.rect->x + surface.rect->width,
            surface.rect->y + surface.rect->height
        };
        app.inputCapture->lockCursor(rect);
    } else if (app.inputCapture && !surface.visible) {
        app.inputCapture->releaseCursor();
    }

    app.ipc.writeResponse(protocol::makeOkResponse(cmd.id));
    return true;
}

static bool handleBitrate(StreamerApp& app, const protocol::CommandEnvelope& cmd) {
    if (cmd.maxBitrateKbps.has_value()) {
        app.targetBitrateKbps = *cmd.maxBitrateKbps;
        spdlog::info("[Main] Bitrate limit updated: {} kbps", app.targetBitrateKbps);
    }
    app.ipc.writeResponse(protocol::makeOkResponse(cmd.id));
    return true;
}

static bool handleUpdateShortcuts(StreamerApp& app, const protocol::CommandEnvelope& cmd) {
    if (cmd.shortcuts.has_value() && app.inputCapture) {
        const auto& sc = *cmd.shortcuts;
        app.inputCapture->updateShortcuts(
            sc.togglePointerLock, sc.stopStream, sc.toggleStats, sc.toggleFullscreen);
    }
    app.ipc.writeResponse(protocol::makeOkResponse(cmd.id));
    return true;
}

static bool handleStop(StreamerApp& app, const protocol::CommandEnvelope& cmd) {
    spdlog::info("[Main] Stop requested: reason={}",
                 cmd.reason.value_or("(none)"));

    app.stopStatsTimer();
    app.streamStarted.store(false);

    if (app.inputCapture) {
        app.inputCapture->releaseCursor();
        app.inputCapture->stop();
        app.inputCapture.reset();
    }
    if (app.peer) {
        app.peer->close();
        app.peer.reset();
    }
    if (app.video) {
        app.video->shutdown();
        app.video.reset();
    }

    app.ipc.writeResponse(protocol::makeOkResponse(cmd.id));
    return false; // Returning false shuts down the IPC loop → process exits
}

// ---------------------------------------------------------------------------
// Enable per-monitor DPI awareness (matches windows_dpi.rs behavior)
// ---------------------------------------------------------------------------
static void enableDpiAwareness() {
    // Try Per-Monitor V2 first (Windows 10 1703+)
    if (auto* fn = reinterpret_cast<decltype(&SetProcessDpiAwarenessContext)>(
            GetProcAddress(GetModuleHandleW(L"user32.dll"), "SetProcessDpiAwarenessContext"))) {
        fn(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        return;
    }
    // Fallback: shcore.dll SetProcessDpiAwareness
    if (auto* fn = reinterpret_cast<decltype(&SetProcessDpiAwareness)>(
            GetProcAddress(GetModuleHandleW(L"shcore.dll"), "SetProcessDpiAwareness"))) {
        fn(PROCESS_PER_MONITOR_DPI_AWARE);
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------
int main() {
    // DPI awareness must be set before any Win32 window creation
    enableDpiAwareness();

    // Configure spdlog to write to stderr (stdout is reserved for IPC)
    auto console = spdlog::stderr_color_mt("streamer");
    spdlog::set_default_logger(console);
    spdlog::set_pattern("[%H:%M:%S.%e] [%^%l%$] %v");
    spdlog::set_level(spdlog::level::info);

    spdlog::info("[Main] OpenNOW C++ Native Streamer starting (protocol v{})",
                 protocol::PROTOCOL_VERSION);

    StreamerApp app;

    // Register command handlers
    app.ipc.on("hello",           [&](const protocol::CommandEnvelope& c) { return handleHello(app, c); });
    app.ipc.on("start",           [&](const protocol::CommandEnvelope& c) { return handleStart(app, c); });
    app.ipc.on("offer",           [&](const protocol::CommandEnvelope& c) { return handleOffer(app, c); });
    app.ipc.on("remote-ice",      [&](const protocol::CommandEnvelope& c) { return handleRemoteIce(app, c); });
    app.ipc.on("input",           [&](const protocol::CommandEnvelope& c) { return handleInput(app, c); });
    app.ipc.on("input-paused",    [&](const protocol::CommandEnvelope& c) { return handleInputPaused(app, c); });
    app.ipc.on("surface",         [&](const protocol::CommandEnvelope& c) { return handleSurface(app, c); });
    app.ipc.on("bitrate",         [&](const protocol::CommandEnvelope& c) { return handleBitrate(app, c); });
    app.ipc.on("update-shortcuts",[&](const protocol::CommandEnvelope& c) { return handleUpdateShortcuts(app, c); });
    app.ipc.on("stop",            [&](const protocol::CommandEnvelope& c) { return handleStop(app, c); });

    // Block here until EOF or "stop" command
    app.ipc.run();

    // Cleanup in case stop wasn't received (e.g. Electron killed the process)
    app.stopStatsTimer();
    if (app.inputCapture) {
        app.inputCapture->releaseCursor();
        app.inputCapture->stop();
    }
    if (app.peer)  app.peer->close();
    if (app.video) app.video->shutdown();

    spdlog::info("[Main] Streamer exited cleanly");
    return 0;
}
