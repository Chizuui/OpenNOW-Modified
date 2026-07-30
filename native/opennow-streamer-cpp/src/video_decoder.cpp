#include "video_decoder.hpp"
#include <spdlog/spdlog.h>
#include <codecvt>
#include <locale>
#include <stdexcept>
#include <thread>
#include <d3dcompiler.h>
#include <mfplay.h>
#include <Mferror.h>
#include <ks.h>
#include <ksmedia.h>

// AV1 GUID (Windows 11 / AV1 Video Extension)
// {694DD1D3-3CAB-4E99-8AD5-FF34AC08EE5D}
static const GUID OPENNOW_MFVideoFormat_AV1 = {
    0x694DD1D3, 0x3CAB, 0x4E99,
    { 0x8A, 0xD5, 0xFF, 0x34, 0xAC, 0x08, 0xEE, 0x5D }
};

// Class name for our private child window
static const char* kWindowClass = "OpenNOW_VideoRenderer";

namespace video {

// ---------------------------------------------------------------------------
// Codec utils
// ---------------------------------------------------------------------------
Codec codecFromString(const std::string& s) {
    if (s == "AV1")  return Codec::AV1;
    if (s == "H265" || s == "HEVC") return Codec::H265;
    if (s == "H264" || s == "AVC")  return Codec::H264;
    return Codec::Unknown;
}

static GUID codecToInputSubtype(Codec c) {
    switch (c) {
    case Codec::H264: return MFVideoFormat_H264;
    case Codec::H265: return MFVideoFormat_HEVC;
    case Codec::AV1:  return OPENNOW_MFVideoFormat_AV1;
    default:          return MFVideoFormat_H264;
    }
}

// ---------------------------------------------------------------------------
// Helper: HRESULT check
// ---------------------------------------------------------------------------
#define HR_CHECK(hr, msg) \
    if (FAILED(hr)) { \
        spdlog::error("[Video] {} failed: HRESULT=0x{:08X}", msg, static_cast<uint32_t>(hr)); \
        return hr; \
    }

// ---------------------------------------------------------------------------
// RtpDepayloader
// ---------------------------------------------------------------------------
RtpDepayloader::RtpDepayloader(Codec codec) : codec_(codec) {}

std::vector<uint8_t> RtpDepayloader::feedPacket(const uint8_t* data, size_t size) {
    if (size < 12) return {}; // Too small for RTP header
    switch (codec_) {
    case Codec::H264: return depayloadH264(data, size);
    case Codec::H265: return depayloadH265(data, size);
    case Codec::AV1:  return depayloadAV1(data, size);
    default:          return depayloadH264(data, size);
    }
}

// H.264 RTP depayloader (RFC 6184)
std::vector<uint8_t> RtpDepayloader::depayloadH264(const uint8_t* rtp, size_t size) {
    // Skip 12-byte fixed RTP header (and any CSRC/extension)
    uint8_t cc     = rtp[0] & 0x0F;
    bool    hasExt = (rtp[0] >> 4) & 1;
    size_t  offset = 12 + cc * 4;

    if (hasExt && offset + 4 <= size) {
        uint16_t extLen = (rtp[offset + 2] << 8) | rtp[offset + 3];
        offset += 4 + extLen * 4;
    }
    if (offset >= size) return {};

    const uint8_t* payload = rtp + offset;
    size_t         payloadSize = size - offset;

    uint8_t nalType = payload[0] & 0x1F;

    static const uint8_t kStartCode[] = { 0x00, 0x00, 0x00, 0x01 };

    if (nalType >= 1 && nalType <= 23) {
        // Single NAL unit
        std::vector<uint8_t> out(4 + payloadSize);
        memcpy(out.data(), kStartCode, 4);
        memcpy(out.data() + 4, payload, payloadSize);
        return out;
    }
    else if (nalType == 24) {
        // STAP-A: multiple NALs in one packet
        std::vector<uint8_t> out;
        size_t i = 1;
        while (i + 2 <= payloadSize) {
            uint16_t nalSize = (payload[i] << 8) | payload[i + 1];
            i += 2;
            if (i + nalSize > payloadSize) break;
            out.insert(out.end(), kStartCode, kStartCode + 4);
            out.insert(out.end(), payload + i, payload + i + nalSize);
            i += nalSize;
        }
        return out;
    }
    else if (nalType == 28) {
        // FU-A: fragmented NAL unit
        if (payloadSize < 2) return {};
        uint8_t fuHeader = payload[1];
        bool    startBit = (fuHeader >> 7) & 1;
        bool    endBit   = (fuHeader >> 6) & 1;
        uint8_t originalNalType = fuHeader & 0x1F;

        if (startBit) {
            fuBuffer_.clear();
            fuActive_ = true;
            // Reconstruct NAL header
            uint8_t nalHdr = (payload[0] & 0xE0) | originalNalType;
            fuBuffer_.push_back(nalHdr);
        }

        if (!fuActive_) return {};
        fuBuffer_.insert(fuBuffer_.end(), payload + 2, payload + payloadSize);

        if (endBit) {
            fuActive_ = false;
            std::vector<uint8_t> out(4 + fuBuffer_.size());
            memcpy(out.data(), kStartCode, 4);
            memcpy(out.data() + 4, fuBuffer_.data(), fuBuffer_.size());
            fuBuffer_.clear();
            return out;
        }
    }
    return {};
}

// H.265 RTP depayloader (RFC 7798)
std::vector<uint8_t> RtpDepayloader::depayloadH265(const uint8_t* rtp, size_t size) {
    uint8_t cc     = rtp[0] & 0x0F;
    bool    hasExt = (rtp[0] >> 4) & 1;
    size_t  offset = 12 + cc * 4;

    if (hasExt && offset + 4 <= size) {
        uint16_t extLen = (rtp[offset + 2] << 8) | rtp[offset + 3];
        offset += 4 + extLen * 4;
    }
    if (offset + 2 >= size) return {};

    const uint8_t* payload = rtp + offset;
    size_t         payloadSize = size - offset;

    static const uint8_t kStartCode[] = { 0x00, 0x00, 0x00, 0x01 };

    // H.265 NAL unit type is in bits 1-6 of the first byte
    uint8_t nalType = (payload[0] >> 1) & 0x3F;

    if (nalType == 48) {
        // AP (Aggregation Packet) — multiple NALs
        std::vector<uint8_t> out;
        size_t i = 2; // skip 2-byte PayloadHdr
        while (i + 2 <= payloadSize) {
            uint16_t nalSize = (payload[i] << 8) | payload[i + 1];
            i += 2;
            if (i + nalSize > payloadSize) break;
            out.insert(out.end(), kStartCode, kStartCode + 4);
            out.insert(out.end(), payload + i, payload + i + nalSize);
            i += nalSize;
        }
        return out;
    }
    else if (nalType == 49) {
        // FU (Fragmentation Unit)
        if (payloadSize < 3) return {};
        uint8_t fuHeader = payload[2];
        bool startBit    = (fuHeader >> 7) & 1;
        bool endBit      = (fuHeader >> 6) & 1;
        uint8_t origType = fuHeader & 0x3F;

        if (startBit) {
            fuBuffer_.clear();
            fuActive_ = true;
            // Reconstruct NAL header (2 bytes for H.265)
            uint8_t hdr0 = (origType << 1) | (payload[0] & 0x01);
            uint8_t hdr1 = payload[1];
            fuBuffer_.push_back(hdr0);
            fuBuffer_.push_back(hdr1);
        }

        if (!fuActive_) return {};
        fuBuffer_.insert(fuBuffer_.end(), payload + 3, payload + payloadSize);

        if (endBit) {
            fuActive_ = false;
            std::vector<uint8_t> out(4 + fuBuffer_.size());
            memcpy(out.data(), kStartCode, 4);
            memcpy(out.data() + 4, fuBuffer_.data(), fuBuffer_.size());
            fuBuffer_.clear();
            return out;
        }
    }
    else if (nalType <= 47) {
        // Single NAL
        std::vector<uint8_t> out(4 + payloadSize);
        memcpy(out.data(), kStartCode, 4);
        memcpy(out.data() + 4, payload, payloadSize);
        return out;
    }
    return {};
}

// AV1 RTP depayloader (RFC 9350) — simplified
std::vector<uint8_t> RtpDepayloader::depayloadAV1(const uint8_t* rtp, size_t size) {
    uint8_t cc     = rtp[0] & 0x0F;
    bool    hasExt = (rtp[0] >> 4) & 1;
    size_t  offset = 12 + cc * 4;

    if (hasExt && offset + 4 <= size) {
        uint16_t extLen = (rtp[offset + 2] << 8) | rtp[offset + 3];
        offset += 4 + extLen * 4;
    }
    if (offset >= size) return {};

    const uint8_t* payload = rtp + offset;
    size_t         payloadSize = size - offset;

    // AV1 RTP: first byte is aggregation header
    // Z=bit7 (continuation), Y=bit6 (more fragments), W=bits5-4 (count), N=bit3 (new sequence)
    if (payloadSize < 1) return {};
    uint8_t aggrHdr = payload[0];
    bool Z = (aggrHdr >> 7) & 1;
    bool Y = (aggrHdr >> 6) & 1;

    const uint8_t* obus = payload + 1;
    size_t         obusSize = payloadSize - 1;

    if (!Z && !Y) {
        // Simple: entire payload is one or more complete OBUs
        return std::vector<uint8_t>(obus, obus + obusSize);
    }

    // Fragmented OBU — accumulate in buffer
    if (!Z) {
        fuBuffer_.clear();
        fuActive_ = true;
    }
    if (fuActive_) {
        fuBuffer_.insert(fuBuffer_.end(), obus, obus + obusSize);
        if (!Y) {
            fuActive_ = false;
            return fuBuffer_;
        }
    }
    return {};
}

// ---------------------------------------------------------------------------
// D3D11Renderer
// ---------------------------------------------------------------------------
D3D11Renderer::D3D11Renderer() {
    LARGE_INTEGER freq;
    QueryPerformanceFrequency(&freq);
}

D3D11Renderer::~D3D11Renderer() {
    windowRunning_ = false;
    if (hwnd_) {
        // Send quit message to the window thread message loop
        PostMessageA(hwnd_, WM_QUIT, 0, 0);
    }
    if (windowThread_.joinable()) {
        windowThread_.join();
    }
}

HRESULT D3D11Renderer::initialize() {
    std::promise<HWND> hwndPromise;
    std::future<HWND> hwndFuture = hwndPromise.get_future();

    windowRunning_ = true;
    windowThread_ = std::thread(&D3D11Renderer::runWindowThread, this, std::ref(hwndPromise));

    // Wait until the window is created on the UI thread
    hwnd_ = hwndFuture.get();
    if (!hwnd_) {
        spdlog::error("[Renderer] Window thread failed to initialize window.");
        return E_FAIL;
    }

    HRESULT hr = createDevice();
    HR_CHECK(hr, "D3D11 createDevice");

    hr = createSwapChain(width_, height_);
    HR_CHECK(hr, "D3D11 createSwapChain");

    spdlog::info("[Renderer] D3D11 renderer initialized ({}x{}) with dedicated UI thread", width_, height_);
    return S_OK;
}

void D3D11Renderer::runWindowThread(std::promise<HWND>& hwndPromise) {
    // Register window class (only once)
    WNDCLASSEXA wc = {};
    wc.cbSize        = sizeof(wc);
    wc.style         = CS_OWNDC;
    wc.lpfnWndProc   = WndProc;
    wc.hInstance     = GetModuleHandleA(nullptr);
    wc.lpszClassName = kWindowClass;
    wc.hCursor       = LoadCursor(nullptr, IDC_ARROW);
    wc.hbrBackground = reinterpret_cast<HBRUSH>(GetStockObject(BLACK_BRUSH));
    RegisterClassExA(&wc); // Ignore "already registered" error

    // Create the child render window
    HWND localHwnd = CreateWindowExA(
        0,
        kWindowClass,
        "OpenNOW Video",
        WS_POPUP | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
        0, 0, width_, height_,
        nullptr,  // Parent set later via setParent()
        nullptr,
        GetModuleHandleA(nullptr),
        this);

    if (!localHwnd) {
        hwndPromise.set_value(nullptr);
        return;
    }

    // Set instance pointer for WndProc
    SetWindowLongPtrA(localHwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(this));

    // Notify calling thread that window is ready
    hwndPromise.set_value(localHwnd);

    // Message loop
    MSG msg;
    while (windowRunning_ && GetMessageA(&msg, nullptr, 0, 0) > 0) {
        TranslateMessage(&msg);
        DispatchMessageA(&msg);
    }

    // Cleanup window on exiting thread
    DestroyWindow(localHwnd);
}

HRESULT D3D11Renderer::createDevice() {
    UINT flags = D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT;
#ifndef NDEBUG
    flags |= D3D11_CREATE_DEVICE_DEBUG;
#endif

    D3D_FEATURE_LEVEL featureLevels[] = {
        D3D_FEATURE_LEVEL_11_1,
        D3D_FEATURE_LEVEL_11_0,
        D3D_FEATURE_LEVEL_10_1,
    };

    ComPtr<ID3D11Device>        dev;
    ComPtr<ID3D11DeviceContext> ctx;
    D3D_FEATURE_LEVEL           achieved;

    HRESULT hr = D3D11CreateDevice(
        nullptr,                    // Default adapter
        D3D_DRIVER_TYPE_HARDWARE,
        nullptr,
        flags,
        featureLevels, ARRAYSIZE(featureLevels),
        D3D11_SDK_VERSION,
        &dev, &achieved, &ctx);

    if (FAILED(hr)) {
        spdlog::warn("[Renderer] Hardware D3D11 failed, trying WARP: 0x{:08X}", static_cast<uint32_t>(hr));
        hr = D3D11CreateDevice(
            nullptr,
            D3D_DRIVER_TYPE_WARP,
            nullptr,
            flags,
            featureLevels, ARRAYSIZE(featureLevels),
            D3D11_SDK_VERSION,
            &dev, &achieved, &ctx);
    }
    HR_CHECK(hr, "D3D11CreateDevice");

    device_  = dev;
    context_ = ctx;

    spdlog::info("[Renderer] D3D11 feature level: 0x{:04X}", static_cast<uint32_t>(achieved));
    return S_OK;
}

HRESULT D3D11Renderer::createSwapChain(int width, int height) {
    ComPtr<IDXGIDevice2> dxgiDev;
    HRESULT hr = device_.As(&dxgiDev);
    HR_CHECK(hr, "QueryInterface IDXGIDevice2");

    ComPtr<IDXGIAdapter> adapter;
    hr = dxgiDev->GetAdapter(&adapter);
    HR_CHECK(hr, "GetAdapter");

    ComPtr<IDXGIFactory2> factory;
    hr = adapter->GetParent(IID_PPV_ARGS(&factory));
    HR_CHECK(hr, "GetParent IDXGIFactory2");

    DXGI_SWAP_CHAIN_DESC1 desc = {};
    desc.Width       = static_cast<UINT>(width);
    desc.Height      = static_cast<UINT>(height);
    desc.Format      = DXGI_FORMAT_B8G8R8A8_UNORM;
    desc.SampleDesc  = { 1, 0 };
    desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    desc.BufferCount = 2;
    desc.SwapEffect  = DXGI_SWAP_EFFECT_FLIP_DISCARD;
    desc.Flags       = DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH;

    hr = factory->CreateSwapChainForHwnd(
        device_.Get(), hwnd_,
        &desc, nullptr, nullptr,
        &swapChain_);
    HR_CHECK(hr, "CreateSwapChainForHwnd");

    width_  = width;
    height_ = height;
    return S_OK;
}

HRESULT D3D11Renderer::resize(int width, int height) {
    if (width == width_ && height == height_) return S_OK;
    std::lock_guard<std::mutex> lock(presentMutex_);

    // Release render target before resize
    context_->ClearState();
    context_->Flush();

    HRESULT hr = swapChain_->ResizeBuffers(
        2,
        static_cast<UINT>(width),
        static_cast<UINT>(height),
        DXGI_FORMAT_B8G8R8A8_UNORM,
        0);

    if (SUCCEEDED(hr)) {
        width_  = width;
        height_ = height;
        SetWindowPos(hwnd_, nullptr, 0, 0, width, height,
                     SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
        spdlog::info("[Renderer] Resized to {}x{}", width, height);
    }
    return hr;
}

void D3D11Renderer::setParent(HWND parentHwnd, int x, int y, int width, int height) {
    if (!hwnd_) return;

    if (width != width_ || height != height_) {
        resize(width, height);
    }

    // Run reparenting asynchronously to prevent cross-process Win32 deadlock.
    // SetParent sends synchronous messages (like WM_PARENTNOTIFY) to the parent window,
    // which can deadlock if the parent process (Electron main thread) is currently
    // blocking or waiting on IPC/stdin write from this process.
    std::thread([this, parentHwnd, x, y, width, height]() {
        // Change style from WS_POPUP to WS_CHILD so it embeds properly
        LONG_PTR style = GetWindowLongPtrW(hwnd_, GWL_STYLE);
        style &= ~WS_POPUP;
        style |= WS_CHILD;
        SetWindowLongPtrW(hwnd_, GWL_STYLE, style);

        // Reparent under Electron's HWND
        SetParent(hwnd_, parentHwnd);
        SetWindowPos(hwnd_, HWND_TOP, x, y, width, height,
                     SWP_SHOWWINDOW | SWP_NOACTIVATE | SWP_FRAMECHANGED);

        spdlog::info("[Renderer] Reparented under HWND=0x{:X} at ({},{}) {}x{} (async)",
                     reinterpret_cast<uintptr_t>(parentHwnd), x, y, width, height);
    }).detach();
}

void D3D11Renderer::setVisible(bool visible) {
    if (hwnd_) {
        ShowWindow(hwnd_, visible ? SW_SHOWNOACTIVATE : SW_HIDE);
    }
}

HRESULT D3D11Renderer::presentTexture(ID3D11Texture2D* texture, int arrayIndex) {
    std::lock_guard<std::mutex> lock(presentMutex_);
    if (!swapChain_ || !texture) return E_INVALIDARG;

    // Get the back buffer
    ComPtr<ID3D11Texture2D> backBuffer;
    HRESULT hr = swapChain_->GetBuffer(0, IID_PPV_ARGS(&backBuffer));
    if (FAILED(hr)) {
        framesDropped_++;
        return hr;
    }

    // Copy decoded texture subresource to back buffer
    D3D11_BOX srcBox = {};
    D3D11_TEXTURE2D_DESC texDesc;
    texture->GetDesc(&texDesc);
    srcBox.right  = texDesc.Width;
    srcBox.bottom = texDesc.Height;
    srcBox.back   = 1;

    context_->CopySubresourceRegion(
        backBuffer.Get(), 0,
        0, 0, 0,
        texture, static_cast<UINT>(arrayIndex),
        &srcBox);

    DXGI_PRESENT_PARAMETERS params = {};
    hr = swapChain_->Present1(0, 0, &params);

    if (SUCCEEDED(hr)) {
        framesRendered_++;
    } else {
        framesDropped_++;
        spdlog::warn("[Renderer] Present1 failed: 0x{:08X}", static_cast<uint32_t>(hr));
    }
    return hr;
}

HRESULT D3D11Renderer::presentBuffer(const uint8_t*, size_t, int, int) {
    // Software path — not implemented in this version (always use hardware decode)
    spdlog::warn("[Renderer] Software presentBuffer not implemented");
    return E_NOTIMPL;
}

LRESULT CALLBACK D3D11Renderer::WndProc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
    switch (msg) {
    case WM_ERASEBKGND:
        return 1; // Don't erase — avoids flicker
    case WM_SIZE:
        // Let the VideoDecoder handle resize via updateSurface
        return 0;
    case WM_DESTROY:
        return 0;
    }
    return DefWindowProcA(hwnd, msg, wp, lp);
}

// ---------------------------------------------------------------------------
// MftDecoder
// ---------------------------------------------------------------------------
MftDecoder::MftDecoder() {}

MftDecoder::~MftDecoder() {
    if (decoder_) {
        decoder_->ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
        decoder_.Reset();
    }
    if (dxgiManager_) {
        dxgiManager_->ResetDevice(nullptr, dxgiManagerToken_);
        dxgiManager_.Reset();
    }
}

HRESULT MftDecoder::initialize(Codec codec, int width, int height, ID3D11Device* device, ID3D11DeviceContext*) {
    codec_ = codec;
    width_ = width;
    height_ = height;

    HRESULT hr = MFStartup(MF_VERSION);
    HR_CHECK(hr, "MFStartup");

    GUID inputSubtype = codecToInputSubtype(codec);

    // Find decoder via MFT enumeration (hardware first, then software)
    hr = findDecoder(inputSubtype, &decoder_);
    if (FAILED(hr) || !decoder_) {
        spdlog::error("[Decoder] Failed to find any MFT decoder for codec (HRESULT=0x{:08X})", static_cast<uint32_t>(hr));
        return FAILED(hr) ? hr : E_FAIL;
    }

    // Configure D3D11 hardware acceleration
    hr = configureD3D11(device);
    if (FAILED(hr)) {
        spdlog::warn("[Decoder] D3D11 configuration failed, continuing without HW accel: 0x{:08X}",
                     static_cast<uint32_t>(hr));
        hwAccelName_ = "Software";
    }

    // Set media types
    ComPtr<IMFMediaType> inputType;
    hr = MFCreateMediaType(&inputType);
    HR_CHECK(hr, "MFCreateMediaType (input)");

    inputType->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
    inputType->SetGUID(MF_MT_SUBTYPE,    inputSubtype);
    MFSetAttributeSize(inputType.Get(), MF_MT_FRAME_SIZE, width, height);

    hr = decoder_->SetInputType(0, inputType.Get(), 0);
    if (FAILED(hr)) {
        spdlog::error("[Decoder] SetInputType failed: 0x{:08X}", static_cast<uint32_t>(hr));
        return hr;
    }

    // Try NV12 output first (native GPU format), then IYUV
    ComPtr<IMFMediaType> outputType;
    MFCreateMediaType(&outputType);
    outputType->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
    outputType->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_NV12);
    MFSetAttributeSize(outputType.Get(), MF_MT_FRAME_SIZE, width, height);

    hr = decoder_->SetOutputType(0, outputType.Get(), 0);
    if (FAILED(hr)) {
        outputType->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_IYUV);
        hr = decoder_->SetOutputType(0, outputType.Get(), 0);
        HR_CHECK(hr, "SetOutputType (IYUV)");
    }

    hr = decoder_->ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0);
    HR_CHECK(hr, "MFT_MESSAGE_NOTIFY_BEGIN_STREAMING");

    spdlog::info("[Decoder] MFT decoder initialized: codec={}, hwAccel={}",
                 codec == Codec::H264 ? "H264" : codec == Codec::H265 ? "H265" : "AV1",
                 hwAccelName_.empty() ? "D3D11" : hwAccelName_);
    return S_OK;
}

HRESULT MftDecoder::findDecoder(const GUID& inputSubtype, IMFTransform** ppTransform) {
    MFT_REGISTER_TYPE_INFO inputInfo = { MFMediaType_Video, inputSubtype };
    MFT_REGISTER_TYPE_INFO outputInfo = { MFMediaType_Video, MFVideoFormat_NV12 };

    UINT32 count = 0;
    IMFActivate** activates = nullptr;

    // Try hardware first
    HRESULT hr = MFTEnumEx(
        MFT_CATEGORY_VIDEO_DECODER,
        MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
        &inputInfo, nullptr,
        &activates, &count);

    if (SUCCEEDED(hr) && count > 0) {
        hr = activates[0]->ActivateObject(IID_PPV_ARGS(ppTransform));
        spdlog::info("[Decoder] Using hardware MFT decoder (count={})", count);
    }

    for (UINT32 i = 0; i < count; i++) activates[i]->Release();
    if (activates) CoTaskMemFree(activates);

    if (SUCCEEDED(hr) && *ppTransform) {
        hwAccelName_ = "D3D11/DXVA2";
        return S_OK;
    }

    // Fallback: software decoder
    count = 0; activates = nullptr;
    hr = MFTEnumEx(
        MFT_CATEGORY_VIDEO_DECODER,
        MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER,
        &inputInfo, nullptr,
        &activates, &count);

    if (SUCCEEDED(hr) && count > 0) {
        hr = activates[0]->ActivateObject(IID_PPV_ARGS(ppTransform));
        spdlog::info("[Decoder] Using software MFT decoder (count={})", count);
        hwAccelName_ = "Software";
    }

    for (UINT32 i = 0; i < count; i++) activates[i]->Release();
    if (activates) CoTaskMemFree(activates);

    return hr;
}

HRESULT MftDecoder::configureD3D11(ID3D11Device* device) {
    // Create DXGI Device Manager for hardware-accelerated decode
    HRESULT hr = MFCreateDXGIDeviceManager(&dxgiManagerToken_, &dxgiManager_);
    HR_CHECK(hr, "MFCreateDXGIDeviceManager");

    hr = dxgiManager_->ResetDevice(device, dxgiManagerToken_);
    HR_CHECK(hr, "ResetDevice");

    // Attach to decoder
    ComPtr<IMFAttributes> attribs;
    hr = decoder_->GetAttributes(&attribs);
    if (FAILED(hr)) return hr;

    hr = attribs->SetUINT32(MF_SA_D3D11_AWARE, TRUE);
    if (FAILED(hr)) return hr;

    hr = decoder_->ProcessMessage(
        MFT_MESSAGE_SET_D3D_MANAGER,
        reinterpret_cast<ULONG_PTR>(dxgiManager_.Get()));
    return hr;
}

HRESULT MftDecoder::feedAccessUnit(const uint8_t* data, size_t size, FrameCallback cb) {
    if (!decoder_ || !data || size == 0) return E_INVALIDARG;

    // Create input sample
    ComPtr<IMFSample>       sample;
    ComPtr<IMFMediaBuffer>  buffer;

    HRESULT hr = MFCreateMemoryBuffer(static_cast<DWORD>(size), &buffer);
    HR_CHECK(hr, "MFCreateMemoryBuffer");

    BYTE* ptr = nullptr;
    buffer->Lock(&ptr, nullptr, nullptr);
    memcpy(ptr, data, size);
    buffer->Unlock();
    buffer->SetCurrentLength(static_cast<DWORD>(size));

    hr = MFCreateSample(&sample);
    HR_CHECK(hr, "MFCreateSample");
    sample->AddBuffer(buffer.Get());

    hr = decoder_->ProcessInput(0, sample.Get(), 0);
    if (hr == MF_E_NOTACCEPTING) {
        // Drain and retry
        drainOutput(cb);
        hr = decoder_->ProcessInput(0, sample.Get(), 0);
    }
    if (FAILED(hr)) {
        spdlog::warn("[Decoder] ProcessInput failed: 0x{:08X}", static_cast<uint32_t>(hr));
        return hr;
    }

    return drainOutput(cb);
}

HRESULT MftDecoder::drainOutput(FrameCallback cb) {
    if (!decoder_) return E_FAIL;

    MFT_OUTPUT_DATA_BUFFER outputBuffer = {};
    DWORD status = 0;

    while (true) {
        HRESULT hr = decoder_->ProcessOutput(0, 1, &outputBuffer, &status);

        if (hr == MF_E_TRANSFORM_NEED_MORE_INPUT) break;
        if (hr == MF_E_TRANSFORM_STREAM_CHANGE) {
            // Resolution or format change — re-read output type
            ComPtr<IMFMediaType> outType;
            decoder_->GetOutputCurrentType(0, &outType);
            if (outType) {
                MFGetAttributeSize(outType.Get(), MF_MT_FRAME_SIZE,
                    reinterpret_cast<UINT32*>(&width_),
                    reinterpret_cast<UINT32*>(&height_));
                spdlog::info("[Decoder] Stream change: {}x{}", width_, height_);
            }
            continue;
        }
        if (FAILED(hr)) break;

        // Got a decoded frame
        if (outputBuffer.pSample) {
            ComPtr<IMFMediaBuffer> outBuffer;
            outputBuffer.pSample->GetBufferByIndex(0, &outBuffer);

            // Try to get D3D11 texture (zero-copy path)
            ComPtr<IMFDXGIBuffer> dxgiBuffer;
            if (SUCCEEDED(outBuffer.As(&dxgiBuffer))) {
                ComPtr<ID3D11Texture2D> tex;
                UINT subIdx = 0;
                dxgiBuffer->GetResource(IID_PPV_ARGS(&tex));
                dxgiBuffer->GetSubresourceIndex(&subIdx);

                D3D11_TEXTURE2D_DESC desc;
                tex->GetDesc(&desc);
                if (width_  == 0) width_  = static_cast<int>(desc.Width);
                if (height_ == 0) height_ = static_cast<int>(desc.Height);

                if (cb) cb(tex.Get(), static_cast<int>(subIdx), width_, height_);
                framesDecoded_++;
            }

            outputBuffer.pSample->Release();
            outputBuffer.pSample = nullptr;
        }
    }

    return S_OK;
}

HRESULT MftDecoder::flush(FrameCallback cb) {
    if (!decoder_) return S_OK;
    decoder_->ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0);
    return drainOutput(cb);
}

// ---------------------------------------------------------------------------
// VideoDecoder
// ---------------------------------------------------------------------------
VideoDecoder::VideoDecoder() {}

VideoDecoder::~VideoDecoder() {
    shutdown();
}

HRESULT VideoDecoder::initialize(const std::string& codec, int initialWidth, int initialHeight) {
    spdlog::info("[VideoDecoder] Starting initialize: codec={}, size={}x{}", codec, initialWidth, initialHeight);
    codec_ = codecFromString(codec);
    if (codec_ == Codec::Unknown) {
        spdlog::warn("[VideoDecoder] Unknown codec '{}', defaulting to H264", codec);
        codec_ = Codec::H264;
    }

    spdlog::info("[VideoDecoder] Instantiating helper objects...");
    depayloader_ = std::make_unique<RtpDepayloader>(codec_);
    renderer_    = std::make_unique<D3D11Renderer>();
    mftDecoder_  = std::make_unique<MftDecoder>();

    spdlog::info("[VideoDecoder] Initializing D3D11Renderer...");
    HRESULT hr = renderer_->initialize();
    if (FAILED(hr)) {
        spdlog::error("[VideoDecoder] Renderer init failed: 0x{:08X}", static_cast<uint32_t>(hr));
        return hr;
    }

    spdlog::info("[VideoDecoder] Initializing MftDecoder...");
    hr = mftDecoder_->initialize(codec_, initialWidth, initialHeight, renderer_->device(), renderer_->context());
    if (FAILED(hr)) {
        spdlog::error("[VideoDecoder] MFT decoder init failed: 0x{:08X}", static_cast<uint32_t>(hr));
        return hr;
    }

    initialized_.store(true);
    spdlog::info("[VideoDecoder] Initialized successfully.");
    return S_OK;
}

void VideoDecoder::feedRtp(const uint8_t* data, size_t size) {
    if (!initialized_.load() || shutdown_.load()) return;

    std::lock_guard<std::mutex> lock(decodeMutex_);

    auto accessUnit = depayloader_->feedPacket(data, size);
    if (accessUnit.empty()) return;

    mftDecoder_->feedAccessUnit(
        accessUnit.data(), accessUnit.size(),
        [this](ID3D11Texture2D* tex, int arrayIdx, int w, int h) {
            // Resize swap chain if resolution changed
            if (w != renderer_->width() || h != renderer_->height()) {
                renderer_->resize(w, h);
            }
            renderer_->presentTexture(tex, arrayIdx);

            // Track render times for FPS calculation
            LARGE_INTEGER now;
            QueryPerformanceCounter(&now);
            std::lock_guard<std::mutex> fpsLock(fpsMutex_);
            renderTimes_.push_back(now.QuadPart);
            if (renderTimes_.size() > 120)
                renderTimes_.erase(renderTimes_.begin());
        });
}

void VideoDecoder::updateSurface(const protocol::NativeRenderSurface& surface) {
    if (!renderer_) return;

    spdlog::info("[VideoDecoder] updateSurface called: visible={}, hasParentHwnd={}, hasRect={}",
                 surface.visible,
                 surface.parentHwnd.has_value(),
                 surface.rect.has_value());

    if (surface.parentHwnd.has_value()) {
        spdlog::info("[VideoDecoder] Parsing parentHwnd: {}", *surface.parentHwnd);
        HWND parent = nullptr;
        try {
            parent = reinterpret_cast<HWND>(
                static_cast<uintptr_t>(std::stoull(*surface.parentHwnd, nullptr, 0)));
        } catch (...) {
            spdlog::warn("[VideoDecoder] Invalid parentHwnd: {}", *surface.parentHwnd);
            return;
        }

        int x = 0, y = 0, w = 1920, h = 1080;
        if (surface.rect.has_value()) {
            x = surface.rect->x;
            y = surface.rect->y;
            w = surface.rect->width;
            h = surface.rect->height;
        }
        spdlog::info("[VideoDecoder] Calling setParent: HWND=0x{:X} rect=({},{}) {}x{}",
                     reinterpret_cast<uintptr_t>(parent), x, y, w, h);
        renderer_->setParent(parent, x, y, w, h);
    }

    renderer_->setVisible(surface.visible);
}

void VideoDecoder::setVisible(bool visible) {
    if (renderer_) renderer_->setVisible(visible);
}

void VideoDecoder::shutdown() {
    if (!shutdown_.exchange(true)) return;
    initialized_.store(false);
    std::lock_guard<std::mutex> lock(decodeMutex_);
    if (mftDecoder_) mftDecoder_.reset();
    if (renderer_)   renderer_.reset();
    if (depayloader_) depayloader_.reset();
    MFShutdown();
}

uint64_t VideoDecoder::framesDecoded()  const { return mftDecoder_ ? mftDecoder_->framesDecoded() : 0; }
uint64_t VideoDecoder::framesRendered() const { return renderer_ ? renderer_->framesRendered() : 0; }
std::string VideoDecoder::hwAccelName() const { return mftDecoder_ ? mftDecoder_->hwAccelName() : "N/A"; }
int VideoDecoder::width()  const { return mftDecoder_ ? mftDecoder_->width()  : 0; }
int VideoDecoder::height() const { return mftDecoder_ ? mftDecoder_->height() : 0; }

double VideoDecoder::renderFps() const {
    std::lock_guard<std::mutex> lock(fpsMutex_);
    if (renderTimes_.size() < 2) return 0.0;

    LARGE_INTEGER freq;
    QueryPerformanceFrequency(&freq);

    auto span = renderTimes_.back() - renderTimes_.front();
    double secs = static_cast<double>(span) / static_cast<double>(freq.QuadPart);
    if (secs <= 0) return 0.0;
    return static_cast<double>(renderTimes_.size() - 1) / secs;
}

} // namespace video
