import assert from "node:assert/strict";
import test from "node:test";

import {
  NATIVE_BITRATE_PUSH_MOVE_THRESHOLD_MBPS,
  NATIVE_BITRATE_PUSH_VERIFY_WINDOW_MS,
  parseNetworkHealthBitrateMbps,
  stepNativeBitratePushVerification,
  type NativeBitratePushVerification,
} from "./networkHealth";

function verification(pushedAtMs: number, baselineMbps: number | null, pushNumber = 1): NativeBitratePushVerification {
  return { pushedAtMs, baselineMbps, pushNumber };
}

test("parseNetworkHealthBitrateMbps extracts the measured bitrate", () => {
  assert.equal(
    parseNetworkHealthBitrateMbps(
      "[NetworkHealth] server rtt=41ms loss=0.0200% rtcp=n/a (receiver-only) sinkDrop=0.00% sink=60.8fps bitrate=2.8Mbps rtcp_sent=...",
    ),
    2.8,
  );
  assert.equal(
    parseNetworkHealthBitrateMbps("[NetworkHealth] server rtt=0ms loss=0.02% rtcp=n/a sinkDrop=0.00% sink=51.6fps bitrate=3.1Mbps rtcp_sent=..."),
    3.1,
  );
});

test("parseNetworkHealthBitrateMbps returns null for unrelated log lines", () => {
  assert.equal(parseNetworkHealthBitrateMbps("Native video sink rate: 59.9 fps; capsFramerate=60/1"), null);
  assert.equal(parseNetworkHealthBitrateMbps("[NetworkHealth] server rtt=0ms loss=0.02%"), null);
  assert.equal(parseNetworkHealthBitrateMbps(""), null);
});

test("first sample after push becomes the baseline", () => {
  const push = verification(1_000, null);
  const event = stepNativeBitratePushVerification(push, 3.0, 2_000);
  assert.deepEqual(event, { kind: "baseline", baselineMbps: 3.0 });
});

test("missing samples keep waiting without advancing the window deadline", () => {
  const push = verification(1_000, null);
  // 20s later with no usable sample: still waiting (deadline is clock-based, but
  // no baseline yet means nothing to compare against — stay open).
  assert.equal(stepNativeBitratePushVerification(push, null, 21_000), null);
});

test("verified when bitrate rises at least the threshold within the window", () => {
  const push = verification(1_000, 3.0);
  const event = stepNativeBitratePushVerification(push, 4.5, 6_000);
  assert.deepEqual(event, {
    kind: "verified",
    baselineMbps: 3.0,
    currentMbps: 4.5,
    elapsedMs: 5_000,
    pushNumber: 1,
  });
});

test("sample below threshold within window keeps waiting", () => {
  const push = verification(1_000, 3.0);
  assert.equal(stepNativeBitratePushVerification(push, 3.4, 5_000), null);
});

test("unchanged when the window expires without a rise", () => {
  const push = verification(1_000, 3.0);
  const event = stepNativeBitratePushVerification(push, 3.2, 1_000 + NATIVE_BITRATE_PUSH_VERIFY_WINDOW_MS);
  assert.deepEqual(event, {
    kind: "unchanged",
    baselineMbps: 3.0,
    elapsedMs: NATIVE_BITRATE_PUSH_VERIFY_WINDOW_MS,
    pushNumber: 1,
  });
});

test("threshold boundary: exactly threshold below the rise is not verified", () => {
  const push = verification(1_000, 3.0);
  assert.equal(
    stepNativeBitratePushVerification(push, 3.0 + NATIVE_BITRATE_PUSH_MOVE_THRESHOLD_MBPS - 0.01, 5_000),
    null,
  );
});
