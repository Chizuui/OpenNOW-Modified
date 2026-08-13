import assert from "node:assert/strict";
import test from "node:test";

import {
  describeDecodeBackend,
  describeEncodeBackend,
  getCodecDecodeBadgeState,
  getCodecToMigrateToAuto,
  getGpuDriverSubtitle,
  isCodecUsableForStream,
  resolveEffectiveCodec,
  resolveNativeCodecAvailability,
  resolveStreamProfileCodec,
  resolveSupportedStreamCodecs,
  shouldShowLinuxHardwareCodecHint,
  shouldShowQuickSyncDriverHint,
  type CodecTestResult,
} from "./codecDiagnostics";
import type { GpuBackendInfo, NativeStreamerStatus } from "@shared/gfn";

function withNavigator(platform: string, userAgent: string, run: () => void): void {
  const original = Object.getOwnPropertyDescriptor(globalThis, "navigator");
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: { platform, userAgent },
  });
  try {
    run();
  } finally {
    if (original) {
      Object.defineProperty(globalThis, "navigator", original);
    } else {
      delete (globalThis as { navigator?: Navigator }).navigator;
    }
  }
}

function codecResult(overrides: Partial<CodecTestResult> = {}): CodecTestResult {
  return {
    codec: "H264",
    webrtcSupported: true,
    decodeSupported: true,
    hwAccelerated: false,
    encodeSupported: false,
    encodeHwAccelerated: false,
    decodeVia: "Software (CPU)",
    encodeVia: "Unsupported",
    profiles: [],
    ...overrides,
  };
}

test("shows Linux hardware hint for software-only codec diagnostics", () => {
  withNavigator("Linux x86_64", "OpenNOW Linux", () => {
    assert.equal(shouldShowLinuxHardwareCodecHint([codecResult()]), true);
  });
});

test("does not show Linux hardware hint for GPU-backed diagnostics", () => {
  withNavigator("Linux x86_64", "OpenNOW Linux", () => {
    assert.equal(shouldShowLinuxHardwareCodecHint([codecResult({ hwAccelerated: true })]), false);
  });
});

test("does not show Linux hardware hint on non-Linux clients", () => {
  withNavigator("Win32", "OpenNOW Windows", () => {
    assert.equal(shouldShowLinuxHardwareCodecHint([codecResult()]), false);
  });
});

test("shows Quick Sync driver hint when H264 encodes on CPU but decodes on GPU (Windows)", () => {
  // The user's exact case: H.264 decode on D3D11 GPU, encode fell back to
  // software — the Intel H.264 encoder MFT is not registered.
  withNavigator("Win32", "OpenNOW Windows", () => {
    const results = [
      codecResult({
        codec: "H264",
        decodeSupported: true,
        hwAccelerated: true,
        encodeSupported: true,
        encodeHwAccelerated: false,
      }),
    ];
    assert.equal(shouldShowQuickSyncDriverHint(results), true);
  });
});

test("does not show Quick Sync hint when H264 encode is also hardware", () => {
  withNavigator("Win32", "OpenNOW Windows", () => {
    const results = [
      codecResult({
        codec: "H264",
        decodeSupported: true,
        hwAccelerated: true,
        encodeSupported: true,
        encodeHwAccelerated: true,
      }),
    ];
    assert.equal(shouldShowQuickSyncDriverHint(results), false);
  });
});

test("does not show Quick Sync hint when H264 decode is not GPU-backed", () => {
  withNavigator("Win32", "OpenNOW Windows", () => {
    const results = [
      codecResult({
        codec: "H264",
        decodeSupported: true,
        hwAccelerated: false,
        encodeSupported: true,
        encodeHwAccelerated: false,
      }),
    ];
    assert.equal(shouldShowQuickSyncDriverHint(results), false);
  });
});

test("decode backend label uses the actual GPU name from the GPU process", () => {
  const gpuInfo: GpuBackendInfo = {
    gpuName: "Intel(R) UHD Graphics",
    vendorName: "Intel",
    driverVersion: "31.0.101",
    decodeAccelerated: true,
    encodeAccelerated: true,
    hardwareDecodeCodecs: ["H264", "H265", "AV1"],
    hardwareEncodeCodecs: ["H265"],
  };
  // Hardware decode → actual GPU model, not the guessed "D3D11".
  assert.equal(describeDecodeBackend(true, gpuInfo), "Intel(R) UHD Graphics (GPU)");
  // Software decode stays honest.
  assert.equal(describeDecodeBackend(false, gpuInfo), "Software (CPU)");
});

test("decode backend falls back to the platform guess when GPU info is missing", () => {
  withNavigator("Win32", "OpenNOW Windows", () => {
    assert.equal(describeDecodeBackend(true, null), "D3D11 (GPU)");
    assert.equal(describeDecodeBackend(false, null), "Software (CPU)");
  });
});

test("encode backend label uses the actual GPU name from the GPU process", () => {
  const gpuInfo: GpuBackendInfo = {
    gpuName: "Intel(R) UHD Graphics",
    vendorName: "Intel",
    driverVersion: "31.0.101",
    decodeAccelerated: true,
    encodeAccelerated: true,
    hardwareDecodeCodecs: ["H264", "H265", "AV1"],
    hardwareEncodeCodecs: ["H265"],
  };
  // Hardware encode → actual GPU model, not the guessed "Media Foundation".
  assert.equal(describeEncodeBackend(true, gpuInfo), "Intel(R) UHD Graphics (GPU)");
  assert.equal(describeEncodeBackend(false, gpuInfo), "Software (CPU)");
});

test("encode backend falls back to the platform guess when GPU info is missing", () => {
  withNavigator("Win32", "OpenNOW Windows", () => {
    assert.equal(describeEncodeBackend(true, null), "Media Foundation (GPU)");
    assert.equal(describeEncodeBackend(false, null), "Software (CPU)");
  });
});

test("does not show Quick Sync hint without an H264 result or on non-Windows", () => {
  // No H264 entry.
  assert.equal(shouldShowQuickSyncDriverHint([
    codecResult({ codec: "AV1" }),
  ]), false);
  // Null / empty results.
  assert.equal(shouldShowQuickSyncDriverHint(null), false);
  assert.equal(shouldShowQuickSyncDriverHint([]), false);
  // Non-Windows client.
  withNavigator("Linux x86_64", "OpenNOW Linux", () => {
    assert.equal(shouldShowQuickSyncDriverHint([
      codecResult({
        codec: "H264",
        decodeSupported: true,
        hwAccelerated: true,
        encodeSupported: true,
        encodeHwAccelerated: false,
      }),
    ]), false);
  });
});

test("supported stream codecs gate AV1 on hardware decode like the official bundle", () => {
  // Software-only AV1 (powerEfficient=false) is excluded, mirroring the
  // official probe Ki(); H265/H264 stay available when decodable.
  assert.deepEqual(resolveSupportedStreamCodecs([
    codecResult({ codec: "H264", decodeSupported: true, webrtcSupported: true }),
    codecResult({ codec: "H265", decodeSupported: true, webrtcSupported: true, hwAccelerated: false }),
    codecResult({ codec: "AV1", decodeSupported: true, webrtcSupported: true, hwAccelerated: false }),
  ]), ["H264", "H265"]);
  // Hardware AV1 decode → AV1 is advertised.
  assert.deepEqual(resolveSupportedStreamCodecs([
    codecResult({ codec: "H264", decodeSupported: true, webrtcSupported: true }),
    codecResult({ codec: "H265", decodeSupported: true, webrtcSupported: true }),
    codecResult({ codec: "AV1", decodeSupported: true, webrtcSupported: true, hwAccelerated: true }),
  ]), ["H264", "H265", "AV1"]);
  // Not decodable (e.g. H265 without the HEVC extension) → excluded.
  assert.deepEqual(resolveSupportedStreamCodecs([
    codecResult({ codec: "H264", decodeSupported: true, webrtcSupported: true }),
    codecResult({ codec: "H265", decodeSupported: false, webrtcSupported: true }),
    codecResult({ codec: "AV1", decodeSupported: false, webrtcSupported: true }),
  ]), ["H264"]);
  // No probe results → sync WebRTC fallback; in the Node test env every check
  // fails so the list floors to H264 (never empty, so ladder resolution stays
  // active in the session request).
  assert.deepEqual(resolveSupportedStreamCodecs(null), ["H264"]);
});

test("explicit codec preference is honored even when the device reports it unsupported", () => {
  // In the Node test env RTCRtpReceiver is undefined, so "auto" falls back to H264.
  assert.equal(resolveEffectiveCodec("H265"), "H265");
  assert.equal(resolveEffectiveCodec("AV1"), "AV1");
});

test("auto codec preference resolves to a usable fallback when capabilities are unknown", () => {
  // No RTCRtpReceiver in Node → every availability check fails → H264 fallback.
  assert.equal(resolveEffectiveCodec("auto"), "H264");
});

test("codec usability requires both decode support and WebRTC receiver support", () => {
  // In WebRTC receiver capabilities but not actually decodable (the GFN-web
  // "Unsupported" case, e.g. H.265 without the HEVC extension).
  assert.equal(isCodecUsableForStream("H265", [
    codecResult({ codec: "H265", webrtcSupported: true, decodeSupported: false }),
  ]), false);
  // Decodable (e.g. AV1 via software dav1d) but absent from the WebRTC
  // receiver capabilities — the exact case that previously left the video
  // m-line rejected when the codec was force-selected.
  assert.equal(isCodecUsableForStream("AV1", [
    codecResult({ codec: "AV1", webrtcSupported: false, decodeSupported: true }),
  ]), false);
  // Fully supported: decodable AND present in WebRTC receiver capabilities.
  assert.equal(isCodecUsableForStream("H265", [
    codecResult({ codec: "H265", webrtcSupported: true, decodeSupported: true }),
  ]), true);
  assert.equal(isCodecUsableForStream("AV1", [
    codecResult({ codec: "AV1", webrtcSupported: true, decodeSupported: true }),
  ]), true);
});

test("saved concrete codec is flagged for auto migration only when unusable", () => {
  // "auto" is never migrated.
  assert.equal(getCodecToMigrateToAuto("auto", [
    codecResult({ codec: "AV1", webrtcSupported: false, decodeSupported: false }),
  ]), null);
  // Fully usable codec → nothing to migrate.
  assert.equal(getCodecToMigrateToAuto("H265", [
    codecResult({ codec: "H265", webrtcSupported: true, decodeSupported: true }),
  ]), null);
  // The GFN-web H.265-without-HEVC case: in receiver caps but not decodable → migrate.
  assert.equal(getCodecToMigrateToAuto("H265", [
    codecResult({ codec: "H265", webrtcSupported: true, decodeSupported: false }),
  ]), "H265");
  // AV1 decodable (e.g. software dav1d) but absent from WebRTC receiver caps → migrate.
  assert.equal(getCodecToMigrateToAuto("AV1", [
    codecResult({ codec: "AV1", webrtcSupported: false, decodeSupported: true }),
  ]), "AV1");
});

test("codec migration waits for diagnostics results before deciding", () => {
  // No result set yet (startup test pending, failed, or cleared) → never
  // migrate, even when the codec might turn out to be unusable.
  assert.equal(getCodecToMigrateToAuto("AV1", null), null);
  assert.equal(getCodecToMigrateToAuto("H265", []), null);
});

test("resolved stream profile pins color quality to the resolved codec", () => {
  // No RTCRtpReceiver in Node → auto resolves to H264 → color pinned to 8-bit.
  assert.deepEqual(resolveStreamProfileCodec("auto", "10bit_420"), {
    codec: "H264",
    colorQuality: "8bit_420",
  });
  // Explicit H265 keeps 10-bit color.
  assert.deepEqual(resolveStreamProfileCodec("H265", "10bit_420"), {
    codec: "H265",
    colorQuality: "10bit_420",
  });
});

test("gpu driver subtitle prefers the vendor name with the driver version", () => {
  const gpuInfo: GpuBackendInfo = {
    gpuName: "Intel(R) UHD Graphics",
    vendorName: "Intel",
    driverVersion: "31.0.101.5336",
    decodeAccelerated: true,
    encodeAccelerated: true,
    hardwareDecodeCodecs: [],
    hardwareEncodeCodecs: [],
  };
  assert.deepEqual(getGpuDriverSubtitle(gpuInfo), {
    name: "Intel",
    version: "31.0.101.5336",
  });
});

test("gpu driver subtitle falls back to the GPU model without a vendor", () => {
  const gpuInfo: GpuBackendInfo = {
    gpuName: "NVIDIA GeForce RTX 3060",
    vendorName: null,
    driverVersion: null,
    decodeAccelerated: true,
    encodeAccelerated: null,
    hardwareDecodeCodecs: ["H264"],
    hardwareEncodeCodecs: [],
  };
  // No driver version → name only, so the panel renders a plain label.
  assert.deepEqual(getGpuDriverSubtitle(gpuInfo), {
    name: "NVIDIA GeForce RTX 3060",
    version: null,
  });
});

test("gpu driver subtitle returns null when no identity is known", () => {
  assert.equal(getGpuDriverSubtitle(null), null);
  const empty: GpuBackendInfo = {
    gpuName: null,
    vendorName: null,
    driverVersion: "",
    decodeAccelerated: null,
    encodeAccelerated: null,
    hardwareDecodeCodecs: [],
    hardwareEncodeCodecs: [],
  };
  assert.equal(getGpuDriverSubtitle(empty), null);
});

function nativeStatus(overrides: Partial<NativeStreamerStatus> = {}): NativeStreamerStatus {
  const d3d12 = {
    backend: "d3d12",
    platform: "windows",
    codecs: [
      { codec: "h264", available: true, decoder: "d3d12h264dec" },
      { codec: "h265", available: true, decoder: "d3d12h265dec" },
      { codec: "av1", available: true, decoder: "d3d12av1dec" },
    ],
    zeroCopyModes: ["D3D12Memory"],
    sink: "d3d12videosink",
    available: true,
  };
  return {
    detected: true,
    gstreamerAvailable: true,
    supportsOfferAnswer: true,
    backend: "gstreamer",
    videoBackends: [
      d3d12,
      {
        backend: "software",
        platform: "cross-platform",
        codecs: [
          { codec: "h264", available: true, decoder: "avdec_h264" },
          { codec: "h265", available: true, decoder: "avdec_h265" },
          { codec: "av1", available: true, decoder: "dav1ddec" },
        ],
        zeroCopyModes: [],
        sink: "autovideosink",
        available: true,
      },
      {
        backend: "videotoolbox",
        platform: "macos",
        codecs: [
          { codec: "h264", available: false, decoder: "vtdec_h264", reason: "does not run on windows" },
        ],
        zeroCopyModes: [],
        available: false,
      },
    ],
    activeVideoBackend: d3d12,
    gstreamerRuntime: { source: "bundled", bundled: true, message: "ok" },
    message: "ok",
    ...overrides,
  };
}

test("native codec availability resolves decodable codecs from the active backend", () => {
  withNavigator("Win32", "OpenNOW Windows", () => {
    const availability = resolveNativeCodecAvailability(nativeStatus());
    assert.ok(availability);
    // The active D3D12 backend decodes all three codecs in hardware.
    assert.deepEqual([...availability!.codecs].sort(), ["AV1", "H264", "H265"]);
    assert.deepEqual([...availability!.hardwareCodecs].sort(), ["AV1", "H264", "H265"]);
    assert.equal(availability!.hardware, true);
  });
});

test("native codec availability returns null when the streamer is not usable", () => {
  withNavigator("Win32", "OpenNOW Windows", () => {
    assert.equal(resolveNativeCodecAvailability(null), null);
    assert.equal(resolveNativeCodecAvailability(nativeStatus({ gstreamerAvailable: false })), null);
    assert.equal(resolveNativeCodecAvailability(nativeStatus({ videoBackends: [] })), null);
  });
});

test("native codec availability excludes backends for other platforms", () => {
  withNavigator("Win32", "OpenNOW Windows", () => {
    // Only the macos backend is available: a Windows host must not inherit it.
    const macOnly = nativeStatus({
      videoBackends: [{
        backend: "videotoolbox",
        platform: "macos",
        codecs: [
          { codec: "h264", available: true, decoder: "vtdec_h264" },
          { codec: "h265", available: true, decoder: "vtdec_h265" },
        ],
        zeroCopyModes: [],
        available: true,
      }],
      activeVideoBackend: undefined,
    });
    assert.equal(resolveNativeCodecAvailability(macOnly), null);
  });
});

test("native codec availability marks software-only paths as CPU", () => {
  withNavigator("Win32", "OpenNOW Windows", () => {
    const softwareOnly = nativeStatus({
      videoBackends: [{
        backend: "software",
        platform: "cross-platform",
        codecs: [{ codec: "h264", available: true, decoder: "avdec_h264" }],
        zeroCopyModes: [],
        sink: "autovideosink",
        available: true,
      }],
      activeVideoBackend: undefined,
    });
    const availability = resolveNativeCodecAvailability(softwareOnly);
    assert.ok(availability);
    assert.deepEqual([...availability!.codecs], ["H264"]);
    assert.deepEqual([...availability!.hardwareCodecs], []);
    assert.equal(availability!.hardware, false);
  });
});

test("native capabilities make H265 usable even when the browser probe says unsupported", () => {
  const availability = resolveNativeCodecAvailability(nativeStatus());
  // Browser probe: H265 in receiver caps but not decodable (no HEVC extension).
  assert.equal(isCodecUsableForStream("H265", [
    codecResult({ codec: "H265", webrtcSupported: true, decodeSupported: false }),
  ]), false);
  // Native mode: the streamer decodes H265 via d3d12h265dec → usable.
  assert.equal(isCodecUsableForStream("H265", [
    codecResult({ codec: "H265", webrtcSupported: true, decodeSupported: false }),
  ], availability), true);
});

test("native availability wins over the browser probe but never removes codecs", () => {
  const availability = resolveNativeCodecAvailability(nativeStatus());
  // Browser-only supported codec (not in the native set) stays usable.
  assert.equal(isCodecUsableForStream("AV1", [
    codecResult({ codec: "AV1", webrtcSupported: true, decodeSupported: true }),
  ], availability), true);
  // Missing native status → browser probe rules again.
  assert.equal(isCodecUsableForStream("H265", [
    codecResult({ codec: "H265", webrtcSupported: true, decodeSupported: false }),
  ], null), false);
});

test("auto codec resolution prefers the native codec order", () => {
  const availability = resolveNativeCodecAvailability(nativeStatus());
  // Native set contains AV1 on a hardware backend → auto picks AV1 (same
  // priority as web mode).
  assert.equal(resolveEffectiveCodec("auto", null, availability), "AV1");
  // Without native status the Node test env falls back to H264.
  assert.equal(resolveEffectiveCodec("auto", null), "H264");
});

function softwareOnlyNativeStatus(codecs: string[] = ["h264", "h265", "av1"]): NativeStreamerStatus {
  const software = {
    backend: "software",
    platform: "cross-platform",
    codecs: codecs.map((codec) => ({ codec, available: true, decoder: `avdec_${codec}` })),
    zeroCopyModes: [],
    sink: "autovideosink",
    available: true,
  };
  return nativeStatus({
    videoBackends: [software],
    activeVideoBackend: software,
  });
}

test("auto codec skips software-only AV1/HEVC in native mode and falls to H264", () => {
  withNavigator("Win32", "OpenNOW Windows", () => {
    // The streamer can decode all three, but only through software backends
    // (avdec_*/dav1d) — no GPU decode. Auto must NOT pick AV1 (the bug: weak
    // iGPU falling into software 1080p60 decode); it lands on H264 instead.
    const availability = resolveNativeCodecAvailability(softwareOnlyNativeStatus());
    assert.ok(availability);
    assert.deepEqual([...availability!.codecs].sort(), ["AV1", "H264", "H265"]);
    assert.deepEqual([...availability!.hardwareCodecs], []);
    assert.equal(resolveEffectiveCodec("auto", null, availability), "H264");
    // Explicit picks are still honored (the user asked for it).
    assert.equal(resolveEffectiveCodec("AV1", null, availability), "AV1");
  });
});

test("auto codec prefers hardware HEVC over software AV1 in native mode", () => {
  withNavigator("Win32", "OpenNOW Windows", () => {
    // AV1 only decodes in software; HEVC decodes on D3D12; no H264. Auto must
    // skip AV1 (no hardware) and pick H265 instead of falling back to H264.
    const d3d12Hevc = {
      backend: "d3d12",
      platform: "windows",
      codecs: [{ codec: "h265", available: true, decoder: "d3d12h265dec" }],
      zeroCopyModes: ["D3D12Memory"],
      sink: "d3d12videosink",
      available: true,
    };
    const software = {
      backend: "software",
      platform: "cross-platform",
      codecs: [{ codec: "av1", available: true, decoder: "dav1ddec" }],
      zeroCopyModes: [],
      sink: "autovideosink",
      available: true,
    };
    const availability = resolveNativeCodecAvailability(nativeStatus({
      videoBackends: [d3d12Hevc, software],
      activeVideoBackend: d3d12Hevc,
    }));
    assert.ok(availability);
    assert.deepEqual([...availability!.codecs].sort(), ["AV1", "H265"]);
    assert.deepEqual([...availability!.hardwareCodecs], ["H265"]);
    assert.equal(resolveEffectiveCodec("auto", null, availability), "H265");
  });
});

test("auto codec in web mode gates AV1 on hardware decode like the ladder", () => {
  // Software-only AV1 (powerEfficient=false) → skipped, auto lands on H264.
  assert.equal(resolveEffectiveCodec("auto", [
    codecResult({ codec: "H264", decodeSupported: true, webrtcSupported: true }),
    codecResult({ codec: "H265", decodeSupported: true, webrtcSupported: true }),
    codecResult({ codec: "AV1", decodeSupported: true, webrtcSupported: true, hwAccelerated: false }),
  ]), "H264");
  // Hardware AV1 decode → AV1 wins (same priority as GFN web's "Auto (AV1)").
  assert.equal(resolveEffectiveCodec("auto", [
    codecResult({ codec: "H264", decodeSupported: true, webrtcSupported: true }),
    codecResult({ codec: "H265", decodeSupported: true, webrtcSupported: true }),
    codecResult({ codec: "AV1", decodeSupported: true, webrtcSupported: true, hwAccelerated: true }),
  ]), "AV1");
});

test("resolved stream profile honors native hardware gating", () => {
  const availability = resolveNativeCodecAvailability(softwareOnlyNativeStatus());
  // Software-only native AV1 → auto profile resolves to H264 and pins color
  // back to 8-bit 4:2:0, exactly like a device without hardware AV1/HEVC.
  assert.deepEqual(resolveStreamProfileCodec("auto", "10bit_420", null, availability), {
    codec: "H264",
    colorQuality: "8bit_420",
  });
});

test("native mode clamps color quality to 8-bit 4:2:0 even on explicit codec picks", () => {
  const availability = resolveNativeCodecAvailability(nativeStatus());
  // The native display path renders 8-bit BT.709 full-range NV12 for EVERY
  // codec (DISPLAY_NV12_FULL_RANGE_CAPS), so advertising 10-bit/4:4:4 would
  // negotiate a stream that is converted down (banding, wrong HDR colors).
  // An explicit H265 pick stays H265, but the color must clamp to 8-bit 4:2:0.
  assert.deepEqual(resolveStreamProfileCodec("H265", "10bit_420", null, availability), {
    codec: "H265",
    colorQuality: "8bit_420",
  });
  assert.deepEqual(resolveStreamProfileCodec("H265", "8bit_444", null, availability), {
    codec: "H265",
    colorQuality: "8bit_420",
  });
  assert.deepEqual(resolveStreamProfileCodec("AV1", "10bit_420", null, availability), {
    codec: "AV1",
    colorQuality: "8bit_420",
  });
});

test("native mode clamps color quality when auto resolves to hardware AV1", () => {
  const availability = resolveNativeCodecAvailability(nativeStatus());
  // Hardware D3D12 decode for all three → auto still picks AV1 (same priority
  // as web mode), but color quality is clamped because the display path is
  // 8-bit — the ladder and the effective profile must agree.
  assert.deepEqual(resolveStreamProfileCodec("auto", "10bit_420", null, availability), {
    codec: "AV1",
    colorQuality: "8bit_420",
  });
});

test("web mode keeps the requested color quality (no native clamp)", () => {
  // Without native availability the 10-bit / 4:4:4 modes are valid: web mode
  // renders through Chromium, not the 8-bit NV12 display path.
  assert.deepEqual(resolveStreamProfileCodec("H265", "10bit_420"), {
    codec: "H265",
    colorQuality: "10bit_420",
  });
  assert.deepEqual(resolveStreamProfileCodec("auto", "8bit_444", [
    codecResult({ codec: "H264", decodeSupported: true, webrtcSupported: true }),
    codecResult({ codec: "H265", decodeSupported: true, webrtcSupported: true }),
    codecResult({ codec: "AV1", decodeSupported: true, webrtcSupported: true, hwAccelerated: true }),
  ]), {
    codec: "AV1",
    colorQuality: "8bit_444",
  });
});

test("native ladder excludes software-only AV1/HEVC but keeps H264", () => {
  const availability = resolveNativeCodecAvailability(softwareOnlyNativeStatus());
  // The streamer decodes all three, but only in software — the ladder must
  // stop advertising AV1/HEVC for auto negotiation (would be software 1080p60)
  // while keeping H264 as the universal fallback.
  assert.deepEqual(resolveSupportedStreamCodecs([], availability), ["H264"]);
});

test("decode badge uses per-codec native hardware info", () => {
  withNavigator("Win32", "OpenNOW Windows", () => {
    // H264 only on a software backend, AV1 on D3D12 — the badge must differ
    // per codec instead of using the coarse all-or-nothing `hardware` flag.
    const d3d12Av1 = {
      backend: "d3d12",
      platform: "windows",
      codecs: [{ codec: "av1", available: true, decoder: "d3d12av1dec" }],
      zeroCopyModes: ["D3D12Memory"],
      sink: "d3d12videosink",
      available: true,
    };
    const software = {
      backend: "software",
      platform: "cross-platform",
      codecs: [{ codec: "h264", available: true, decoder: "avdec_h264" }],
      zeroCopyModes: [],
      sink: "autovideosink",
      available: true,
    };
    const availability = resolveNativeCodecAvailability(nativeStatus({
      videoBackends: [d3d12Av1, software],
      activeVideoBackend: d3d12Av1,
    }));
    assert.ok(availability);
    assert.equal(getCodecDecodeBadgeState("H264", null, false, availability), "cpu");
    assert.equal(getCodecDecodeBadgeState("AV1", null, false, availability), "gpu");
  });
});

test("native capabilities add codecs to the supported list for the ladder/hint", () => {
  const availability = resolveNativeCodecAvailability(nativeStatus());
  // Browser probe says H265 is not decodable, but native adds it back.
  assert.deepEqual(resolveSupportedStreamCodecs([
    codecResult({ codec: "H264", decodeSupported: true, webrtcSupported: true }),
    codecResult({ codec: "H265", decodeSupported: false, webrtcSupported: true }),
    codecResult({ codec: "AV1", decodeSupported: false, webrtcSupported: true }),
  ], availability), ["H264", "H265", "AV1"]);
});

test("native capabilities prevent saved H265 from being migrated to auto", () => {
  const availability = resolveNativeCodecAvailability(nativeStatus());
  // Browser-only view: H265 not decodable → would migrate.
  assert.equal(getCodecToMigrateToAuto("H265", [
    codecResult({ codec: "H265", webrtcSupported: true, decodeSupported: false }),
  ]), "H265");
  // Native mode: the streamer decodes it → keep the saved preference.
  assert.equal(getCodecToMigrateToAuto("H265", [
    codecResult({ codec: "H265", webrtcSupported: true, decodeSupported: false }),
  ], availability), null);
});
