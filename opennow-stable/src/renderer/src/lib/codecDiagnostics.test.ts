import assert from "node:assert/strict";
import test from "node:test";

import {
  getCodecToMigrateToAuto,
  isCodecUsableForStream,
  resolveEffectiveCodec,
  resolveStreamProfileCodec,
  shouldShowLinuxHardwareCodecHint,
  type CodecTestResult,
} from "./codecDiagnostics";

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
