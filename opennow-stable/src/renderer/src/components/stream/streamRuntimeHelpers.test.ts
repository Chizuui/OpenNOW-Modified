/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  clampRecordingBitrate,
  computeRecordingFrameShortfall,
  fitThumbnailSize,
  getShortcutConflictError,
  selectRecordingMimeType,
  selectRecordingStrategy,
} from "./streamRuntimeHelpers";

test("shortcut conflict validation preserves empty, invalid, and conflict errors", () => {
  assert.equal(getShortcutConflictError("", []), "Shortcut cannot be empty.");
  assert.equal(getShortcutConflictError("Ctrl+UnknownNamedKey", []), "Invalid shortcut format.");
  assert.equal(
    getShortcutConflictError("shift+ctrl+s", ["Ctrl+Shift+S"]),
    "Shortcut conflicts with an existing binding.",
  );
  assert.equal(getShortcutConflictError("Ctrl+Shift+S", ["Ctrl+R", undefined]), null);
});

test("recording MIME selection uses the first supported preference", () => {
  assert.equal(
    selectRecordingMimeType((mimeType) => mimeType === "video/webm;codecs=h264"),
    "video/webm;codecs=h264",
  );
  assert.equal(selectRecordingMimeType(() => false), "video/webm");
});

test("recording strategy uses raw track only when AVC encode is hardware", async () => {
  // AVC supported AND power-efficient (hardware) → raw track (GFN-like).
  assert.deepEqual(
    await selectRecordingStrategy(() => true, async () => ({ powerEfficient: true })),
    {
      strategy: "raw-track",
      mimeType: "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
      hwAccelerated: true,
    },
  );
  // Only non-baseline AVC MP4 supported, hardware → still raw-track.
  assert.deepEqual(
    await selectRecordingStrategy(
      (mimeType) => mimeType === "video/mp4;codecs=avc1",
      async () => ({ powerEfficient: true }),
    ),
    { strategy: "raw-track", mimeType: "video/mp4;codecs=avc1", hwAccelerated: true },
  );
  // AVC supported but the encoder is SOFTWARE (OpenH264 fallback) → the
  // full-res raw track would starve the WebRTC decoder (8-fps field report),
  // so it falls back to the bounded canvas downscale.
  assert.deepEqual(
    await selectRecordingStrategy(() => true, async () => ({ powerEfficient: false })),
    {
      strategy: "canvas-downscale",
      mimeType: "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
      hwAccelerated: false,
    },
  );
  // Probe unavailable / throws → safe canvas path, never a software full-res
  // raw track.
  assert.deepEqual(
    await selectRecordingStrategy(() => true, async () => undefined),
    {
      strategy: "canvas-downscale",
      mimeType: "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
      hwAccelerated: false,
    },
  );
  assert.deepEqual(
    await selectRecordingStrategy(() => true, async () => {
      throw new Error("no mediaCapabilities");
    }),
    {
      strategy: "canvas-downscale",
      mimeType: "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
      hwAccelerated: false,
    },
  );
  // Software-only platforms (e.g. VP8 on Linux without VAAPI) → canvas
  // downscale regardless of the probe.
  assert.deepEqual(
    await selectRecordingStrategy(
      (mimeType) => mimeType === "video/webm;codecs=vp8",
      async () => ({ powerEfficient: true }),
    ),
    { strategy: "canvas-downscale", mimeType: "video/webm;codecs=vp8", hwAccelerated: false },
  );
  // Nothing supported → last-resort webm still routes through the canvas.
  assert.deepEqual(
    await selectRecordingStrategy(() => false, async () => ({ powerEfficient: true })),
    { strategy: "canvas-downscale", mimeType: "video/webm", hwAccelerated: false },
  );
});

test("recording bitrate clamp honors strategy ceilings", () => {
  // Auto mode is left untouched in both strategies.
  assert.equal(clampRecordingBitrate(null, "raw-track"), undefined);
  assert.equal(clampRecordingBitrate(null, "canvas-downscale"), undefined);
  // Canvas-downscale caps at 12 Mbps (720p30 ceiling).
  assert.equal(clampRecordingBitrate(12, "canvas-downscale"), 12_000_000);
  assert.equal(clampRecordingBitrate(50, "canvas-downscale"), 12_000_000);
  assert.equal(clampRecordingBitrate(3, "canvas-downscale"), 3_000_000);
  // Raw-track records at stream resolution — the slider ceiling (75) applies.
  assert.equal(clampRecordingBitrate(30, "raw-track"), 30_000_000);
  assert.equal(clampRecordingBitrate(80, "raw-track"), 75_000_000);
  assert.equal(clampRecordingBitrate(10.4, "raw-track"), 10_000_000);
  // Degenerate inputs stay at the floor.
  assert.equal(clampRecordingBitrate(0, "raw-track"), 1_000_000);
  assert.equal(clampRecordingBitrate(-5, "canvas-downscale"), 1_000_000);
});

test("thumbnail sizing preserves aspect ratio within recording bounds", () => {
  assert.deepEqual(fitThumbnailSize(1920, 1080), { width: 320, height: 180 });
  assert.deepEqual(fitThumbnailSize(1024, 768), { width: 240, height: 180 });
  assert.deepEqual(fitThumbnailSize(1080, 1920), { width: 101, height: 180 });
  assert.deepEqual(fitThumbnailSize(160, 90), { width: 160, height: 90 });
});

test("recording frame shortfall counts draws below the target rate", () => {
  // 30 s at 30 fps = 900 expected; drawing all of them → no shortfall.
  assert.equal(computeRecordingFrameShortfall(900, 30_000, 30), 0);
  // Drawn only 870 → 30 frames short.
  assert.equal(computeRecordingFrameShortfall(870, 30_000, 30), 30);
  // Never negative on overdraw (rounded draws above the target).
  assert.equal(computeRecordingFrameShortfall(901, 30_000, 30), 0);
});

test("recording frame shortfall guards degenerate inputs", () => {
  assert.equal(computeRecordingFrameShortfall(10, 0, 30), 0);
  assert.equal(computeRecordingFrameShortfall(10, -5, 30), 0);
  assert.equal(computeRecordingFrameShortfall(10, 10_000, 0), 0);
  assert.equal(computeRecordingFrameShortfall(10, Number.NaN, 30), 0);
});
