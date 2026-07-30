#pragma once
// =============================================================================
// video_decoder.hpp — Hardware video decode + Direct3D 11 render
//
// Decodes raw RTP packets (H.264 / H.265 / AV1) using Windows Media Foundation
// MFT (Media Foundation Transform) with D3D11-aware hardware acceleration.
//
// Rendering pipeline:
//   RTP bytes → RTP depayloader → bitstream → IMFTransform → ID3D11Texture2D
//               → IDXGISwapChain::Present → child HWND
//
// Zero-copy: the decoded texture lives entirely on the GPU.
// NVDEC / Intel Quick Sync / AMD VCE are picked automatically by WMF.
// AV1 is supported on Windows 11 or Windows 10 with AV1 Video Extension.
// =============================================================================

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <d3d11.h>
#include <dxgi1_2.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mftransform.h>
#include <mferror.h>
#include <wrl/client.h>  // ComPtr

#include <atomic>
#include <memory>
#include <mutex>
#include <string>
#include <vector>
#include <functional>
#include <cstdint>
#include <thread>
#include <future>

#include "protocol.hpp"

using Microsoft::WRL::ComPtr;

namespace video {

// Codec selection enum
enum class Codec { H264, H265, AV1, Unknown };
Codec codecFromString(const std::string& s);

// ---------------------------------------------------------------------------
// RtpDepayloader
//
// Stateful RTP depayloader that accepts raw RTP packets and outputs
// complete NAL/OBU access units suitable for the WMF decoder.
// ---------------------------------------------------------------------------
class RtpDepayloader {
public:
    explicit RtpDepayloader(Codec codec);

    // Feed one raw RTP packet (including 12-byte header).
    // Returns a complete access unit if assembly is done, else empty vector.
    std::vector<uint8_t> feedPacket(const uint8_t* data, size_t size);

private:
    std::vector<uint8_t> depayloadH264(const uint8_t* rtp, size_t size);
    std::vector<uint8_t> depayloadH265(const uint8_t* rtp, size_t size);
    std::vector<uint8_t> depayloadAV1(const uint8_t* rtp, size_t size);

    Codec codec_;
    uint16_t lastSeq_{0};
    std::vector<uint8_t> fuBuffer_;   // FU-A / FU fragment buffer
    uint16_t fuStartSeq_{0};
    bool fuActive_{false};
};

// ---------------------------------------------------------------------------
// D3D11Renderer
//
// Owns the child HWND, D3D11 device, and swap chain.
// Receives decoded D3D11 textures and presents them.
// ---------------------------------------------------------------------------
class D3D11Renderer {
public:
    D3D11Renderer();
    ~D3D11Renderer();

    // Create device + child window (hidden until parent is assigned)
    HRESULT initialize();

    // Resize swap chain to match new render rect
    HRESULT resize(int width, int height);

    // Reparent under Electron's HWND and reposition
    void setParent(HWND parentHwnd, int x, int y, int width, int height);

    // Show / hide the render window
    void setVisible(bool visible);

    // Present a decoded frame texture to the screen (zero-copy blit)
    HRESULT presentTexture(ID3D11Texture2D* texture, int arrayIndex = 0);

    // Present a raw YUV/RGB buffer (software fallback)
    HRESULT presentBuffer(const uint8_t* data, size_t size, int width, int height);

    HWND hwnd() const { return hwnd_; }
    int width()  const { return width_; }
    int height() const { return height_; }
    ID3D11Device* device() const { return device_.Get(); }
    ID3D11DeviceContext* context() const { return context_.Get(); }
    IDXGISwapChain1* swapChain() const { return swapChain_.Get(); }

    // Stats
    uint64_t framesRendered() const { return framesRendered_.load(); }
    uint64_t framesDropped()  const { return framesDropped_.load(); }

private:
    HRESULT createDevice();
    HRESULT createSwapChain(int width, int height);
    HRESULT createShaders();
    static LRESULT CALLBACK WndProc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp);

    HWND hwnd_{nullptr};
    int width_{1920};
    int height_{1080};

    ComPtr<ID3D11Device>         device_;
    ComPtr<ID3D11DeviceContext>  context_;
    ComPtr<IDXGISwapChain1>      swapChain_;

    // Shaders for NV12/YUV → RGBA conversion (for software path)
    ComPtr<ID3D11VertexShader>   vsShader_;
    ComPtr<ID3D11PixelShader>    psShader_;
    ComPtr<ID3D11Buffer>         vertexBuffer_;
    ComPtr<ID3D11SamplerState>   sampler_;

    std::atomic<uint64_t> framesRendered_{0};
    std::atomic<uint64_t> framesDropped_{0};
    std::mutex presentMutex_;

    // UI Window thread management
    std::thread windowThread_;
    std::atomic<bool> windowRunning_{false};
    void runWindowThread(std::promise<HWND>& hwndPromise);
};

// ---------------------------------------------------------------------------
// MftDecoder
//
// Windows Media Foundation Transform decoder for H.264 / H.265 / AV1.
// Operates on the GPU via D3D11DeviceManager for zero-copy decode.
// ---------------------------------------------------------------------------
class MftDecoder {
public:
    MftDecoder();
    ~MftDecoder();

    // Initialize with codec, resolution and the D3D11 device from the renderer
    HRESULT initialize(Codec codec, int width, int height, ID3D11Device* device, ID3D11DeviceContext* context);

    // Feed a complete access unit (NAL / OBU bitstream)
    // Returns S_OK if a decoded frame was emitted via the frameCallback
    using FrameCallback = std::function<void(ID3D11Texture2D*, int arrayIndex, int width, int height)>;
    HRESULT feedAccessUnit(const uint8_t* data, size_t size, FrameCallback cb);

    // Flush pending frames
    HRESULT flush(FrameCallback cb);

    // Current resolution after decode
    int width()  const { return width_;  }
    int height() const { return height_; }
    std::string hwAccelName() const { return hwAccelName_; }

    // Stats
    uint64_t framesDecoded() const { return framesDecoded_.load(); }

private:
    HRESULT findDecoder(const GUID& inputSubtype, IMFTransform** ppTransform);
    HRESULT configureD3D11(ID3D11Device* device);
    HRESULT drainOutput(FrameCallback cb);

    Codec codec_{Codec::Unknown};
    ComPtr<IMFTransform>  decoder_;
    ComPtr<IMFDXGIDeviceManager> dxgiManager_;
    UINT dxgiManagerToken_{0};

    int width_{0};
    int height_{0};
    std::string hwAccelName_;
    std::atomic<uint64_t> framesDecoded_{0};
};

// ---------------------------------------------------------------------------
// VideoDecoder — top-level class combining depayloader + MFT + renderer
// ---------------------------------------------------------------------------
class VideoDecoder {
public:
    VideoDecoder();
    ~VideoDecoder();

    // Initialize everything. Call once before feedRtp.
    HRESULT initialize(const std::string& codec, int initialWidth, int initialHeight);

    // Called from the peer connection RTP callback (may be on any thread)
    void feedRtp(const uint8_t* data, size_t size);

    // Update render surface (called from "surface" IPC command)
    void updateSurface(const protocol::NativeRenderSurface& surface);

    // Show/hide
    void setVisible(bool visible);

    // Stop decoding + rendering
    void shutdown();

    // Stats accessors
    uint64_t framesDecoded()  const;
    uint64_t framesRendered() const;
    double   renderFps()      const;
    std::string hwAccelName() const;
    int width()  const;
    int height() const;

private:
    std::unique_ptr<RtpDepayloader> depayloader_;
    std::unique_ptr<MftDecoder>     mftDecoder_;
    std::unique_ptr<D3D11Renderer>  renderer_;

    Codec codec_{Codec::Unknown};
    std::mutex decodeMutex_;
    std::atomic<bool> initialized_{false};
    std::atomic<bool> shutdown_{false};

    // FPS tracking
    mutable std::mutex fpsMutex_;
    std::vector<LONGLONG> renderTimes_;  // last-N present timestamps (QPC)
};

} // namespace video
