/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  framesToRefreshRate,
  recommendedStreamFps,
  shouldAutoUpgradeStreamFps,
} from "./displayRefreshRate";

test("recommendedStreamFps maps display refresh to GFN FPS tiers like GFN web", () => {
  // >=233Hz -> 240 FPS tier
  assert.equal(recommendedStreamFps(240), 240);
  assert.equal(recommendedStreamFps(233), 240);
  // >=117Hz -> 120 FPS tier (official eE(h) { return h >= 117 })
  assert.equal(recommendedStreamFps(360), 240);
  assert.equal(recommendedStreamFps(165), 120);
  assert.equal(recommendedStreamFps(144), 120);
  assert.equal(recommendedStreamFps(120), 120);
  assert.equal(recommendedStreamFps(117), 120);
  // >=90Hz -> 90 FPS tier
  assert.equal(recommendedStreamFps(116), 90);
  assert.equal(recommendedStreamFps(100), 90);
  assert.equal(recommendedStreamFps(90), 90);
  // below 90Hz (or unknown) -> 60 FPS
  assert.equal(recommendedStreamFps(89), 60);
  assert.equal(recommendedStreamFps(75), 60);
  assert.equal(recommendedStreamFps(60), 60);
  assert.equal(recommendedStreamFps(0), 60);
  assert.equal(recommendedStreamFps(Number.NaN), 60);
});

test("framesToRefreshRate floors like the official client (no tier-boundary flips)", () => {
  // 2s window: count includes the first callback AND the one crossing the
  // window edge (+up to 1), so floor must absorb the overcount — a 116 Hz
  // display stays 116 (not 117 -> 120 tier), 89 Hz stays 89 (not 90 tier).
  assert.equal(framesToRefreshRate(288, 2000), 144);
  assert.equal(framesToRefreshRate(289, 2000), 144);
  assert.equal(framesToRefreshRate(240, 2000), 120);
  assert.equal(framesToRefreshRate(241, 2000), 120);
  assert.equal(framesToRefreshRate(232, 2000), 116);
  assert.equal(framesToRefreshRate(233, 2000), 116);
  assert.equal(framesToRefreshRate(178, 2000), 89);
  assert.equal(framesToRefreshRate(179, 2000), 89);
  assert.equal(framesToRefreshRate(120, 1000), 120);
  assert.equal(framesToRefreshRate(0, 2000), 0);
  assert.equal(framesToRefreshRate(100, 0), 0);
  assert.equal(framesToRefreshRate(Number.NaN, 2000), 0);
});

test("auto FPS upgrade only touches the untouched default FPS", () => {
  // Default 60 on a high-refresh display -> upgrade.
  assert.equal(shouldAutoUpgradeStreamFps(60, 60, 120), true);
  assert.equal(shouldAutoUpgradeStreamFps(60, 60, 240), true);
  // Explicit choices (even 60, or a lower-than-default value) are respected.
  assert.equal(shouldAutoUpgradeStreamFps(120, 60, 120), false);
  assert.equal(shouldAutoUpgradeStreamFps(90, 60, 120), false);
  assert.equal(shouldAutoUpgradeStreamFps(30, 60, 120), false);
  // No higher tier available / unknown display -> stay put.
  assert.equal(shouldAutoUpgradeStreamFps(60, 60, 60), false);
  assert.equal(shouldAutoUpgradeStreamFps(60, 60, 0), false);
});
