/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  computeRecordingFrameShortfall,
  fitThumbnailSize,
  getShortcutConflictError,
  selectRecordingMimeType,
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
