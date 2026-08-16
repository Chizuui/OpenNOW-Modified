/// <reference types="node" />

import assert from "node:assert/strict";
import test from "node:test";

import {
  computeRelativeMouseDelta,
  quantizeMouseDeltaWithResidual,
  subsampleCoalescedPointerEvents,
} from "./mouseInput";

test("relative deltas are raw 1:1 — no server-width ÷ window-width scaling", () => {
  // The exact HID counts must pass through at the default scale: the flush
  // path must never multiply by anything derived from the window size.
  assert.deepEqual(computeRelativeMouseDelta(10, -5), {
    dxServer: 10,
    dyServer: -5,
    residualX: 0,
    residualY: 0,
  });
  assert.deepEqual(computeRelativeMouseDelta(-1, 7), {
    dxServer: -1,
    dyServer: 7,
    residualX: 0,
    residualY: 0,
  });
  assert.deepEqual(computeRelativeMouseDelta(0, 0), null);
});

test("fractional deltas quantize and carry the residual", () => {
  const result = computeRelativeMouseDelta(10.6, -3.2);
  assert.equal(result?.dxServer, 11);
  assert.equal(result?.dyServer, -3);
  assert.equal(result?.residualX, 10.6 - 11);
  assert.equal(result?.residualY, -3.2 - -3);
});

test("deltas below half a count produce nothing to send", () => {
  assert.equal(computeRelativeMouseDelta(0.4, 0), null);
  assert.equal(computeRelativeMouseDelta(0.4, -0.4), null);
  assert.equal(computeRelativeMouseDelta(0, 0.4), null);
});

test("deltas clamp to the wire format's i16 range", () => {
  assert.deepEqual(computeRelativeMouseDelta(40000, 0), {
    dxServer: 32767,
    dyServer: 0,
    residualX: 0,
    residualY: 0,
  });
  assert.deepEqual(computeRelativeMouseDelta(-40000, 0), {
    dxServer: -32768,
    dyServer: 0,
    residualX: 0,
    residualY: 0,
  });
  assert.deepEqual(computeRelativeMouseDelta(0, 40000), {
    dxServer: 0,
    dyServer: 32767,
    residualX: 0,
    residualY: 0,
  });
  // The residual still carries the fractional part even when the integer send
  // is clamped, so no movement is lost across the i16 boundary.
  const clamped = computeRelativeMouseDelta(32767.5, 0);
  assert.equal(clamped?.dxServer, 32767);
  assert.equal(clamped?.residualX, 32767.5 - 32768);
});

test("quantizeMouseDeltaWithResidual rounds and preserves the remainder", () => {
  const positive = quantizeMouseDeltaWithResidual(10.6);
  assert.equal(positive.send, 11);
  assert.ok(Math.abs(positive.residual - -0.4) < 1e-9, `residual ${positive.residual}`);

  const negative = quantizeMouseDeltaWithResidual(-3.2);
  assert.equal(negative.send, -3);
  assert.ok(Math.abs(negative.residual - -0.2) < 1e-9, `residual ${negative.residual}`);
});

test("subsampleCoalescedPointerEvents preserves the summed movement", () => {
  const samples = [
    { movementX: 3, movementY: 1 },
    { movementX: -1, movementY: 2 },
    { movementX: 5, movementY: 0 },
  ];
  const { events, stride } = subsampleCoalescedPointerEvents(samples, 0, 1);
  const totalX = events.reduce((sum, event) => sum + event.movementX, 0);
  const totalY = events.reduce((sum, event) => sum + event.movementY, 0);
  assert.equal(stride, 3);
  assert.equal(totalX, 7);
  assert.equal(totalY, 3);
  assert.equal(events.length, 1);
});
