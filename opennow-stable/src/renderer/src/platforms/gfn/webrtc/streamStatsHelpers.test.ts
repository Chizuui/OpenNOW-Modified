import assert from "node:assert/strict";
import test from "node:test";

import {
  computeIntervalFrameRates,
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
