/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  clampRecordingBitrate,
  computeRecordingFrameShortfall,
  fitThumbnailSize,
  getShortcutConflictError,
  resolveRecordCap,
  resolveRecordingStreamFps,
  selectRecordingMimeType,
  selectRecordingStrategy,
} from "./streamRuntimeHelpers";

const CAP_720P30 = { width: 1280, height: 720, fps: 30 };
const STREAM_720P30 = { width: 1280, height: 720, fps: 30 };
const STREAM_1080P60 = { width: 1920, height: 1080, fps: 60 };

async function pick(
  isTypeSupported: (mimeType: string) => boolean,
  stream: { width: number; height: number; fps: number | null },
  cap: { width: number; height: number; fps: number },
  encodedTransformAvailable = false,
  webCodecsAvailable = false,
) {
  const choice = await selectRecordingStrategy(
    isTypeSupported,
    stream,
    cap,
    encodedTransformAvailable,
    webCodecsAvailable,
  );
  return { strategy: choice.strategy, mimeType: choice.mimeType, hwAccelerated: choice.hwAccelerated };
}

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

test("recording strategy prefers encoded transform whenever it is available (cap not applied)", async () => {
  // Encoded transform available → bitstream capture wins over EVERY other
  // path, even when the stream exceeds the cap and even without AVC support
  // (AV1/VP9 capture goes to WebM). "Jangan di cap": no cap involved.
  assert.deepEqual(
    await pick(() => true, STREAM_1080P60, CAP_720P30, true),
    { strategy: "encoded-transform", mimeType: "video/mp4", hwAccelerated: true },
  );
  assert.deepEqual(
    await pick(() => false, STREAM_720P30, CAP_720P30, true),
    { strategy: "encoded-transform", mimeType: "video/mp4", hwAccelerated: true },
  );
});

test("recording strategy uses the raw track whenever the stream fits the cap", async () => {
  // Stream already within the 720p30 cap → direct track capture. The canvas
  // pipeline would re-encode the SAME pixels with extra per-frame main-thread
  // work (it does not downscale anything within the cap), so it is never
  // chosen here — the field render drops during recording came from exactly
  // that overhead.
  assert.deepEqual(
    await pick(() => true, STREAM_720P30, CAP_720P30),
    {
      strategy: "raw-track",
      mimeType: "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
      hwAccelerated: true,
    },
  );
  // Unknown stream frame rate still qualifies for raw-track when the
  // resolution gate passes (unknown ≠ known-exceeding).
  assert.deepEqual(
    await pick(() => true, { width: 1280, height: 720, fps: null }, CAP_720P30),
    { strategy: "raw-track", mimeType: "video/mp4;codecs=avc1.42E01E,mp4a.40.2", hwAccelerated: true },
  );
  // Only non-baseline AVC MP4 supported → still raw-track.
  assert.deepEqual(
    await pick((mimeType) => mimeType === "video/mp4;codecs=avc1", STREAM_720P30, CAP_720P30),
    { strategy: "raw-track", mimeType: "video/mp4;codecs=avc1", hwAccelerated: true },
  );
  // Stream resolution EXCEEDS the cap → the encode is bounded to the cap
  // instead of re-encoding the full stream (the field-report lag: recording
  // starves the decode pipeline).
  assert.deepEqual(
    await pick(() => true, STREAM_1080P60, CAP_720P30),
    {
      strategy: "canvas-downscale",
      mimeType: "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
      hwAccelerated: false,
    },
  );
  // Stream within resolution but above the cap FPS (60 vs 30) → bounded too:
  // a 2x frame-rate encode is still a real load on the decode pipeline.
  assert.deepEqual(
    await pick(() => true, { width: 1280, height: 720, fps: 60 }, CAP_720P30),
    { strategy: "canvas-downscale", mimeType: "video/mp4;codecs=avc1.42E01E,mp4a.40.2", hwAccelerated: false },
  );
  // No AVC container (e.g. VP8-only runtime) → canvas regardless of the
  // stream fitting the cap.
  assert.deepEqual(
    await pick((mimeType) => mimeType === "video/webm;codecs=vp8", STREAM_720P30, CAP_720P30),
    { strategy: "canvas-downscale", mimeType: "video/webm;codecs=vp8", hwAccelerated: false },
  );
  // Nothing supported → last-resort webm still routes through the canvas.
  assert.deepEqual(
    await pick(() => false, STREAM_720P30, CAP_720P30),
    { strategy: "canvas-downscale", mimeType: "video/webm", hwAccelerated: false },
  );
});

test("recording strategy prefers the WebCodecs recorder for within-cap streams when available", async () => {
  // Within-cap stream + WebCodecs surface → the drop-controlled recorder:
  // MSTP ingest means a lagging recorder drops frames from the FILE instead
  // of backpressuring the decode pipeline (the field stutter).
  assert.deepEqual(
    await pick(() => true, STREAM_1080P60, { width: 1920, height: 1080, fps: 60 }, false, true),
    {
      strategy: "webcodecs-track",
      mimeType: "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
      hwAccelerated: true,
    },
  );
  // The encoded transform still wins: zero re-encode beats any re-encode.
  assert.deepEqual(
    await pick(() => true, STREAM_1080P60, CAP_720P30, true, true),
    { strategy: "encoded-transform", mimeType: "video/mp4", hwAccelerated: true },
  );
  // Over-cap streams keep the bounded canvas downscale — the WebCodecs
  // recorder encodes at stream resolution and would blow the user's cap.
  assert.deepEqual(
    await pick(() => true, STREAM_1080P60, CAP_720P30, false, true),
    {
      strategy: "canvas-downscale",
      mimeType: "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
      hwAccelerated: false,
    },
  );
  // Unknown frame rate still qualifies (resolution gate is dominant).
  assert.deepEqual(
    await pick(
      () => true,
      { width: 1920, height: 1080, fps: null },
      { width: 1920, height: 1080, fps: 60 },
      false,
      true,
    ),
    {
      strategy: "webcodecs-track",
      mimeType: "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
      hwAccelerated: true,
    },
  );
  // Without the WebCodecs surface the within-cap fallback stays raw-track.
  assert.deepEqual(
    await pick(() => true, STREAM_1080P60, { width: 1920, height: 1080, fps: 60 }),
    {
      strategy: "raw-track",
      mimeType: "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
      hwAccelerated: true,
    },
  );
});

test("recording stream fps resolves from the negotiated rate first, track estimate second", () => {
  // Field case: a 60 fps session whose remote track reported 103.7 fps
  // (decode catch-up burst). The negotiated rate is the authoritative bound.
  assert.equal(resolveRecordingStreamFps(60, 103.6998012713595), 60);
  // The live estimate is only a fallback when the session exposes none.
  assert.equal(resolveRecordingStreamFps(null, 60), 60);
  assert.equal(resolveRecordingStreamFps(undefined, 30), 30);
  // Degenerate values fall through to null (unknown ≠ known-exceeding).
  assert.equal(resolveRecordingStreamFps(0, 0), null);
  assert.equal(resolveRecordingStreamFps(Number.NaN, Number.POSITIVE_INFINITY), null);
  assert.equal(resolveRecordingStreamFps(undefined, undefined), null);
});

test("field case: bogus 103.7 fps track estimate no longer forces the canvas path", async () => {
  // Stream: 720p60, cap: 1080p60 → fits. With the raw track estimate the
  // strategy gate saw >cap fps and picked the bounded canvas re-encode; with
  // the negotiated fps it stays on the zero-draw-loop raw track.
  const fps = resolveRecordingStreamFps(60, 103.6998012713595);
  assert.deepEqual(
    await pick(() => true, { width: 1280, height: 720, fps }, { width: 1920, height: 1080, fps: 60 }),
    { strategy: "raw-track", mimeType: "video/mp4;codecs=avc1.42E01E,mp4a.40.2", hwAccelerated: true },
  );
});

test("recording cap resolution honors settings and clamps fps", () => {
  // Default settings → 720p30.
  assert.deepEqual(resolveRecordCap("720p", 30), { width: 1280, height: 720, fps: 30 });
  // Explicit 1080p60.
  assert.deepEqual(resolveRecordCap("1080p", 60), { width: 1920, height: 1080, fps: 60 });
  // FPS clamps to the 60 ceiling; fractional values round.
  assert.deepEqual(resolveRecordCap("1080p", 90), { width: 1920, height: 1080, fps: 60 });
  assert.deepEqual(resolveRecordCap("1080p", 29.5), { width: 1920, height: 1080, fps: 30 });
  // Unknown resolution / degenerate fps fall back to the 720p30 default.
  assert.deepEqual(resolveRecordCap("4k", 30), { width: 1280, height: 720, fps: 30 });
  assert.deepEqual(resolveRecordCap("720p", Number.NaN), { width: 1280, height: 720, fps: 30 });
  assert.deepEqual(resolveRecordCap("720p", 0), { width: 1280, height: 720, fps: 30 });
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
  // WebCodecs records at stream resolution too — same ceiling as raw-track.
  assert.equal(clampRecordingBitrate(null, "webcodecs-track"), undefined);
  assert.equal(clampRecordingBitrate(30, "webcodecs-track"), 30_000_000);
  assert.equal(clampRecordingBitrate(80, "webcodecs-track"), 75_000_000);
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
