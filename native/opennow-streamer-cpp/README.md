# OpenNOW C++ Native Streamer

Lean, GStreamer-free replacement for the Rust native streamer.
Uses **libdatachannel** + **Windows Media Foundation** + **Direct3D 11**.

## Stack

| Concern | Library |
|---|---|
| WebRTC (ICE/DTLS/SRTP) | [libdatachannel](https://github.com/paullouisageneau/libdatachannel) |
| Video decode (H.264/H.265/AV1) | Windows Media Foundation (built-in, NVDEC auto) |
| Rendering (zero-copy) | Direct3D 11 + DXGI SwapChain |
| Mouse/Keyboard capture | Win32 Raw Input (`RIDEV_INPUTSINK`) |
| IPC | JSON lines over stdin/stdout (protocol v4) |
| JSON | nlohmann-json (header-only) |
| Logging | spdlog → stderr |

## Build Requirements

- **CMake 3.20+**
- **MSVC 2019 or 2022** (VS 2015/MSVC 14.0 is too old — C++17 required)
  - Download Build Tools: https://aka.ms/vs/17/release/vs_BuildTools.exe
- **vcpkg** (recommended) for libdatachannel, nlohmann-json, spdlog
  - `vcpkg install libdatachannel nlohmann-json spdlog`

## Quick Build

```bat
:: From this directory (native/opennow-streamer-cpp)
set VCPKG_ROOT=C:\vcpkg
build.bat
```

## Codec Support

| Codec | Decode | Acceleration |
|---|---|---|
| H.264 | ✅ | NVDEC / Intel QSV / AMD VCE (via WMF) |
| H.265 | ✅ | NVDEC / Intel QSV / AMD VCE (via WMF) |
| AV1 | ✅* | NVDEC (RTX 40xx) / Intel ARC |

*AV1 requires Windows 11 or Windows 10 + [AV1 Video Extension](https://apps.microsoft.com/detail/9mvzqvxjbq9v) from Microsoft Store.

## Architecture

```
stdin (JSON)                                    stdout (JSON)
    │                                               ▲
    ▼                                               │
IpcDispatcher (main thread)                AsyncEventWriter (thread)
    │                                               │
    ├──── PeerConnection (libdatachannel)           │
    │       • SDP offer/answer                      │
    │       • ICE candidates     ──────────────────►│ local-ice events
    │       • Data channel (input send)             │
    │       • Video track → RTP bytes               │
    │                    │                          │
    │                    ▼                          │
    ├──── VideoDecoder                              │
    │       • RtpDepayloader (H264/H265/AV1)       │
    │       • MftDecoder (WMF hardware)             │
    │       • D3D11Renderer (child HWND)            │
    │                                               │
    └──── InputCapture (dedicated thread)           │
            • RIDEV_INPUTSINK message window       │
            • ClipCursor lifecycle                 │
            • GFN binary wire encoding             │
            • Sends via DataChannel   ────────────►│ input-capture-changed
```

## Protocol Compatibility

100% compatible with the existing Rust streamer IPC protocol v4.
No Electron/TypeScript changes needed — just update `executableDiscovery.ts`
to prefer `opennow-streamer-cpp.exe`.
