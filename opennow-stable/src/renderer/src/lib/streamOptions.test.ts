import assert from "node:assert/strict";
import test from "node:test";

import {
  cyclePacingMode,
  FPS_DOWNGRADE_LADDER,
  isCustomPacingFps,
  nextLowerFps,
  nextLowerResolution,
  pacingFpsOptions,
  pacingModeOptions,
} from "./streamOptions";

test("nextLowerFps steps one rung down the ladder", () => {
  // 240 → 120 → 60 → 30; below the bottom rung there is nothing to step to.
  assert.deepEqual(FPS_DOWNGRADE_LADDER, [240, 120, 60, 30]);
  assert.equal(nextLowerFps(240), 120);
  assert.equal(nextLowerFps(165), 120);
  assert.equal(nextLowerFps(144), 120);
  assert.equal(nextLowerFps(120), 60);
  assert.equal(nextLowerFps(60), 30);
  // Already minimal: no lower profile (the caller then steps resolution).
  assert.equal(nextLowerFps(30), null);
  assert.equal(nextLowerFps(24), null);
});

test("nextLowerResolution steps one rung down preserving aspect ratio", () => {
  // 16:9 family: 3840x2160 → 2560x1440 → 1920x1080 → 1280x720.
  assert.equal(nextLowerResolution("3840x2160"), "2560x1440");
  assert.equal(nextLowerResolution("2560x1440"), "1920x1080");
  assert.equal(nextLowerResolution("1920x1080"), "1280x720");
  // Bottom of the family: no lower rung.
  assert.equal(nextLowerResolution("1280x720"), null);
  // Unknown resolution falls back to the safe 1080p mid-rung.
  assert.equal(nextLowerResolution("9999x9999"), "1920x1080");
});

test("pacing mode constants expose named modes plus custom fps", () => {
  // Named modes are the native present-limiter sentinels; custom fps values
  // are serialized as numeric strings and detected via isCustomPacingFps.
  assert.deepEqual([...pacingModeOptions], ["auto", "stream", "vrr", "off"]);
  assert.deepEqual([...pacingFpsOptions], [60, 120, 144, 165, 240]);
  assert.equal(isCustomPacingFps("120"), true);
  assert.equal(isCustomPacingFps("60"), true);
  assert.equal(isCustomPacingFps("auto"), false);
  assert.equal(isCustomPacingFps("vrr"), false);
  assert.equal(isCustomPacingFps(""), false);
  assert.equal(isCustomPacingFps("120fps"), false);
});

test("cyclePacingMode steps the named cycle and wraps", () => {
  // auto → stream → vrr → off → auto (the chip order in the quality panel).
  assert.equal(cyclePacingMode("auto"), "stream");
  assert.equal(cyclePacingMode("stream"), "vrr");
  assert.equal(cyclePacingMode("vrr"), "off");
  assert.equal(cyclePacingMode("off"), "auto");
  // A custom fps override is outside the named loop: step to "off" (end of
  // the cycle), so the next press wraps back to auto.
  assert.equal(cyclePacingMode("144"), "off");
  assert.equal(cyclePacingMode("60"), "off");
});
