import assert from "node:assert/strict";
import test from "node:test";

import {
  computeIntervalFrameRates,
  mapServerGpuType,
  smoothJitterMs,
  type IntervalFrameRateParams,
} from "./streamStatsHelpers";

function baseParams(overrides: Partial<IntervalFrameRateParams> = {}): IntervalFrameRateParams {
  return {
    framesReceived: 120,
    framesDecoded: 120,
    totalDecodeTime: 1.0,
    prevFramesReceived: 60,
    prevFramesDecoded: 60,
    prevTotalDecodeTime: 0.5,
    timeDeltaMs: 1000,
    prevReceiveFps: 60,
    prevDecodeFps: 60,
    prevDecodeTimeMs: 8.3,
    ...overrides,
  };
}

test("computes 60fps RX and decode rates from a 60-frame interval", () => {
  const rates = computeIntervalFrameRates(baseParams());
  assert.equal(rates.receiveFps, 60);
  assert.equal(rates.decodeFps, 60);
});

test("per-interval decode time divides the decode-time delta by decoded frames", () => {
  // 0.5s of decode time over 60 frames = 8.33ms each.
  const rates = computeIntervalFrameRates(baseParams());
  assert.equal(rates.decodeTimeMs, 8.3);
});

test("decode rate below the RX rate exposes a local decoder bottleneck", () => {
  const rates = computeIntervalFrameRates(
    baseParams({ framesDecoded: 105, prevFramesDecoded: 60 }),
  );
  assert.equal(rates.receiveFps, 60);
  assert.equal(rates.decodeFps, 45);
});

test("frames arriving but none decoded reports decodeFps 0 (stall)", () => {
  const rates = computeIntervalFrameRates(
    baseParams({ framesDecoded: 60, prevFramesDecoded: 60 }),
  );
  assert.equal(rates.receiveFps, 60);
  assert.equal(rates.decodeFps, 0);
});

test("static frame keeps the last measured rates instead of flashing 0", () => {
  const rates = computeIntervalFrameRates(
    baseParams({ framesReceived: 60, framesDecoded: 60 }),
  );
  assert.equal(rates.receiveFps, 60);
  assert.equal(rates.decodeFps, 60);
  assert.equal(rates.decodeTimeMs, 8.3);
});

test("decode time over the 60fps frame budget (16.7ms) is surfaced", () => {
  const rates = computeIntervalFrameRates(
    baseParams({ totalDecodeTime: 1.5, prevTotalDecodeTime: 0.5, framesDecoded: 120, prevFramesDecoded: 60 }),
  );
  // 1.0s decode time over 60 frames = 16.7ms — right at the budget.
  assert.equal(rates.decodeTimeMs, 16.7);
  const overBudget = computeIntervalFrameRates(
    baseParams({ totalDecodeTime: 1.6, prevTotalDecodeTime: 0.5 }),
  );
  assert.ok(overBudget.decodeTimeMs > 16.7);
});

test("decode time keeps its last value when nothing was decoded", () => {
  const rates = computeIntervalFrameRates(
    baseParams({ framesDecoded: 60, prevFramesDecoded: 60 }),
  );
  assert.equal(rates.decodeTimeMs, 8.3);
});

test("non-positive time delta returns the previous rates unchanged", () => {
  const rates = computeIntervalFrameRates(baseParams({ timeDeltaMs: 0 }));
  assert.deepEqual(rates, {
    receiveFps: 60,
    decodeFps: 60,
    decodeTimeMs: 8.3,
  });
});

test("negative frame counters (Chromium resets on codec switch) keep the last rates", () => {
  const rates = computeIntervalFrameRates(
    baseParams({ framesReceived: 10, prevFramesReceived: 90, framesDecoded: 10, prevFramesDecoded: 90 }),
  );
  // The clamp treats the reset as "nothing new", so the HUD keeps the last
  // healthy rates instead of flashing 0 mid-session.
  assert.equal(rates.receiveFps, 60);
  assert.equal(rates.decodeFps, 60);
  assert.equal(rates.decodeTimeMs, 8.3);
});

test("mapServerGpuType translates raw CloudMatch codes to official rig names", () => {
  assert.equal(mapServerGpuType("2080d / T10"), "GeForce RTX");
  assert.equal(mapServerGpuType("3080p / A10Gx2"), "GeForce RTX 3080");
  assert.equal(mapServerGpuType("4080h / L40S"), "GeForce RTX 4080");
  assert.equal(mapServerGpuType("5080h / B40"), "GeForce RTX 5080");
  assert.equal(mapServerGpuType("1060b / T10-8"), "Basic Rig");
});

test("mapServerGpuType passes unknown codes through unchanged and trims input", () => {
  assert.equal(mapServerGpuType("9999z / X99"), "9999z / X99");
  assert.equal(mapServerGpuType(" 2080d / T10 "), "GeForce RTX");
  assert.equal(mapServerGpuType("   "), "");
  assert.equal(mapServerGpuType(""), "");
});

test("smoothJitterMs returns the first sample as-is for an immediate readout", () => {
  assert.equal(smoothJitterMs(8.4, 0), 8.4);
});

test("smoothJitterMs clips a one-off spike but lands sustained shifts", () => {
  // Steady 5ms EWMA; a single 30ms spike moves the readout only partway.
  const afterSpike = smoothJitterMs(30, 5);
  assert.ok(afterSpike > 5 && afterSpike < 30, `spike blended to ${afterSpike}`);
  // Sustained 30ms for a few polls converges toward the new level.
  let ewma = 5;
  for (let i = 0; i < 6; i += 1) {
    ewma = smoothJitterMs(30, ewma);
  }
  assert.ok(ewma > 25, `converged to ${ewma}`);
});

test("smoothJitterMs decays toward zero when the stream stops reporting", () => {
  const afterDecay = smoothJitterMs(0, 8);
  assert.ok(afterDecay < 8, `decayed to ${afterDecay}`);
  assert.ok(afterDecay > 0, `still holds residual ${afterDecay}`);
});
